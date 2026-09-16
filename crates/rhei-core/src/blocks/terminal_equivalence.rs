//! Checked many-to-one terminals; all other ownership remains injective.
//! §FS-rhei-library.7.1
use super::references::Names;
use super::*;
use crate::state_machine::StateMachine;

pub(crate) struct TerminalGroup {
    stable: String,
    members: Vec<String>,
    context: String,
}

pub(crate) fn resolve(
    map: &CompatibilityMap,
    children: &BTreeMap<String, CompiledBlock>,
    local: &CompiledBlock,
    names: &mut Names,
) -> CompileResult<()> {
    for (stable, targets) in &map.terminals {
        if targets.len() < 2 {
            return Err(format!("compatibility.terminals '{stable}' needs at least two distinct child terminals; use states for one target"));
        }
        if map.states.contains_key(stable)
            || std::iter::once(local)
                .chain(children.values())
                .any(|b| b.fragment.machine.states.contains_key(stable))
        {
            return Err(format!("compatibility.terminals '{stable}' collides with an existing state or states mapping"));
        }
        let mut members = Vec::new();
        let mut origins = Vec::new();
        for target in targets {
            let (alias, name) = split_endpoint(target)
                .ok_or_else(|| format!("terminal '{target}' must be <alias>.<local-state>"))?;
            let child = children.get(alias).ok_or_else(|| {
                format!("terminal '{target}' names an unknown child; check compatibility.terminals")
            })?;
            let member = Qualifier::new(vec![alias.into()]).qualify(name);
            if !child.fragment.machine.states.contains_key(&member) {
                return Err(format!(
                    "terminal '{target}' does not exist in {}; choose a declared terminal",
                    child.source.display()
                ));
            }
            if names.states.insert(member.clone(), stable.clone()).is_some() {
                return Err(format!("terminal '{target}' is claimed more than once by compatibility; use one stable identity"));
            }
            members.push(member);
            origins.push(format!("'{target}' in {}", child.source.display()));
        }
        names.terminal_groups.push(TerminalGroup {
            stable: stable.clone(),
            members,
            context: format!("compatibility.terminals '{stable}': {}", origins.join(" and ")),
        });
    }
    Ok(())
}

pub(crate) fn validate(
    machine: &StateMachine,
    groups: &[TerminalGroup],
) -> CompileResult<Vec<String>> {
    let mut redundant = Vec::new();
    for group in groups {
        let error = |detail: String| {
            format!(
                "{}: {detail}; use separate identities or make the operative contracts equivalent",
                group.context
            )
        };
        let mut reference = None;
        let mut reference_role = None;
        for member in &group.members {
            let state = &machine.states[member];
            if !state.terminal || machine.transitions.iter().any(|r| r.from.0 == *member) {
                return Err(error(format!(
                    "'{member}' must be an unconsumed terminal with no outgoing exact transitions"
                )));
            }
            let cancellation = machine.is_cancellation(member);
            if reference_role.is_some_and(|role| role != cancellation) {
                return Err(error(format!("'{member}' differs in cancellation role")));
            }
            reference_role = Some(cancellation);
            if crate::state_machine::is_cancelled_state_name(&group.stable) && !cancellation {
                return Err(error(
                    "role differs from the reserved stable cancellation identity".into(),
                ));
            }
            machine
                .validate_state_prompt_template(member, state)
                .map_err(|e| error(e.to_string()))?;
            let mut normalized = state.clone();
            normalized.description = None;
            normalized.instructions = machine.effective_instructions(state);
            normalized.personality = machine.effective_personality(state);
            normalized.prompt_template = None;
            normalized.role = cancellation.then(|| "cancellation".into());
            for artifact in normalized.inputs.iter_mut().chain(&mut normalized.outputs) {
                artifact.description = None;
            }
            let mut contract =
                serde_json::to_value(normalized).map_err(|e| error(e.to_string()))?;
            contract["transition_contracts"] = incoming_contracts(machine, member)?;
            if let Some(previous) = &reference {
                if previous != &contract {
                    let previous: &serde_json::Value = previous;
                    let field = contract
                        .as_object()
                        .unwrap()
                        .iter()
                        .find(|(key, value)| previous.get(key.as_str()) != Some(*value))
                        .map(|(key, _)| key.as_str())
                        .unwrap_or("contract");
                    return Err(error(format!("'{member}' differs in {field}")));
                }
                redundant.push(member.clone());
            } else {
                reference = Some(contract);
            }
        }
    }
    Ok(redundant)
}

fn incoming_contracts(machine: &StateMachine, member: &str) -> CompileResult<serde_json::Value> {
    let mut contracts = BTreeSet::new();
    for rule in machine.transitions.iter().filter(|r| r.to.0 == member) {
        let mut value = serde_json::to_value(rule).map_err(|e| e.to_string())?;
        let fields = value.as_object_mut().unwrap();
        for field in ["from", "to", "sources"] {
            fields.remove(field);
        }
        if !fields.is_empty() {
            contracts.insert(value.to_string());
        }
    }
    Ok(serde_json::json!(contracts))
}
