/// Validate the effective task/state rule against the plan graph. State-only
/// validation cannot judge task overlays or cross-rhei Prior/ancestor sources.
// §FS-rhei-snapshots.11 §FS-rhei-plan-language.3.14
fn effective_snapshot_emitter_error(
    task: &rhei_core::ast::Task,
    inherit: &rhei_validator::SnapshotInheritConfig,
    machines: &ResolvedMachineSet,
) -> Option<String> {
    let task_id = task.id.to_string();
    let axis = inherit.from_axis.as_deref().unwrap_or("self");
    let sources = match axis {
        "self" => vec![(task_id.clone(), machines.machine_for_task_str(&task_id))],
        "ancestor" => nearest_matching_ancestor_source(task, inherit, machines).into_iter().collect(),
        "prior" => task
            .prior
            .iter()
            .map(|prior| {
                let id = prior.to_string();
                let machine = machines.machine_for_task_str(&id);
                (id, machine)
            })
            .collect(),
        _ => return None,
    };

    let matching_sources = sources
        .into_iter()
        .filter_map(|(source_id, machine)| {
            let states = matching_snapshot_emitter_states(machine, inherit);
            (!states.is_empty()).then_some((source_id, states))
        })
        .collect::<Vec<_>>();

    if matching_sources.is_empty() {
        return Some(format!(
            "Task {} has unresolvable effective snapshot inheritance '{}' from {} (no possible snapshot.emit source matches)",
            task.id, inherit.name, axis
        ));
    }

    None
}

fn nearest_matching_ancestor_source<'a>(
    task: &rhei_core::ast::Task,
    inherit: &rhei_validator::SnapshotInheritConfig,
    machines: &'a ResolvedMachineSet,
) -> Option<(String, &'a rhei_validator::StateMachine)> {
    let mut ancestor = task.id.parent();
    while let Some(id) = ancestor {
        let source_id = id.to_string();
        let machine = machines.machine_for_task_str(&source_id);
        if !matching_snapshot_emitter_states(machine, inherit).is_empty() {
            return Some((source_id, machine));
        }
        ancestor = id.parent();
    }
    None
}

fn matching_snapshot_emitter_states<'a>(
    machine: &'a rhei_validator::StateMachine,
    inherit: &rhei_validator::SnapshotInheritConfig,
) -> Vec<&'a str> {
    let selected_state =
        inherit.select.as_ref().and_then(|select| select.state.as_deref());
    machine
        .states
        .iter()
        .filter(|(state_name, state)| {
            selected_state.is_none_or(|selected| selected == state_name.as_str())
                && state
                    .snapshot
                    .as_ref()
                    .and_then(|snapshot| snapshot.emit.as_ref())
                    .is_some_and(|emit| emit.name == inherit.name)
        })
        .map(|(state_name, _)| state_name.as_str())
        .collect()
}
