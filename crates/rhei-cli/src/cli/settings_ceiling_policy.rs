// The one machine-only settings key, `defaults.clamp_projects`, and the files
// a bound's sentences name.
//
// Its own part because the key is unlike every other setting: it is not merged
// at all. The machine file alone is read for it, and every other place it could
// be written refuses it by its presence, on the raw document, before a merge or
// a deserializer can drop it unseen.

// §AR-source-file-size.3 §FS-rhei-budgets.2 §FS-rhei-budgets.2.1 §FS-rhei-agents.1.1.1

/// What the machine decided about the two count ceilings, and the files every
/// bound's phrase, row and remedy names by the paths rhei read.
/// §FS-rhei-budgets.2 §FS-rhei-budgets.8
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct CountCeilingPolicy {
    /// `true` exactly when the machine settings file sets
    /// `defaults.clamp_projects: false`: each count bound the project declares
    /// then becomes the ceiling in place of the machine's. Omitting the key
    /// leaves this `false`, and the machine's ceiling stands. §FS-rhei-budgets.2
    delegated: bool,
    /// The machine file as the loader opens it and the project file the
    /// loader chose. §FS-rhei-budgets.8
    files: rhei_core::budget::SettingsFiles,
}

/// The machine settings file as the loader opens it: `$HOME` joined with the
/// fixed relative path, or `None` where there is no home to join.
/// §FS-rhei-budgets.8
fn machine_settings_file() -> Option<PathBuf> {
    home_dir().ok().map(|home| home.join(".config/rhei/settings.json"))
}

/// A path as a message prints it: absolute, so a reader can open it from
/// anywhere, without resolving links the loader did not resolve.
/// §FS-rhei-budgets.8
fn settings_file_as_read(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Read `defaults.clamp_projects` from the machine document, the one place it
/// is lawful, as a strict boolean: a string, a number or `null` is a settings
/// error naming the key. Omission means `true`. Returns whether the machine
/// delegated, i.e. whether the key is `false`. §FS-rhei-agents.1.1.1
fn machine_delegates_count_ceilings(raw: &serde_json::Value, path: &Path) -> MietteResult<bool> {
    match json_child(raw, "defaults").get("clamp_projects") {
        None => Ok(false),
        Some(serde_json::Value::Bool(clamp)) => Ok(!clamp),
        Some(other) => Err(miette!(
            help = "Write `\"clamp_projects\": false` to delegate the two count ceilings to \
                    the project, or remove the key to keep the machine's.",
            "`defaults.clamp_projects` in '{}' must be a boolean; got {other}",
            path.display()
        )),
    }
}

/// Refuse `clamp_projects` in a project settings document by its presence,
/// whatever its value, `true` and `null` included: a repository cannot opt
/// itself out of the ceiling of the machine that runs it.
/// §FS-rhei-agents.1.1.1 §FS-rhei-budgets.2.1
fn refuse_clamp_projects_in_project(
    raw: &serde_json::Value,
    found_in: &Path,
    machine: Option<&Path>,
) -> MietteResult<()> {
    let present = json_nested_field_present(raw, "defaults", "clamp_projects")
        || json_field_present(raw, "clamp_projects");
    if !present {
        return Ok(());
    }
    let machine = machine
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "~/.config/rhei/settings.json".to_string());
    Err(miette!(
        help = "Only the machine that pays may delegate its count ceilings; remove the key \
                from the project settings file.",
        "`defaults.clamp_projects` may only be set in the machine settings file\n       \
         found in:  {}\n       \
         set it in: {machine}",
        settings_file_as_read(found_in).display()
    ))
}
