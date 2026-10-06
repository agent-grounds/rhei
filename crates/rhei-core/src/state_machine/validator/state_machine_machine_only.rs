// The settings key a state machine may never carry, refused on the raw YAML.
//
// Its own part because the refusal is about *who* may write a key rather than
// about what a machine means: `defaults.clamp_projects` belongs to the paying
// machine's settings file, and a plan that carried it would be a template
// opting its users out of their own ceiling.

// §AR-source-file-size.3 §FS-rhei-agents.1.1.1

/// The machine settings file, as the CLI's settings loader opens it, which is
/// where the refusal sends its reader. §FS-rhei-budgets.2.1
fn machine_settings_file_name() -> String {
    match std::env::var_os("HOME") {
        Some(home) => PathBuf::from(home).join(".config/rhei/settings.json").display().to_string(),
        None => "~/.config/rhei/settings.json".to_string(),
    }
}

/// Every place in a state machine `clamp_projects` is written, as the dotted
/// field an author would search for: at the root, in the root `defaults`, and
/// on or anywhere under a profile. Checked on the raw document because no
/// deserializer here refuses an unknown key, so the typed machine would drop it
/// in silence. §FS-rhei-agents.1.1.1 §FS-rhei-budgets.2.1
fn clamp_projects_fields(raw: &serde_yaml::Value) -> Vec<String> {
    const KEY: &str = "clamp_projects";
    fn under(value: &serde_yaml::Value, path: &str, found: &mut Vec<String>) {
        let Some(map) = value.as_mapping() else { return };
        for (key, child) in map {
            let Some(key) = key.as_str() else { continue };
            let field = format!("{path}.{key}");
            if key == KEY {
                found.push(field.clone());
            }
            under(child, &field, found);
        }
    }
    let mut found = Vec::new();
    if raw.get(KEY).is_some() {
        found.push(KEY.to_string());
    }
    if raw.get("defaults").and_then(|defaults| defaults.get(KEY)).is_some() {
        found.push(format!("defaults.{KEY}"));
    }
    if let Some(profiles) = raw.get("profiles") {
        under(profiles, "profiles", &mut found);
    }
    found
}

/// Refuse `clamp_projects` wherever a state machine carries it, whatever its
/// value — `true` and `null` included — naming the key, the file and field it
/// was found in, and the machine settings file where it is permitted.
/// §FS-rhei-agents.1.1.1 §FS-rhei-budgets.2.1
fn reject_clamp_projects(
    raw: &serde_yaml::Value,
    source: Option<&Path>,
) -> Result<(), StateMachineLoadError> {
    let Some(field) = clamp_projects_fields(raw).into_iter().next() else { return Ok(()) };
    let file = source
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "the state machine".to_string());
    Err(StateMachineLoadError::Invalid(format!(
        "`defaults.clamp_projects` may only be set in the machine settings file\n       \
         found in:  {file} ({field})\n       \
         set it in: {}",
        machine_settings_file_name()
    )))
}
