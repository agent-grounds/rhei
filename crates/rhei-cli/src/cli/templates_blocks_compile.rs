    // The CLI resolves and renders; recursive graph lowering belongs to core.
    // §AR-rhei-library.1–3 §FS-rhei-library.8
    struct BlockFrontend {
        scratch: tempfile::TempDir,
        stack: Vec<(PathBuf, String)>,
        value_counter: usize,
        render_counter: usize,
    }
    impl BlockFrontend {
        fn new() -> MietteResult<Self> {
            Ok(Self { scratch: tempfile::tempdir().map_err(|e| miette!(help = "check that the compiler temporary directory is writable", "compiler scratch: {e}"))?, stack: Vec::new(), value_counter: 0, render_counter: 0 })
        }
        fn resolve_values(
            &mut self,
            manifest: &TemplateManifest,
            template_ref: &str,
            supplied: &BTreeMap<String, YamlValue>,
        ) -> MietteResult<BTreeMap<String, serde_json::Value>> {
            if supplied.is_empty() {
                return collect_template_inputs(manifest, template_ref, &[], &[], &[], &[]);
            }
            self.value_counter += 1;
            let path = self
                .scratch
                .path()
                .join(format!("values-{}.yaml", self.value_counter));
            let rendered = serde_yaml::to_string(supplied)
                .map_err(|err| miette!(help = "provide serializable values for the mounted inputs", "failed to serialize mounted input values: {err}"))?;
            fs::write(&path, rendered)
                .map_err(|err| file_io_report(&path, "failed to write compiler values", err))?;
            collect_template_inputs(manifest, template_ref, &[path], &[], &[], &[])
        }


        fn prepare(&mut self, reference: &str, relative_to: Option<&Path>, supplied: &BTreeMap<String, YamlValue>, alias: &str) -> MietteResult<Block> {
            let effective = if let Some(parent) = relative_to {
                if template_reference_is_path(reference) { parent.join(reference).display().to_string() } else { reference.into() }
            } else { reference.into() };
            let resolved = resolve_template_reference(&effective).map_err(|e| e.wrap_err(format!("block '{reference}' mounted as '{alias}'; check rhei templates")))?;
            let dir = fs::canonicalize(resolved.path()).map_err(|e| file_io_report(resolved.path(), "resolve block source", e))?;
            let source_identity = composition_source_identity(reference, &resolved, &dir)?;
            let manifest_path = if resolved._extracted.is_some() { PathBuf::from(format!("built-in/{reference}/template.yaml")) } else { dir.join("template.yaml") };
            if self.stack.iter().any(|(path, _)| path == &manifest_path) {
                let mut chain = self.stack.iter().map(|(p,a)| format!("{} as {a}", p.display())).collect::<Vec<_>>();
                chain.push(format!("{} as {alias}", manifest_path.display()));
                return Err(miette!(help = "remove one use: entry to break the recursive cycle", "block composition cycle: {}; break the recursive use chain", chain.join(" -> ")));
            }
            let manifest = self.load_manifest(&dir, alias)?;
            let values = self.resolve_values(&manifest, reference, supplied)?;
            self.stack.push((manifest_path.clone(), alias.into()));
            let chain = self.stack.iter().map(|(_,a)| a.as_str()).collect::<Vec<_>>().join(".");
            let result = self.prepare_inner(&dir, reference, &manifest, &values)
                .map(|mut block| { block.source = manifest_path.clone(); block.source_identity = source_identity; block })
                .map_err(|e| e.wrap_err(format!("{} [mount {chain}]", manifest_path.display())));
            self.stack.pop();
            result
        }
        // Schema errors can precede rendering, but still belong to this mount chain. §FS-rhei-library.8
        fn load_manifest(&self, dir: &Path, alias: &str) -> MietteResult<TemplateManifest> {
            let chain = self.stack.iter().map(|(_, alias)| alias.as_str())
                .chain(std::iter::once(alias)).collect::<Vec<_>>().join(".");
            load_template_manifest(dir)
                .map_err(|err| err.wrap_err(format!("{} [mount {chain}]", dir.join("template.yaml").display())))
        }
        fn prepare_inner(&mut self, dir: &Path, reference: &str, manifest: &TemplateManifest, values: &BTreeMap<String, serde_json::Value>) -> MietteResult<Block> {
            let manifest = select_block_declarations(manifest, values, dir, reference)?;
            for binding in &manifest.block.bind {
                let Some((alias, _)) = split_endpoint(&binding.to) else {
                    return Err(miette!(help = "use a child input endpoint in alias.input form", "invalid bind target '{}' in '{}'", binding.to, dir.join("template.yaml").display()));
                };
                if !manifest.block.mounts.iter().any(|mount| mount.alias == alias) {
                    return Err(miette!(help = "use one of the aliases declared by use", "bind target '{}' names unknown child alias '{}' in '{}'", binding.to, alias, dir.join("template.yaml").display()));
                }
            }
            let mut children = Vec::new();
            for mount in &manifest.block.mounts {
                let child_reference = {
                    let path = Path::new(&mount.block);
                    if path.is_absolute()
                        || mount.block.contains('/')
                        || mount.block.starts_with('.')
                    {
                        dir.join(path).display().to_string()
                    } else {
                        mount.block.clone()
                    }
                };
                let child_resolved = resolve_template_reference(&child_reference).map_err(|err| {
                    err.wrap_err(format!(
                        "failed to resolve child block '{}' mounted as '{}' from '{}'",
                        mount.block,
                        mount.alias,
                        dir.join("template.yaml").display()
                    ))
                })?;
                let child_manifest = self.load_manifest(child_resolved.path(), &mount.alias)?;
                let mut child_values = BTreeMap::new();
                let mut bound_targets = BTreeSet::new();
                for binding in manifest.block.bind.iter().filter(|binding| {
                    split_endpoint(&binding.to).is_some_and(|(alias, _)| alias == mount.alias)
                }) {
                    let (_, target) = split_endpoint(&binding.to).expect("filtered endpoint");
                    let source_input = manifest
                        .inputs
                        .iter()
                        .find(|input| input.name == binding.input)
                        .ok_or_else(|| {
                        miette!(
                            help = "declare the parent input before binding it",
                            "bind '{}' in '{}' names missing parent input '{}'",
                            binding.to,
                            dir.join("template.yaml").display(),
                            binding.input
                        )
                    })?;
                    let target_input = child_manifest
                        .inputs
                        .iter()
                        .find(|input| input.name == target)
                        .ok_or_else(|| {
                            let alternatives = child_manifest
                                .inputs
                                .iter()
                                .map(|input| format!("{}.{}", mount.alias, input.name))
                                .collect::<Vec<_>>()
                                .join(", ");
                            miette!(
                                help = format!("valid child inputs: {alternatives}"),
                                "bind target '{}' from '{}' does not exist in '{}'",
                                binding.to,
                                dir.join("template.yaml").display(),
                                fs::canonicalize(child_resolved.path()).unwrap_or_else(|_| child_resolved.path().to_path_buf()).join("template.yaml").display()
                            )
                        })?;
                    if !compatible_input_schema(&source_input.schema, &target_input.schema) {
                        return Err(miette!(
                            help = "make the source and target input schemas compatible",
                            "bind '{}' connects incompatible input schemas in '{}' and '{}'",
                            binding.to,
                            dir.join("template.yaml").display(),
                            fs::canonicalize(child_resolved.path()).unwrap_or_else(|_| child_resolved.path().to_path_buf()).join("template.yaml").display()
                        ));
                    }
                    if !bound_targets.insert(target) {
                        return Err(miette!(help = "remove the duplicate bind for this child input", "bind target '{}' is assigned more than once", binding.to));
                    }
                    let value = values.get(&binding.input).ok_or_else(|| {
                        miette!(help = "supply the parent input before compiling the block", "resolved parent input '{}' is missing", binding.input)
                    })?;
                    child_values.insert(
                        target.to_string(),
                        serde_yaml::to_value(value).map_err(|err| {
                            miette!(help = "use a serializable value for the bound input", "failed to lower binding '{}': {err}", binding.to)
                        })?,
                    );
                }

                let child = self.prepare(&mount.block, Some(dir), &child_values, &mount.alias)?;
                children.push((mount.alias.clone(), child));
            }
            // A mounting manifest may contribute its own plan and machine.
            let local = Some(self.render_fragment(dir, reference, values, !manifest.block.mounts.is_empty())?);
            Ok(Block { name: manifest.name.clone(), source: dir.join("template.yaml"), version: manifest.version_string(), source_identity: SourceIdentity::unavailable(reference), manifest: manifest.block.clone(), local, children })
        }
    }
