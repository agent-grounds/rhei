// Hidden staging, prospective project validation, and the final publication
// boundary for a template that becomes a Panta member.

/// Choose an unused hidden sibling of the requested output. The final rename
/// therefore stays within one filesystem. §FS-rhei-templates.6.1.2
fn hidden_staging_path(output: &Path) -> MietteResult<PathBuf> {
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    let name = output
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| miette!(
            help = "choose --output with a new, named directory beneath the project",
            "output path '{}' has no usable final name", output.display()
        ))?;
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
        .ok_or_else(|| miette!(
            help = "choose --output with a UTF-8 directory name; that name becomes the member id",
            "output path '{}' has no UTF-8 member id", output.display()
        ))?;
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

/// Commit settings and publish once, restoring the project on failure. A
/// competing output always belongs to its creator, including with retention.
/// §FS-rhei-templates.6.1.2 §FS-rhei-templates.6.2
fn publish_staged_member(
    staged: &Path,
    output: &Path,
    settings: Option<&PreparedProjectSettings>,
    keep_on_error: bool,
) -> MietteResult<()> {
    let publication = (|| {
        if let Some(settings) = settings {
            settings.commit()?;
        }
        rename_member_noreplace(staged, output)
            .map_err(|err| file_io_report(output, "failed to publish instantiated member", err))
    })();
    if let Err(err) = publication {
        if let Some(settings) = settings {
            settings.undo();
        }
        let retention = if keep_on_error {
            if let Some(settings) = settings {
                settings.restore_staged()?;
            }
            format!("rendered output is retained at '{}' for inspection", staged.display())
        } else {
            let _ = remove_path(staged, false);
            "staged output was discarded".to_string()
        };
        let message = if keep_on_error {
            format!(
                "failed to publish instantiated member at '{}': {err}; rendered output is retained at:\n{}",
                output.display(),
                staged.display()
            )
        } else {
            format!("failed to publish instantiated member at '{}': {err}", output.display())
        };
        return Err(miette!(
            help = format!(
                "{retention}. Choose a free --output path and retry on a filesystem supporting \
                 atomic no-replace directory rename; an existing destination is never replaced."
            ),
            "{}", message
        ));
    }
    Ok(())
}
