fn yaml_key(name: &str) -> YamlValue {
    YamlValue::String(name.to_string())
}

fn task_id_yaml_key(task_id: &TaskId) -> YamlValue {
    // Multi-segment ids (e.g., `1.2`, `api.cache`) are serialised as their
    // dotted-path string. Single-segment ids preserve their numeric shape
    // when numeric so existing frontmatter keys stay unchanged.
    if let Some(n) = task_id.as_number() {
        serde_yaml::to_value(n).expect("numeric task id should serialize")
    } else {
        yaml_key(&task_id.to_string())
    }
}

fn yaml_u64(value: u64) -> YamlValue {
    serde_yaml::to_value(value).expect("numeric YAML value should serialize")
}

fn yaml_value_to_u64(value: &YamlValue) -> Option<u64> {
    match value {
        YamlValue::Number(number) => number.as_u64(),
        _ => None,
    }
}

fn task_metadata_map<'a>(
    metadata: Option<&'a Metadata>,
    task_id: &TaskId,
) -> Option<&'a YamlMapping> {
    let root = metadata?;
    let metadata_section = root.get(yaml_key("metadata"))?.as_mapping()?;
    let tasks = metadata_section.get(yaml_key("tasks"))?.as_mapping()?;
    tasks.get(task_id_yaml_key(task_id))?.as_mapping()
}

fn task_metadata_number(metadata: Option<&Metadata>, task_id: &TaskId, field: &str) -> Option<u64> {
    task_metadata_map(metadata, task_id)
        .and_then(|task_map| task_map.get(yaml_key(field)))
        .and_then(yaml_value_to_u64)
}

fn task_visit_count(metadata: Option<&Metadata>, task_id: &TaskId, state_name: &str) -> u64 {
    task_metadata_map(metadata, task_id)
        .and_then(|task_map| task_map.get(yaml_key("stateVisits")))
        .and_then(YamlValue::as_mapping)
        .and_then(|state_visits| state_visits.get(yaml_key(state_name)))
        .and_then(yaml_value_to_u64)
        .map(|count| count.max(1))
        .unwrap_or(0)
}

fn parsed_task_state(
    raw_state: &str,
    machine: &rhei_validator::StateMachine,
) -> rhei_validator::ParsedTaskState {
    rhei_validator::parse_task_state(raw_state, machine)
}

fn normalized_state_name(raw_state: &str, machine: &rhei_validator::StateMachine) -> String {
    parsed_task_state(raw_state, machine).state
}

fn raw_state_visit_count(
    raw_state: &str,
    machine: &rhei_validator::StateMachine,
    expected_state: &str,
) -> u64 {
    let parsed = parsed_task_state(raw_state, machine);
    if parsed.state != expected_state || state_visit_limit(machine, expected_state).is_none() {
        return 0;
    }

    parsed.visit.map(u64::from).unwrap_or(1)
}

fn format_task_state_value(
    state_name: &str,
    visit_count: Option<u64>,
    machine: &rhei_validator::StateMachine,
) -> String {
    match visit_count.filter(|count| *count > 1) {
        Some(count) if state_visit_limit(machine, state_name).is_some() => {
            format!("{state_name}-{count}")
        }
        _ => state_name.to_string(),
    }
}

fn format_state_metadata_value(raw_state: &str) -> String {
    if raw_state.starts_with('`') && raw_state.ends_with('`') {
        raw_state.to_string()
    } else if raw_state.contains(' ') {
        format!("`{raw_state}`")
    } else {
        raw_state.to_string()
    }
}

fn state_visit_limit(machine: &rhei_validator::StateMachine, state_name: &str) -> Option<u64> {
    machine.states.get(state_name).and_then(|def| def.visits).map(u64::from)
}

/// Whether the engine keeps a `stateVisits` counter for this state.
///
/// A `visits:` budget has always been counted. Every non-poll state the machine
/// declares a self-loop from joins it, capped or not: `visitCount` is what such
/// a loop's own exit condition reads, and an uncounted one compares against `0`
/// forever. A supervising state is one of these by construction — its release
/// edge is a self-loop — and every visit is its own invocation, keyed by number
/// in the snapshots, artifacts, and checkpoints it produces. A poll state keeps
/// its own attempt accounting instead.
// §FS-rhei-supervision.4.2 §FS-rhei-supervision.1.2 §FS-rhei-transitions.4.3
fn state_counts_visits(machine: &rhei_validator::StateMachine, state_name: &str) -> bool {
    if state_visit_limit(machine, state_name).is_some() {
        return true;
    }
    let Some(def) = machine.states.get(state_name) else { return false };
    if def.poll.is_some() {
        return false;
    }
    def.execute_on().is_some() || state_declares_self_loop(machine, state_name)
}

/// Whether the machine declares a literal self-loop from this state.
// §FS-rhei-supervision.4.2
fn state_declares_self_loop(machine: &rhei_validator::StateMachine, state_name: &str) -> bool {
    machine
        .transitions()
        .iter()
        .any(|rule| rule.from.0 == state_name && rule.to.0 == state_name)
}

fn current_state_visit_count(
    metadata: Option<&Metadata>,
    task_id: &TaskId,
    current_state: &str,
    current_state_raw: &str,
    machine: &rhei_validator::StateMachine,
) -> u64 {
    let current = task_visit_count(metadata, task_id, current_state).max(raw_state_visit_count(
        current_state_raw,
        machine,
        current_state,
    ));
    if current > 0 {
        return current;
    }

    if state_counts_visits(machine, current_state) {
        return 1;
    }

    if machine.states.get(current_state).and_then(|def| def.poll.as_ref()).is_some() {
        return 1;
    }

    0
}

#[allow(clippy::too_many_arguments)]
fn resolve_condition_operand(
    token: &str,
    metadata: Option<&Metadata>,
    task_id: &TaskId,
    task: Option<&rhei_core::ast::Task>,
    current_state: &str,
    current_state_raw: &str,
    machine: &rhei_validator::StateMachine,
) -> MietteResult<i64> {
    if let Ok(value) = token.parse::<i64>() {
        return Ok(value);
    }

    match token {
        "visitCount" | "visit_count" => Ok(current_state_visit_count(
            metadata,
            task_id,
            current_state,
            current_state_raw,
            machine,
        ) as i64),
        "visits" => {
            let limit = state_visit_limit(machine, current_state).ok_or_else(|| {
                miette!(
                    help = "add `max_visits:` to the state, or drop the visit condition from the transition.",
                    "state '{}' does not declare a visit limit", current_state
                )
            })?;
            Ok(limit as i64)
        }
        // §FS-rhei-supervision.4.1: the subtree as the plan reads *now* —
        // callers hand in the node they re-read after the subprocess exited.
        "openDescendants" => {
            let Some(task) = task else {
                return Err(miette!(
                    help = open_descendants_operand_help(),
                    "condition operand 'openDescendants' needs the transitioning task's subtree, which is not available here"
                ));
            };
            Ok(open_descendant_count(task, machine) as i64)
        }
        "pollAttempts" => {
            let Some(_) = machine.states.get(current_state).and_then(|def| def.poll.as_ref())
            else {
                return Err(miette!(
                    help = poll_operand_help(),
                    "condition operand 'pollAttempts' is only available on poll states"
                ));
            };
            Ok(current_state_visit_count(
                metadata,
                task_id,
                current_state,
                current_state_raw,
                machine,
            ) as i64)
        }
        "pollMaxAttempts" => {
            let limit = machine
                .states
                .get(current_state)
                .and_then(|def| def.poll.as_ref())
                .map(|poll| poll.max_attempts)
                .ok_or_else(|| {
                    miette!(
                        help = poll_operand_help(),
                        "condition operand 'pollMaxAttempts' is only available on poll states"
                    )
                })?;
            Ok(i64::from(limit))
        }
        other => {
            let value = task_metadata_number(metadata, task_id, other).ok_or_else(|| {
                miette!(
                    help = "transition conditions read task metadata fields. Check the operand name against the state machine spec, then re-run: rhei validate <plan>",
                    "condition operand '{}' is not available in task metadata", other
                )
            })?;
            Ok(value as i64)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn evaluate_transition_condition(
    condition: &str,
    metadata: Option<&Metadata>,
    task_id: &TaskId,
    task: Option<&rhei_core::ast::Task>,
    current_state: &str,
    current_state_raw: &str,
    machine: &rhei_validator::StateMachine,
) -> MietteResult<bool> {
    let parts = condition.split_whitespace().collect::<Vec<_>>();
    if parts.len() != 3 {
        return Err(miette!(
            help = "write the condition as `<lhs> <op> <rhs>`, e.g. `visits >= 2`.",
            "unsupported transition condition '{}'; expected '<lhs> <op> <rhs>'",
            condition
        ));
    }

    let lhs = resolve_condition_operand(
        parts[0],
        metadata,
        task_id,
        task,
        current_state,
        current_state_raw,
        machine,
    )?;
    let rhs = resolve_condition_operand(
        parts[2],
        metadata,
        task_id,
        task,
        current_state,
        current_state_raw,
        machine,
    )?;

    let outcome = match parts[1] {
        "<" => lhs < rhs,
        "<=" => lhs <= rhs,
        ">" => lhs > rhs,
        ">=" => lhs >= rhs,
        "==" => lhs == rhs,
        "!=" => lhs != rhs,
        op => {
            return Err(miette!(
                help = "conditions use one of == != < <= > >=.",
                "unsupported operator '{}' in transition condition '{}'",
                op,
                condition
            ))
        }
    };

    Ok(outcome)
}

/// Explain why a specific declared transition is not applicable right now,
/// in user-facing prose. Returns a short phrase (e.g. "condition `visitCount
/// \>= visits` evaluated to false" or "visit budget for state 'review' is
/// exhausted (2/2 visits)"). A spent loop budget is reported by the check that
/// measured it, so the phrase names that budget's own state — the destination
/// on a loop back out of a gate — and not the state being left.
/// §FS-rhei-errors.1.5
///
/// Does NOT re-check applicability — callers are expected to invoke this only
/// when `transition_rule_is_applicable` returned false.
fn describe_blocked_transition(
    rule: &rhei_core::ast::TransitionRule,
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
    task_id: &TaskId,
    current_state: &str,
    current_state_raw: &str,
) -> String {
    if let Some(budget) = spent_loop_budget(
        machine,
        metadata,
        task_id,
        current_state,
        current_state_raw,
        &rule.to.0,
    ) {
        return budget.to_string();
    }
    if let Some(condition) = rule.condition.as_deref() {
        return format!("condition `{}` evaluated to false", condition);
    }
    "transition is not currently applicable".to_string()
}

/// Return the list of `to` states reachable from `from` whose applicability
/// check currently passes. Used to build actionable error messages when a
/// specific transition is blocked.
fn applicable_alternatives(
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
    task_id: &TaskId,
    task: Option<&rhei_core::ast::Task>,
    from: &str,
    current_state_raw: &str,
) -> Vec<String> {
    let mut out = Vec::new();
    for rule in machine.transitions() {
        if !machine.transition_matches_source(rule, from) {
            continue;
        }
        match transition_rule_is_applicable(
            rule,
            machine,
            metadata,
            task_id,
            task,
            from,
            current_state_raw,
        ) {
            Ok(true) => {
                if !out.contains(&rule.to.0) {
                    out.push(rule.to.0.clone());
                }
            }
            _ => continue,
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn transition_rule_is_applicable(
    rule: &rhei_core::ast::TransitionRule,
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
    task_id: &TaskId,
    task: Option<&rhei_core::ast::Task>,
    current_state: &str,
    current_state_raw: &str,
) -> MietteResult<bool> {
    // Source sets retain wildcard semantics. §FS-rhei-transitions.4.6
    if !machine.transition_matches_source(rule, current_state) {
        return Ok(false);
    }
    if !loop_reentry_allowed(
        machine,
        metadata,
        task_id,
        current_state,
        current_state_raw,
        &rule.to.0,
    ) {
        return Ok(false);
    }

    if let Some(condition) = rule.condition.as_deref() {
        return evaluate_transition_condition(
            condition,
            metadata,
            task_id,
            task,
            current_state,
            current_state_raw,
            machine,
        );
    }

    Ok(true)
}
