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
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| miette!(
                help = program_log_help(),
                "failed to create log directory '{}': {e}", parent.display()
            ))?;
    }

    let log_file = fs::File::create(log_path)
        .map_err(|e| miette!(
            help = program_log_help(),
            "failed to create log file '{}': {e}", log_path.display()
        ))?;
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
    let supervised_label =
        format!("{}@{}", render_context.task.id, render_context.state_name);
    let mut supervised = match Supervised::spawn(&mut cmd, &supervised_label) {
        Ok(supervised) => supervised,
        // As for agents: the run was interrupted before this program started,
        // so it never ran and the ticket keeps its state.
        // §FS-rhei-run.3.2 §FS-rhei-agents.8
        Err(err) if spawn_was_interrupted(&err) => {
            use std::io::Write as _;
            let mut f = &log_file;
            let _ = writeln!(f, "\nprogram not started: the run was interrupted first");
            let _ = writeln!(f, "\n=== exit ===");
            let _ = writeln!(f, "code: -");
            let _ = writeln!(f, "interrupted: true");
            let _ = writeln!(f, "===");
            return Ok(ProgramSpawnOutcome {
                status: never_started_status(),
                timed_out: false,
                interrupted: true,
                timeout_secs: resolved.timeout_secs,
            });
        }
        Err(e) => {
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

    Ok(ProgramSpawnOutcome {
        status,
        timed_out,
        interrupted,
        timeout_secs: resolved.timeout_secs,
    })
}

fn transition_matches_exit_code(rule: &rhei_core::ast::TransitionRule, exit_code: i32) -> bool {
    match rule.exit_code.as_ref() {
        Some(YamlValue::Number(number)) => number.as_i64() == Some(i64::from(exit_code)),
        Some(YamlValue::Sequence(values)) => {
            values.iter().filter_map(YamlValue::as_i64).any(|value| value == i64::from(exit_code))
        }
        Some(YamlValue::String(value)) if value == "nonzero" => exit_code != 0,
        _ => false,
    }
}

fn transition_has_exact_exit_code(rule: &rhei_core::ast::TransitionRule) -> bool {
    matches!(rule.exit_code, Some(YamlValue::Number(_)) | Some(YamlValue::Sequence(_)))
}

fn transition_is_nonzero_exit_code(rule: &rhei_core::ast::TransitionRule) -> bool {
    matches!(rule.exit_code, Some(YamlValue::String(ref value)) if value == "nonzero")
}

fn program_transition_is_applicable(
    rule: &rhei_core::ast::TransitionRule,
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
    task: &rhei_core::ast::Task,
    current_state: &str,
) -> bool {
    transition_rule_is_applicable(
        rule,
        machine,
        metadata,
        &task.id,
        Some(task),
        current_state,
        task.state.as_str(),
    )
    .unwrap_or(false)
}

// Keyed strictly to `poll:` states, mutually exclusive with `visits`, so an
// exhausted supervising `visits` budget never widens into this carve-out
// (agent-grounds/rhei#122). §FS-rhei-run.5.1
fn poll_attempts_exhausted(
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
    task: &rhei_core::ast::Task,
    current_state: &str,
) -> bool {
    machine.states.get(current_state).and_then(|def| def.poll.as_ref()).is_some()
        && !loop_reentry_allowed(
            machine,
            metadata,
            &task.id,
            current_state,
            task.state.as_str(),
            current_state,
        )
}

/// How the edge a program's exit selected matched that exit.
///
/// A step-2 exact match is the program naming the route it wants, so the engine
/// records no subprocess-failure entry for it; a `"nonzero"` catch-all is the
/// program merely failing, and an edge carrying no `exit_code` at all — the
/// exit-`0` fall-through — is neither. A spent poll budget is its own answer:
/// the edge was selected regardless of what the program exited with, so the
/// exit named nothing whatever the rule declares. §FS-rhei-programs.3.2
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExitCodeMatch {
    /// An integer or integer-array `exit_code:` condition: a **declared route**.
    Exact,
    /// A `"nonzero"` catch-all condition.
    Nonzero,
    /// The selected rule declares no `exit_code:` at all.
    None,
    /// The state's poll attempt budget was spent, so the engine selected this
    /// edge regardless of its `exit_code:` — whatever it declares, the exit did
    /// not choose it. §FS-rhei-programs.3.2 §FS-rhei-run.5.1
    PollExhausted,
}

impl ExitCodeMatch {
    fn of(rule: &rhei_core::ast::TransitionRule) -> Self {
        if transition_has_exact_exit_code(rule) {
            Self::Exact
        } else if transition_is_nonzero_exit_code(rule) {
            Self::Nonzero
        } else {
            Self::None
        }
    }

    /// Whether this exit chose its own edge, and so owns the account of the
    /// outcome the engine would otherwise write. §FS-rhei-programs.3.2
    fn is_declared_route(self) -> bool {
        matches!(self, Self::Exact)
    }
}

/// The edge a program's exit selects, and how it matched.
///
/// The two travel together because only the selection can answer the second
/// question: it is the one place a rule's `condition:` is evaluated, so an
/// exact edge its condition disqualified is a rule the machine still declares
/// and the exit still did not take. Asking the machine again afterwards would
/// read that exit as a declared route and drop the engine's account of a
/// genuine failure. §FS-rhei-programs.3.2
#[derive(Debug, Clone)]
struct ProgramExitRoute {
    to: String,
    matched: ExitCodeMatch,
}

fn find_program_exit_transition(
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
    task: &rhei_core::ast::Task,
    current_state: &str,
    exit_code: i32,
) -> MietteResult<Option<ProgramExitRoute>> {
    let applicable_exact_match_exists = exit_code != 0
        && machine
            .transitions()
            .iter()
            .filter(|rule| rule.from.0 == current_state)
            .filter(|rule| transition_has_exact_exit_code(rule))
            .filter(|rule| transition_matches_exit_code(rule, exit_code))
            .any(|rule| {
                program_transition_is_applicable(rule, machine, metadata, task, current_state)
            });

    let poll_exhaustion_active =
        exit_code != 0 && poll_attempts_exhausted(machine, metadata, task, current_state);

    // A non-zero exit selects only an exit_code-declaring rule — except a
    // poll state's exhaustion edge, the spec'd non-self-loop match once
    // attempts are spent. §FS-rhei-programs.3.2 §FS-rhei-programs.3.3 §FS-rhei-run.5.1
    let ordered_match = machine
        .transitions()
        .iter()
        .filter(|rule| rule.from.0 == current_state)
        .filter(|rule| {
            if exit_code == 0 {
                rule.exit_code.is_none() || transition_matches_exit_code(rule, exit_code)
            } else if rule.exit_code.is_none() {
                poll_exhaustion_active && rule.to.0 != current_state
            } else {
                transition_matches_exit_code(rule, exit_code)
            }
        });

    for rule in ordered_match {
        if applicable_exact_match_exists && transition_is_nonzero_exit_code(rule) {
            continue;
        }
        if program_transition_is_applicable(rule, machine, metadata, task, current_state) {
            // Classified here, where the condition fired, not re-derived later.
            // An exhausted poll budget picks the edge on its own, so the exit
            // chose nothing and the engine still owes the account. §FS-rhei-programs.3.2
            let matched = if poll_exhaustion_active {
                ExitCodeMatch::PollExhausted
            } else {
                ExitCodeMatch::of(rule)
            };
            return Ok(Some(ProgramExitRoute { to: rule.to.0.clone(), matched }));
        }
    }

    Ok(None)
}
