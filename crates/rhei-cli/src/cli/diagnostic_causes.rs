// Shared remedies are grouped by measured cause, never by caller area. §FS-rhei-errors.6

/// Shared wording for impossible generated representations only. §FS-rhei-errors.1.2
const INTERNAL_DIAGNOSTIC_HELP: &str = "this is an internal invariant failure. Please report a bug at https://github.com/agent-grounds/rhei/issues with the command and diagnostic.";

/// Shared wording for invalid or damaged recovery-marker evidence. §FS-rhei-errors.1.2
const RECOVERY_MARKER_REPAIR_HELP: &str = "stop recovery and preserve .rhei/forced-recovery.json and its image files. Restore the marker from a trusted copy; do not delete it or guess its evidence. If rhei wrote this invalid marker, report a bug with the diagnostic.";

/// Shared wording for ambiguous saved recovery evidence. §FS-rhei-errors.1.2
const RECOVERY_EVIDENCE_REPAIR_HELP: &str = "preserve the recovery marker and ledger. Restore the named evidence from a trusted backup before a human operator runs rhei recover again at an interactive terminal.";

/// Shared wording for failed OS account discovery. §FS-rhei-errors.1.2
const OS_ACCOUNT_HELP: &str = "restore a working whoami utility on PATH that returns the invoking OS account as UTF-8; do not substitute an environment identity.";

/// Shared wording for recovery inputs changed since confirmation. §FS-rhei-errors.1.2
const FRESH_OPERATOR_CONFIRMATION_HELP: &str = "a human operator must inspect the changed recovery scope and run the command again at an interactive terminal with fresh typed confirmation; an agent may not perform this ceremony.";

/// Shared wording for the worker retry authority boundary. §FS-rhei-errors.1.2
const WORKER_RETRY_HANDOFF_HELP: &str = "hand this request to an operator outside the worker. The worker cannot grant itself more attempts.";

/// Root interlocks carry their own recovery or account-directory guidance.
/// Other access errors retain their kind, context path, and lower-level text. §FS-rhei-errors.6
fn root_access_diagnostic(path: &Path, error: std::io::Error) -> Box<miette::MietteDiagnostic> {
    let measured_path = rhei_core::root_access::error_path(&error);
    let path = measured_path.unwrap_or(path);
    let help = rhei_core::root_access::refusal_help(&error)
        .map(str::to_owned)
        .unwrap_or_else(|| io_error_help(path, error.kind()));
    let message = if measured_path.is_some() { error.to_string() }
        else { format!("cannot access '{}': {error}", path.display()) };
    Box::new(miette::MietteDiagnostic::new(message)
        .with_help(help))
}

/// Use the same conversion for every CLI root-access entry point. §FS-rhei-errors.6
fn root_access_report(path: &Path, error: std::io::Error) -> Report {
    Report::new(*root_access_diagnostic(path, error))
}

/// Persisted ledger corruption is evidence damage, not a retryable spawn.
/// Preserve the original parser failure and the ledger being repaired. §FS-rhei-errors.6
fn ledger_report(path: &Path, error: impl std::fmt::Display) -> Report {
    miette!(
        help = format!("preserve {} and restore its valid UTF-8 transition rows and complete force audit pairs from a trusted backup before continuing. Do not delete history to bypass the refusal.", shell_quote(&path.display().to_string())),
        "invalid transition ledger '{}': {error}", path.display()
    )
}

/// A visit limit and a poll limit are distinct authored blockers. §FS-rhei-errors.6
fn loop_budget_help(spent: &SpentLoopBudget) -> String {
    format!("take a declared outgoing edge that does not re-enter the exhausted state, or have the workflow author increase {} in the governing states.yaml if more iterations are intended. --force cannot bypass this budget.", loop_budget_key(spent))
}

/// The authored limit a spent budget is measured against, as the workflow author would
/// raise it: `states.<s>.visits above <used>`. §FS-rhei-errors.6
fn loop_budget_key(spent: &SpentLoopBudget) -> String {
    let key = match spent.kind {
        LoopBudgetKind::Visits => "visits",
        LoopBudgetKind::PollAttempts => "poll.max_attempts",
    };
    format!("states.{}.{key} above {}", spent.state, spent.used)
}

/// The failed edge's actual predicate or measured budget determines help. §FS-rhei-errors.6
fn forced_edge_help(
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
    key: &TaskId,
    task: &rhei_core::ast::Task,
    from: &str,
    to: &str,
    rule: &rhei_core::ast::TransitionRule,
) -> String {
    if let Some(spent) = spent_loop_budget(machine, metadata, key, from, &task.state, to) {
        return loop_budget_help(&spent);
    }
    format!("satisfy the transition condition `{}` before taking this edge, or choose another applicable edge shown by: {}. --force does not override conditions.",
        rule.condition.as_deref().unwrap_or("the declared edge predicate"), help_command_or_bare("states"))
}

/// Use the shared operator command builder for the ancestor's own state and
/// profile. The requested destination must also be valid for that ancestor, and every
/// destination must be one the forced path's budget check lets it enter, measured
/// against its own counted visits as `rhei complete` measures them for its ancestor
/// line, so the line printed runs. §FS-rhei-errors.1.2 §FS-rhei-reset.5.1
fn ancestor_reopen_help(
    request: &ForcedRequest<'_>,
    loaded: &LoadedPlan,
    input: &Path,
    ancestor: &rhei_core::ast::Task,
    machine: &rhei_validator::StateMachine,
) -> String {
    let (id, from) = (ancestor.id.to_string(), normalized_state_name(&ancestor.state, machine));
    let visits = forced_hop_visits(loaded, input, &id, ancestor, &from, machine);
    let mut spent: Vec<SpentLoopBudget> = Vec::new();
    let mut permitted = |state: &str| {
        let allowed = machine.states.get(state).is_some_and(|definition| !definition.terminal)
            && ensure_task_profile_allows_state(machine, &id, &ancestor.kind,
                ancestor.profile_level(), state).is_ok();
        // A spent state is passed over and named, each once. §FS-rhei-errors.1.2
        match allowed.then(|| visits.as_ref()?.spent_entering(machine, state)).flatten() {
            Some(budget) if spent.iter().any(|named| named.state == budget.state) => false,
            Some(budget) => { spent.push(budget); false }
            None => allowed,
        }
    };
    let to = if permitted(request.to) {
        Some(request.to.to_owned())
    } else {
        initial_state_for_node(machine, &ancestor.kind, ancestor.profile_level()).ok()
            .filter(|state| permitted(state))
            .or_else(|| machine.states.keys().find(|state| permitted(state)).cloned())
    };
    match to {
        Some(to) => format!("a human operator must reopen ancestor {} first at an interactive terminal with fresh typed confirmation: {}.{} Supply the operator's own reason; an agent may not run this ceremony.",
            ancestor.id, ResetOffer::new(request.input).under_machine(request.machine_path)
                .forced_reopen(&id, &from, &to), passed_over(&to, &spent)),
        // Every candidate passed over for a spent budget: no line runs. §FS-rhei-errors.1.2
        None if !spent.is_empty() => format!("ancestor {} must be reopened first, but it cannot be forced back into {}. --force cannot bypass a visit budget: have the workflow author increase {} in the governing states.yaml if more iterations are intended, before a human operator reopens it at an interactive terminal; an agent may not perform the ceremony.",
            ancestor.id, spent_states(&spent), spent.iter().map(loop_budget_key).collect::<Vec<_>>().join(" or ")),
        None => format!("ancestor {} must be reopened first, but its profile has no permitted non-terminal destination. Ask the workflow author to add a non-terminal state for that ancestor's kind and level before a human operator reopens it at an interactive terminal; an agent may not perform the ceremony.", ancestor.id),
    }
}
