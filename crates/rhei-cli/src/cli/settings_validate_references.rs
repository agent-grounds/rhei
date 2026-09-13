fn tasks_for_machine<'a>(
    rhei: &'a rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    machine: &rhei_validator::StateMachine,
) -> Vec<&'a rhei_core::ast::Task> {
    fn visit<'a>(
        tasks: &'a [rhei_core::ast::Task],
        machines: &rhei_validator::MachineSet,
        fingerprint: &str,
        matching: &mut Vec<&'a rhei_core::ast::Task>,
    ) {
        for task in tasks {
            if machines.for_task(&task.id).fingerprint() == fingerprint {
                matching.push(task);
            }
            visit(&task.children, machines, fingerprint, matching);
        }
    }

    let mut matching = Vec::new();
    visit(&rhei.tasks, machines, &machine.fingerprint(), &mut matching);
    matching
}

/// Validate merged-settings references in the execution contexts the plan can
/// actually use. A task `**Target:**` owns its full identity and cannot make a
/// shadowed settings mode applicable. §FS-rhei-validate.4 §FS-rhei-agents.1.4.1
fn validate_plan_settings_references(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    settings: &RheiSettings,
) -> Vec<String> {
    let mut errors = Vec::new();
    for machine in machines.distinct() {
        errors.extend(validate_machine_settings_references_inner(
            machine,
            settings,
            false,
        ));
        let tasks = tasks_for_machine(rhei, machines, machine);
        let opts = default_run_options();
        validate_effective_static_agent_modes(machine, settings, &mut errors, |state| {
            !tasks.is_empty()
                && tasks.iter().all(|task| {
                    task.target.is_some()
                        && task_execution_override_applies_to_state(state, settings, &opts)
                })
        });
    }
    errors.extend(validate_task_execution_override_settings_references(rhei, settings));
    errors
}

fn validate_task_execution_override_settings_references(
    rhei: &rhei_core::ast::Rhei,
    settings: &RheiSettings,
) -> Vec<String> {
    fn visit(task: &rhei_core::ast::Task, settings: &RheiSettings, errors: &mut Vec<String>) {
        if let Some(selector) = task.target.as_deref() {
            match parse_execution_target(selector) {
                Ok(target) => {
                    let Some(profile) = settings.agents.get(target.agent.as_str()) else {
                        errors.push(format!(
                            "Task {} references unknown target agent '{}' in **Target:** '{}' ({})",
                            task.id,
                            target.agent,
                            selector,
                            known_agents_hint(settings)
                        ));
                        return;
                    };
                    if let Some(mode) = target.mode.as_deref() {
                        if !profile.modes.contains_key(mode) {
                            errors.push(format!(
                                "Task {} references unknown target mode '{}' for agent '{}' in **Target:** '{}' ({})",
                                task.id,
                                mode,
                                target.agent,
                                selector,
                                known_modes_hint(settings, &target.agent, profile)
                            ));
                        }
                    }
                }
                Err(_) => {
                    // Shape errors are reported by the semantic validator.
                }
            }
        }

        for child in &task.children {
            visit(child, settings, errors);
        }
    }

    // §FS-rhei-plan-language.3.11: Task `**Target:**` uses state target registry checks.
    let mut errors = Vec::new();
    for task in &rhei.tasks {
        visit(task, settings, &mut errors);
    }
    errors
}
