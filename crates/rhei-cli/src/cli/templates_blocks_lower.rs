    // Lowering derives the outer route, runtime data wiring, settings, private
    // files, and provenance in the ordinary output formats. §FS-rhei-library.5–6
    impl BlockCompiler {
        fn write_workspace(
            mut self,
            root_name: &str,
            ordered_nodes: &[CompiledNode],
            output: &Path,
        ) -> MietteResult<()> {
            fs::create_dir_all(output)
                .map_err(|err| file_io_report(output, "failed to create compiled block", err))?;
            let mut machine = merge_machines(root_name, &self.leaves, ordered_nodes, &self.seams)?;
            apply_state_file_passes(&mut machine, &self.passes)?;
            apply_task_export_passes(&mut self.leaves, &self.passes)?;
            if let Some(compatibility) = &self.compatibility {
                apply_compatibility_machine(&mut machine, compatibility)?;
                apply_compatibility_tasks(&mut self.leaves, compatibility);
            }

            let provenance = provenance_lines(root_name, &self.leaves);
            let machine_text = serde_yaml::to_string(&machine)
                .map_err(|err| miette!("failed to serialize composed state machine: {err}"))?;
            fs::write(output.join("states.yaml"), format!("{provenance}{machine_text}"))
                .map_err(|err| file_io_report(output, "failed to write composed states", err))?;

            let mut kinds = BTreeSet::from(["task".to_string()]);
            for leaf in &self.leaves {
                kinds.extend(leaf.custom_kinds.iter().cloned());
            }
            let index = format!(
                "{}# Rhei: {root_name}\n**States:** {root_name}\n\n---\nstructure:\n  maxLevels: 4\n  nodeKinds: [{}]\n---\n",
                provenance_markdown_lines(root_name, &self.leaves),
                kinds.into_iter().collect::<Vec<_>>().join(", ")
            );
            fs::write(output.join("index.rhei.md"), index)
                .map_err(|err| file_io_report(output, "failed to write composed index", err))?;
            let tasks_dir = output.join("tasks");
            fs::create_dir_all(&tasks_dir)
                .map_err(|err| file_io_report(&tasks_dir, "failed to create composed tasks", err))?;
            let mut task_index = 0usize;
            for leaf in &self.leaves {
                for (_, text) in &leaf.tasks {
                    task_index += 1;
                    let path =
                        tasks_dir.join(format!("{task_index:03}-{}.md", leaf.qualifier.prefix()));
                    fs::write(
                        &path,
                        format!(
                            "{}{text}",
                            provenance_markdown_lines(
                                root_name,
                                std::slice::from_ref(leaf)
                            )
                        ),
                    )
                    .map_err(|err| {
                        file_io_report(&path, "failed to write composed task", err)
                    })?;
                }
                copy_mounted_files(leaf, output)?;
            }
            write_merged_settings(&self.leaves, output, self.compatibility.as_ref())?;
            Ok(())
        }
    }

    fn merge_machines(
        root_name: &str,
        leaves: &[CompiledLeaf],
        ordered_nodes: &[CompiledNode],
        seams: &[ResolvedSeam],
    ) -> MietteResult<YamlValue> {
        let mut states = YamlMapping::new();
        let mut transitions = Vec::new();
        let mut profiles = YamlMapping::new();
        let mut models = Vec::new();
        let mut by_type = YamlMapping::new();
        let mut overrides = Vec::new();
        for leaf in leaves {
            let root = leaf.machine.as_mapping().expect("machine mapping");
            for (key, value) in yaml_map(&leaf.machine, "states")? {
                states.insert(key.clone(), value.clone());
            }
            transitions.extend(
                root.get(YamlValue::String("transitions".into()))
                    .and_then(YamlValue::as_sequence)
                    .cloned()
                    .unwrap_or_default(),
            );
            if let Some(local_profiles) = root
                .get(YamlValue::String("profiles".into()))
                .and_then(YamlValue::as_mapping)
            {
                for (key, value) in local_profiles {
                    profiles.insert(key.clone(), value.clone());
                }
            }
            if let Some(policy) = root
                .get(YamlValue::String("node_policy".into()))
                .and_then(YamlValue::as_mapping)
            {
                if let Some(local) = policy
                    .get(YamlValue::String("by_type".into()))
                    .and_then(YamlValue::as_mapping)
                {
                    for (key, value) in local {
                        by_type.insert(key.clone(), value.clone());
                    }
                }
                overrides.extend(
                    policy
                        .get(YamlValue::String("overrides".into()))
                        .and_then(YamlValue::as_sequence)
                        .cloned()
                        .unwrap_or_default(),
                );
            }
            models.extend(
                root.get(YamlValue::String("models".into()))
                    .and_then(YamlValue::as_sequence)
                    .cloned()
                    .unwrap_or_default(),
            );
        }
        for seam in seams {
            if let Some(state) = states
                .get_mut(YamlValue::String(seam.from.clone()))
                .and_then(YamlValue::as_mapping_mut)
            {
                state.insert(YamlValue::String("final".into()), YamlValue::Bool(false));
            }
            transitions.push(serde_yaml::to_value(serde_json::json!({
                "from": seam.from,
                "to": seam.to,
            }))
            .expect("seam YAML"));
        }
        let flow_states = ordered_nodes
            .iter()
            .flat_map(|node| node.primary_states.iter().cloned())
            .collect::<Vec<_>>();
        let initial = ordered_nodes
            .first()
            .map(|node| node.entry.clone())
            .ok_or_else(|| miette!("composition requires at least one mount"))?;
        profiles.insert(
            YamlValue::String("flow".into()),
            serde_yaml::to_value(serde_json::json!({ "initial": initial, "allowed": flow_states }))
                .expect("flow profile YAML"),
        );
        let node_policy = serde_yaml::to_value(serde_json::json!({
            "root": "flow",
            "default": "flow",
            "by_type": by_type,
            "overrides": overrides,
        }))
        .expect("node policy YAML");
        Ok(serde_yaml::to_value(serde_json::json!({
            "name": root_name,
            "version": 1,
            "models": models,
            "states": states,
            "transitions": transitions,
            "profiles": profiles,
            "node_policy": node_policy,
        }))
        .expect("composed machine YAML"))
    }

    fn apply_state_file_passes(machine: &mut YamlValue, passes: &[ResolvedPass]) -> MietteResult<()> {
        for pass in passes.iter().filter(|pass| pass.source.kind == DataKind::StateFile) {
            let source_state = pass.source.state.as_deref().expect("state endpoint");
            let target_state = pass.target.state.as_deref().expect("state endpoint");
            let states = yaml_map_mut(machine, "states")?;
            let source_path = artifact_path(states, source_state, "outputs", &pass.source.name)?;
            set_artifact_path(states, target_state, "inputs", &pass.target.name, &source_path)?;
        }
        Ok(())
    }

    fn artifact_path(
        states: &YamlMapping,
        state: &str,
        direction: &str,
        name: &str,
    ) -> MietteResult<String> {
        states
            .get(YamlValue::String(state.into()))
            .and_then(YamlValue::as_mapping)
            .and_then(|state| state.get(YamlValue::String(direction.into())))
            .and_then(YamlValue::as_sequence)
            .and_then(|items| {
                items.iter().find_map(|item| {
                    let item = item.as_mapping()?;
                    (item.get(YamlValue::String("name".into())).and_then(YamlValue::as_str)
                        == Some(name))
                    .then(|| {
                        item.get(YamlValue::String("path".into()))
                            .and_then(YamlValue::as_str)
                            .map(str::to_string)
                    })
                    .flatten()
                })
            })
            .ok_or_else(|| miette!("missing artifact '{}.{}' while lowering pass", state, name))
    }

    fn set_artifact_path(
        states: &mut YamlMapping,
        state: &str,
        direction: &str,
        name: &str,
        path: &str,
    ) -> MietteResult<()> {
        let items = states
            .get_mut(YamlValue::String(state.into()))
            .and_then(YamlValue::as_mapping_mut)
            .and_then(|state| state.get_mut(YamlValue::String(direction.into())))
            .and_then(YamlValue::as_sequence_mut)
            .ok_or_else(|| miette!("missing artifacts for state '{}'", state))?;
        for item in items {
            let Some(item) = item.as_mapping_mut() else { continue };
            if item.get(YamlValue::String("name".into())).and_then(YamlValue::as_str) == Some(name) {
                item.insert(YamlValue::String("path".into()), YamlValue::String(path.into()));
                return Ok(());
            }
        }
        Err(miette!("missing artifact '{}.{}' while lowering pass", state, name))
    }

    fn apply_task_export_passes(
        leaves: &mut [CompiledLeaf],
        passes: &[ResolvedPass],
    ) -> MietteResult<()> {
        for pass in passes.iter().filter(|pass| pass.source.kind == DataKind::TaskExport) {
            let producer = pass.source.task.as_deref().expect("task endpoint");
            let consumer = pass.target.task.as_deref().expect("task endpoint");
            let consumes = format!("{}:{}", producer, pass.source.name);
            let mut found = false;
            for leaf in leaves.iter_mut() {
                for (_, text) in &mut leaf.tasks {
                    if text.lines().any(|line| line.contains(&format!(" {consumer}:"))) {
                        *text = insert_task_consumes(text, consumer, &consumes);
                        found = true;
                    }
                }
            }
            if !found {
                return Err(miette!(
                    "pass '{}={}' could not find consumer task '{}'",
                    pass.source_label, pass.target_label, consumer
                ));
            }
        }
        Ok(())
    }

    fn insert_task_consumes(text: &str, task: &str, consumes: &str) -> String {
        let mut out = Vec::new();
        let mut in_task = false;
        let mut inserted = false;
        for line in text.lines() {
            if line.starts_with("###") && line.contains(&format!(" {task}:")) {
                in_task = true;
            } else if in_task && line.starts_with("###") {
                if !inserted {
                    out.push(format!("**Consumes:** {consumes}"));
                    inserted = true;
                }
                in_task = false;
            }
            if in_task && line.starts_with("**Consumes:**") {
                out.push(format!("{}, {consumes}", line.trim_end()));
                inserted = true;
                continue;
            }
            if in_task && line.is_empty() && !inserted {
                out.push(format!("**Consumes:** {consumes}"));
                inserted = true;
            }
            out.push(line.to_string());
        }
        if in_task && !inserted {
            out.push(format!("**Consumes:** {consumes}"));
        }
        let mut joined = out.join("\n");
        joined.push('\n');
        joined
    }

    fn provenance_lines(root: &str, leaves: &[CompiledLeaf]) -> String {
        let mut out = format!("# Generated by rhei block compiler v1; root: {root}\n");
        for leaf in leaves {
            out.push_str(&format!(
                "# block: {} {} source={} alias={}\n",
                leaf.name,
                leaf.version,
                leaf.source.display(),
                leaf.qualifier.chain().join(".")
            ));
        }
        out
    }

    fn provenance_markdown_lines(root: &str, leaves: &[CompiledLeaf]) -> String {
        let mut out = format!("<!-- Generated by rhei block compiler v1; root: {root} -->\n");
        for leaf in leaves {
            out.push_str(&format!(
                "<!-- block: {} {} source={} alias={} -->\n",
                leaf.name,
                leaf.version,
                leaf.source.display(),
                leaf.qualifier.chain().join(".")
            ));
        }
        out
    }

    fn copy_mounted_files(leaf: &CompiledLeaf, output: &Path) -> MietteResult<()> {
        let prompt_source = leaf.rendered.join("prompt_templates");
        if prompt_source.is_dir() {
            let prompt_output = output.join("prompt_templates");
            fs::create_dir_all(&prompt_output).map_err(|err| {
                file_io_report(&prompt_output, "failed to create prompt templates", err)
            })?;
            for entry in fs::read_dir(&prompt_source)
                .map_err(|err| file_io_report(&prompt_source, "failed to read prompts", err))?
            {
                let entry = entry.map_err(|err| miette!("failed to read prompt entry: {err}"))?;
                if !entry.path().is_file() {
                    continue;
                }
                let path = entry.path();
                let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else { continue };
                let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("md");
                let target = prompt_output.join(format!("{}.{}", leaf.qualifier.qualify(stem), extension));
                fs::copy(entry.path(), &target)
                    .map_err(|err| file_io_report(&target, "failed to copy mounted prompt", err))?;
            }
        }
        let private = output
            .join(".agent-grounds/rhei/blocks")
            .join(leaf.qualifier.prefix());
        copy_private_tree(&leaf.rendered, &leaf.rendered, &private)?;
        Ok(())
    }

    fn copy_private_tree(root: &Path, current: &Path, output: &Path) -> MietteResult<()> {
        let mut entries = fs::read_dir(current)
            .map_err(|err| file_io_report(current, "failed to read mounted private files", err))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| miette!("failed to read mounted private entry: {err}"))?;
        entries.sort_by_key(|entry| entry.path());
        for entry in entries {
            let path = entry.path();
            let relative = path.strip_prefix(root).expect("mounted relative path");
            let first = relative.components().next().and_then(|part| part.as_os_str().to_str());
            if matches!(
                first,
                Some("tasks" | "prompt_templates" | ".agent-grounds")
            ) || matches!(
                relative.to_str(),
                Some("index.rhei.md" | "plan.rhei.md" | "states.yaml")
            ) {
                continue;
            }
            let target = output.join(relative);
            if path.is_dir() {
                fs::create_dir_all(&target).map_err(|err| {
                    file_io_report(&target, "failed to create mounted private directory", err)
                })?;
                copy_private_tree(root, &path, output)?;
            } else {
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent).map_err(|err| {
                        file_io_report(parent, "failed to create mounted private directory", err)
                    })?;
                }
                fs::copy(&path, &target).map_err(|err| {
                    file_io_report(&target, "failed to copy mounted private file", err)
                })?;
            }
        }
        Ok(())
    }

    fn write_merged_settings(
        leaves: &[CompiledLeaf],
        output: &Path,
        compatibility: Option<&ResolvedCompatibility>,
    ) -> MietteResult<()> {
        let mut merged = serde_json::Map::new();
        for section in ["agents", "models", "mcp_servers", "skills"] {
            merged.insert(section.into(), serde_json::Value::Object(serde_json::Map::new()));
        }
        for leaf in leaves {
            let Some(settings) = leaf.settings.as_ref().and_then(serde_json::Value::as_object) else {
                continue;
            };
            let ownership = settings_ownership(leaf.settings.as_ref(), &leaf.qualifier);
            for section in ["agents", "models", "mcp_servers", "skills"] {
                let Some(entries) = settings.get(section).and_then(serde_json::Value::as_object) else {
                    continue;
                };
                let target = merged
                    .get_mut(section)
                    .and_then(serde_json::Value::as_object_mut)
                    .expect("merged settings section");
                for (id, value) in entries {
                    let mut value = value.clone();
                    if section == "models" {
                        qualify_model_settings(&mut value, &ownership);
                    }
                    target.insert(leaf.qualifier.qualify(id), value);
                }
            }
        }
        if let Some(compatibility) = compatibility {
            for section in ["agents", "models", "mcp_servers", "skills"] {
                let entries = merged
                    .get_mut(section)
                    .and_then(serde_json::Value::as_object_mut)
                    .expect("merged settings section");
                let original = std::mem::take(entries);
                for (id, mut value) in original {
                    let stable = compatibility.settings.get(&id).cloned().unwrap_or(id);
                    replace_json_settings(&mut value, &compatibility.settings);
                    entries.insert(stable, value);
                }
            }
        }
        if merged.values().all(|value| value.as_object().is_some_and(|map| map.is_empty())) {
            return Ok(());
        }
        let dir = output.join(".agent-grounds/rhei");
        fs::create_dir_all(&dir)
            .map_err(|err| file_io_report(&dir, "failed to create composed settings directory", err))?;
        let rendered = serde_json::to_string_pretty(&serde_json::Value::Object(merged))
            .map_err(|err| miette!("failed to serialize composed settings: {err}"))?;
        fs::write(dir.join("settings.json"), format!("{rendered}\n"))
            .map_err(|err| file_io_report(&dir, "failed to write composed settings", err))?;
        Ok(())
    }

    fn replace_json_settings(
        value: &mut serde_json::Value,
        replacements: &BTreeMap<String, String>,
    ) {
        match value {
            serde_json::Value::String(current) => {
                if let Some(stable) = replacements.get(current) {
                    *current = stable.clone();
                }
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    replace_json_settings(value, replacements);
                }
            }
            serde_json::Value::Object(values) => {
                for value in values.values_mut() {
                    replace_json_settings(value, replacements);
                }
            }
            _ => {}
        }
    }

    fn qualify_model_settings(value: &mut serde_json::Value, ownership: &SettingsOwnership) {
        let Some(model) = value.as_object_mut() else { return };
        if let Some(agent) = model.get_mut("default_agent") {
            if let Some(local) = agent.as_str().map(str::to_string) {
                if ownership.agents.contains(&local) {
                    *agent = serde_json::Value::String(ownership.qualifier.qualify(&local));
                }
            }
        }
        if let Some(agents) = model.get_mut("agents").and_then(serde_json::Value::as_object_mut) {
            let original = std::mem::take(agents);
            for (id, binding) in original {
                let id = if ownership.agents.contains(&id) {
                    ownership.qualifier.qualify(&id)
                } else {
                    id
                };
                agents.insert(id, binding);
            }
        }
    }
