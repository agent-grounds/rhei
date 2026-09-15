// Diagnostics for plan paths whose filesystem shape is real but not a Rhei
// input. Kept separate from generic filesystem guidance because a missing
// manifest is an authoring-shape failure, not an I/O failure.

/// Diagnose a directory that has neither plan manifest, with a direct
/// correction when `rhei run DIRECTORY --rhei ID` named its child workspace.
/// §FS-rhei-errors.3.2
fn unrecognized_plan_directory_report(
    path: &Path,
    run: Option<RunPlanInput<'_>>,
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
    let help = run
        .and_then(|run| {
            (run.options.rhei_scope().len() == 1)
                .then(|| (run, run.options.rhei_scope()[0].trim()))
        })
        .filter(|(_, id)| !id.is_empty())
        .and_then(|(run, id)| {
            let child = path.join(id);
            let is_selected_workspace = workspace::is_workspace(&child)
                && workspace::rhei_id_for_path(&child).is_ok_and(|child_id| child_id == id);
            is_selected_workspace.then(|| {
                let command = corrected_run_command(&child, run);
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

#[derive(Clone, Copy)]
struct RunPlanInput<'a> {
    options: &'a RunOptions,
    state_machine: Option<&'a Path>,
}

/// Rebuild a parsed run invocation with only its plan path corrected.
/// Every emitted word is quoted by `shell_command`. §FS-rhei-errors.1.2
fn corrected_run_command(path: &Path, run: RunPlanInput<'_>) -> String {
    let standalone = &run.options.standalone;
    let agent = &run.options.agent;
    let program = &run.options.program;
    let snapshot = &run.options.snapshot;
    let mut arguments = vec!["rhei".to_string(), "run".to_string(), path.display().to_string()];

    push_path_option(&mut arguments, "--state-machine", run.state_machine);
    push_flag(&mut arguments, "--dry-run", standalone.dry_run);
    push_flag(&mut arguments, "--no-callbacks", standalone.no_callbacks);
    push_flag(&mut arguments, "--continue-on-error", standalone.continue_on_error);
    if standalone.parallel != 1 {
        push_option(&mut arguments, "--parallel", standalone.parallel.to_string());
    }
    push_path_option(&mut arguments, "--prices", standalone.prices.as_deref());
    for rhei in &standalone.rhei {
        push_option(&mut arguments, "--rhei", rhei.clone());
    }
    push_flag(&mut arguments, "--tui", standalone.tui);
    push_flag(&mut arguments, "--no-tui", standalone.no_tui);
    push_flag(
        &mut arguments,
        "--json",
        standalone.json || headless_launcher_flag_was_supplied("--json"),
    );
    push_flag(
        &mut arguments,
        "--json-agent-output",
        standalone.json_agent_output
            || headless_launcher_flag_was_supplied("--json-agent-output"),
    );
    push_flag(
        &mut arguments,
        "--headless",
        standalone.headless || headless_launcher_flag_was_supplied("--headless"),
    );
    push_flag(&mut arguments, "--dashboard", standalone.dashboard);
    push_flag(&mut arguments, "--no-dashboard", standalone.no_dashboard);

    push_flag(&mut arguments, "--no-agent", agent.no_agent);
    push_string_option(&mut arguments, "--agent", agent.agent.as_deref());
    push_string_option(&mut arguments, "--agent-mode", agent.agent_mode.as_deref());
    push_string_option(&mut arguments, "--model", agent.model.as_deref());
    push_flag(&mut arguments, "--no-program", program.no_program);
    push_string_option(&mut arguments, "--program-timeout", program.program_timeout.as_deref());
    push_string_option(&mut arguments, "--from-snapshot", snapshot.from_snapshot.as_deref());
    push_flag(&mut arguments, "--override-inherit", snapshot.override_inherit);
    push_string_option(&mut arguments, "--task", snapshot.snapshot_task.as_deref());
    push_string_option(&mut arguments, "--target", snapshot.snapshot_target.as_deref());

    shell_command(arguments)
}

fn push_flag(arguments: &mut Vec<String>, flag: &str, enabled: bool) {
    if enabled {
        arguments.push(flag.to_string());
    }
}

fn push_option(arguments: &mut Vec<String>, flag: &str, value: String) {
    // Keep leading hyphens in a value from becoming CLI options on paste. §FS-rhei-errors.1.2
    if value.starts_with('-') {
        arguments.push(format!("{flag}={value}"));
    } else {
        arguments.push(flag.to_string());
        arguments.push(value);
    }
}

fn push_string_option(arguments: &mut Vec<String>, flag: &str, value: Option<&str>) {
    if let Some(value) = value {
        push_option(arguments, flag, value.to_string());
    }
}

fn push_path_option(arguments: &mut Vec<String>, flag: &str, value: Option<&Path>) {
    if let Some(value) = value {
        push_option(arguments, flag, value.display().to_string());
    }
}
