/// `interrupted` is set when the run was shutting down: the engine ended the
/// program's process group, so its exit status says nothing about the ticket
/// and **no transition may fire** for this invocation. §FS-rhei-run.3.2
#[derive(Debug, Clone)]
struct ProgramSpawnOutcome {
    status: std::process::ExitStatus,
    timed_out: bool,
    interrupted: bool,
    timeout_secs: Option<u64>,
}

impl InvocationOutcome for ProgramSpawnOutcome {
    fn was_interrupted(&self) -> bool {
        self.interrupted
    }

    fn timed_out(&self) -> bool {
        self.timed_out
    }

    fn status(&self) -> std::process::ExitStatus {
        self.status
    }
}

fn build_program_command(
    resolved: &ResolvedProgram,
    render_context: &RuntimeTemplateContext<'_>,
    // Which attempt of this state visit this run is. §FS-rhei-programs.2
    attempt: u64,
) -> MietteResult<std::process::Command> {
    let working_dir = resolved
        .program
        .working_directory
        .as_deref()
        .map(|path| resolve_runtime_template_text(path, render_context))
        .map(|path| render_context.workspace_root.join(path))
        .unwrap_or_else(|| render_context.workspace_root.to_path_buf());

    let mut cmd = if resolved.program.shell {
        let rendered = match &resolved.program.command {
            ProgramCommand::Shell(command) => {
                resolve_runtime_template_text(command, render_context)
            }
            ProgramCommand::Exec(args) => args
                .iter()
                .map(|arg| resolve_runtime_template_text(arg, render_context))
                .collect::<Vec<_>>()
                .join(" "),
        };
        // The platform's own shell: `sh -c` here, `cmd /c` on Windows, which is
        // what §FS-rhei-programs.1.1 promises a string-form command.
        rhei_core::platform::system_shell_command(&rendered)
    } else {
        let ProgramCommand::Exec(args) = &resolved.program.command else {
            return Err(miette!(
                help = state_machine_help(),
                "program command cannot disable shell when command is a string"
            ));
        };
        let rendered = args
            .iter()
            .map(|arg| resolve_runtime_template_text(arg, render_context))
            .collect::<Vec<_>>();
        let mut command = std::process::Command::new(&rendered[0]);
        for arg in &rendered[1..] {
            command.arg(arg);
        }
        command
    };

    // `cmd.exe` refuses a verbatim working directory. §REQ-cross-platform.5
    cmd.current_dir(rhei_core::platform::plain_path(working_dir))
        .env("RHEI_PLAN_PATH", render_context.plan_path)
        .env("RHEI_TASK_ID", render_context.task.id.to_string())
        .env("RHEI_TASK_ID_LOCAL", rhei_local_id_of(render_context.task))
        // Always set, for every state: a program has no prompt to read the path
        // from, and a program whose exit finishes the ticket must leave this
        // file non-empty. §FS-rhei-programs.2 §FS-rhei-states.3.3
        .env(
            "RHEI_RESULT_PATH",
            absolute_result_file_path(
                render_context.workspace_root,
                &render_context.task.id.to_string(),
            ),
        )
        .env("RHEI_STATE", render_context.state_name)
        .env(
            "RHEI_VISIT_COUNT",
            render_visit_count(
                render_context.metadata,
                &render_context.task.id,
                render_context.state_name,
                render_context.current_state_raw,
                render_context.machine,
            )
            .to_string(),
        )
        // A program has no prompt, so the environment is the only place it can
        // be told it is a retry. §FS-rhei-programs.2
        .env("RHEI_ATTEMPT", attempt.to_string());
    if let Some(path) = render_context.state_machine_path {
        cmd.env("RHEI_STATE_MACHINE_PATH", path);
    }
    if let Some(model) = render_context.model {
        cmd.env("RHEI_MODEL", model);
    }
    // A measuring state's invocation is handed the iteration its success
    // would confirm, so a new workspace can name its artifacts from Rhei's
    // counter instead of keeping a second one. §FS-rhei-metrics.2
    if let Some(iteration) = next_metric_iteration(
        render_context.workspace_root,
        render_context.machine,
        render_context.state_name,
    ) {
        cmd.env("RHEI_ITERATION", iteration.to_string());
    }

    // Expose declared input artifact paths and existence flags so programs
    // can branch on optional inputs without shelling out to test -f.
    let input_visit_count = Some(render_visit_count(
        render_context.metadata,
        &render_context.task.id,
        render_context.state_name,
        render_context.current_state_raw,
        render_context.machine,
    ));
    if let Some(state_def) = render_context.machine.states.get(render_context.state_name) {
        for artifact in &state_def.inputs {
            let env_base = artifact.name.to_uppercase().replace(['-', ' '], "_");
            let (relative, path) = resolve_artifact_path(
                render_context.workspace_root,
                artifact,
                &render_context.task.id.to_string(),
                render_context.state_name,
                input_visit_count,
                render_context.target,
                render_context.model,
                render_context.model_provider,
                render_context.model_name,
                render_context.agent,
                render_context.agent_mode,
            );
            if artifact_relative_path_escapes_root(&relative) {
                return Err(miette!(
                    help = "artifact paths are workspace-relative. Remove the leading '/' or the '..' segments from this artifact's path in the state machine.",
                    "input artifact '{}' expands to '{}' which escapes the workspace root",
                    artifact.name,
                    relative
                ));
            }
            cmd.env(format!("RHEI_INPUT_{env_base}_EXISTS"), path.exists().to_string());
            cmd.env(format!("RHEI_INPUT_{env_base}_PATH"), relative);
        }
    }

    for (key, value) in &resolved.program.env {
        cmd.env(key, resolve_runtime_template_text(value, render_context));
    }

    Ok(cmd)
}

fn spawn_and_wait_program(
    resolved: &ResolvedProgram,
    render_context: &RuntimeTemplateContext<'_>,
    log_path: &Path,
    // Which attempt of which visit this is, and where the record of it goes
    // once the command has actually run. §FS-rhei-agents.8.4
    plan: &SpawnPlan,
    // Only to carry the shutdown notice: a program's own output goes to its
    // log, not the journal. §FS-rhei-run.3.2
    sink: &Arc<dyn rhei_tui::EventSink>,
) -> MietteResult<ProgramSpawnOutcome> {
    // Prepare only the canonical parent; leave result content and env overrides
    // to the program. A preparation error prevents spawn. §FS-rhei-programs.2
    let result_path = absolute_result_file_path(
        render_context.workspace_root,
        &render_context.task.id.to_string(),
    );
    if let Some(parent) = result_path.parent() {
        fs::create_dir_all(parent).map_err(|e| miette!(
            help = "program results are written under runtime/results/. Check that directory is writable.",
            "failed to create result directory '{}': {e}", parent.display()
        ))?;
    }

    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| miette!(
                help = program_log_help(),
                "failed to create log directory '{}': {e}", parent.display()
            ))?;
    }

    // Never truncated, as an agent's is not, and removed if the command never
    // starts. §FS-rhei-programs.5.1 §FS-rhei-agents.8.1
    let mut unstarted = UnstartedLog::new(log_path);
    let task_id = render_context.task.id.to_string();
    let log_file = create_log_exclusively(
        log_path, &task_id, plan.reset.as_ref(), program_log_help(),
    )?;
    unstarted.created();
    let command_label = match &resolved.program.command {
        ProgramCommand::Shell(command) => resolve_runtime_template_text(command, render_context),
        ProgramCommand::Exec(args) => args
            .iter()
            .map(|arg| resolve_runtime_template_text(arg, render_context))
            .collect::<Vec<_>>()
            .join(" "),
    };
    let started_wall = std::time::SystemTime::now();
    {
        use std::io::Write as _;
        let mut f = &log_file;
        let _ = writeln!(f, "=== rhei program log v1 ===");
        let _ = writeln!(f, "program: {command_label}");
        let _ = writeln!(f, "task: {}", render_context.task.id);
        let _ = writeln!(f, "state: {}", render_context.state_name);
        if let Some(timeout) = resolved.timeout_secs {
            let _ = writeln!(f, "timeout: {timeout}s");
        }
        let _ = writeln!(f, "plan: {}", render_context.plan_path.display());
        let _ = writeln!(f, "===\n");
    }

    let log_stdout =
        log_file.try_clone().map_err(|e| miette!(
            help = program_log_help(),
            "failed to clone log file handle: {e}"
        ))?;
    let log_stderr =
        log_file.try_clone().map_err(|e| miette!(
            help = program_log_help(),
            "failed to clone log file handle: {e}"
        ))?;
    let mut cmd = build_program_command(resolved, render_context, plan.attempt)?;
    cmd.stdout(log_stdout).stderr(log_stderr);
    // A program is never handed the operator's terminal, and it leads its own
    // process group so its children go with it. §FS-rhei-run.3.2
    cmd.stdin(std::process::Stdio::null());
    let mut evidence = begin_program_evidence(resolved, &cmd, render_context, plan, log_path)?;
    let supervised_label =
        format!("{}@{}", render_context.task.id, render_context.state_name);
    let mut supervised = match Supervised::spawn(&mut cmd, &supervised_label) {
        Ok(supervised) => {
            unstarted.started();
            supervised
        }
        // As for agents: the run was interrupted before this program started,
        // so it never ran, the ticket keeps its state, and the log goes with
        // `unstarted`. §FS-rhei-run.3.2 §FS-rhei-agents.8.1
        Err(err) if spawn_was_interrupted(&err) => {
            evidence.take().into_iter().for_each(EvidenceInvocation::abandon);
            return Ok(ProgramSpawnOutcome {
                status: never_started_status(),
                timed_out: false,
                interrupted: true,
                timeout_secs: resolved.timeout_secs,
            });
        }
        Err(e) => {
            evidence.take().into_iter().for_each(EvidenceInvocation::abandon);
            return Err(miette!(
                help = "the program state could not start its command. Check the command exists and is executable, then re-run.",
                "failed to spawn program: {e}"
            ))
        }
    };
    let start = Instant::now();

    // One wait for all three endings: exit, deadline, run interruption.
    // §FS-rhei-run.3.2
    let ended = supervised
        .wait(
            resolved.timeout_secs.map(Duration::from_secs),
            &INTERRUPT,
            &notify_through_sink(sink),
        )
        .map_err(|e| miette!(
            help = internal_error_help(),
            "error waiting for program: {e}"
        ))?;
    let status = ended.status;
    let timed_out = ended.cause == EndCause::TimedOut;
    let interrupted = ended.cause == EndCause::Interrupted;

    let elapsed = start.elapsed().as_secs();
    // The command ran. Recorded beside the footer, for the same reason an
    // agent's spawn is: a log alone cannot prove it. §FS-rhei-agents.8.4
    plan.record_spawn(SpawnEnding {
        task_id: &render_context.task.id.to_string(),
        state_name: render_context.state_name,
        kind: "program",
        worker: &command_label,
        started: &format_iso8601_utc(started_wall),
        ended: &format_iso8601_utc(std::time::SystemTime::now()),
        duration: &format!("{elapsed}s"),
        code: status.code(),
        ending: if timed_out {
            "timed out"
        } else if interrupted {
            "interrupted"
        } else {
            "exited"
        },
    });
    {
        use std::io::Write as _;
        let mut f = fs::OpenOptions::new()
            .append(true)
            .open(log_path)
            .map_err(|e| miette!(
                help = program_log_help(),
                "failed to append to log file: {e}"
            ))?;
        // §FS-rhei-agents.8: the program footer carries the same two
        // early-ending lines the agent footer does.
        if timed_out {
            if let Some(timeout_secs) = resolved.timeout_secs {
                let _ = writeln!(
                    f,
                    "\nprogram timed out after {}",
                    format_duration_human(timeout_secs)
                );
            }
        } else if interrupted {
            let _ = writeln!(
                f,
                "\nprogram interrupted by run shutdown after {}",
                format_duration_human(elapsed)
            );
        }
        let _ = writeln!(f, "\n=== exit ===");
        let _ = writeln!(f, "code: {}", status.code().unwrap_or(-1));
        let _ = writeln!(f, "duration: {elapsed}s");
        if timed_out {
            let _ = writeln!(f, "timed_out: true");
        }
        if interrupted {
            let _ = writeln!(f, "interrupted: true");
        }
        let _ = writeln!(f, "===");
    }
    let ending = if timed_out { "timed out" } else if interrupted { "interrupted" } else { "exited" };
    evidence.into_iter().for_each(|e| e.finish(status.code(), ending, Some(log_path), None));

    Ok(ProgramSpawnOutcome {
        status,
        timed_out,
        interrupted,
        timeout_secs: resolved.timeout_secs,
    })
}
