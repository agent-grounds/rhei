// The manifest stays static; only these three groups may be selected.
// §FS-rhei-library.1.1
fn select_block_declarations(
    manifest: &TemplateManifest,
    values: &BTreeMap<String, serde_json::Value>,
    dir: &Path,
    reference: &str,
) -> MietteResult<TemplateManifest> {
    let Some(source) = &manifest.select else { return Ok(manifest.clone()); };
    let path = dir.join("template.yaml");
    let selected = (|| {
        let text = render_template_text(source, values, &path, reference)?;
        let raw: YamlValue = serde_yaml::from_str(&text)
            .map_err(|e| miette!(help = "fix the YAML rendered by select", "invalid selected YAML: {e}"))?;
        selected_keys(&raw, "select", &["ports", "data", "compatibility", "expose"])?;
        let mut result = manifest.clone();
        result.select = None;
        let source_manifest: YamlValue = serde_yaml::from_str(
            &fs::read_to_string(&path)
                .map_err(|error| file_io_report(&path, "read selected block manifest", error))?,
        )
        .map_err(|error| miette!("invalid template manifest: {error}"))?;
        for (key, value) in raw.as_mapping().expect("checked mapping") {
            let group = key.as_str().expect("checked key");
            if manifest.static_declarations.contains(group)
                || (group == "expose" && source_manifest.get("expose").is_some())
            {
                return Err(miette!(help = "remove the duplicate static or selected declaration group", "'{group}' is both static and selected; author the group in only one place"));
            }
            match group {
                "ports" => {
                    selected_keys(value, "ports", &["entry", "exits"])?;
                    result.block.ports = Some(serde_yaml::from_value(value.clone()).map_err(|e| miette!(help = "fix the selected ports declaration", "ports: {e}"))?);
                }
                "data" => {
                    selected_keys(value, "data", &["inputs", "outputs"])?;
                    for direction in ["inputs", "outputs"] {
                        if let Some(endpoints) = value.get(direction) {
                            let endpoints = endpoints.as_mapping().ok_or_else(|| miette!(help = "declare selected data endpoints as a mapping", "data.{direction} must be a mapping"))?;
                            for (name, endpoint) in endpoints {
                                selected_keys(endpoint, &format!("data.{direction}.{name:?}"), &["kind", "state", "task", "name"])?;
                            }
                        }
                    }
                    result.block.data = serde_yaml::from_value(value.clone()).map_err(|e| miette!(help = "fix the selected data declaration", "data: {e}"))?;
                }
                "compatibility" => {
                    selected_keys(value, "compatibility", &["states", "terminals", "tasks", "profiles", "settings", "artifacts"])?;
                    result.block.compatibility = serde_yaml::from_value(value.clone()).map_err(|e| miette!(help = "fix the selected compatibility declaration", "compatibility: {e}"))?;
                }
                "expose" => {
                    validate_selected_exposure(value)?;
                    result.block.expose = serde_yaml::from_value(value.clone()).map_err(|e| miette!(help = "fix the selected typed exposure declaration", "expose: {e}"))?;
                }
                _ => unreachable!("checked group"),
            }
        }
        let ident = Regex::new(r"^[A-Za-z][A-Za-z0-9_-]*$").unwrap();
        validate_block_manifest(&result, dir, &ident)?;
        Ok(result)
    })();
    selected.map_err(|e: Report| e.wrap_err(format!("{} select: correct the selected ports, data, compatibility, or expose declarations", path.display())))
}

/// Selection renders the whole closed exposure group and receives the same
/// field validation as a static declaration. §FS-rhei-library.1.1–1.2
fn validate_selected_exposure(value: &YamlValue) -> MietteResult<()> {
    selected_keys(value, "expose", &["states", "tasks", "settings"])?;
    for kind in ["states", "tasks"] {
        if let Some(declarations) = value.get(kind) {
            validate_selected_exposure_map(declarations, &format!("expose.{kind}"))?;
        }
    }
    if let Some(settings) = value.get("settings") {
        selected_keys(
            settings,
            "expose.settings",
            &["agents", "models", "mcp_servers", "skills"],
        )?;
        for registry in ["agents", "models", "mcp_servers", "skills"] {
            if let Some(declarations) = settings.get(registry) {
                validate_selected_exposure_map(
                    declarations,
                    &format!("expose.settings.{registry}"),
                )?;
            }
        }
    }
    Ok(())
}

fn validate_selected_exposure_map(value: &YamlValue, group: &str) -> MietteResult<()> {
    let declarations = value.as_mapping().ok_or_else(|| {
        miette!(help = "render exposure declarations as a mapping", "{group} must be a mapping")
    })?;
    for (public, target) in declarations {
        selected_keys(target, &format!("{group}.{public:?}"), &["local", "mount", "name"])?;
    }
    Ok(())
}

fn selected_keys(value: &YamlValue, group: &str, allowed: &[&str]) -> MietteResult<()> {
    let mapping = value.as_mapping().ok_or_else(|| miette!(help = "render a mapping for the selected declaration group", "{group} must select a mapping, not null or a scalar"))?;
    for key in mapping.keys() {
        if !key.as_str().is_some_and(|key| allowed.contains(&key)) {
            return Err(miette!(help = "remove the unsupported field or use one of the listed fields", "unsupported field {key:?} in {group}; allowed fields: {}", allowed.join(", ")));
        }
    }
    Ok(())
}
