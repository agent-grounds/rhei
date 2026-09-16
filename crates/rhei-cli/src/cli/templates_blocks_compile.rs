    // Recursive resolution renders each block in isolation and rejects graph
    // defects before any requested output exists. §AR-rhei-library.3
    impl BlockCompiler {
        fn new() -> MietteResult<Self> {
            Ok(Self {
                scratch: tempfile::tempdir().map_err(|err| {
                    miette!("failed to create block compiler scratch directory: {err}")
                })?,
                leaves: Vec::new(),
                seams: Vec::new(),
                passes: Vec::new(),
                stack: Vec::new(),
                value_counter: 0,
                compatibility: None,
            })
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
                .map_err(|err| miette!("failed to serialize mounted input values: {err}"))?;
            fs::write(&path, rendered)
                .map_err(|err| file_io_report(&path, "failed to write compiler values", err))?;
            collect_template_inputs(manifest, template_ref, &[path], &[], &[], &[])
        }

        fn compile_node(
            &mut self,
            reference: &str,
            relative_to: Option<&Path>,
            qualifier: Qualifier,
            supplied: &BTreeMap<String, YamlValue>,
            via_alias: &str,
        ) -> MietteResult<CompiledNode> {
            let effective_reference = if let Some(parent) = relative_to {
                let path = Path::new(reference);
                if path.is_absolute() || reference.contains('/') || reference.starts_with('.') {
                    parent.join(path).display().to_string()
                } else {
                    reference.to_string()
                }
            } else {
                reference.to_string()
            };
            let resolved = resolve_template_reference(&effective_reference).map_err(|err| {
                miette!(
                    help = "check the block reference with `rhei templates` or use a path to its template directory.",
                    "failed to resolve block '{}' mounted as '{}': {err}",
                    reference, via_alias
                )
            })?;
            let dir = resolved.path().to_path_buf();
            let manifest_path = dir.join("template.yaml");
            let identity = if template_reference_is_path(&effective_reference) {
                fs::canonicalize(&manifest_path)
                    .unwrap_or_else(|_| manifest_path.clone())
                    .display()
                    .to_string()
            } else {
                format!("name:{effective_reference}")
            };
            if let Some(position) = self.stack.iter().position(|(key, _, _)| key == &identity) {
                let mut chain = self.stack[position..]
                    .iter()
                    .map(|(_, path, alias)| format!("{} as {}", path.display(), alias))
                    .collect::<Vec<_>>();
                chain.push(format!("{} as {}", manifest_path.display(), via_alias));
                return Err(miette!(
                    help = "break the recursive `use` chain; reuse in a separate branch is allowed.",
                    "block composition cycle: {}",
                    chain.join(" -> ")
                ));
            }
            let manifest = load_template_manifest(&dir)?;
            let values = self.resolve_values(&manifest, &effective_reference, supplied)?;
            self.stack.push((identity, manifest_path.clone(), via_alias.to_string()));

            let result = if manifest.block.mounts.is_empty() {
                self.compile_leaf(&dir, &effective_reference, &manifest, &values, qualifier)
            } else {
                self.compile_group(&dir, &manifest, &values, qualifier)
            };
            self.stack.pop();
            result
        }

        fn compile_group(
            &mut self,
            dir: &Path,
            manifest: &TemplateManifest,
            values: &BTreeMap<String, serde_json::Value>,
            qualifier: Qualifier,
        ) -> MietteResult<CompiledNode> {
            for binding in &manifest.block.bind {
                let Some((alias, _)) = split_endpoint(&binding.to) else {
                    return Err(miette!("invalid bind target '{}' in '{}'", binding.to, dir.join("template.yaml").display()));
                };
                if !manifest.block.mounts.iter().any(|mount| mount.alias == alias) {
                    return Err(miette!("bind target '{}' names unknown child alias '{}' in '{}'", binding.to, alias, dir.join("template.yaml").display()));
                }
            }
            let mut children = BTreeMap::new();
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
                    miette!(
                        "failed to resolve child block '{}' mounted as '{}' from '{}': {err}",
                        mount.block,
                        mount.alias,
                        dir.join("template.yaml").display()
                    )
                })?;
                let child_manifest = load_template_manifest(child_resolved.path())?;
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
                                child_resolved.path().join("template.yaml").display()
                            )
                        })?;
                    if !compatible_input_schema(&source_input.schema, &target_input.schema) {
                        return Err(miette!(
                            "bind '{}' connects incompatible input schemas in '{}' and '{}'",
                            binding.to,
                            dir.join("template.yaml").display(),
                            child_resolved.path().join("template.yaml").display()
                        ));
                    }
                    if !bound_targets.insert(target) {
                        return Err(miette!("bind target '{}' is assigned more than once", binding.to));
                    }
                    let value = values.get(&binding.input).ok_or_else(|| {
                        miette!("resolved parent input '{}' is missing", binding.input)
                    })?;
                    child_values.insert(
                        target.to_string(),
                        serde_yaml::to_value(value).map_err(|err| {
                            miette!("failed to lower binding '{}': {err}", binding.to)
                        })?,
                    );
                }
                let child = self.compile_node(
                    &mount.block,
                    Some(dir),
                    qualifier.child(&mount.alias),
                    &child_values,
                    &mount.alias,
                )?;
                children.insert(mount.alias.clone(), child);
            }

            let order = self.connect_mounts(&manifest.block, &children, dir)?;
            let ports = manifest.block.ports.as_ref().expect("validated block ports");
            let entry = resolve_group_control(&ports.entry, true, &children, &manifest.name, dir)?;
            let exits = ports
                .exits
                .iter()
                .map(|(name, endpoint)| {
                    resolve_group_control(endpoint, false, &children, &manifest.name, dir)
                        .map(|state| (name.clone(), state))
                })
                .collect::<MietteResult<BTreeMap<_, _>>>()?;

            let mut primary_states = Vec::new();
            for alias in order {
                primary_states.extend(children[&alias].primary_states.iter().cloned());
            }
            if qualifier.chain().is_empty() && !manifest.block.compatibility.is_empty() {
                self.compatibility = Some(resolve_compatibility(
                    &manifest.block.compatibility,
                    &children,
                    &qualifier,
                    &dir.join("template.yaml"),
                )?);
            }
            Ok(CompiledNode {
                name: manifest.name.clone(),
                manifest: dir.join("template.yaml"),
                entry,
                exits,
                inputs: BTreeMap::new(),
                outputs: BTreeMap::new(),
                primary_states,
            })
        }

        fn compile_leaf(
            &mut self,
            dir: &Path,
            template_ref: &str,
            manifest: &TemplateManifest,
            values: &BTreeMap<String, serde_json::Value>,
            qualifier: Qualifier,
        ) -> MietteResult<CompiledNode> {
            let layout = detect_template_layout(dir)?;
            let leaf_dir = self.scratch.path().join(format!("leaf-{}", self.leaves.len()));
            let materialized = materialize_template(
                dir,
                template_ref,
                layout,
                &leaf_dir,
                values,
                true,
            )?;
            let state_path = materialized.state_machine_path().ok_or_else(|| {
                miette!(
                    help = "a mounted leaf block must render states.yaml.",
                    "block '{}' has no state-machine fragment",
                    dir.join("template.yaml").display()
                )
            })?;
            let raw = fs::read_to_string(&state_path)
                .map_err(|err| file_io_report(&state_path, "failed to read block states", err))?;
            let machine: YamlValue = serde_yaml::from_str(&raw).map_err(|err| {
                miette!("failed to parse mounted state fragment '{}': {err}", state_path.display())
            })?;
            validate_local_machine_references(&machine, manifest, &state_path)?;

            let settings_path = leaf_dir.join(".agent-grounds/rhei/settings.json");
            let settings = if settings_path.is_file() {
                let raw = fs::read_to_string(&settings_path).map_err(|err| {
                    file_io_report(&settings_path, "failed to read mounted settings", err)
                })?;
                Some(serde_json::from_str(&raw).map_err(|err| {
                    miette!("failed to parse mounted settings '{}': {err}", settings_path.display())
                })?)
            } else {
                None
            };
            let ownership = settings_ownership(settings.as_ref(), &qualifier);
            let machine = qualify_machine(machine, &qualifier, &ownership, &state_path)?;
            let (tasks, custom_kinds) = qualify_tasks(&leaf_dir, layout, &qualifier, &ownership)?;
            let primary_states = primary_states(&machine)?;

            let ports = manifest.block.ports.as_ref().expect("validated block ports");
            let states = yaml_map(&machine, "states")?;
            let entry = qualified_local_control(&ports.entry, &qualifier, states, manifest, dir)?;
            let exits = ports
                .exits
                .iter()
                .map(|(name, state)| {
                    qualified_local_control(state, &qualifier, states, manifest, dir)
                        .map(|state| (name.clone(), state))
                })
                .collect::<MietteResult<BTreeMap<_, _>>>()?;
            let inputs = resolve_leaf_data(
                &manifest.block.data.inputs,
                &qualifier,
                &machine,
                &tasks,
                dir,
                false,
            )?;
            let outputs = resolve_leaf_data(
                &manifest.block.data.outputs,
                &qualifier,
                &machine,
                &tasks,
                dir,
                true,
            )?;

            self.leaves.push(CompiledLeaf {
                name: manifest.name.clone(),
                version: manifest.version_string(),
                source: dir.to_path_buf(),
                qualifier,
                machine,
                tasks,
                custom_kinds,
                settings,
                rendered: leaf_dir,
            });
            Ok(CompiledNode {
                name: manifest.name.clone(),
                manifest: dir.join("template.yaml"),
                entry,
                exits,
                inputs,
                outputs,
                primary_states,
            })
        }

        fn connect_mounts(
            &mut self,
            block: &BlockManifest,
            nodes: &BTreeMap<String, CompiledNode>,
            dir: &Path,
        ) -> MietteResult<Vec<String>> {
            let aliases = block.mounts.iter().map(|mount| mount.alias.clone()).collect::<Vec<_>>();
            let seams = block.seams.clone().unwrap_or_else(|| {
                aliases
                    .windows(2)
                    .map(|pair| Seam {
                        from: format!("{}.done", pair[0]),
                        to: format!("{}.entry", pair[1]),
                        pass: BTreeMap::new(),
                    })
                    .collect()
            });
            let order = linear_seam_order(&aliases, &seams).map_err(|message| {
                miette!(
                    help = format!("declare a complete chain covering: {}", aliases.join(", ")),
                    "invalid seams in '{}': {message}",
                    dir.join("template.yaml").display()
                )
            })?;
            for seam in seams {
                self.add_resolved_seam(&seam, nodes)?;
            }
            Ok(order)
        }

        fn add_resolved_seam(
            &mut self,
            seam: &Seam,
            nodes: &BTreeMap<String, CompiledNode>,
        ) -> MietteResult<()> {
            let (from_alias, from_port) = split_endpoint(&seam.from)
                .ok_or_else(|| miette!("invalid control endpoint '{}'", seam.from))?;
            let source = nodes.get(from_alias).ok_or_else(|| {
                miette!("control endpoint '{}' names unknown mount '{}'", seam.from, from_alias)
            })?;
            let from = source.exits.get(from_port).ok_or_else(|| {
                miette!(
                    help = format!(
                        "public exits of '{}' from {}: {}",
                        source.name,
                        source.manifest.display(),
                        source.exits.keys().cloned().collect::<Vec<_>>().join(", ")
                    ),
                    "unknown control port '{}'",
                    seam.from
                )
            })?;
            let (to_alias, to_port) = split_endpoint(&seam.to)
                .ok_or_else(|| miette!("invalid control endpoint '{}'", seam.to))?;
            let target = nodes.get(to_alias).ok_or_else(|| {
                miette!("control endpoint '{}' names unknown mount '{}'", seam.to, to_alias)
            })?;
            if to_port != "entry" {
                return Err(miette!(
                    help = format!("'{}.entry' is the public entry of '{}'", to_alias, target.name),
                    "unknown control port '{}'",
                    seam.to
                ));
            }
            self.seams.push(ResolvedSeam { from: from.clone(), to: target.entry.clone() });
            for (from_data, to_data) in &seam.pass {
                self.add_resolved_pass(from_data, to_data, nodes)?;
            }
            Ok(())
        }

        fn add_resolved_pass(
            &mut self,
            source_label: &str,
            target_label: &str,
            nodes: &BTreeMap<String, CompiledNode>,
        ) -> MietteResult<()> {
            let (source_alias, source_name) = split_endpoint(source_label)
                .ok_or_else(|| miette!("invalid data endpoint '{}'", source_label))?;
            let source_node = nodes
                .get(source_alias)
                .ok_or_else(|| miette!("data endpoint '{}' names unknown mount", source_label))?;
            let source = source_node.outputs.get(source_name).ok_or_else(|| {
                miette!(
                    help = format!(
                        "public outputs of '{}' from {}: {}",
                        source_node.name,
                        source_node.manifest.display(),
                        source_node.outputs.keys().cloned().collect::<Vec<_>>().join(", ")
                    ),
                    "unknown data endpoint '{}'",
                    source_label
                )
            })?;
            let (target_alias, target_name) = split_endpoint(target_label)
                .ok_or_else(|| miette!("invalid data endpoint '{}'", target_label))?;
            let target_node = nodes
                .get(target_alias)
                .ok_or_else(|| miette!("data endpoint '{}' names unknown mount", target_label))?;
            let target = target_node.inputs.get(target_name).ok_or_else(|| {
                miette!(
                    help = format!(
                        "public inputs of '{}' from {}: {}",
                        target_node.name,
                        target_node.manifest.display(),
                        target_node.inputs.keys().cloned().collect::<Vec<_>>().join(", ")
                    ),
                    "unknown data endpoint '{}'",
                    target_label
                )
            })?;
            if source.kind != target.kind {
                return Err(miette!(
                    "data pass '{}' ({}, {}) -> '{}' ({}, {}) has incompatible kinds {} and {}",
                    source_label,
                    source.manifest.display(),
                    source_node.name,
                    target_label,
                    target.manifest.display(),
                    target_node.name,
                    source.kind,
                    target.kind
                ));
            }
            self.passes.push(ResolvedPass {
                source: source.clone(),
                target: target.clone(),
                source_label: source_label.to_string(),
                target_label: target_label.to_string(),
            });
            Ok(())
        }

    }
