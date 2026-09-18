// Machine projection and state categories for visualization. §FS-rhei-viz.8

/// Normalize a raw `**State:**` value through the machine (e.g. resolving the
/// `-N` visit suffix on counted states for a live render).
pub fn normalize_state(raw_state: &str, machine: &StateMachine) -> String {
    parse_task_state(raw_state, machine).state
}

/// The set of states that are the entry of at least one profile, unioned with
/// any state flagged `initial: true` directly. §FS-rhei-viz §8, §FS-rhei-states
fn initial_states(machine: &StateMachine) -> HashSet<String> {
    let mut set: HashSet<String> = machine
        .states
        .iter()
        .filter(|(_, def)| def.initial)
        .map(|(name, _)| name.clone())
        .collect();
    if let Some(profiles) = &machine.profiles {
        for profile in profiles.values() {
            set.insert(profile.initial.clone());
        }
    }
    set
}

/// Flatten a [`StateMachine`] into the model's [`Machine`]: states in declared
/// order, each with its outgoing transitions (explicit first, then applicable
/// `from: "*"` wildcard edges) and artifact contracts. §FS-rhei-viz.8
pub fn flatten_machine(machine: &StateMachine) -> Machine {
    let initials = initial_states(machine);

    let to_artifacts = |defs: &[StateArtifactDef]| {
        defs.iter()
            .map(|a| Artifact {
                name: a.name.clone(),
                path: a.path.clone(),
                description: a.description.clone(),
                optional: a.optional,
            })
            .collect::<Vec<_>>()
    };

    let states = machine
        .states
        .iter()
        .map(|(name, def)| {
            let mut transitions: Vec<Transition> = machine
                .transitions
                .iter()
                .filter(|rule| rule.from.0 == *name)
                .map(|rule| Transition {
                    to: rule.to.0.clone(),
                    condition: rule.condition.clone(),
                    wildcard: false,
                })
                .collect();
            // Show wildcard edges only on matching sources. §FS-rhei-transitions.4.6
            if !def.terminal {
                for rule in machine.transitions.iter().filter(|rule| {
                    rule.from.0 == "*" && machine.transition_matches_source(rule, name)
                }) {
                    if rule.to.0 != *name && !transitions.iter().any(|t| t.to == rule.to.0) {
                        transitions.push(Transition {
                            to: rule.to.0.clone(),
                            condition: rule.condition.clone(),
                            wildcard: true,
                        });
                    }
                }
            }
            MachineState {
                name: name.clone(),
                description: def.description.clone(),
                instructions: machine.effective_instructions(def),
                visits: def.visits,
                initial: initials.contains(name),
                terminal: def.terminal,
                gating: def.gating,
                waiting_on: def.waiting_on_person().map(str::to_string),
                process: state_process_kind(def),
                transitions,
                inputs: to_artifacts(&def.inputs),
                outputs: to_artifacts(&def.outputs),
                template_context: template_context(def),
                template_contexts: fanout_template_contexts(def),
            }
        })
        .collect();

    Machine { name: machine.name.clone(), states }
}

fn state_process_kind(def: &crate::rhei_validator::StateDef) -> Option<MachineProcessKind> {
    if def.program.is_some() {
        Some(MachineProcessKind::Program)
    } else if def.agent.is_some()
        || def.model.is_some()
        || def.target.is_some()
        || !def.all_models.is_empty()
        || !def.all_targets.is_empty()
    {
        Some(MachineProcessKind::Agent)
    } else {
        None
    }
}

fn target_template_context(target: crate::rhei_validator::ExecutionTarget) -> TemplateContext {
    TemplateContext {
        target: Some(target.selector()),
        target_slug: Some(target.slug()),
        model_provider: target.provider.clone(),
        model_name: Some(target.model.clone()),
        model: Some(target.model),
        agent: Some(target.agent),
        agent_mode: target.mode,
    }
}

fn model_template_context(def: &crate::rhei_validator::StateDef, model: String) -> TemplateContext {
    TemplateContext {
        model: Some(model.clone()),
        model_name: Some(model),
        agent: def.agent.as_ref().map(|agent| agent.id().to_string()),
        agent_mode: def.agent_mode.clone(),
        ..TemplateContext::default()
    }
}

fn explicit_template_context(def: &crate::rhei_validator::StateDef) -> TemplateContext {
    if let Some(selector) = def.target.as_deref() {
        if let Ok(target) = parse_execution_target(selector) {
            return target_template_context(target);
        }
    }
    if let Some(model) = def.model.as_ref().map(|model| model.trim().to_string()) {
        return model_template_context(def, model);
    }
    TemplateContext {
        agent: def.agent.as_ref().map(|agent| agent.id().to_string()),
        agent_mode: def.agent_mode.clone(),
        ..TemplateContext::default()
    }
}

// Static prompt/artifact previews resolve only authored concrete values; multi
// fanout expands into per-target/model variants instead of guessing. §FS-rhei-viz.8
fn template_context(def: &crate::rhei_validator::StateDef) -> TemplateContext {
    let contexts = authored_fanout_template_contexts(def);
    if contexts.len() == 1 {
        contexts.into_iter().next().unwrap_or_default()
    } else {
        explicit_template_context(def)
    }
}

fn fanout_template_contexts(def: &crate::rhei_validator::StateDef) -> Vec<TemplateContext> {
    let contexts = authored_fanout_template_contexts(def);
    if contexts.len() > 1 {
        contexts
    } else {
        Vec::new()
    }
}

fn authored_fanout_template_contexts(
    def: &crate::rhei_validator::StateDef,
) -> Vec<TemplateContext> {
    if !def.all_targets.is_empty() {
        return def
            .all_targets
            .iter()
            .filter_map(|selector| parse_execution_target(selector).ok())
            .map(target_template_context)
            .collect();
    }
    if !def.all_models.is_empty() {
        return def
            .all_models
            .iter()
            .map(|model| model_template_context(def, model.trim().to_string()))
            .collect();
    }
    Vec::new()
}

/// Classify a persisted state into one of the seven categories: machine flags
/// first, state name second; `Active` is the catch-all. Mirrors the asset's
/// `category()`. §FS-rhei-viz.1.1
pub fn category(machine: &StateMachine, state: &str) -> Category {
    let def = machine.states.get(state);
    if state == "completed" {
        return Category::Done;
    }
    if state == "failed" {
        return Category::Failed;
    }
    if state == "blocked" {
        return Category::Blocked;
    }
    // A person wait shares the gate row rather than earning an eighth category:
    // same thing to the eye, differing only in who moves it on.
    // §FS-rhei-viz.1.1 §FS-rhei-states.2.5
    if def.map(|d| d.gating || d.waiting_on_person().is_some()).unwrap_or(false)
        || state == "human-review"
    {
        return Category::Gate;
    }
    if def.map(|d| d.terminal).unwrap_or(false) {
        return if state == "completed" { Category::Done } else { Category::Retired };
    }
    if state == "cancelled" || state == "archived" {
        return Category::Retired;
    }
    let is_initial =
        def.map(|d| d.initial).unwrap_or(false) || initial_states(machine).contains(state);
    if state == "draft" || state == "pending" || is_initial {
        return Category::Idle;
    }
    Category::Active
}

/// Derive the level-0 plan state from top-level task states: the pure derivation
/// over state names; the live host additionally promotes to `active` when a
/// top-level task is assigned to a running slot. §FS-rhei-viz.9
pub fn derive_plan_state(tasks: &[TaskRow], machine: &StateMachine) -> String {
    let roots: Vec<&str> =
        tasks.iter().filter(|t| t.depth == 0).map(|t| t.state.as_str()).collect();
    if roots.is_empty() {
        return "draft".into();
    }
    if roots.iter().all(|s| *s == "draft") {
        return "draft".into();
    }
    if roots.iter().all(|s| *s == "completed") {
        return "completed".into();
    }

    let terminal: HashSet<&str> = machine
        .states
        .iter()
        .filter_map(|(name, def)| def.terminal.then_some(name.as_str()))
        .collect();
    if roots.iter().all(|s| terminal.contains(s)) {
        return "archived".into();
    }

    // active-like = a non-terminal state that is not in the `idle` category.
    let any_active_like = roots.iter().any(|s| category(machine, s) == Category::Active);
    if any_active_like {
        "active".into()
    } else {
        "pending".into()
    }
}
