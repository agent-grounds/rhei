// The timeout requirement is a statically decidable property of the effective
// agent binding, so it is decided with the other settings-aware checks rather
// than at the spawn that consumes it. §FS-rhei-validate.4 §FS-rhei-agents.3.2.2

/// Where a missing `agent_timeout` may be supplied, in the order the four-level
/// chain reads it. §FS-rhei-agents.7.1
fn missing_agent_timeout_help(agent_id: &str) -> String {
    format!(
        "set `agent_timeout` on the state, on `models.<id>.agents.{agent_id}.timeout`, on \
         `agents.{agent_id}.timeout`, or on `defaults.agent_timeout` in settings.json"
    )
}

/// The one sentence `rhei validate`, `rhei run --dry-run` and `rhei run` give
/// for a state that resolves to no finite timeout — rendered here so the three
/// surfaces cannot drift apart in wording. §FS-rhei-agents.3.2.2
fn missing_agent_timeout_sentence(state_name: &str, agent_id: &str) -> String {
    format!(
        "state '{state_name}' is driven by `rhei run` (orchestrator completion authority) \
         but no `agent_timeout` resolves for agent '{agent_id}'. Deterministic completion \
         requires a finite timeout"
    )
}

/// A collected validation error is a `String` with no help slot of its own, so
/// the four places the value may go ride along as a parenthetical, the way an
/// unknown agent carries its listing. §FS-rhei-errors.1.4
fn missing_agent_timeout_error(state_name: &str, agent_id: &str) -> String {
    format!(
        "{} ({})",
        missing_agent_timeout_sentence(state_name, agent_id),
        missing_agent_timeout_help(agent_id)
    )
}

/// Reject every non-gating, non-final state that resolves to an agent
/// invocation with no finite `agent_timeout`, judged from the plan and the
/// merged settings alone. It resolves through the same function execution
/// spawns through, per task identity, so a `**Target:**` override is judged as
/// it would be spawned and a fan-out member by member; `rhei run --no-agent`
/// resolves no invocation and so resolves nothing to reject.
/// §FS-rhei-validate.4 §FS-rhei-agents.3.2.2
fn validate_orchestrator_timeouts(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    settings: &RheiSettings,
    opts: &RunOptions,
) -> Vec<String> {
    let mut refused = BTreeSet::new();
    for machine in machines.distinct() {
        let tasks = tasks_for_machine(rhei, machines, machine);
        for (state_name, state) in &machine.states {
            // A gating, final or program state spawns no agent, so the rule
            // does not range over it. §FS-rhei-agents.3.2.2
            if state.terminal || state.gating || state.program.is_some() {
                continue;
            }
            if tasks.is_empty() {
                collect_unbounded_agent_invocations(
                    machine,
                    state_name,
                    settings,
                    opts,
                    None,
                    &mut refused,
                );
                continue;
            }
            for task in tasks.iter().copied() {
                collect_unbounded_agent_invocations(
                    machine,
                    state_name,
                    settings,
                    opts,
                    Some(task),
                    &mut refused,
                );
            }
        }
    }
    refused.into_iter().collect()
}

/// A resolution that fails at all is some other check's diagnostic — an unknown
/// agent, an undeclared mode — and a state that resolves no agent is not this
/// error either. Neither is restated as a missing timeout.
/// §FS-rhei-agents.3.2.2
fn collect_unbounded_agent_invocations(
    machine: &rhei_validator::StateMachine,
    state_name: &str,
    settings: &RheiSettings,
    opts: &RunOptions,
    task: Option<&rhei_core::ast::Task>,
    refused: &mut BTreeSet<String>,
) {
    let Ok(invocations) =
        resolve_agent_invocations_for_task(machine, state_name, settings, opts, task)
    else {
        return;
    };
    for resolved in invocations {
        if resolved.timeout_secs.is_none() {
            refused.insert(missing_agent_timeout_error(state_name, resolved.agent.id()));
        }
    }
}
