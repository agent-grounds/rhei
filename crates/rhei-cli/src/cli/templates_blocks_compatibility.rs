    // Compatibility is a checked root rename after ownership qualification;
    // mounted wrappers are qualified normally by their parent. §FS-rhei-library.7
    /// Resolve root compatibility targets while the child public surfaces are
    /// still available. The stored maps run compiled identity -> stable root
    /// identity, which makes lowering a checked rename. §FS-rhei-library.7
    fn resolve_compatibility(
        authored: &rhei_core::blocks::CompatibilityMap,
        children: &BTreeMap<String, CompiledNode>,
        qualifier: &Qualifier,
        manifest: &Path,
    ) -> MietteResult<ResolvedCompatibility> {
        let resolve_id = |target: &str| -> MietteResult<String> {
            let (alias, local) = split_endpoint(target).ok_or_else(|| {
                miette!("compatibility target '{}' in '{}' must be `<alias>.<local>`", target, manifest.display())
            })?;
            if !children.contains_key(alias) {
                return Err(miette!("compatibility target '{}' names unknown child '{}' in '{}'", target, alias, manifest.display()));
            }
            Ok(qualifier.child(alias).qualify(local))
        };
        let invert = |map: &BTreeMap<String, String>| -> MietteResult<BTreeMap<String, String>> {
            map.iter()
                .map(|(stable, target)| resolve_id(target).map(|target| (target, stable.clone())))
                .collect()
        };
        let mut artifacts = Vec::new();
        for (stable, target) in &authored.artifacts {
            let (alias, endpoint) = split_endpoint(target).ok_or_else(|| {
                miette!("compatibility artifact target '{}' in '{}' must be `<alias>.<endpoint>`", target, manifest.display())
            })?;
            let child = children.get(alias).ok_or_else(|| {
                miette!("compatibility artifact target '{}' names unknown child '{}'", target, alias)
            })?;
            let endpoint = child
                .outputs
                .get(endpoint)
                .or_else(|| child.inputs.get(endpoint))
                .ok_or_else(|| {
                    miette!(
                        help = format!(
                            "public data endpoints: {}",
                            child
                                .inputs
                                .keys()
                                .chain(child.outputs.keys())
                                .cloned()
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        "compatibility artifact target '{}' is not declared by '{}'",
                        target,
                        child.manifest.display()
                    )
                })?;
            if endpoint.kind != DataKind::StateFile {
                return Err(miette!("compatibility artifact target '{}' is {}, not state-file", target, endpoint.kind));
            }
            artifacts.push((stable.clone(), endpoint.clone()));
        }
        Ok(ResolvedCompatibility {
            states: invert(&authored.states)?,
            tasks: invert(&authored.tasks)?,
            profiles: invert(&authored.profiles)?,
            settings: invert(&authored.settings)?,
            artifacts,
        })
    }

    fn replace_compatible(value: &mut YamlValue, replacements: &BTreeMap<String, String>) {
        if let Some(current) = value.as_str() {
            if let Some(stable) = replacements.get(current) {
                *value = YamlValue::String(stable.clone());
            }
        }
    }

    fn rename_yaml_keys(map: &mut YamlMapping, replacements: &BTreeMap<String, String>) {
        let original = std::mem::take(map);
        for (key, value) in original {
            let key = key
                .as_str()
                .and_then(|key| replacements.get(key))
                .map(|stable| YamlValue::String(stable.clone()))
                .unwrap_or(key);
            map.insert(key, value);
        }
    }

    fn apply_compatibility_machine(
        machine: &mut YamlValue,
        compatibility: &ResolvedCompatibility,
    ) -> MietteResult<()> {
        let mut artifact_paths = BTreeMap::new();
        {
            let states = yaml_map(machine, "states")?;
            for (stable, endpoint) in &compatibility.artifacts {
                let state = endpoint.state.as_deref().expect("state-file endpoint");
                let direction = if states
                    .get(YamlValue::String(state.into()))
                    .and_then(YamlValue::as_mapping)
                    .and_then(|state| state.get(YamlValue::String("outputs".into())))
                    .is_some()
                {
                    "outputs"
                } else {
                    "inputs"
                };
                let path = artifact_path(states, state, direction, &endpoint.name)?;
                artifact_paths.insert(path, stable.clone());
            }
        }
        let root = machine.as_mapping_mut().expect("machine mapping");
        let states = root
            .get_mut(YamlValue::String("states".into()))
            .and_then(YamlValue::as_mapping_mut)
            .expect("states mapping");
        for definition in states.values_mut() {
            let Some(definition) = definition.as_mapping_mut() else { continue };
            for direction in ["inputs", "outputs"] {
                if let Some(artifacts) = definition
                    .get_mut(YamlValue::String(direction.into()))
                    .and_then(YamlValue::as_sequence_mut)
                {
                    for artifact in artifacts {
                        let Some(artifact) = artifact.as_mapping_mut() else { continue };
                        if let Some(path) = artifact.get_mut(YamlValue::String("path".into())) {
                            replace_compatible(path, &artifact_paths);
                        }
                    }
                }
            }
            for field in ["agent", "model"] {
                if let Some(value) = definition.get_mut(YamlValue::String(field.into())) {
                    replace_compatible(value, &compatibility.settings);
                }
            }
            for field in ["target", "all_targets"] {
                if let Some(value) = definition.get_mut(YamlValue::String(field.into())) {
                    replace_target_settings(value, &compatibility.settings);
                }
            }
        }
        rename_yaml_keys(states, &compatibility.states);

        if let Some(transitions) = root
            .get_mut(YamlValue::String("transitions".into()))
            .and_then(YamlValue::as_sequence_mut)
        {
            for transition in transitions {
                let Some(transition) = transition.as_mapping_mut() else { continue };
                for field in ["from", "to"] {
                    if let Some(value) = transition.get_mut(YamlValue::String(field.into())) {
                        replace_compatible(value, &compatibility.states);
                    }
                }
            }
        }
        if let Some(profiles) = root
            .get_mut(YamlValue::String("profiles".into()))
            .and_then(YamlValue::as_mapping_mut)
        {
            for profile in profiles.values_mut() {
                let Some(profile) = profile.as_mapping_mut() else { continue };
                if let Some(initial) = profile.get_mut(YamlValue::String("initial".into())) {
                    replace_compatible(initial, &compatibility.states);
                }
                if let Some(allowed) = profile
                    .get_mut(YamlValue::String("allowed".into()))
                    .and_then(YamlValue::as_sequence_mut)
                {
                    for state in allowed {
                        replace_compatible(state, &compatibility.states);
                    }
                }
            }
            rename_yaml_keys(profiles, &compatibility.profiles);
        }
        if let Some(policy) = root
            .get_mut(YamlValue::String("node_policy".into()))
            .and_then(YamlValue::as_mapping_mut)
        {
            for field in ["root", "default"] {
                if let Some(profile) = policy.get_mut(YamlValue::String(field.into())) {
                    replace_compatible(profile, &compatibility.profiles);
                }
            }
            if let Some(by_type) = policy
                .get_mut(YamlValue::String("by_type".into()))
                .and_then(YamlValue::as_mapping_mut)
            {
                for profile in by_type.values_mut() {
                    replace_compatible(profile, &compatibility.profiles);
                }
            }
        }
        Ok(())
    }

    fn replace_target_settings(value: &mut YamlValue, replacements: &BTreeMap<String, String>) {
        if let Some(selector) = value.as_str().map(str::to_string) {
            *value = YamlValue::String(replace_target_selector(&selector, replacements));
        } else if let Some(selectors) = value.as_sequence_mut() {
            for selector in selectors {
                if let Some(text) = selector.as_str().map(str::to_string) {
                    *selector = YamlValue::String(replace_target_selector(&text, replacements));
                }
            }
        }
    }

    fn replace_target_selector(selector: &str, replacements: &BTreeMap<String, String>) -> String {
        let Ok(mut target) = rhei_validator::parse_execution_target(selector) else {
            return replacements.get(selector).cloned().unwrap_or_else(|| selector.into());
        };
        if let Some(stable) = replacements.get(&target.agent) {
            target.agent = stable.clone();
        }
        if let Some(stable) = replacements.get(&target.model) {
            target.model = stable.clone();
        }
        target.selector()
    }

    fn apply_compatibility_tasks(
        leaves: &mut [CompiledLeaf],
        compatibility: &ResolvedCompatibility,
    ) {
        let heading = Regex::new(
            r"^(#{3,6}\s+[A-Za-z][A-Za-z0-9_-]*\s+)([A-Za-z0-9_-]+(?:\.[A-Za-z0-9_-]+)*)(:.*)$",
        )
        .expect("qualified task heading");
        for leaf in leaves {
            for (_, text) in &mut leaf.tasks {
                let mut rewritten = String::new();
                for line in text.lines() {
                    let line = if let Some(captures) = heading.captures(line) {
                        let id = captures.get(2).expect("id").as_str();
                        format!(
                            "{}{}{}",
                            captures.get(1).expect("prefix").as_str(),
                            compatibility.tasks.get(id).map(String::as_str).unwrap_or(id),
                            captures.get(3).expect("suffix").as_str()
                        )
                    } else {
                        compatibility_task_metadata(line, compatibility)
                    };
                    rewritten.push_str(&line);
                    rewritten.push('\n');
                }
                *text = rewritten;
            }
        }
    }

    fn compatibility_task_metadata(
        line: &str,
        compatibility: &ResolvedCompatibility,
    ) -> String {
        let Some((field, value)) = line.strip_prefix("**").and_then(|line| line.split_once(":**")) else {
            return line.to_string();
        };
        let value = value.trim();
        match field {
            "State" => format!(
                "**State:** {}",
                compatibility.states.get(value).map(String::as_str).unwrap_or(value)
            ),
            "Prior" => format!(
                "**Prior:** {}",
                value
                    .split(',')
                    .map(|item| {
                        let item = item.trim();
                        let (kind, id) = item.split_once(' ').unwrap_or(("", item));
                        let stable = compatibility.tasks.get(id).map(String::as_str).unwrap_or(id);
                        if kind.is_empty() { stable.to_string() } else { format!("{kind} {stable}") }
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            "Consumes" => format!(
                "**Consumes:** {}",
                value
                    .split(',')
                    .map(|item| {
                        let item = item.trim();
                        item.split_once(':')
                            .map(|(task, export)| {
                                format!("{}:{export}", compatibility.tasks.get(task).map(String::as_str).unwrap_or(task))
                            })
                            .unwrap_or_else(|| item.to_string())
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            "Model" => format!(
                "**Model:** {}",
                compatibility.settings.get(value).map(String::as_str).unwrap_or(value)
            ),
            "Target" => format!("**Target:** {}", replace_target_selector(value, &compatibility.settings)),
            _ => line.to_string(),
        }
    }
