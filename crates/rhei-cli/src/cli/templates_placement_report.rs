// Reporting an attempted placement whose validation bytes will be discarded.

/// Render validation reasons and per-error guidance without ordinary validate's
/// persisted-plan recovery. The caller supplies the placement context.
/// §FS-rhei-library.7.5
fn placement_validation_report(
    input: &Path,
    sources: &[ValidationMachineSource],
    errors: &[String],
    guidance: &[String],
) -> Report {
    miette!(
        help = guidance.join("\n"),
        "{}", render_validation_diagnostic(input, sources, errors)
    )
}

/// Validation and load failures of a discarded proposal keep their reasons and
/// correction advice, but retry the placement rather than the unchanged host.
/// Callers respell their mirror paths while those copies still exist.
/// §FS-rhei-library.7.5
fn discarded_placement_report(context: &str, entrypoint: &Path, report: Report) -> Report {
    let mut help = report.help().map(|help| help.to_string()).unwrap_or_default().replace(
        &plan_authoring_help(),
        "Check the proposed plan's task metadata (**State:**, **Prior:**, **Assignee:**).",
    );
    let retry = "the original rhei instantiate invocation with the same arguments";
    for command in [help_command_or("validate", entrypoint), help_command("validate")] {
        help = help.replace(&command, retry);
    }
    if !help.is_empty() {
        help.push('\n');
    }
    help.push_str(
        "Correct the attempted placement's inputs (the template, supplied values, or \
         target configuration as appropriate), then retry rhei instantiate with the \
         original arguments.",
    );
    miette!(help = help, "{context}\nNothing was written.\n\n{report}")
}

/// A discarded member has a proposed destination, not an existing repairable
/// output. Keep the real project and distinguish its proposed machine sources.
/// §FS-rhei-library.7.5
fn discarded_member_report(project: &Path, output: &Path, report: Report) -> Report {
    let context = format!(
        "Placement of proposed member '{}' into project '{}' was refused.\n\
         Files beneath the proposed member destination, including its state-machine \
         sources, have not been published.",
        crate::display_path(output),
        crate::display_path(project),
    );
    discarded_placement_report(&context, project, report)
}
