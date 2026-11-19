/// Render ownership consistently for effective and displaced sources.
/// §FS-rhei-validate.6.1
impl ValidationMachineSource {
    fn ownership_entry(&self) -> String {
        let mut ownership = Vec::new();
        if self.project_default {
            ownership.push("project default".to_string());
        }
        if !self.rheis.is_empty() {
            ownership.push(format!("rhei: {}", self.rheis.join(", ")));
        }
        format!(
            "  - {} ({})",
            state_machine_label(self.path.as_deref()),
            ownership.join("; "),
        )
    }
}

/// Compare source locations across relative, absolute and symlink spellings,
/// without making provenance a new load/refusal boundary. §FS-rhei-validate.6.1
fn validation_source_identity(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// Explain the automatic sources displaced by a whole-project override.
/// Only locations are inspected: an override still bypasses malformed YAML.
/// §FS-rhei-validate.6.1
fn validation_override_context(
    input: &Path,
    loaded: &LoadedPlan,
    state_machine: Option<&Path>,
) -> Option<String> {
    let selected = state_machine.filter(|_| loaded.is_panta_project())?;
    let selected_identity = validation_source_identity(selected);
    let default = automatic_default_machine_path(input);
    let default_identity = default.as_deref().map(validation_source_identity);
    let mut sources = vec![ValidationMachineSource {
        path: default,
        project_default: true,
        rheis: Vec::new(),
    }];
    let mut identities = vec![default_identity.clone()];
    let mut rhei_ids = loaded.rhei_ids.clone();
    rhei_ids.sort();
    for rhei_id in rhei_ids {
        let local = loaded
            .rhei_roots
            .get(&rhei_id)
            .and_then(|root| automatic_rhei_machine_path(&rhei_id, root));
        let identity = local
            .as_deref()
            .map(validation_source_identity)
            .or_else(|| default_identity.clone());
        if identity.as_ref() == Some(&selected_identity) {
            continue;
        }
        if let Some(index) = identities.iter().position(|source| *source == identity) {
            sources[index].rheis.push(rhei_id);
        } else {
            identities.push(identity);
            sources.push(ValidationMachineSource {
                path: local,
                project_default: false,
                rheis: vec![rhei_id],
            });
        }
    }

    // An unused default has no owning rheis to report as displaced. §FS-rhei-validate.6.1
    sources.retain(|source| !source.rheis.is_empty());
    let mut lines = vec![format!(
        "State-machine override '{}' applies to every rhei in the project.",
        selected.display(),
    )];
    if sources.is_empty() {
        lines.push("No automatic sources displaced.".to_string());
    } else {
        lines.push("Automatic sources displaced:".to_string());
        lines.extend(sources.iter().map(ValidationMachineSource::ownership_entry));
    }
    Some(lines.join("\n"))
}
