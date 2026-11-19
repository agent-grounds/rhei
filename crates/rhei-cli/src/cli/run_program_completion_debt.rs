// Completion debt belongs to the program's actual exit route, both immediately
// after it exits and when a later run finds its visit spent.
// §AR-source-file-size.3 §FS-rhei-run-report.3.1 §FS-rhei-programs.3.2

/// The part of the completion condition this exit owes: a zero exit owes state
/// outputs as well as a terminal result; an exact nonzero route owes only its
/// terminal result. Other failures carry the engine's account, and a self-loop
/// owes no departure artifacts. §FS-rhei-programs.3.2
struct ProgramExitCompletion {
    state_outputs: bool,
}

fn program_exit_completion(
    route: &ProgramExitRoute,
    state: &str,
    code: i32,
) -> Option<ProgramExitCompletion> {
    (route.to != state && (code == 0 || route.matched.is_declared_route()))
        .then_some(ProgramExitCompletion { state_outputs: code == 0 })
}

/// Read the exit from the spawn planner's previous attempt of this current
/// visit, never a historical state record or declaration-order agent edge.
/// Both pre-spawn warnings and the shared halt classifier ask this reader.
/// §FS-rhei-run-report.3.1 §FS-rhei-programs.3.2
fn missing_previous_program_outputs(
    artifact_root: &Path,
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
    task: &rhei_core::ast::Task,
    state: &str,
    plan: &SpawnPlan,
) -> Vec<MissingRequiredOutput> {
    let previous = plan.previous.as_ref().filter(|record| {
        record.kind == "program" && record.ending == "exited" && record.reverted.is_none()
    });
    let Some(code) = previous.and_then(|record| record.code) else {
        return Vec::new();
    };
    let Ok(Some(route)) = find_program_exit_transition(machine, metadata, task, state, code) else {
        return Vec::new();
    };
    let Some(completion) = program_exit_completion(&route, state, code) else {
        return Vec::new();
    };
    if completion.state_outputs {
        let visit = render_visit_count(metadata, &task.id, state, task.state.as_str(), machine);
        return missing_required_outputs_for_invocation(
            artifact_root, machine, task, state, visit, Some(&route.to),
            (None, None, None, None, None, None),
        );
    }
    missing_terminal_result_entry(
        artifact_root, machine, task, Some(&route.to), ResultInvocation::whole_task(),
    ).into_iter().collect()
}
