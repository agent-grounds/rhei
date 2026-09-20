// The completion condition asked of one invocation at a time, and the two
// questions `rhei run` puts to it: *may this invocation be skipped* before a
// pass spawns it, and *has the state finished* once one has exited.
//
// Its own part because those two moments used to hold two different rules. The
// scheduler asked only whether the declared `outputs:` were on disk, so an
// invocation that had failed the condition on one pass — outputs written, the
// ticket's terminal result never written — was read on the next as having
// nothing left to do, and the ticket advanced into a `final: true` state on the
// strength of artifacts that had never answered for it. One rule, one place.

// §AR-source-file-size.3 §FS-rhei-agents.3.2 §FS-rhei-run.3

/// One state visit, ready to be asked the completion condition about each of
/// its invocations.
///
/// `artifact_root` is the owning rhei's execution root: where declared
/// `outputs:` resolve and where results live. One field, not two same-typed
/// roots, because both resolve against the same place and a run-level root
/// passed here would look in the wrong directory for a Panta project member.
/// §FS-rhei-agents.3.2 condition (2)
/// `runtime_dir` is where this invocation's spawn record and the transition
/// ledger used by its run live. §FS-rhei-agents.8.4
/// `finishes_ticket` is a property of the *edge* the exit would select, not of
/// the state, which is why it is settled once here rather than re-derived per
/// invocation.
// §FS-rhei-agents.3.2 §FS-rhei-states.3.3 §FS-rhei-panta.6.2
struct InvocationCompletion<'a> {
    artifact_root: &'a Path,
    runtime_dir: &'a Path,
    task: &'a rhei_core::ast::Task,
    state_name: &'a str,
    current_state_raw: &'a str,
    machine: &'a rhei_validator::StateMachine,
    metadata: Option<&'a Metadata>,
    state_def: &'a rhei_validator::StateDef,
    finishes_ticket: bool,
    visit_count: u64,
    moves: u64,
}

impl InvocationCompletion<'_> {
    /// Whether an existing artifact belongs to work eligible for this visit.
    ///
    /// With neither a record nor evidence of a prior visit to this state, an upgraded
    /// workspace retains the legacy first-visit interpretation. Moves through
    /// other states do not establish re-entry. Otherwise only this invocation's
    /// successful current-visit record can answer.
    // §FS-rhei-agents.3.2 §FS-rhei-agents.8.4 §FS-rhei-transitions.4.3
    fn work_is_eligible_for_visit(&self, resolved: &ResolvedAgent) -> bool {
        let task_id = self.task.id.to_string();
        let suffix = resolved_agent_log_suffix(resolved, Some(self.visit_count));
        let record = read_spawn_record(&spawn_record_path(
            self.runtime_dir,
            &task_id,
            self.state_name,
            suffix.as_deref(),
        ));
        match record {
            Some(record) => {
                record.proves_successful_work(&task_id, self.state_name, self.moves)
            }
            None => !self.has_prior_state_visit(),
        }
    }

    /// A departure proves a prior visit, including a visit that began before
    /// the ledger. Two arrivals also prove re-entry in a partial history.
    /// A lone arrival can be the first visit, even after other task moves.
    // §FS-rhei-agents.3.2
    fn has_prior_state_visit(&self) -> bool {
        let owning = self.artifact_root.join("runtime/state-transitions.log");
        let running = self.runtime_dir.join("state-transitions.log");
        let task_id = self.task.id.to_string();
        let mut entries = 0;
        for path in std::iter::once(&owning).chain((running != owning).then_some(&running)) {
            let Ok(raw) = fs::read_to_string(path) else { continue };
            for (id, from, to) in parse_ledger(&raw) {
                if id != task_id {
                    continue;
                }
                if normalized_state_name(&from, self.machine) == self.state_name {
                    return true;
                }
                if normalized_state_name(&to, self.machine) == self.state_name {
                    entries += 1;
                    if entries > 1 {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Whether this invocation still owes the ticket something: successful
    /// work eligible for this visit, a declared `outputs:` artifact of *its*
    /// identity, or — when the edge this exit would select finishes the ticket
    /// — its own result.
    ///
    /// The current caller's exit is still held by its post-exit path. Reading a
    /// persisted exit here answers only whether on-disk work can be reused
    /// rather than spawned again, at both moments this condition is asked.
    // §FS-rhei-agents.3.2 §FS-rhei-states.3.3
    fn invocation_is_pending(&self, resolved: &ResolvedAgent) -> bool {
        if !self.state_def.outputs.is_empty() && !self.work_is_eligible_for_visit(resolved) {
            return true;
        }
        if !state_outputs_exist_for_resolved_invocation(
            self.artifact_root,
            self.task,
            self.state_name,
            self.current_state_raw,
            self.machine,
            self.metadata,
            self.state_def,
            resolved,
        ) {
            return true;
        }
        if !self.finishes_ticket {
            return false;
        }
        let identity = fanout_result_identity(
            Some(self.state_def),
            resolved.target.as_ref(),
            resolved.model.as_deref(),
        );
        let path = invocation_result_file_path(
            self.artifact_root,
            &self.task.id.to_string(),
            ResultInvocation {
                state: self.state_name,
                visit_count: self.visit_count,
                identity: identity.as_deref(),
            },
        );
        !file_has_content(&path)
    }
}

/// Whether any invocation of this state still owes the ticket something.
///
/// One invocation exiting is not the state finishing: the run must not select a
/// transition while a sibling is still to write. Gating on declared outputs
/// alone let a fan-out state with none advance on the first exit, with the merge
/// then running once per invocation and, on a terminal edge, once per invocation
/// that arrived after the ticket had left.
// §FS-rhei-agents.3.2 §FS-rhei-states.3.3 §FS-rhei-panta.6.2
#[allow(clippy::too_many_arguments)]
fn task_has_pending_agent_invocations(
    artifact_root: &Path,
    runtime_dir: &Path,
    task: &rhei_core::ast::Task,
    state_name: &str,
    current_state_raw: &str,
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
    state_def: &rhei_validator::StateDef,
    settings: &RheiSettings,
    opts: &RunOptions,
    selected_to: Option<&str>,
) -> MietteResult<bool> {
    let invocations = resolve_agent_invocations_for_task(
        machine,
        state_name,
        settings,
        opts,
        Some(task),
    )?;
    let completion = InvocationCompletion {
        artifact_root,
        runtime_dir,
        task,
        state_name,
        current_state_raw,
        machine,
        metadata,
        state_def,
        finishes_ticket: selected_to.is_some_and(|to| is_terminal_state(to, machine)),
        visit_count: render_visit_count(
            metadata,
            &task.id,
            state_name,
            current_state_raw,
            machine,
        ),
        moves: ticket_move_count(artifact_root, runtime_dir, &task.id.to_string()),
    };
    Ok(invocations.iter().any(|resolved| completion.invocation_is_pending(resolved)))
}

/// Which of the invocations a pass resolved for `task` it must actually spawn.
///
/// A pass skips an invocation only when the whole completion condition already
/// holds for it — the declared artifacts *and*, on a terminal edge, its own
/// result. Skipping on the declared outputs alone is what let a stalled ticket
/// be reclassified as finished a pass later; the recovery the execution loop
/// prescribes is to run the state again, and this is where that happens.
///
/// A state that declares no `outputs:` is never skipped. It has no artifact of
/// its own that could stand as proof its work was done, and the ticket's result
/// file cannot stand in for one: it is shared with every state the ticket has
/// passed through, so a result written earlier would excuse a state that has not
/// run at all.
// §FS-rhei-agents.3.2 §FS-rhei-run.3
#[allow(clippy::too_many_arguments)]
fn agent_invocations_to_spawn(
    loaded: &LoadedPlan,
    workspace_root: &Path,
    runtime_dir: &Path,
    task: &rhei_core::ast::Task,
    machine: &rhei_validator::StateMachine,
    state_name: &str,
    state_def: &rhei_validator::StateDef,
    invocations: Vec<ResolvedAgent>,
) -> Vec<ResolvedAgent> {
    if state_def.outputs.is_empty() {
        return invocations;
    }
    let current_state_raw = task.state.as_str();
    let metadata = loaded.rhei.metadata.as_ref();
    let artifact_root = loaded.task_root(&task.id.to_string(), workspace_root);
    let completion = InvocationCompletion {
        artifact_root: &artifact_root,
        runtime_dir,
        task,
        state_name,
        current_state_raw,
        machine,
        metadata,
        state_def,
        finishes_ticket: selected_forward_transition(&loaded.rhei, machine, task)
            .is_some_and(|to| is_terminal_state(&to, machine)),
        visit_count: render_visit_count(
            metadata,
            &task.id,
            state_name,
            current_state_raw,
            machine,
        ),
        moves: ticket_move_count(&artifact_root, runtime_dir, &task.id.to_string()),
    };
    invocations
        .into_iter()
        .filter(|resolved| completion.invocation_is_pending(resolved))
        .collect()
}
