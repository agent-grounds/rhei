/// Retain syntax before ticket/path resolution consumes the parsed invocation.
/// §FS-rhei-errors.1.2
enum OperatorInvocation {
    Force { positional_ticket: bool },
    Recover(PathBuf),
}

impl OperatorInvocation {
    /// Only attended commands need a complete human retry. §FS-rhei-errors.1.2
    fn of(command: &Commands) -> Option<Self> {
        match command {
            Commands::Transition { force: true, task, .. } =>
                Some(Self::Force { positional_ticket: task.is_none() }),
            Commands::Recover { execution_root } => Some(Self::Recover(execution_root.clone())),
            _ => None,
        }
    }
}

/// One handoff for early contention, non-terminal input and the typing race.
/// §FS-rhei-errors.1.2 §FS-rhei-transition-cmd.6 §FS-rhei-recover.3
struct OperatorHandoff {
    operation: &'static str,
    expected: String,
    task: String,
    retry: String,
    target: String,
}

impl OperatorHandoff {
    /// Preserve the selector form, supplied/discovered target and all recovery inputs.
    /// §FS-rhei-errors.1.2 §FS-rhei-transition-cmd.6
    fn force(request: &ForcedRequest<'_>, task_id: &str) -> Self {
        let positional = current_help_target().is_some_and(|target|
            matches!(target.operator, Some(OperatorInvocation::Force { positional_ticket: true })));
        let target = target_word(Some(request.input));
        let selector = if positional { shell_quote(request.task) } else { target.clone() };
        let mut retry = format!("rhei transition --from={} --to={} --force --reason={}",
            shell_quote(request.from), shell_quote(request.to), shell_quote(request.reason));
        if !positional { retry.push_str(&format!(" --task={}", shell_quote(request.task))); }
        if let Some(result) = request.result {
            retry.push_str(&format!(" --result={}", shell_quote(result)));
        }
        let machine = current_help_target().and_then(|target|
            target.machine.as_deref().map(|path| target.spelled(path)))
            .or_else(|| request.machine_path.map(shell_word));
        if let Some(machine) = machine { retry.push_str(&format!(" --state-machine={machine}")); }
        // Equals binds option values; the terminator protects option-like paths/tickets.
        // §FS-rhei-errors.1.2
        retry.push_str(&format!(" -- {selector}"));
        Self { operation: "--force", expected: format!("force {task_id} {} -> {}", request.from, request.to),
            task: task_id.into(), retry, target }
    }

    /// Marker-derived confirmation never requires loading the in-doubt plan.
    /// §FS-rhei-recover.3 §FS-rhei-errors.1.2
    fn recovery(root: &Path, marker: &ForcedMarker, decision: ForcedDecision) -> Self {
        let target = current_help_target().and_then(|target| match &target.operator {
            Some(OperatorInvocation::Recover(path)) => Some(shell_word(path)),
            _ => None,
        }).unwrap_or_else(|| shell_word(root));
        Self { operation: "recover", expected: format!("recover {} {} {} -> {} {}",
            marker.recovery_id, marker.hop.task_id, marker.hop.from, marker.hop.to, decision.word()),
            task: marker.hop.task_id.clone(), retry: format!("rhei recover -- {target}"), target }
    }

    /// Keep the terminal headline even when the same handoff also carries contention.
    /// §FS-rhei-errors.1.2
    fn refusal(&self, root: Option<&Path>, confirmed: bool) -> miette::Report {
        use std::io::IsTerminal;
        let blocker = root.map(|root| operator_blocker(root, Some(&self.task)));
        let directory = std::env::current_dir().map(|path| shell_word(&path))
            .unwrap_or_else(|_| "the original invocation directory (unavailable)".into());
        let help = format!("{}Hand this to a human operator at an interactive terminal. An agent may not run the operator ceremony.\nInvocation directory: {directory}\nResolved target: {}\n{OPERATOR_RETRY_LEAD_IN}{}\nType exactly: {}",
            blocker.as_ref().map(|blocker| format!("{}\n{}\n", blocker.summary, blocker.guidance)).unwrap_or_default(),
            self.target, self.retry, self.expected);
        if !confirmed && !std::io::stdin().is_terminal() {
            miette!(help = help, "{} requires an interactive terminal and fresh typed confirmation", self.operation)
        } else {
            miette!(help = help, "{}", blocker.expect("confirmed or terminal contention root").summary)
        }
    }
}

/// Observation retains no exclusion and treats missing/uncertain evidence as no new gate.
/// §FS-rhei-transition-cmd.6 §FS-rhei-recover.3
fn operator_preflight(roots: &[PathBuf], handoff: &OperatorHandoff) -> MietteResult<()> {
    for root in roots {
        if matches!(probe_run_lock(root), RunLockProbe::Held) {
            return Err(handoff.refusal(Some(root), false));
        }
    }
    Ok(())
}

/// Both observation and acquisition describe contention through this evidence reader.
/// Unverified bytes never become a run identity or a process-control instruction.
/// §FS-rhei-errors.1.2 §FS-rhei-transition-cmd.6 §FS-rhei-recover.3
fn operator_blocker(root: &Path, task: Option<&str>) -> OperatorBlocker {
    if let Some(run) = verified_operator_run(root) {
        let scope = match live_budget_scope(root, &run.id) {
            Some(scope) => {
                let selection = scope.as_ref().map(|names| names.iter().cloned().collect::<Vec<_>>().join(", "))
                    .unwrap_or_else(|| "whole project".into());
                let membership = task.map(|task| format!("; {task} is {}",
                    if task_in_rhei_scope(&scope, task) { "in scope" } else { "outside that scope" }))
                    .unwrap_or_default();
                format!("schedules {selection}{membership}")
            }
            None => "has unknown scheduling scope; ticket membership is unknown".into(),
        };
        return OperatorBlocker {
            summary: format!("Run {} holds the run lock at {}", run.id, root.display()),
            guidance: format!("Run {} {scope}. Its run lock excludes the whole project regardless of scheduling scope. Wait for it to finish, or {}.",
                run.id, live_run_stop_advice(root, Some(&run), true)),
        };
    }
    // Operator ownership has its own schema; never reinterpret its pid as a run.
    // §FS-rhei-recover.3
    let operator = read_run_lock_owner(&root.join(".rhei/run.lock")).ok()
        .and_then(|body| serde_json::from_str::<serde_json::Value>(&body).ok())
        .and_then(|value| (value["operation"] == "forced-recovery")
            .then(|| value["operator"].as_str().map(str::to_owned)).flatten());
    match operator {
        Some(operator) => OperatorBlocker {
            summary: format!("Another operator ({operator}) holds the run lock at {} for forced recovery", root.display()),
            guidance: "The lock excludes the whole project; wait for that operator to finish.".into(),
        },
        None => OperatorBlocker {
            summary: format!("The run lock at {} is held; the holder's identity and scope are unknown", root.display()),
            guidance: "The lock excludes the whole project; wait for the holder to finish, or use its original launcher/process controls after identifying it. No verified stop target is available.".into(),
        },
    }
}

/// A concise identified holder and its evidence-safe remedy. §FS-rhei-errors.1.2
struct OperatorBlocker {
    summary: String,
    guidance: String,
}
