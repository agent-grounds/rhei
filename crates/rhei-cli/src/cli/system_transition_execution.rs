
struct TransitionTaskInfo {
    task: rhei_core::ast::Task,
    level: u8,
    /// The task's ancestors, nearest first. Supervision reads them to find the
    /// nearest supervising one. §FS-rhei-supervision.2.2
    ancestors: Vec<rhei_core::ast::Task>,
}

fn task_profile_allows_state(
    machine: &rhei_validator::StateMachine,
    kind: &str,
    level: u8,
    state: &str,
) -> bool {
    machine
        .profile_for_node(kind, level)
        .is_none_or(|profile| profile.allowed.iter().any(|allowed| allowed == state))
}

/// The plan path as a user would type it: relative to the working directory
/// when it sits beneath it, absolute otherwise.
///
/// A `help =` names commands the user is meant to run next, and every other one
/// echoes the argument they typed. The transition path only has the resolved,
/// canonicalized plan path, so it renders that back down to the same shape
/// rather than pasting an absolute path into the middle of a suggested command.
// §FS-rhei-errors.2
fn plan_arg_for_help(plan_path: &Path) -> String {
    // One spelling on both sides of the `strip_prefix`: the plan path is the
    // plain form, so the working directory has to be canonicalized the same way
    // or nothing ever matches on Windows. §REQ-cross-platform.5
    let shown = std::env::current_dir()
        .ok()
        .and_then(|cwd| rhei_core::platform::canonical_path(&cwd).ok())
        .and_then(|cwd| plan_path.strip_prefix(cwd).ok())
        .map(|relative| {
            if relative.as_os_str().is_empty() {
                PathBuf::from(".")
            } else {
                relative.to_path_buf()
            }
        })
        .unwrap_or_else(|| plan_path.to_path_buf());
    shell_quote(&shown.display().to_string())
}

/// Reject a move into a `final: true` state while the task still has a
/// non-terminal descendant.
///
/// Descendants-first is a property of the graph, not of a command: it belongs
/// on the shared transition path beside compare-and-swap, artifact
/// enforcement, and callbacks, so `rhei transition`, `rhei complete`, `rhei
/// run`'s auto-advance, and a callback redirect all enforce it identically.
/// It cannot be delegated to machine authors either — transition `condition:`
/// expressions see only visit and exit-code variables, so no machine can gate
/// a parent's terminal edge on its children.
///
/// This is deliberately not symmetric with `**Prior:**` readiness, which
/// `rhei transition` skips as the human escape hatch: an out-of-order prior is
/// a `rhei validate` warning, a terminal parent with an open descendant is an
/// error. `transition` may produce a warning; it must never produce an error.
// §FS-rhei-transition-cmd.3.1 §FS-rhei-states.2.3
fn ensure_descendants_terminal_for_terminal_entry(
    machine: &rhei_validator::StateMachine,
    task: &rhei_core::ast::Task,
    local_id: &str,
    qualified_id: &str,
    to: &str,
    plan_path: &Path,
) -> MietteResult<()> {
    if task.children.is_empty()
        || !machine.states.get(to).map(|def| def.terminal).unwrap_or(false)
    {
        return Ok(());
    }
    // The task tree parsed here carries rhei-local ids; the caller knows the
    // project-qualified form, so recover the prefix and report descendants the
    // way every other surface prints them. §FS-rhei-panta.6
    let prefix = qualified_id.strip_suffix(local_id).unwrap_or("");
    let open = non_terminal_descendants(task, machine, prefix);
    if open.is_empty() {
        return Ok(());
    }
    // Name the command that shows the open work and the one that claims it:
    // "finish the descendants" is the answer, but the user still has to find
    // them. §FS-rhei-errors.2
    let plan = plan_arg_for_help(plan_path);
    Err(miette!(
        help = format!(
            "a parent is finished after its subtree is, and nothing finishes it on its \
             children's behalf. See the open work with: rhei list {plan} --non-terminal, \
             then claim it with: rhei next {plan}"
        ),
        "Task {} cannot enter terminal state '{}' while descendant tasks remain non-terminal.\n\
         Offending descendants: {}",
        qualified_id,
        to,
        open.join(", ")
    ))
}

fn ensure_task_profile_allows_state(
    machine: &rhei_validator::StateMachine,
    task_id_str: &str,
    kind: &str,
    level: u8,
    state: &str,
) -> MietteResult<()> {
    let Some(profile) = machine.profile_for_node(kind, level) else {
        return Ok(());
    };
    if profile.allowed.iter().any(|allowed| allowed == state) {
        return Ok(());
    }

    Err(miette!(
        help = "the task's node profile restricts which states it may enter. Change the task's profile, or widen the profile in the state machine.",
        "Task {} cannot enter state '{}': state is not allowed by its resolved profile. Profile allows: [{}]",
        task_id_str,
        state,
        profile.allowed.join(", ")
    ))
}

#[allow(clippy::too_many_arguments)]
fn execute_transition_with_origin(
    files: TransitionFiles<'_>,
    callback_paths: &CallbackPaths,
    machine: &rhei_validator::StateMachine,
    task_id_str: &str,
    from: &str,
    to: &str,
    no_callbacks: bool,
    origin: TransitionOrigin,
    claim: Option<&ClaimEligibilityContext<'_>>,
    run_options: Option<&RunOptions>,
) -> MietteResult<String> {
    // §FS-rhei-recover.4: direct engine/callback entries retain root access too.
    let _root_guard = rhei_core::root_access::for_input(files.artifact_root).map_err(|err| diagnostic!("{err}"))?;
    let task_file = files.task_file;
    let metadata_file = files.metadata_file;
    let workspace_root = execution_workspace_root(&callback_paths.plan_path);
    let settings = load_merged_settings(&workspace_root)?;
    let default_options = default_run_options();
    let run_options = run_options.unwrap_or(&default_options);

    // Validate that both `from` and `to` are valid states.
    if !machine.is_valid_state(from) {
        let allowed = machine.allowed_states().collect::<Vec<_>>().join(", ");
        return Err(miette!(
            help = unknown_state_help(),
            "'{}' is not a valid state. Allowed: [{}]", from, allowed
        ));
    }
    if !machine.is_valid_state(to) {
        let allowed = machine.allowed_states().collect::<Vec<_>>().join(", ");
        return Err(miette!(
            help = unknown_state_help(),
            "'{}' is not a valid state. Allowed: [{}]", to, allowed
        ));
    }

    #[cfg(test)]
    if origin.claim {
        run_claim_before_lock_hook();
    }

    // Stable sidecars survive every replacement. All writers take metadata,
    // then a distinct task file, then the ledger. §AR-agent-orchestrator-workflow.3.3.1
    let metadata_handle = LockedPlanFile::open(metadata_file)?;
    let task_handle = if task_file == metadata_file {
        None
    } else {
        Some(LockedPlanFile::open(task_file)?)
    };

    // Read the raw markdown while holding the locks.
    let metadata_raw = metadata_handle.read_to_string("failed to read plan file")?;
    let task_raw = match task_handle.as_ref() {
        None => metadata_raw.clone(),
        Some(handle) => handle.read_to_string("failed to read plan file")?,
    };

    // Parse to validate structure and find the task.
    // Try full plan parse first; fall back to manifest + task-file parse.
    let target_id = parse_task_id(task_id_str);
    // The metadata key is the ticket's local id everywhere except the basin,
    // whose metadata shares the project manifest under qualified ids.
    let metadata_key = parse_task_id(files.metadata_id);
    let manifest = if task_file == metadata_file {
        None
    } else {
        Some(parse_metadata_manifest(metadata_file, &metadata_raw)?)
    };
    let task_info = find_task_transition_info(
        &task_raw,
        task_file,
        manifest.as_ref().map(|index| &index.structure),
        &target_id,
        task_id_str,
    )?;
    if origin.claim {
        if let Some(existing) = task_info.task.assignee.as_deref() {
            if let Some(task_handle) = &task_handle {
                task_handle.release();
            }
            metadata_handle.release();
            return Err(miette!(
                help = format!(
                    "release it with: rhei release {} — or work on a different task.",
                    files.artifact_id
                ),
                "Task {} is already assigned to {}",
                files.artifact_id,
                existing
            ));
        }
    }
    let current_state_raw = task_info.task.state.clone();
    let current_state = normalized_state_name(&current_state_raw, machine);
    let metadata = if task_file == metadata_file {
        rhei_core::parse(&metadata_raw)
            .map_err(|err| {
                miette!(
                    help = plan_authoring_help(),
                    "failed to parse plan for transition metadata: {}", err.message
                )
            })?
            .metadata
    } else {
        manifest.and_then(|index| index.metadata)
    };

    // Compare-and-swap: verify the task's current state matches `from`.
    // This runs before the transition-legality check so a wrong `--from`
    // produces the actionable "task is in state X" error instead of the
    // less informative "transition not allowed" error.
    if current_state != from {
        if let Some(task_handle) = &task_handle {
            task_handle.release();
        }
        metadata_handle.release();
        return Err(miette!(
            help = task_moved_help(),
            "conflict: Task {} is in state '{}', expected '{}'",
            files.artifact_id,
            current_state_raw,
            from
        ));
    }
    // The explicit operation context is a relationship claim about the locked tree.
    // Validate it after compare-and-swap, before target policy, edge selection,
    // callbacks, artifacts, or effects. §FS-rhei-transition-cmd.3
    let supervising_owner =
        nearest_in_scope_supervising_owner(machine, &task_info.ancestors);
    if let Err(err) = ensure_operation_supervisor_matches(
        origin.supervisor.as_ref(),
        supervising_owner,
        task_id_str,
        files.artifact_id,
    ) {
        if let Some(task_handle) = &task_handle {
            task_handle.release();
        }
        metadata_handle.release();
        return Err(err);
    }
    // The CLI resolver carries a project-qualified id; checkpoint suppression
    // compares the same accepted identity in the locked file's local id space.
    // §FS-rhei-transition-cmd.2
    let operation_supervisor =
        origin.supervisor.as_ref().and(supervising_owner.map(|owner| &owner.id));
    if let Some(claim) = claim {
        if let Err(err) = ensure_claimable_under_lock(claim, files.artifact_id) {
            if let Some(task_handle) = &task_handle {
                task_handle.release();
            }
            metadata_handle.release();
            return Err(err);
        }
    }
    if let Err(err) = ensure_task_profile_allows_state(
        machine,
        files.artifact_id,
        &task_info.task.kind,
        task_info.level,
        to,
    ) {
        if let Some(task_handle) = &task_handle {
            task_handle.release();
        }
        metadata_handle.release();
        return Err(err);
    }

    // Now that we know the task really is in `from`, check whether the
    // declared transitions permit `from -> to`.
    // §FS-rhei-transitions.4.6: wildcards never authorize departure from a final state.
    let matching_rule =
        machine.transitions().iter().find(|rule| rule.from.0 == from && rule.to.0 == to).or_else(
            || machine.transitions().iter().find(|rule| rule.from.0 == "*" && machine.transition_matches_source(rule, from) && rule.to.0 == to),
        );
    let Some(matching_rule) = matching_rule else {
        if let Some(task_handle) = &task_handle {
            task_handle.release();
        }
        metadata_handle.release();
        return Err(miette!(
            help = "the machine declares no such edge. List the edges with: rhei states",
            "transition from '{}' to '{}' is not allowed by the state machine",
            from,
            to
        ));
    };

    let normalized_metadata = ensure_current_state_visit_count(
        metadata.as_ref(),
        &metadata_key,
        from,
        &current_state_raw,
        machine,
    );
    let metadata_for_checks = normalized_metadata.as_ref().or(metadata.as_ref());

    if !transition_rule_is_applicable(
        matching_rule,
        machine,
        metadata_for_checks,
        &metadata_key,
        Some(&task_info.task),
        from,
        &current_state_raw,
    )? {
        if let Some(task_handle) = &task_handle {
            task_handle.release();
        }
        metadata_handle.release();
        let reason = describe_blocked_transition(
            matching_rule,
            machine,
            metadata_for_checks,
            &metadata_key,
            from,
            &current_state_raw,
        );
        let alternatives = applicable_alternatives(
            machine,
            metadata_for_checks,
            &metadata_key,
            Some(&task_info.task),
            from,
            &current_state_raw,
        );
        let suffix = if alternatives.is_empty() {
            "No other transitions from this state are currently applicable.".to_string()
        } else {
            format!(
                "Currently applicable transitions from '{}': {}.",
                from,
                alternatives.join(", ")
            )
        };
        return Err(miette!(
            help = "the edge exists but its condition is unmet. Inspect the machine with: rhei states",
            "transition from '{}' to '{}' is not currently applicable: {}. {}",
            from,
            to,
            reason,
            suffix
        ));
    }

    // Descendants-first runs once the edge is declared and applicable, before
    // any callback: no "close your subtree" for a move that was never
    // available anyway. §FS-rhei-transition-cmd.3
    if let Err(err) = ensure_descendants_terminal_for_terminal_entry(
        machine,
        &task_info.task,
        task_id_str,
        files.artifact_id,
        to,
        &callback_paths.plan_path,
    ) {
        if let Some(task_handle) = &task_handle {
            task_handle.release();
        }
        metadata_handle.release();
        return Err(err);
    }

    let from_state_def = machine
        .states
        .get(from)
        .ok_or_else(|| miette!(
            help = internal_error_help(),
            "state '{}' missing from loaded machine", from
        ))?;

    let from_invocations = resolve_agent_invocations_for_task(
        machine,
        from,
        &settings,
        run_options,
        Some(&task_info.task),
    )
    .unwrap_or_default();
    let callback_contexts = callback_contexts_for_state(from_state_def, &from_invocations);

    // Parse the plan once for callback-context serialization. Failure here
    // means we fall back to a minimal payload rather than aborting — the
    // transition should still run even if the plan is only partially
    // structured.
    let plan_for_context = rhei_core::parse(&metadata_raw).ok();

    // Accumulated `transitionData` that flows from on_leave callbacks to
    // on_enter. Starts from the engine-seeded payload (e.g. timeout data)
    // and each callback's `data` merges last-write-wins.
    let mut transition_data: serde_json::Value = origin
        .seed_data
        .clone()
        .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));
    // A callback may request a redirect via `next_state`. The first such
    // request wins; later callbacks are still executed against the original
    // `to` for rejection checks, but their redirects are ignored.
    let mut redirect_next_state: Option<String> = None;

    // One identity spans model fan-out, redirects, and the enter phase. It is
    // allocated only after the pre-callback guards accept. §FS-rhei-transitions.1.2
    let callback_bearing = matching_rule.on_leave.is_some() || matching_rule.on_enter.is_some();
    let firing_id = (!no_callbacks && callback_bearing).then(|| uuid::Uuid::new_v4().to_string());
    let ledger_status = rhei_core::callback::TransitionLedgerStatus::Pending;

    // Execute on_leave callback before the state change.
    if !no_callbacks {
        if let Some(ref cb) = matching_rule.on_leave {
            let executor = ShellCallbackExecutor;
            for (model, agent) in callback_contexts {
                let context_json = build_transition_context_json(
                    plan_for_context.as_ref(),
                    &callback_paths.plan_path,
                    task_id_str,
                    files.artifact_id,
                    from,
                    to,
                    from,
                    firing_id
                        .as_deref()
                        .expect("callback-bearing transition has a firing identity"),
                    ledger_status,
                    origin.triggered_by.unwrap_or("user"),
                    &transition_data,
                    &callback_paths.working_dir,
                );
                let ctx = CallbackContext {
                    // §FS-rhei-panta.6: callbacks see the qualified id; the
                    // rhei-local heading id rides along for file edits.
                    task_id: files.artifact_id,
                    task_id_local: task_id_str,
                    from_state: from,
                    to_state: to,
                    firing_id: firing_id.as_deref(),
                    ledger_status: Some(ledger_status),
                    plan_path: &callback_paths.plan_path,
                    callback_cwd: &callback_paths.working_dir,
                    model,
                    agent,
                    context_json: Some(&context_json),
                };
                let result = executor.execute(cb, &ctx).map_err(|e| miette!(
                    help = state_machine_help(),
                    "{e}"
                ))?;
                if !result.success {
                    if let Some(task_handle) = &task_handle {
                        task_handle.release();
                    }
                    metadata_handle.release();
                    let message = result
                        .error
                        .clone()
                        .unwrap_or_else(|| "transition rejected by callback".to_string());
                    return Err(miette!(
                        help = callback_command_help(),
                        "on_leave callback '{}' rejected the transition: {message}",
                        cb.0
                    ));
                }
                if let Some(data) = result.data.as_ref() {
                    merge_transition_data(&mut transition_data, data);
                }
                if let Some(redirect) = result.next_state.clone() {
                    if redirect_next_state.is_none() {
                        redirect_next_state = Some(redirect);
                    }
                }
            }
        }
    }

    // Resolve redirects before committing state: validate the redirect is a
    // declared transition from the current state. A redirect to the same
    // target is a no-op.
    let (effective_to, effective_rule) = if let Some(redirect) = redirect_next_state.as_deref() {
        if redirect == to {
            (to.to_string(), matching_rule)
        } else if !machine.is_valid_state(redirect) {
            if let Some(task_handle) = &task_handle {
                task_handle.release();
            }
            metadata_handle.release();
            return Err(miette!(
                help = callback_command_help(),
                "on_leave callback redirected to unknown state '{}'", redirect
            ));
        } else if let Err(err) = ensure_task_profile_allows_state(
            machine,
            files.artifact_id,
            &task_info.task.kind,
            task_info.level,
            redirect,
        ) {
            if let Some(task_handle) = &task_handle {
                task_handle.release();
            }
            metadata_handle.release();
            return Err(err);
        } else if let Some(rule) =
            machine.transitions().iter().find(|r| r.from.0 == from && r.to.0 == redirect).or_else(
                || machine.transitions().iter().find(|r| r.from.0 == "*" && machine.transition_matches_source(r, from) && r.to.0 == redirect),
            )
        {
            (redirect.to_string(), rule)
        } else {
            if let Some(task_handle) = &task_handle {
                task_handle.release();
            }
            metadata_handle.release();
            return Err(miette!(
                help = callback_command_help(),
                "on_leave callback redirected to '{}', but no transition from '{}' to '{}' is declared",
                redirect,
                from,
                redirect
            ));
        }
    } else {
        (to.to_string(), matching_rule)
    };
    let to = effective_to.as_str();
    let matching_rule = effective_rule;

    // §FS-rhei-transition-cmd.3.1: re-check against the effective target so a
    // `nextState` redirect cannot smuggle a terminal entry past the guard.
    if let Err(err) = ensure_descendants_terminal_for_terminal_entry(
        machine,
        &task_info.task,
        task_id_str,
        files.artifact_id,
        to,
        &callback_paths.plan_path,
    ) {
        if let Some(task_handle) = &task_handle {
            task_handle.release();
        }
        metadata_handle.release();
        return Err(err);
    }

    let to_state_def = machine
        .states
        .get(to)
        .ok_or_else(|| miette!(
            help = internal_error_help(),
            "state '{}' missing from loaded machine", to
        ))?;

    let mut updated_metadata =
        update_metadata_for_transition(metadata_for_checks, &metadata_key, to, machine)
            .or_else(|| normalized_metadata.clone());
    if from_state_def.poll.is_some() && to != from {
        updated_metadata = clear_poll_state_metadata(
            updated_metadata.as_ref().or(metadata_for_checks),
            &metadata_key,
            from,
        );
    }
    if to != from {
        updated_metadata = clear_provider_limit_state_metadata(
            updated_metadata.as_ref().or(metadata_for_checks),
            &metadata_key,
            from,
        );
    }
    let from_visit_count = Some(render_visit_count(
        metadata_for_checks,
        &metadata_key,
        from,
        &current_state_raw,
        machine,
    ));
    let to_visit_count = updated_metadata
        .as_ref()
        .map(|meta| task_visit_count(Some(meta), &metadata_key, to))
        .filter(|count| *count > 0);

    // The barrier is maintained here so `rhei run`'s auto-advance, `rhei
    // transition`, `rhei complete`, and a callback redirect all maintain it
    // identically. §FS-rhei-supervision.2 §FS-rhei-supervision.3.3
    if let Some(next) = supervision_after_transition(
        updated_metadata.as_ref().or(metadata_for_checks),
        machine,
        &task_info,
        files,
        AppliedTransition {
            metadata_key: &metadata_key,
            local_id: task_id_str,
            from,
            to,
            to_visit: to_visit_count.unwrap_or(1),
        },
        supervising_owner,
        operation_supervisor,
    ) {
        updated_metadata = Some(next);
    }

    // §AR-rhei-panta.2: artifact templates render the project-qualified id
    // against the owning rhei's execution root, matching the paths agents and
    // ready-checks were shown.

    // A cancel abandons the work, so the abandoned step's artifact contract is
    // moot; the target's inputs and the terminal result below still apply.
    // §FS-rhei-transitions.4.5

    // Explicit roles and legacy spellings share one waiver. §FS-rhei-states.1.4
    let cancelling = machine.is_cancellation(&normalized_state_name(to, machine));
    // Whether the source state's outputs were actually established on this
    // edge. The engine's own account of a callback-only move reports this
    // rather than asserting it. §FS-rhei-run.3
    let source_outputs_verified = !origin.skip_source_outputs && !cancelling;
    if source_outputs_verified {
        ensure_state_outputs_exist_for_transition(
            files.artifact_root,
            Some(&task_info.task),
            files.artifact_id,
            from,
            from_state_def,
            from_visit_count,
            machine,
            &settings,
            run_options,
            machine.states.get(to).map(|def| def.terminal).unwrap_or(false),
        )?;
    }
    // After `on_leave` settles the target and source outputs get first refusal,
    // guard exports before target inputs or terminal effects; cancellation waives it.
    // §FS-rhei-transition-cmd.3.3 §FS-rhei-plan-language.3.12.3
    if to_state_def.terminal && !cancelling {
        ensure_declared_task_exports_exist(
            files.artifact_root,
            &task_info.task,
            files.artifact_id,
            to,
        )?;
    }
    ensure_state_inputs_exist_for_transition_with_options(
        files.artifact_root,
        Some(&task_info.task),
        files.artifact_id,
        to,
        to_state_def,
        to_visit_count,
        machine,
        &settings,
        run_options,
        &format!("Task {} cannot enter state {}.", files.artifact_id, to),
    )?;
    // Ownership comes from the effective target, including a redirect. Resolution failures
    // retain the manual fallback; output prints its existing warning after commit. §FS-rhei-next.3.1
    let claim_assignee = resolve_claim_assignee(origin.claim, machine, to, &settings, &task_info.task);
    // A caller that knows the outcome carries the message; otherwise the
    // engine's own account stands in, but only where it is true.
    // §FS-rhei-states.3.3 §FS-rhei-run.3
    let recorded_message = origin.result_message.clone().or_else(|| {
        let lands_terminal = machine.states.get(to).map(|def| def.terminal).unwrap_or(false);
        if lands_terminal && !task_result_is_present(files.artifact_root, files.artifact_id) {
            origin
                .terminal_result_fallback
                .as_ref()
                .map(|account| account.sentence(source_outputs_verified))
        } else {
            None
        }
    });

    // The terminal result is the one artifact contract of a `final: true` state
    // that no machine declares, checked here beside the declared ones and
    // against the same effective target. §FS-rhei-states.3.3
    if let Err(err) = ensure_terminal_result_available(
        machine,
        files.artifact_root,
        files.artifact_id,
        from,
        to,
        recorded_message.as_deref(),
        &callback_paths.plan_path,
    ) {
        if let Some(task_handle) = &task_handle {
            task_handle.release();
        }
        metadata_handle.release();
        return Err(err);
    }

    // A claim redirect cannot finish work. The generic result check retains its no-result
    // diagnostic, but an existing result cannot satisfy this refusal. §FS-rhei-next.3.1
    if origin.claim && machine.states.get(to).map(|def| def.terminal).unwrap_or(false) {
        if let Some(task_handle) = &task_handle {
            task_handle.release();
        }
        metadata_handle.release();
        return Err(miette!(
            help = "finish the work explicitly with `rhei complete`; `rhei next` only takes ownership of non-terminal work",
            "Task {} cannot enter terminal state '{}' during a claim",
            files.artifact_id,
            to
        ));
    }

    // The edge is charged before the state that records it is written, so a
    // ticket at its bound is refused rather than moved and then told. A crash
    // between the two over-counts by one, which is the conservative direction.
    // §FS-rhei-budgets.4.1 §FS-rhei-budgets.1
    match budget_charge_applied_edge(
        &TransitionCharge {
            workspace_root: &workspace_root,
            machine,
            settings: &settings,
            task: Some(&task_info.task),
            metadata_key: &metadata_key,
            metadata_file,
            // The project-qualified id, which is what the run loop admitted
            // this visit under and what the ledger line names. The local id
            // next door names the ticket inside its own file and would find no
            // claim to convert. §FS-rhei-panta.6
            task_id_str: files.artifact_id,
            from,
            to,
        },
        updated_metadata.as_ref().or(metadata_for_checks),
    ) {
        Ok(None) => {}
        Ok(minted) => updated_metadata = minted,
        Err(err) => {
            if let Some(task_handle) = &task_handle {
                task_handle.release();
            }
            metadata_handle.release();
            return Err(err);
        }
    }

    let rendered_to_state = format_task_state_value(to, to_visit_count, machine);
    // §FS-rhei-supervision.3.4: the release self-loop ends the visit, so it
    // ends the claim the visit was taken under.
    let ends_visit = transition_ends_supervisor_visit(machine, from, to);
    let metadata_raw_updated = if task_file == metadata_file {
        let new_task_raw =
            rewrite_task_for_transition(&task_raw, task_id_str, &rendered_to_state, ends_visit)?;
        if let Some(updated_metadata) = updated_metadata.as_ref() {
            rewrite_frontmatter(&new_task_raw, updated_metadata)?
        } else {
            new_task_raw
        }
    } else if let Some(updated_metadata) = updated_metadata.as_ref() {
        rewrite_frontmatter(&metadata_raw, updated_metadata)?
    } else {
        metadata_raw.clone()
    };

    let task_raw_updated = if task_file == metadata_file {
        None
    } else {
        Some(rewrite_task_for_transition(
            &task_raw,
            task_id_str,
            &rendered_to_state,
            ends_visit,
        )?)
    };

    // A claim holds the ledger before plan writes; ordinary transitions take the same
    // serialization only when recording. §FS-rhei-next.3.1
    let mut claim_transaction = if origin.claim {
        Some(ClaimTransaction::begin(
            files,
            &metadata_handle,
            task_handle.as_ref(),
            &metadata_raw,
            &task_raw,
        )?)
    } else {
        None
    };

    // Atomic write(s): write to temp file in the same directory, then rename.
    // The sidecars remain held across every replacement and any restoration.
    // §AR-agent-orchestrator-workflow.3.3.1
    persist_transition_state(
        &mut claim_transaction,
        metadata_file,
        task_file,
        &metadata_handle,
        task_handle.as_ref(),
        &metadata_raw_updated,
        task_raw_updated.as_deref(),
    )?;

    #[cfg(test)]
    if origin.claim {
        run_claim_after_state_write_hook();
    }

    // Execute on_enter callback after the state change (not model-looped).
    let triggered_by = origin.triggered_by.unwrap_or(if redirect_next_state.is_some() {
        "callback"
    } else {
        "user"
    });
    if !no_callbacks {
        if let Some(ref cb) = matching_rule.on_enter {
            let on_enter_context_json = build_transition_context_json(
                plan_for_context.as_ref(),
                &callback_paths.plan_path,
                task_id_str,
                files.artifact_id,
                from,
                to,
                to,
                firing_id
                    .as_deref()
                    .expect("callback-bearing transition has a firing identity"),
                ledger_status,
                triggered_by,
                &transition_data,
                &callback_paths.working_dir,
            );
            let callback_ctx = CallbackContext {
                task_id: files.artifact_id,
                task_id_local: task_id_str,
                from_state: from,
                to_state: to,
                firing_id: firing_id.as_deref(),
                ledger_status: Some(ledger_status),
                plan_path: &callback_paths.plan_path,
                callback_cwd: &callback_paths.working_dir,
                model: None,
                agent: None,
                context_json: Some(&on_enter_context_json),
            };
            let executor = ShellCallbackExecutor;
            let result = match executor.execute(cb, &callback_ctx) {
                Ok(result) => result,
                Err(err) if origin.claim => {
                    let original = miette!(help = state_machine_help(), "{err}");
                    return Err(finish_failed_claim_enter(
                        &mut claim_transaction,
                        original,
                        files,
                        callback_paths,
                        machine,
                        task_id_str,
                        from,
                        no_callbacks,
                        task_handle.as_ref(),
                        &metadata_handle,
                    ));
                }
                Err(err) => return Err(miette!(help = state_machine_help(), "{err}")),
            };
            if !result.success {
                // Spec §Example 8: on_enter failure rolls back the state
                // write to the original, then the error_handling policy
                // applies. We implement the rollback; policy execution is
                // a follow-up.
                let message =
                    result.error.clone().unwrap_or_else(|| "on_enter callback failed".to_string());
                let original = miette!(
                    help = callback_command_help(),
                    "on_enter callback '{}' failed: {message}", cb.0
                );
                if origin.claim {
                    return Err(finish_failed_claim_enter(
                        &mut claim_transaction,
                        original,
                        files,
                        callback_paths,
                        machine,
                        task_id_str,
                        from,
                        no_callbacks,
                        task_handle.as_ref(),
                        &metadata_handle,
                    ));
                }
                let rollback_err = write_file_atomic_locked(
                    metadata_file,
                    &metadata_raw,
                    Some(&metadata_handle),
                )
                .err();
                let task_rollback_err = if task_raw_updated.is_some() {
                    write_file_atomic_locked(task_file, &task_raw, task_handle.as_ref()).err()
                } else {
                    None
                };
                if let Some(task_handle) = &task_handle {
                    task_handle.release();
                }
                metadata_handle.release();
                if rollback_err.is_some() || task_rollback_err.is_some() {
                    return Err(miette!(
                        help = callback_command_help(),
                        "on_enter callback '{}' failed ({message}); rollback also failed — plan file may be inconsistent",
                        cb.0
                    ));
                }
                return Err(miette!(
                    help = callback_command_help(),
                    "on_enter callback '{}' failed: {message}", cb.0
                ));
            }
        }
    }

    commit_claim(
        claim_transaction.as_mut(),
        claim_assignee.as_deref(),
        &metadata_raw_updated,
        task_raw_updated.as_deref(),
        task_id_str,
        from,
        to,
    )?;

    // Release the claim ledger before task and metadata, the reverse of the
    // shared acquisition order. Ordinary recording below scopes its own ledger
    // hold the same way. §AR-agent-orchestrator-workflow.3.3.1
    drop(claim_transaction.take());

    // Inside the lock, after `on_enter` had its chance to roll the write back,
    // so no caller can apply a transition and forget the ledger or the result.
    // §FS-rhei-complete.3 §FS-rhei-transition-cmd.3
    let record = if origin.claim {
        Ok(())
    } else {
        record_transition_result(
            files.artifact_root,
            files.task_file,
            task_id_str,
            machine,
            files.artifact_id,
            from,
            to,
            recorded_message.as_deref(),
        )
    };

    if let Some(task_handle) = task_handle {
        task_handle.release();
    }
    metadata_handle.release();
    record?;
    Ok(to.to_string())
}

/// The task's markdown after this transition: the new `**State:**` value, and
/// — only when the edge ends a supervisor's visit — without the
/// `**Assignee:**` that visit was claimed under.
///
/// Rewritten in memory rather than through the claim command so an `on_enter`
/// failure rolls the claim back with the state it was written beside.
// §FS-rhei-supervision.3.4 §FS-rhei-transition-cmd.3
fn rewrite_task_for_transition(
    raw: &str,
    local_id: &str,
    rendered_to_state: &str,
    ends_supervisor_visit: bool,
) -> MietteResult<String> {
    let rewritten = rewrite_task_state(raw, local_id, rendered_to_state)?;
    if !ends_supervisor_visit {
        return Ok(rewritten);
    }
    Ok(without_task_assignee(&rewritten, local_id).0)
}

/// Extract the current state and node-policy inputs for a task from raw
/// markdown content.
///
/// Tries full-plan parsing first, falls back to workspace task-file parsing.
fn find_task_transition_info(
    raw: &str,
    file_path: &Path,
    workspace_structure: Option<&rhei_core::ast::Structure>,
    target_id: &TaskId,
    task_id_str: &str,
) -> MietteResult<TransitionTaskInfo> {
    // Try full plan parse.
    if let Ok(rhei) = rhei_core::parse(raw) {
        if let Some(task) = find_task_by_id(&rhei.tasks, target_id) {
            return Ok(TransitionTaskInfo {
                task: task.clone(),
                level: task.id.depth() as u8,
                ancestors: ancestor_chain(&rhei.tasks, target_id)
                    .into_iter()
                    .cloned()
                    .collect(),
            });
        }
    }

    // Try workspace task-file parse.
    let workspace_tasks = match workspace_structure {
        Some(structure) => rhei_core::parser::parse_workspace_tasks_with_structure(raw, structure),
        None => rhei_core::parser::parse_workspace_tasks(raw),
    };
    if let Ok(tasks) = workspace_tasks {
        if let Some(task) = find_task_by_id(&tasks, target_id) {
            return Ok(TransitionTaskInfo {
                task: task.clone(),
                level: task.id.depth() as u8,
                ancestors: ancestor_chain(&tasks, target_id).into_iter().cloned().collect(),
            });
        }
    }

    Err(miette!(
        help = task_id_help(),
        "task '{}' not found in {}", task_id_str, file_path.display()
    ))
}

// ─── Agent Configuration ──────────────────────────────────────────────
