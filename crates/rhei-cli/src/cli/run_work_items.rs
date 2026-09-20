// What one pass of `rhei run` has to schedule: the agent and program work
// items, the messages a parallel worker sends back about one of them, and the
// scan that turns the live ready set into those items.
//
// Its own part because collection answers a different question from execution —
// which invocations are claimable right now, before anything is spawned — and
// the sequential driver and the worker pool both start from its answer.

// §AR-source-file-size.3 §FS-rhei-run.3


/// What one pass of the agent-mode loop has accumulated so far.
///
/// Each field borrows a local of `run_agent_mode`, so the sequential driver and
/// the worker pool can live in parts of their own while still saying exactly
/// which pass counters they touch: nothing here is derived, and nothing else in
/// the pass is reachable through it.
// §FS-rhei-run.3
struct AgentPassProgress<'a> {
    advanced_any: &'a mut bool,
    agents_spawned: &'a mut u32,
    programs_spawned: &'a mut u32,
    /// Tickets whose worker finished this pass without moving them.
    stalled_tasks: &'a mut HashSet<String>,
    /// Tickets whose prompt would not compose; they must not be rescheduled.
    unpromptable_tasks: &'a mut HashSet<String>,
}

#[derive(Clone, Debug)]
struct SnapshotOverrideRunSelection {
    task_id: String,
    target_slug: String,
    /// Preload consumes the override on the scheduler thread, before spawning
    /// a worker; the binding survives later passes and refills.
    /// §FS-rhei-snapshot-operations.2
    consumed: std::cell::Cell<bool>,
}

/// Bind once per run, retaining the chosen invocation even after preload
/// consumes it; later passes must not select another override recipient.
/// §FS-rhei-snapshot-operations.2
#[derive(Default)]
struct SnapshotOverrideRun {
    selection: Option<SnapshotOverrideRunSelection>,
}

impl SnapshotOverrideRun {
    fn select(
        &mut self,
        input: &Path,
        machines: &ExecutionMachines,
        opts: &RunOptions,
        invocations: &[(String, String, String, ResolvedAgent)],
    ) -> MietteResult<Option<&SnapshotOverrideRunSelection>> {
        if self.selection.is_none() {
            self.selection =
                select_snapshot_override_run_invocation(input, machines, opts, invocations)?;
        }
        Ok(self.selection.as_ref())
    }
}

#[derive(Clone)]
struct AgentWorkItem {
    task_id_str: String,
    current_state: String,
    resolved: ResolvedAgent,
}

#[derive(Clone)]
struct ProgramWorkItem {
    task_id_str: String,
    current_state: String,
    resolved: ResolvedProgram,
}

struct ParallelAgentCompletion {
    task_id_str: String,
    state_name: String,
    started_at: std::time::SystemTime,
    /// The slot release the worker read off the finished agent, not yet
    /// emitted: a poll self-loop makes it a wait, and only the main thread
    /// selects the transition that says so. §FS-rhei-states.2.2
    release: PendingSlotRelease,
    resolved: ResolvedAgent,
    log: PathBuf,
    snapshot_preload: SnapshotPreload,
    visit_count: u64,
    /// Whether the visit this invocation belongs to has an attempt left after
    /// it. Decided at the spawn, where the budget is resolved, and read where
    /// the run says what it will do next.
    // §FS-rhei-agents.3.2.1 §FS-rhei-agents.3.2.3
    retry_outlook: RetryOutlook,
    /// The supervisor's subtree as this spawn found it, for a supervising
    /// state; `None` for every other. §FS-rhei-supervision.3.6
    subtree_before: Option<SubtreeShape>,
    /// Where this spawn recorded itself, so a withheld release edge can give
    /// back the attempt it was charged. §FS-rhei-agents.3.2.3
    spawn_record: PathBuf,
    result: MietteResult<AgentSpawnOutcome>,
    accounting_recorded: bool,
    accounting_warning: Option<String>,
    slot: rhei_tui::Slot,
}

/// A parallel agent that ran to completion, handed to the post-exit handling
/// that decides what its exit means. Split out of `ParallelAgentCompletion`
/// because the interrupted and spawn-failure arms answer for themselves.
struct ParallelAgentExit {
    task_id_str: String,
    state_name: String,
    started_at: std::time::SystemTime,
    /// Carried on from the completion, still unemitted: this is the path that
    /// selects the transition telling a wait from a failure.
    // §FS-rhei-states.2.2
    release: PendingSlotRelease,
    resolved: ResolvedAgent,
    log: PathBuf,
    snapshot_preload: SnapshotPreload,
    visit_count: u64,
    /// Whether the visit this invocation belongs to has an attempt left after
    /// it. Decided at the spawn, where the budget is resolved, and read where
    /// the run says what it will do next.
    // §FS-rhei-agents.3.2.1 §FS-rhei-agents.3.2.3
    retry_outlook: RetryOutlook,
    /// The supervisor's subtree as this spawn found it, for a supervising
    /// state; `None` for every other. §FS-rhei-supervision.3.6
    subtree_before: Option<SubtreeShape>,
    /// Where this spawn recorded itself, so a withheld release edge can give
    /// back the attempt it was charged. §FS-rhei-agents.3.2.3
    spawn_record: PathBuf,
    accounting_recorded: bool,
    outcome: AgentSpawnOutcome,
}

struct ParallelProgramCompletion {
    task_id_str: String,
    state_name: String,
    /// Whether the visit this invocation belongs to has an attempt left after
    /// it. Decided at the spawn, where the budget is resolved, and read where
    /// the run says what it will do next.
    // §FS-rhei-agents.3.2.1 §FS-rhei-agents.3.2.3
    retry_outlook: RetryOutlook,
    /// The slot release the worker read off the finished process, not yet
    /// emitted: a poll self-loop makes it a wait, and only the main thread
    /// selects the transition that says so. §FS-rhei-states.2.2
    release: PendingSlotRelease,
    result: MietteResult<ProgramSpawnOutcome>,
    slot: rhei_tui::Slot,
}

enum ParallelAgentThreadMessage {
    Completed(ParallelAgentCompletion),
    ProgramCompleted(ParallelProgramCompletion),
    Panicked { task_id_str: String, state_name: String, slot: rhei_tui::Slot },
}

struct ParallelAgentSpawned {
    task_id_str: String,
    state_name: String,
    handle: std::thread::JoinHandle<()>,
}

enum ParallelAgentSpawnOutcome {
    Spawned(ParallelAgentSpawned),
    Advanced,
    Skipped,
    /// The task's prompt could not be composed and the run was told to carry
    /// on. It must not be scheduled again, or every later pass retries it.
    // §FS-rhei-run.3: an uncomposable prompt fails its task, not the run.
    Unpromptable(String),
}

struct ParallelProgramSpawned {
    task_id_str: String,
    state_name: String,
    handle: std::thread::JoinHandle<()>,
}

enum ParallelProgramSpawnOutcome {
    Spawned(ParallelProgramSpawned),
    Skipped,
}

fn select_snapshot_override_run_invocation(
    input: &Path,
    machines: &ExecutionMachines,
    opts: &RunOptions,
    invocations: &[(String, String, String, ResolvedAgent)],
) -> MietteResult<Option<SnapshotOverrideRunSelection>> {
    if opts.snapshot_override_ref().is_none() {
        return Ok(None);
    }

    let loaded = load_plan(input)?;
    let explicit_task = opts.snapshot_task_selector();
    let mut candidates = Vec::new();
    for (task_id, _raw_state, current_state, resolved) in invocations {
        let Some(task) = find_task_by_id_str(&loaded.rhei.tasks, task_id) else {
            continue;
        };
        let machine = machines.for_task_str(task_id);
        let has_effective_contract =
            effective_snapshot_inherit(machine, task, current_state).is_some();
        let explicitly_selected_opt_out = explicit_task == Some(task_id.as_str())
            && matches!(
                task.inherits.as_ref(),
                Some(rhei_core::ast::TaskSnapshotInherit::Disabled)
            );
        if !has_effective_contract && !explicitly_selected_opt_out {
            continue;
        }
        // Candidate selection uses freshly read task metadata and the active
        // state rule; an explicit `none` survives only to report its opt-out.
        // §FS-rhei-snapshot-operations.2 §FS-rhei-plan-language.3.14
        let target_slug = snapshot_target_slug_or_err(resolved)?;
        candidates.push(SnapshotOverrideRunSelection {
            task_id: task_id.clone(),
            target_slug,
            consumed: std::cell::Cell::new(false),
        });
    }

    let mut selected = candidates.clone();
    if let Some(task_selector) = opts.snapshot_task_selector() {
        selected.retain(|candidate| candidate.task_id == task_selector);
    }
    if let Some(target_selector) = opts.snapshot_target_selector() {
        selected.retain(|candidate| candidate.target_slug == target_selector);
    }

    if selected.len() == 1 {
        return Ok(selected.pop());
    }

    let candidate_lines = format_snapshot_override_candidates(&candidates);
    if selected.is_empty() {
        return Err(miette!(
            help = snapshot_candidates_help(),
            "--from-snapshot did not match an active snapshot.inherit invocation; candidates:\n{}",
            candidate_lines
        ));
    }
    Err(miette!(
        help = snapshot_candidates_help(),
        "--from-snapshot is ambiguous; matched {} active snapshot.inherit invocations:\n{}\nretry with --task <id> and --target <slug>",
        selected.len(),
        format_snapshot_override_candidates(&selected)
    ))
}

fn format_snapshot_override_candidates(candidates: &[SnapshotOverrideRunSelection]) -> String {
    if candidates.is_empty() {
        return "  <none>".to_string();
    }
    let mut sorted = candidates.to_vec();
    sorted.sort_by(|a, b| a.task_id.cmp(&b.task_id).then_with(|| a.target_slug.cmp(&b.target_slug)));
    sorted
        .iter()
        .map(|candidate| {
            format!("  task={} target={}", candidate.task_id, candidate.target_slug)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// §FS-rhei-run.3: pending invocations predict a spawn, not their eventual edge.
fn format_dry_run_agent_spawn(
    task_id: &str,
    state: &str,
    resolved: &ResolvedAgent,
    exclusions: &ResolvedExclusions,
) -> String {
    let base = format!(
        "Would spawn: agent '{}' for Task {task_id} [{state}]",
        resolved.agent.id()
    );
    let target = match resolved_agent_target_slug(resolved) {
        Some(target_slug) => format!("{base} [target={target_slug}]"),
        None => base,
    };
    format!(
        "{target}{}",
        exclusions.dry_run_suffix(resolved.profile.deny_read.is_some())
    )
}

fn format_dry_run_agent_transition(
    task_id: &str,
    from: &str,
    to: &str,
    resolved: &ResolvedAgent,
    machine: &rhei_validator::StateMachine,
    exclusions: &ResolvedExclusions,
) -> String {
    let base = format_dry_run_transition(task_id, from, to, machine);
    let target = match resolved_agent_target_slug(resolved) {
        Some(target_slug) => format!("{base} [target={target_slug}]"),
        None => base,
    };
    format!(
        "{target}{}",
        exclusions.dry_run_suffix(resolved.profile.deny_read.is_some())
    )
}

fn agent_template_context(resolved: &ResolvedAgent) -> rhei_viz_model::TemplateContext {
    rhei_viz_model::TemplateContext {
        target: resolved.target.as_ref().map(ExecutionTarget::selector),
        target_slug: resolved.target.as_ref().map(ExecutionTarget::slug),
        model: resolved.model.clone(),
        model_provider: resolved.model_provider.clone(),
        model_name: resolved.model_name.clone().or_else(|| resolved.model.clone()),
        agent: Some(resolved.agent.id().to_string()),
        agent_mode: resolved.mode.clone(),
    }
}

fn emit_run_message(
    sink: &Arc<dyn rhei_tui::EventSink>,
    level: rhei_tui::MessageLevel,
    text: impl Into<String>,
) {
    sink.emit(rhei_tui::RunEvent::Message { level, text: text.into() });
}

#[allow(clippy::too_many_arguments)]
fn collect_ready_agent_work_items(
    loaded: &LoadedPlan,
    machines: &ExecutionMachines,
    settings: &RheiSettings,
    opts: &RunOptions,
    workspace_root: &Path,
    runtime_dir: &Path,
    active_task_ids: &HashSet<String>,
    active_nonconcurrent_states: &HashSet<String>,
) -> MietteResult<(Vec<AgentWorkItem>, Vec<String>)> {
    let mut agent_tasks = Vec::new();
    let mut state_claimant: HashMap<String, String> = HashMap::new();
    let mut deferred: BTreeSet<String> = BTreeSet::new();

    // §FS-rhei-panta.6.1: `--rhei` narrows candidates, not prior resolution.
    let rhei_scope = rhei_scope_set(opts.rhei_scope());
    for task in narrow_to_rhei_scope(
        find_runnable_tasks_with_options(
            &loaded.rhei,
            &machines.set,
            &ReadySetRoots { workspace_root, task_roots: &loaded.task_roots },
            active_task_ids,
            opts,
        ),
        &rhei_scope,
    ) {
        let task_id_str = task.id.to_string();
        if active_task_ids.contains(&task_id_str) {
            continue;
        }

        let machine = machines.for_task(&task.id);
        let current_state_raw = task.state.as_str().to_string();
        let current_state = normalized_state_name(&current_state_raw, machine);
        let Some(state_def) = machine.states.get(&current_state) else {
            continue;
        };
        if state_def.program.is_some()
            || state_def.terminal
            || state_def.gating
            || opts.no_agent()
        {
            continue;
        }

        // A refill resolves from the freshly loaded task, including its full
        // execution override. §FS-rhei-run.5
        let invocations = resolve_agent_invocations_for_task(
            machine,
            &current_state,
            settings,
            opts,
            Some(task),
        )?;
        if invocations.is_empty() {
            if state_declares_autonomous_agent_work(state_def) {
                return Err(miette!(
                    help = "give the state an `agent:` or `target:` in the state machine, or run it yourself with: rhei next <plan>",
                    "no agent configured for ready state '{}'", current_state
                ));
            }
            continue;
        }

        // The rule the pass driver and the post-exit check apply: a refill
        // re-spawns an invocation whose completion condition is unmet, and skips
        // only one that is genuinely finished. §FS-rhei-agents.3.2
        let task_root = loaded.task_root(&task_id_str, workspace_root);
        let agent_runtime_dir = if machines.uses_member_local_runtime(&task_id_str) {
            task_root.join("runtime")
        } else {
            runtime_dir.to_path_buf()
        };
        let pending = agent_invocations_to_spawn(
            loaded,
            workspace_root,
            &agent_runtime_dir,
            task,
            machine,
            &current_state,
            state_def,
            invocations,
        )
        .into_iter()
        .filter(|resolved| {
            resolved_provider_deadline(
                &loaded.rhei,
                &machines.set,
                resolved,
                current_unix_secs(),
            )
            .is_none()
        })
        .collect::<Vec<_>>();

        if pending.is_empty() {
            continue;
        }

        if !opts.dry_run() {
            for resolved in &pending {
                ensure_orchestrator_timeout(resolved, &current_state)?;
            }
        }

        let is_concurrent = state_def.concurrent;
        if !is_concurrent && active_nonconcurrent_states.contains(&current_state) {
            deferred.insert(task_id_str);
            continue;
        }
        if !is_concurrent {
            match state_claimant.get(&current_state) {
                Some(claimant) if claimant == &task_id_str => {}
                Some(_) => {
                    deferred.insert(task_id_str);
                    continue;
                }
                None => {
                    state_claimant.insert(current_state.clone(), task_id_str.clone());
                }
            }
        }

        for resolved in pending {
            agent_tasks.push(AgentWorkItem {
                task_id_str: task_id_str.clone(),
                current_state: current_state.clone(),
                resolved,
            });
        }
    }

    Ok((agent_tasks, deferred.into_iter().collect()))
}

#[allow(clippy::too_many_arguments)]
fn collect_ready_program_work_items(
    loaded: &LoadedPlan,
    machines: &ExecutionMachines,
    settings: &RheiSettings,
    opts: &RunOptions,
    workspace_root: &Path,
    active_task_ids: &HashSet<String>,
    active_nonconcurrent_states: &HashSet<String>,
) -> MietteResult<(Vec<ProgramWorkItem>, Vec<String>)> {
    let mut program_tasks = Vec::new();
    let mut state_claimant: HashMap<String, String> = HashMap::new();
    let mut deferred: BTreeSet<String> = BTreeSet::new();

    // §FS-rhei-panta.6.1: `--rhei` narrows candidates, not prior resolution.
    let rhei_scope = rhei_scope_set(opts.rhei_scope());
    for task in narrow_to_rhei_scope(
        find_runnable_tasks_with_options(
            &loaded.rhei,
            &machines.set,
            &ReadySetRoots { workspace_root, task_roots: &loaded.task_roots },
            active_task_ids,
            opts,
        ),
        &rhei_scope,
    ) {
        let task_id_str = task.id.to_string();
        if active_task_ids.contains(&task_id_str) {
            continue;
        }

        let machine = machines.for_task(&task.id);
        let current_state_raw = task.state.as_str().to_string();
        let current_state = normalized_state_name(&current_state_raw, machine);
        let Some(state_def) = machine.states.get(&current_state) else {
            continue;
        };
        if state_def.program.is_none() || state_def.terminal || state_def.gating || opts.no_program()
        {
            continue;
        }

        let Some(resolved) = resolve_program(machine, &current_state, settings, opts)? else {
            continue;
        };

        let is_concurrent = state_def.concurrent;
        if !is_concurrent && active_nonconcurrent_states.contains(&current_state) {
            deferred.insert(task_id_str);
            continue;
        }
        if !is_concurrent {
            match state_claimant.get(&current_state) {
                Some(claimant) if claimant == &task_id_str => {}
                Some(_) => {
                    deferred.insert(task_id_str);
                    continue;
                }
                None => {
                    state_claimant.insert(current_state.clone(), task_id_str.clone());
                }
            }
        }

        program_tasks.push(ProgramWorkItem {
            task_id_str,
            current_state,
            resolved,
        });
    }

    Ok((program_tasks, deferred.into_iter().collect()))
}
