    // Semantic state/settings qualification keeps owned names injective while
    // leaving external references and free text untouched. §FS-rhei-library.4
    fn qualified_local_control(
        local: &str,
        qualifier: &Qualifier,
        states: &YamlMapping,
        manifest: &TemplateManifest,
        dir: &Path,
    ) -> MietteResult<String> {
        if local.contains('.') {
            return Err(miette!("leaf block '{}' control port '{}' must name a local state", manifest.name, local));
        }
        let qualified = qualifier.qualify(local);
        if !states.contains_key(YamlValue::String(qualified.clone())) {
            return Err(miette!(
                help = format!("declare state '{}' in the rendered states.yaml", local),
                "block '{}' from '{}' exposes missing state '{}'",
                manifest.name, dir.join("template.yaml").display(), local
            ));
        }
        Ok(qualified)
    }

    fn resolve_group_control(
        endpoint: &str,
        entry: bool,
        children: &BTreeMap<String, CompiledNode>,
        block_name: &str,
        dir: &Path,
    ) -> MietteResult<String> {
        let (alias, port) = split_endpoint(endpoint).ok_or_else(|| {
            miette!(
                "composition-only block '{}' control endpoint '{}' must be `<child>.<port>` in '{}'",
                block_name, endpoint, dir.join("template.yaml").display()
            )
        })?;
        let child = children.get(alias).ok_or_else(|| {
            miette!("block '{}' control endpoint '{}' names unknown child '{}'", block_name, endpoint, alias)
        })?;
        if entry {
            if port != "entry" {
                return Err(miette!("block '{}' entry '{}' must re-export a child entry", block_name, endpoint));
            }
            Ok(child.entry.clone())
        } else {
            child.exits.get(port).cloned().ok_or_else(|| {
                miette!(
                    help = format!("public exits: {}", child.exits.keys().cloned().collect::<Vec<_>>().join(", ")),
                    "block '{}' exit re-exports unknown child endpoint '{}'",
                    block_name, endpoint
                )
            })
        }
    }

    fn compatible_input_schema(left: &TemplateValueSchema, right: &TemplateValueSchema) -> bool {
        left.value_type == right.value_type
            && left.validate == right.validate
            && left.format == right.format
            && match (left.items.as_deref(), right.items.as_deref()) {
                (Some(left), Some(right)) => compatible_input_schema(left, right),
                (None, None) => true,
                _ => false,
            }
            && left.properties.len() == right.properties.len()
            && left.properties.iter().all(|(name, left)| {
                right
                    .properties
                    .get(name)
                    .is_some_and(|right| compatible_input_schema(left, right))
            })
    }

    fn yaml_map<'a>(value: &'a YamlValue, key: &str) -> MietteResult<&'a YamlMapping> {
        value
            .as_mapping()
            .and_then(|map| map.get(YamlValue::String(key.to_string())))
            .and_then(YamlValue::as_mapping)
            .ok_or_else(|| miette!("state fragment is missing mapping '{}'", key))
    }

    fn yaml_map_mut<'a>(value: &'a mut YamlValue, key: &str) -> MietteResult<&'a mut YamlMapping> {
        value
            .as_mapping_mut()
            .and_then(|map| map.get_mut(YamlValue::String(key.to_string())))
            .and_then(YamlValue::as_mapping_mut)
            .ok_or_else(|| miette!("state fragment is missing mapping '{}'", key))
    }

    fn validate_local_machine_references(
        machine: &YamlValue,
        manifest: &TemplateManifest,
        source: &Path,
    ) -> MietteResult<()> {
        let states = yaml_map(machine, "states")?;
        let known = states.keys().filter_map(YamlValue::as_str).collect::<BTreeSet<_>>();
        if let Some(transitions) = machine
            .as_mapping()
            .and_then(|map| map.get(YamlValue::String("transitions".into())))
            .and_then(YamlValue::as_sequence)
        {
            for transition in transitions {
                let Some(map) = transition.as_mapping() else { continue };
                for field in ["from", "to"] {
                    let Some(state) = map
                        .get(YamlValue::String(field.into()))
                        .and_then(YamlValue::as_str)
                    else { continue };
                    if state != "*" && !known.contains(state) {
                        return Err(miette!(
                            help = "fix the owned state reference before mounting this block.",
                            "block '{}' mounted state reference '{}' in '{}' names missing state '{}'",
                            manifest.name, field, source.display(), state
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    #[derive(Default)]
    struct SettingsOwnership {
        agents: BTreeSet<String>,
        models: BTreeSet<String>,
        mcp_servers: BTreeSet<String>,
        skills: BTreeSet<String>,
        qualifier: Qualifier,
    }

    fn settings_ownership(
        settings: Option<&serde_json::Value>,
        qualifier: &Qualifier,
    ) -> SettingsOwnership {
        let ids = |section: &str| {
            settings
                .and_then(|value| value.get(section))
                .and_then(serde_json::Value::as_object)
                .map(|map| map.keys().cloned().collect())
                .unwrap_or_default()
        };
        SettingsOwnership {
            agents: ids("agents"),
            models: ids("models"),
            mcp_servers: ids("mcp_servers"),
            skills: ids("skills"),
            qualifier: qualifier.clone(),
        }
    }

    fn qualify_machine(
        mut machine: YamlValue,
        qualifier: &Qualifier,
        ownership: &SettingsOwnership,
        source: &Path,
    ) -> MietteResult<YamlValue> {
        let local_states = yaml_map(&machine, "states")?
            .keys()
            .filter_map(YamlValue::as_str)
            .map(str::to_string)
            .collect::<Vec<_>>();
        let original_states = yaml_map(&machine, "states")?.clone();
        let mut states = YamlMapping::new();
        for (name, mut definition) in original_states {
            let name = name.as_str().ok_or_else(|| miette!("state names in '{}' must be strings", source.display()))?;
            qualify_state_definition(&mut definition, qualifier, ownership)?;
            states.insert(YamlValue::String(qualifier.qualify(name)), definition);
        }
        *yaml_map_mut(&mut machine, "states")? = states;

        let root = machine.as_mapping_mut().expect("machine mapping");
        if let Some(models) = root
            .get_mut(YamlValue::String("models".into()))
            .and_then(YamlValue::as_sequence_mut)
        {
            for model in models {
                if let Some(local) = model.as_str().map(str::to_string) {
                    if ownership.models.contains(&local) {
                        *model = YamlValue::String(qualifier.qualify(&local));
                    }
                }
            }
        }

        let transitions_key = YamlValue::String("transitions".into());
        let original = root
            .get(&transitions_key)
            .and_then(YamlValue::as_sequence)
            .cloned()
            .unwrap_or_default();
        let mut transitions = Vec::new();
        for transition in original {
            let wildcard = transition
                .as_mapping()
                .and_then(|map| map.get(YamlValue::String("from".into())))
                .and_then(YamlValue::as_str)
                == Some("*");
            let sources = if wildcard { local_states.clone() } else { vec![String::new()] };
            for wildcard_source in sources {
                let mut transition = transition.clone();
                let map = transition.as_mapping_mut().expect("transition mapping");
                for field in ["from", "to"] {
                    let key = YamlValue::String(field.into());
                    if let Some(value) = map.get_mut(&key) {
                        if field == "from" && wildcard {
                            *value = YamlValue::String(qualifier.qualify(&wildcard_source));
                        } else if let Some(local) = value.as_str().map(str::to_string) {
                            if local != "*" {
                                *value = YamlValue::String(qualifier.qualify(&local));
                            }
                        }
                    }
                }
                transitions.push(transition);
            }
        }
        root.insert(transitions_key, YamlValue::Sequence(transitions));

        if let Some(original_profiles) = root
            .get(YamlValue::String("profiles".into()))
            .and_then(YamlValue::as_mapping)
            .cloned()
        {
            let mut profiles = YamlMapping::new();
            for (name, mut profile) in original_profiles {
                let local = name.as_str().expect("profile string");
                if let Some(map) = profile.as_mapping_mut() {
                    qualify_string_field(map, "initial", qualifier);
                    qualify_string_list(map, "allowed", qualifier);
                }
                profiles.insert(YamlValue::String(qualifier.qualify(local)), profile);
            }
            root.insert(YamlValue::String("profiles".into()), YamlValue::Mapping(profiles));
        }
        if let Some(policy) = root
            .get_mut(YamlValue::String("node_policy".into()))
            .and_then(YamlValue::as_mapping_mut)
        {
            qualify_string_field(policy, "root", qualifier);
            qualify_string_field(policy, "default", qualifier);
            if let Some(by_type) = policy
                .get(YamlValue::String("by_type".into()))
                .and_then(YamlValue::as_mapping)
                .cloned()
            {
                let mut qualified = YamlMapping::new();
                for (kind, profile) in by_type {
                    let kind = kind.as_str().expect("kind string");
                    let profile = profile.as_str().expect("profile string");
                    qualified.insert(
                        YamlValue::String(qualifier.qualify(kind)),
                        YamlValue::String(qualifier.qualify(profile)),
                    );
                }
                policy.insert(YamlValue::String("by_type".into()), YamlValue::Mapping(qualified));
            }
            if let Some(overrides) = policy
                .get_mut(YamlValue::String("overrides".into()))
                .and_then(YamlValue::as_sequence_mut)
            {
                for item in overrides {
                    if let Some(map) = item.as_mapping_mut() {
                        qualify_string_field(map, "profile", qualifier);
                        if let Some(selector) = map
                            .get_mut(YamlValue::String("match".into()))
                            .and_then(YamlValue::as_mapping_mut)
                        {
                            qualify_string_field(selector, "type", qualifier);
                        }
                    }
                }
            }
        }
        Ok(machine)
    }

    fn qualify_state_definition(
        definition: &mut YamlValue,
        qualifier: &Qualifier,
        ownership: &SettingsOwnership,
    ) -> MietteResult<()> {
        let Some(map) = definition.as_mapping_mut() else { return Ok(()) };
        if let Some(prompt) = map.get_mut(YamlValue::String("prompt_template".into())) {
            if let Some(name) = prompt.as_str().map(str::to_string) {
                *prompt = YamlValue::String(qualifier.qualify(&name));
            } else if let Some(prompt) = prompt.as_mapping_mut() {
                qualify_string_field(prompt, "name", qualifier);
            }
        }
        for key in ["inputs", "outputs"] {
            if let Some(artifacts) = map
                .get_mut(YamlValue::String(key.into()))
                .and_then(YamlValue::as_sequence_mut)
            {
                for artifact in artifacts {
                    let Some(artifact) = artifact.as_mapping_mut() else { continue };
                    let path_key = YamlValue::String("path".into());
                    let Some(path) = artifact.get(&path_key).and_then(YamlValue::as_str) else { continue };
                    let local = Path::new(path);
                    if local.is_absolute() || local.components().any(|part| matches!(part, std::path::Component::ParentDir)) {
                        return Err(miette!("mounted artifact path '{}' must be relative and contain no '..'", path));
                    }
                    artifact.insert(
                        path_key,
                        YamlValue::String(format!(
                            "runtime/blocks/{}/{}",
                            qualifier.prefix(),
                            path.replace('\\', "/")
                        )),
                    );
                }
            }
        }
        qualify_owned_scalar(map, "model", &ownership.models, qualifier);
        qualify_owned_scalar(map, "agent", &ownership.agents, qualifier);
        qualify_owned_list(map, "all_models", &ownership.models, qualifier);
        qualify_owned_tool_list(map, "mcp_servers", &ownership.mcp_servers, qualifier);
        qualify_owned_tool_list(map, "skills", &ownership.skills, qualifier);
        if let Some(program) = map
            .get_mut(YamlValue::String("program".into()))
            .and_then(YamlValue::as_mapping_mut)
        {
            let key = YamlValue::String("working_directory".into());
            if let Some(path) = program.get(&key).and_then(YamlValue::as_str) {
                let path = Path::new(path);
                if path.is_absolute()
                    || path.components().any(|part| {
                        matches!(part, std::path::Component::ParentDir)
                    })
                {
                    return Err(miette!(
                        "mounted program working_directory '{}' must be relative and contain no '..'",
                        path.display()
                    ));
                }
                program.insert(
                    key,
                    YamlValue::String(format!(
                        ".agent-grounds/rhei/blocks/{}/{}",
                        qualifier.prefix(),
                        path.display()
                    )),
                );
            }
        }
        for key in ["target", "all_targets"] {
            if let Some(value) = map.get_mut(YamlValue::String(key.into())) {
                qualify_execution_targets(value, ownership);
            }
        }
        Ok(())
    }

    fn qualify_string_field(map: &mut YamlMapping, key: &str, qualifier: &Qualifier) {
        let key = YamlValue::String(key.into());
        if let Some(value) = map.get_mut(&key) {
            if let Some(local) = value.as_str().map(str::to_string) {
                *value = YamlValue::String(qualifier.qualify(&local));
            }
        }
    }

    fn qualify_string_list(map: &mut YamlMapping, key: &str, qualifier: &Qualifier) {
        if let Some(values) = map
            .get_mut(YamlValue::String(key.into()))
            .and_then(YamlValue::as_sequence_mut)
        {
            for value in values {
                if let Some(local) = value.as_str().map(str::to_string) {
                    *value = YamlValue::String(qualifier.qualify(&local));
                }
            }
        }
    }

    fn qualify_owned_scalar(
        map: &mut YamlMapping,
        key: &str,
        owned: &BTreeSet<String>,
        qualifier: &Qualifier,
    ) {
        let key = YamlValue::String(key.into());
        if let Some(value) = map.get_mut(&key) {
            if let Some(local) = value.as_str().map(str::to_string) {
                if owned.contains(&local) {
                    *value = YamlValue::String(qualifier.qualify(&local));
                }
            }
        }
    }

    fn qualify_owned_list(
        map: &mut YamlMapping,
        key: &str,
        owned: &BTreeSet<String>,
        qualifier: &Qualifier,
    ) {
        if let Some(values) = map
            .get_mut(YamlValue::String(key.into()))
            .and_then(YamlValue::as_sequence_mut)
        {
            for value in values {
                if let Some(local) = value.as_str().map(str::to_string) {
                    if owned.contains(&local) {
                        *value = YamlValue::String(qualifier.qualify(&local));
                    }
                }
            }
        }
    }

    fn qualify_owned_tool_list(
        map: &mut YamlMapping,
        key: &str,
        owned: &BTreeSet<String>,
        qualifier: &Qualifier,
    ) {
        let Some(values) = map
            .get_mut(YamlValue::String(key.into()))
            .and_then(YamlValue::as_sequence_mut)
        else { return };
        for value in values {
            if let Some(local) = value.as_str().map(str::to_string) {
                if owned.contains(&local) {
                    *value = YamlValue::String(qualifier.qualify(&local));
                }
            } else if let Some(object) = value.as_mapping_mut() {
                qualify_owned_scalar(object, "id", owned, qualifier);
            }
        }
    }

    fn qualify_execution_targets(value: &mut YamlValue, ownership: &SettingsOwnership) {
        if let Some(selector) = value.as_str().map(str::to_string) {
            *value = YamlValue::String(qualify_execution_target(&selector, ownership));
        } else if let Some(values) = value.as_sequence_mut() {
            for selector in values {
                if let Some(text) = selector.as_str().map(str::to_string) {
                    *selector = YamlValue::String(qualify_execution_target(&text, ownership));
                }
            }
        }
    }

    fn qualify_execution_target(selector: &str, ownership: &SettingsOwnership) -> String {
        let Ok(mut target) = rhei_validator::parse_execution_target(selector) else {
            return selector.to_string();
        };
        if ownership.agents.contains(&target.agent) {
            target.agent = ownership.qualifier.qualify(&target.agent);
        }
        if ownership.models.contains(&target.model) {
            target.model = ownership.qualifier.qualify(&target.model);
        }
        target.selector()
    }
