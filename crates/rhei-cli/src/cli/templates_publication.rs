// Hidden staging, prospective project validation, and the final publication
// boundary for a template that becomes a Panta member.

/// Choose an unused hidden sibling of the requested output. The final rename
/// therefore stays within one filesystem. §FS-rhei-templates.6.1.2
fn hidden_staging_path(output: &Path) -> MietteResult<PathBuf> {
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    let name = output
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| miette!("output path '{}' has no usable final name", output.display()))?;
    for sequence in 0..1024u32 {
        let candidate =
            parent.join(format!(".rhei-instantiate-{name}-{}-{sequence}", std::process::id()));
        if !candidate.exists() && candidate.symlink_metadata().is_err() {
            return Ok(candidate);
        }
    }
    Err(miette!(
        help = "remove stale hidden .rhei-instantiate-* directories beside the output and retry",
        "could not allocate a hidden staging directory beside '{}'",
        output.display()
    ))
}

/// Validate hidden bytes as the final project-qualified member without making
/// the staging name part of the authored graph.
// §FS-rhei-templates.6.1.2 §FS-rhei-templates.6.2
fn validate_staged_project_member(
    project: &Path,
    output: &Path,
    staged: &Path,
    staged_settings: bool,
) -> MietteResult<()> {
    let intended_id = output
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| miette!("output path '{}' has no UTF-8 member id", output.display()))?;
    let loaded = load_project_with_member_for_validation(project, intended_id, staged)?;
    let pass = validation_pass_for_loaded(
        project,
        None,
        loaded,
        staged_settings.then_some(staged),
    )?;
    if !pass.errors.is_empty() {
        return Err(validation_report(
            project,
            &pass.state_machine_sources,
            &pass.errors,
            &pass.help,
        ));
    }
    print_validation_report(&pass.warnings);
    Ok(())
}
