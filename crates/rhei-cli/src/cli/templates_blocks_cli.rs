    // The explicit direct-composition grammar cannot reinterpret legacy
    // positional template inputs. §FS-rhei-library.3 §FS-rhei-library.8
    fn parse_mounts(raw: &[String]) -> MietteResult<Vec<Mount>> {
        let mut mounts = Vec::new();
        let mut seen = BTreeMap::<String, String>::new();
        for value in raw {
            let Some((alias, block)) = value.split_once('=') else {
                return Err(miette!("--mount expects ALIAS=BLOCK, got '{}'", value));
            };
            if !rhei_core::blocks::valid_identifier(alias) {
                return Err(miette!(
                    help = "an alias must start with a letter and continue with letters, digits, '_' or '-'.",
                    "invalid mount alias '{}'",
                    alias
                ));
            }
            if let Some(previous) = seen.insert(alias.to_string(), block.to_string()) {
                return Err(miette!(
                    "duplicate mount alias '{}': manifests '{}' and '{}'",
                    alias, previous, block
                ));
            }
            mounts.push(Mount { alias: alias.into(), block: block.into() });
        }
        Ok(mounts)
    }

    fn parse_cli_seams(raw_seams: &[String], raw_passes: &[String]) -> MietteResult<Option<Vec<Seam>>> {
        if raw_seams.is_empty() {
            if raw_passes.is_empty() {
                return Ok(None);
            }
            return Err(miette!("--pass requires a declared --seam between its endpoint mounts"));
        }
        let mut seams = raw_seams
            .iter()
            .map(|raw| {
                let (from, to) = raw.split_once('=').ok_or_else(|| {
                    miette!("--seam expects SOURCE=TARGET, got '{}'", raw)
                })?;
                Ok(Seam { from: from.into(), to: to.into(), pass: BTreeMap::new() })
            })
            .collect::<MietteResult<Vec<_>>>()?;
        for raw in raw_passes {
            let (from, to) = raw.split_once('=').ok_or_else(|| {
                miette!("--pass expects SOURCE=TARGET, got '{}'", raw)
            })?;
            let (from_alias, _) = split_endpoint(from)
                .ok_or_else(|| miette!("invalid data endpoint '{}'", from))?;
            let (to_alias, _) = split_endpoint(to)
                .ok_or_else(|| miette!("invalid data endpoint '{}'", to))?;
            let matching = seams
                .iter_mut()
                .filter(|seam| {
                    split_endpoint(&seam.from).map(|(alias, _)| alias) == Some(from_alias)
                        && split_endpoint(&seam.to).map(|(alias, _)| alias) == Some(to_alias)
                })
                .collect::<Vec<_>>();
            if matching.len() != 1 {
                return Err(miette!(
                    "pass '{}={}' must match exactly one declared seam between '{}' and '{}'",
                    from, to, from_alias, to_alias
                ));
            }
            matching.into_iter().next().expect("one seam").pass.insert(from.into(), to.into());
        }
        Ok(Some(seams))
    }

    fn print_mounted_inputs(mounts: &[Mount]) -> MietteResult<()> {
        for mount in mounts {
            let resolved = resolve_template_reference(&mount.block).map_err(|err| {
                miette!("failed to resolve block '{}' mounted as '{}': {err}", mount.block, mount.alias)
            })?;
            let manifest = load_template_manifest(resolved.path())?;
            for input in &manifest.inputs {
                println!(
                    "{}.{}  {}  {}",
                    mount.alias,
                    input.name,
                    input.value_type().as_str(),
                    if input.is_required() { "required" } else { "optional" }
                );
                println!("  {}", input.description);
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn instantiate_direct_blocks(
        raw_mounts: &[String],
        raw_seams: &[String],
        raw_passes: &[String],
        values_files: &[PathBuf],
        set_values: &[String],
        set_files: &[String],
        output: Option<&Path>,
        execute: bool,
        dry_run: bool,
        keep_on_error: bool,
        list_inputs: bool,
        execute_args: &[String],
    ) -> MietteResult<()> {
        let mounts = parse_mounts(raw_mounts)?;
        if mounts.is_empty() {
            return Err(miette!("direct block composition requires at least one --mount"));
        }
        if list_inputs {
            return print_mounted_inputs(&mounts);
        }
        let inputs = MountedInputValues::from_cli(values_files, set_values, set_files)?;
        let aliases = mounts.iter().map(|mount| mount.alias.clone()).collect::<Vec<_>>();
        inputs.validate_aliases(&aliases)?;
        let seams = parse_cli_seams(raw_seams, raw_passes)?;
        let root_name = aliases.join("-");

        let mut frontend = BlockFrontend::new()?;
        let mut children = Vec::new();
        for mount in &mounts {
            children.push((mount.alias.clone(), frontend.prepare(&mount.block, None, &inputs.for_alias(&mount.alias), &mount.alias)?));
        }
        let block = Block { name: root_name.clone(), source: PathBuf::from("<command line>"), version: "1".into(), manifest: BlockManifest { mounts, seams, ..Default::default() }, local: None, children };
        let compiled = block.compile().map_err(|e| miette!("{e}"))?;
        instantiate_compiled_workspace(
            compiled,
            &root_name,
            output,
            execute,
            dry_run,
            keep_on_error,
            execute_args,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn instantiate_curated_block(
        template: &str,
        _dir: &Path,
        manifest: &TemplateManifest,
        input_args: &[String],
        set_values: &[String],
        set_files: &[String],
        values_files: &[PathBuf],
        output: Option<&Path>,
        execute: bool,
        dry_run: bool,
        keep_on_error: bool,
        list_inputs: bool,
        execute_args: &[String],
    ) -> MietteResult<()> {
        if list_inputs {
            print_template_inputs(manifest, template);
            return Ok(());
        }
        let values = collect_template_inputs(
            manifest,
            template,
            values_files,
            input_args,
            set_values,
            set_files,
        )?;
        let supplied = values
            .iter()
            .map(|(key, value)| {
                serde_yaml::to_value(value)
                    .map(|value| (key.clone(), value))
                    .map_err(|err| miette!("failed to lower curated input '{}': {err}", key))
            })
            .collect::<MietteResult<BTreeMap<_, _>>>()?;
        let mut frontend = BlockFrontend::new()?;
        let block = frontend.prepare(template, None, &supplied, &manifest.name)?;
        let compiled = block.compile().map_err(|e| miette!("{e}"))?;
        instantiate_compiled_workspace(
            compiled,
            &manifest.name,
            output,
            execute,
            dry_run,
            keep_on_error,
            execute_args,
        )
    }
