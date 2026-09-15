// Diagnostics for plan paths whose filesystem shape is real but not a Rhei
// input. Kept separate from generic filesystem guidance because a missing
// manifest is an authoring-shape failure, not an I/O failure.

/// Diagnose a directory that has neither plan manifest, with a direct
/// correction when `rhei run DIRECTORY --rhei ID` named its child workspace.
/// §FS-rhei-errors.3.2
fn unrecognized_plan_directory_report(
    path: &Path,
    run_rhei_scope: Option<&[String]>,
) -> MietteResult<Report> {
    for manifest in [workspace::PANTA_INDEX_FILE, workspace::RHEI_INDEX_FILE] {
        let manifest = path.join(manifest);
        match fs::metadata(&manifest) {
            Ok(_) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                return Err(file_io_report(&manifest, "failed to inspect plan manifest", err));
            }
        }
    }

    let shape_help = format!(
        "add `{}` to make this directory a Panta Project, or add `{}` instead to make it a \
         Directory Workspace. Otherwise, pass the actual plan or workspace path.",
        workspace::PANTA_INDEX_FILE,
        workspace::RHEI_INDEX_FILE
    );
    let help = run_rhei_scope
        .and_then(|scope| (scope.len() == 1).then(|| scope[0].trim()))
        .filter(|id| !id.is_empty())
        .and_then(|id| {
            let child = path.join(id);
            let is_selected_workspace = workspace::is_workspace(&child)
                && workspace::rhei_id_for_path(&child).is_ok_and(|child_id| child_id == id);
            is_selected_workspace.then(|| {
                let child_arg = child.display().to_string();
                let command = shell_command(["rhei", "run", &child_arg, "--rhei", id]);
                format!(
                    "{shape_help} As invoked, '{}' is one level above the selected Directory \
                     Workspace '{}'. Run that workspace directly with: {command}",
                    path.display(),
                    child.display()
                )
            })
        })
        .unwrap_or(shape_help);

    Ok(miette!(
        help = help,
        "'{}' is not a recognized Panta Project or Directory Workspace",
        path.display()
    ))
}
