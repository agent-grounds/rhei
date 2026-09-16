impl StateMachine {
    fn validate_snapshot_configuration(&self) -> Result<(), StateMachineLoadError> {
        for (state_name, state) in &self.states {
            let Some(snapshot) = state.snapshot.as_ref() else {
                continue;
            };

            // This pass covers the state-machine-local snapshot grammar and
            // static lineage checks. Settings-dependent profile checks are
            // §FS-rhei-snapshots.11: CLI performs registry-dependent snapshot checks.
            if let Some(emit) = snapshot.emit.as_ref() {
                validate_snapshot_name(state_name, "snapshot.emit.name", &emit.name)?;
                if let Some(on) = emit.on.as_deref() {
                    match on {
                        "success" | "failure" | "always" => {}
                        _ => {
                            return Err(StateMachineLoadError::Invalid(format!(
                                "state '{state_name}' has unsupported snapshot.emit.on '{on}' (expected success, failure, or always)"
                            )));
                        }
                    }
                }
                if state.terminal {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' is final and cannot declare 'snapshot.emit' (terminal states have no work to snapshot)"
                    )));
                }
                if state.gating {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' is gating and cannot declare 'snapshot.emit' (gating states have no autonomous execution)"
                    )));
                }
                if state.program.is_some() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' is a program state and cannot declare 'snapshot.emit' (programs have no agent transcript)"
                    )));
                }
            }

            if let Some(inherit) = snapshot.inherit.as_ref() {
                validate_snapshot_name(state_name, "snapshot.inherit.name", &inherit.name)?;
                if let Some(from_axis) = inherit.from_axis.as_deref() {
                    match from_axis {
                        "self" | "ancestor" => {}
                        _ => {
                            return Err(StateMachineLoadError::Invalid(format!(
                                "state '{state_name}' has unsupported snapshot.inherit.from '{from_axis}' (expected self or ancestor)"
                            )));
                        }
                    }
                }
                if let Some(compat) = inherit.compat.as_deref() {
                    match compat {
                        "native" | "none" => {}
                        _ => {
                            return Err(StateMachineLoadError::Invalid(format!(
                                "state '{state_name}' has unsupported snapshot.inherit.compat '{compat}' (expected native or none)"
                            )));
                        }
                    }
                }
                if inherit.required == Some(true) && inherit.compat.as_deref() == Some("none") {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' declares snapshot.inherit.required: true with compat: none"
                    )));
                }
                if state.terminal {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' is final and cannot declare 'snapshot.inherit' (terminal states have no work)"
                    )));
                }
                if state.gating {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' is gating and cannot declare 'snapshot.inherit' (gating states have no autonomous execution)"
                    )));
                }
                if state.program.is_some() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' is a program state and cannot declare 'snapshot.inherit' (programs do not consume agent transcripts)"
                    )));
                }
                if state.poll.is_some() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' declares both 'poll' and 'snapshot.inherit'; polling states cannot inherit snapshots in v1"
                    )));
                }

                if let Some(select) = inherit.select.as_ref() {
                    if let Some(selected_state) = select.state.as_deref() {
                        if !self.states.contains_key(selected_state) {
                            return Err(StateMachineLoadError::Invalid(format!(
                                "state '{state_name}' has snapshot.inherit.select.state '{selected_state}' but no such state is defined"
                            )));
                        }
                    }
                    if let Some(target) = select.target.as_deref() {
                        if target.trim().is_empty() {
                            return Err(StateMachineLoadError::Invalid(format!(
                                "state '{state_name}' has empty snapshot.inherit.select.target"
                            )));
                        }
                        if target == "all" {
                            return Err(StateMachineLoadError::Invalid(format!(
                                "state '{state_name}' has unsupported snapshot.inherit.select.target 'all' (fanout aggregation is not supported in v1)"
                            )));
                        }
                        if target == "same" && !state_declares_snapshot_target_shape(state) {
                            return Err(StateMachineLoadError::Invalid(format!(
                                "state '{state_name}' uses snapshot.inherit.select.target: same but the inheriting state does not declare target, all_targets, all_models, agent, or model"
                            )));
                        }
                    }
                    if let Some(visit) = select.visit.as_ref() {
                        validate_snapshot_selector_value(
                            state_name,
                            "snapshot.inherit.select.visit",
                            visit,
                            &["latest"],
                        )?;
                    }
                    if let Some(generation) = select.generation.as_ref() {
                        validate_snapshot_selector_value(
                            state_name,
                            "snapshot.inherit.select.generation",
                            generation,
                            &["current", "latest"],
                        )?;
                    }
                }

                let selected_state =
                    inherit.select.as_ref().and_then(|select| select.state.as_deref());
                let select_target =
                    inherit.select.as_ref().and_then(|select| select.target.as_deref());
                let possible_emitters: Vec<(&String, &StateDef)> = self
                    .states
                    .iter()
                    .filter(|(candidate_name, candidate)| {
                        selected_state.is_none_or(|selected| selected == candidate_name.as_str())
                            && candidate
                                .snapshot
                                .as_ref()
                                .and_then(|snapshot| snapshot.emit.as_ref())
                                .is_some_and(|emit| emit.name == inherit.name)
                    })
                    .collect();

                if possible_emitters.is_empty() {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' has unresolvable snapshot.inherit reference '{}' (no possible snapshot.emit source matches)",
                        inherit.name
                    )));
                }
                if possible_emitters.len() > 1 && selected_state.is_none() {
                    let emitters = possible_emitters
                        .iter()
                        .map(|(name, _)| name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' has ambiguous snapshot.inherit reference '{}' (possible emitting states: {emitters}); add snapshot.inherit.select.state",
                        inherit.name
                    )));
                }

                if possible_emitters
                    .iter()
                    .any(|(_, emitter)| state_declares_snapshot_fanout_source(emitter))
                    && select_target.is_none()
                {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "state '{state_name}' inherits snapshot '{}' from a fanout source; set snapshot.inherit.select.target",
                        inherit.name
                    )));
                }

                if inherit.required == Some(true) {
                    if let Some(inheritor_agent) = statically_resolved_snapshot_agent(state) {
                        for (emitter_name, emitter) in &possible_emitters {
                            if let Some(emitter_agent) = statically_resolved_snapshot_agent(emitter)
                            {
                                if emitter_agent != inheritor_agent {
                                    return Err(StateMachineLoadError::Invalid(format!(
                                        "state '{state_name}' requires snapshot '{}' from state '{emitter_name}', but source agent '{emitter_agent}' does not match inheritor agent '{inheritor_agent}'",
                                        inherit.name
                                    )));
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

}
