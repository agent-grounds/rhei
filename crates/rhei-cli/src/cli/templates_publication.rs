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
/// the staging name part of the authored graph, or of what a refusal names.
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
    let respell = |report: Report| respell_staging(report, staged, output);
    let loaded =
        load_project_with_member_for_validation(project, intended_id, staged).map_err(respell)?;
    let pass = validation_pass_for_loaded(
        project,
        None,
        loaded,
        staged_settings.then_some(staged),
    )
    .map_err(respell)?;
    if !pass.errors.is_empty() {
        return Err(respell(validation_report(
            project,
            &pass.state_machine_sources,
            &pass.errors,
            &pass.help,
        )));
    }
    print_validation_report(&pass.warnings);
    Ok(())
}

/// Attribute a project parse refusal to an existing sibling when its source
/// lies outside the staged member and the project manifest. Preserve the
/// parser's source context, replace its generic remedy with the owning entry,
/// and make no claim about validation that the interrupted load did not finish.
/// §FS-rhei-templates.6.1.2 §FS-rhei-errors.4
pub(super) fn project_member_parse_report(
    err: &rhei_core::parser::ParseError,
    project: &Path,
    staged: &Path,
) -> Report {
    let report = nested_parse_report(err);
    let Some(sibling) = err.file.as_deref().filter(|path| {
        path.starts_with(project)
            && !path.starts_with(staged)
            && *path != project.join(workspace::PANTA_INDEX_FILE)
    }) else {
        return report;
    };
    // A task fragment is inspected here but validated through its workspace; a basin
    // fragment has no index of its own, so through the project. §FS-rhei-templates.6.1.2
    let owner = sibling.strip_prefix(project).ok()
        .and_then(|relative| relative.components().next())
        .and_then(|component| if component.as_os_str() == workspace::BASIN_RHEI_ID {
            Some(project.to_path_buf())
        } else {
            workspace::workspace_dir(&project.join(component))
        });
    let repair_target = crate::display_path(owner.as_deref().unwrap_or(sibling));
    let sibling = crate::display_path(sibling);
    miette!(
        help = format!("repair the existing sibling, then re-run: {}",
            shell_command(["rhei", "validate", repair_target.as_str()])),
        "an existing sibling plan at '{}' blocks project validation of the instantiated output.\n\n{}",
        sibling,
        report
    )
}

/// A report that names a hidden staging directory, respelled to name the path
/// it is published at. The two are siblings and the staging name is unique to
/// this process, so swapping that one segment respells the path however the
/// report spelled it — relative, absolute or resolved. §FS-rhei-templates.6.1.2
fn respell_staging(report: Report, staged: &Path, output: &Path) -> Report {
    let name = |path: &Path| path.file_name().and_then(|name| name.to_str()).map(str::to_owned);
    match (name(staged), name(output)) {
        (Some(hidden), Some(published)) if hidden != published => {
            respell_report(report, &[(hidden, published)])
        }
        _ => report,
    }
}

/// Each spelling of a copy a message may use — as written, as resolution
/// canonicalized it, and as a diagnostic shortens it against the working
/// directory — paired with the path it stands for. A copy already removed is
/// resolved through its parent.
///
/// Longest first: a canonical spelling can hold the written one, as macOS's
/// `/private/var/…` holds `/var/…` and Windows' `\\?\C:\…` holds `C:\…`, and
/// replacing the shorter first would leave the rest of the longer in front of
/// the path it was respelled to.
fn copy_respellings(copy: &Path, original: &Path) -> Vec<(String, String)> {
    let shown = |path: &Path| path.display().to_string();
    let resolved = fs::canonicalize(copy).ok().or_else(|| {
        let parent = fs::canonicalize(copy.parent()?).ok()?;
        Some(parent.join(copy.file_name()?))
    });
    let mut respellings = vec![(shown(copy), shown(original))];
    if let Some(resolved) = resolved {
        respellings.push((shown(&resolved), shown(original)));
    }
    respellings.push((crate::display_path(copy), crate::display_path(original)));
    respellings.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    respellings.dedup_by(|a, b| a.0 == b.0);
    respellings
}

/// `report` with each `(from, to)` replaced in its message and its help, or
/// `report` itself when it names none of them. Every report a template command
/// builds is text and help, so nothing else is lost in the rebuild; one with
/// no help gains the plainest next action. §FS-rhei-errors.6
fn respell_report(report: Report, respellings: &[(String, String)]) -> Report {
    let respell = |text: String| {
        respellings.iter().fold(text, |text, (from, to)| text.replace(from.as_str(), to))
    };
    let text = report.to_string();
    let help = report.help().map(|help| help.to_string());
    let named = |text: &str| respellings.iter().any(|(from, _)| text.contains(from.as_str()));
    if !named(&text) && !help.as_deref().is_some_and(named) {
        return report;
    }
    let help = help.map_or_else(|| "fix what it names, then run this again.".to_owned(), respell);
    miette!(help = help, "{}", respell(text))
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
