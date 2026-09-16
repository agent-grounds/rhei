// The `rhei run` agent-mode loop: run-wide setup, the pass loop that decides
// what each pass may claim, and the finalization that reports what the run did.
//
// The pieces a pass hands off — collecting work items, spawning and scheduling
// parallel workers, and interpreting one invocation's exit — are their own
// parts beside this one; what stays here is the order they happen in.

// §AR-source-file-size.3 §FS-rhei-run.3

/// Agent-driven execution mode: spawn coding agents for tasks.
fn run_agent_mode(
    input: &Path,
    live: &mut LiveRunContext,
    opts: &RunOptions,
    max_parallel: usize,
    initial_warnings: &[String],
    identity: &RunIdentity,
) -> MietteResult<()> {
    use rhei_tui::{MessageLevel, RunEvent, RunSummary};

    let workspace_root = run_execution_root(input);
    let runtime_dir = workspace_root.join("runtime");
    // §FS-rhei-run-report.3.1: run duration shown in the end-of-run summary.
    // §FS-rhei-run.2.7: one identity per run, computed by the caller, so the
    // report and the run descriptor name the same run.
    let run_started = identity.started;
    let run_started_wall = identity.started_wall;
    let run_id = identity.id.clone();

    let command = current_command_line();

    let (initial_total_tasks, initial_states) = {
        let loaded = load_plan(input)?;
        (
            total_task_count(&loaded.rhei),
            collect_initial_states(&loaded.rhei, &live.machines.set),
        )
    };
    // §FS-rhei-run-report.1: declared before the frontend so it drops *after* the
    // terminal is restored; the happy path disarms it once the full report is
    // written, so it only fires when the run returns early with an error.
    let mut report_guard = RunReportGuard {
        input,
        machines: live.machines.set.clone(),
        runtime_dir: runtime_dir.clone(),
        run_started,
        run_started_wall,
        run_id: run_id.clone(),
        workspace_root: workspace_root.clone(),
        command: command.clone(),
        parallel: max_parallel,
        mode: "agent",
        initial_states: initial_states.clone(),
        dry_run: opts.dry_run(),
        summary: None,
        armed: true,
    };
    // This run's copy of "the run is ending abnormally", handed to the
    // frontend below and raised by the subprocess guard on its way out.
    // §FS-rhei-run-tui.1.5.7
    let run_shutdown = RunShutdown::default();
    let frontend_parallel = max_parallel.max(1).min(u16::MAX as usize) as u16;
    let frontend = start_run_frontend(
        &workspace_root,
        input,
        &live.machines,
        opts,
        frontend_parallel,
        initial_total_tasks,
        &run_shutdown,
        identity,
    );
    // Declared after the frontend so it drops *before* it: the surface must
    // learn the run is unwinding before it decides whether to park.
    // §FS-rhei-run.3.2 §FS-rhei-run-tui.1.5.7
    let mut subprocess_guard = RunSubprocessGuard::install(run_shutdown);
    let sink = frontend.sink.clone();
    // Route leaf-helper diagnostics through the frontend for the run's duration
    // instead of letting them write straight to the terminal and corrupt the
    // TUI. §FS-rhei-run-tui.1.8
    let diag_guard = RunDiagGuard::install(sink.clone());
    // Held past the frontend drop so the end-of-run summary can read per-task
    // activity after the TUI restores the terminal. §FS-rhei-run-report.3
    let summary_sink = frontend.summary.clone();
    report_guard.summary = Some(summary_sink.clone());
    let dashboard_enabled = frontend.dashboard.is_some();
    // AR §7: present only when the dashboard is live; each spawned agent's stdin
    // is registered here so `/intervene` can stream messages to it.
    let intervene = frontend.intervene.clone();
    sink.emit(RunEvent::RunStarted {
        run_id: run_id.clone(),
        workspace: workspace_root.clone(),
        parallel: frontend_parallel,
        total_tasks: initial_total_tasks,
    });
    emit_initial_validation_warnings(&sink, initial_warnings);
    frontend.announce_dashboard();

    macro_rules! run_message {
        ($level:expr, $($arg:tt)*) => {{
            sink.emit(RunEvent::Message {
                level: $level,
                text: format!($($arg)*),
            });
        }};
    }

    macro_rules! run_info {
        ($($arg:tt)*) => {
            run_message!(MessageLevel::Info, $($arg)*);
        };
    }

    macro_rules! run_warn {
        ($($arg:tt)*) => {
            run_message!(MessageLevel::Warn, $($arg)*);
        };
    }

    let loaded = load_plan(input)?;
    let initial_terminal_count = terminal_task_count(&loaded.rhei, &live.machines.set);
    run_info!(
        "Running {} '{}' with {} task(s) ({} terminal at start).",
        if workspace::is_workspace(input) { "workspace" } else { "plan" },
        loaded.rhei.title,
        total_task_count(&loaded.rhei),
        initial_terminal_count
    );
    run_info!("Initial states: {}", format_state_counts(&loaded.rhei));

    let mut agents_spawned = 0u32;
    let mut programs_spawned = 0u32;
    let mut callback_transitions_made = 0u32;
    let mut pass = 0u32;
    // One-time notice so the gate-wait below does not spam the journal each tick.
    let mut awaiting_gate_announced = false;
    // Re-check persisted deadlines and task states in short slices without
    // repeating the same foreground notice on every slice. §FS-rhei-run.3.3
    let mut awaiting_deadline_announced: Option<u64> = None;
    // Manual-only tasks reported by a dry run; the command still exits
    // non-zero once the scan is complete. §FS-rhei-run.4
    let mut manual_only_dry_run: Vec<String> = Vec::new();
    // Set when the pass loop found nothing to schedule and at least one
    // remaining ticket needs a human, so a dry run ends the way the real run
    // does. §FS-rhei-run.4
    let mut halted_needs_human = false;
    // Left in the ready set, these would be re-picked every pass and the run
    // would never reach their siblings.
    // §FS-rhei-run.3: an uncomposable prompt fails its task, not the run.
    let mut unpromptable_tasks: HashSet<String> = HashSet::new();
    // Tickets whose worker finished this pass without moving them. Both modes
    // refill from the live ready set and would re-pick them at once; a stall
    // takes one ticket out of the pass, it does not end the run. §FS-rhei-run.3

    // Keyed by ticket, not by (ticket, invocation), and that is the right grain
    // even under fan-out: the completion condition is per invocation, so one
    // invocation failing means the state cannot complete this pass however its
    // siblings fare. Re-spawning them would only redo work the ticket cannot use
    // until the failed invocation is retried, which is what the next pass is for.
    let mut stalled_tasks: HashSet<String> = HashSet::new();
    // Anything advanced since `stalled_tasks` was last emptied? A pass ends when
    // every claimable ticket advanced or stalled; one that moved something earns
    // the stalled ones another pass. §FS-rhei-run.3
    let mut progress_since_stall_reset = false;
    // §FS-rhei-panta.6.1: `--rhei` narrows candidates, not prior resolution.
    let rhei_scope = rhei_scope_set(opts.rhei_scope());
    if rhei_scope.is_some() {
        // The pre-launch stdout scope report is hidden while the TUI holds
        // the alternate screen, so the journal repeats it where an
        // interactive run can see it. §FS-rhei-panta.6
        run_info!("Scope: narrowed to {}", scope_label(&rhei_scope));
    }

    loop {
        // Schedule nothing new once the run is interrupted; the in-flight
        // invocations have already ended themselves. §FS-rhei-run.3.2
        if interrupt_requested() {
            // Through the journal, not stderr: when no subprocess is in flight
            // the operator may still be looking at a live TUI.
            // §FS-rhei-run-tui.1.8
            if let Some(notice) = take_interruption_announcement() {
                run_warn!("{notice}");
            }
            break;
        }
        // This is both the ordinary graph reload and the admission checkpoint.
        // A final producer action therefore becomes visible before the next
        // scheduling or stopping decision. §FS-rhei-panta.6.2 §FS-rhei-run.3
        let (loaded, admitted) = live.checkpoint(input, &workspace_root, opts, identity)?;
        if !admitted.is_empty() {
            run_info!("Admitted {} new rhei member(s): {}", admitted.len(), admitted.join(", "));
            report_guard.machines = live.machines.set.clone();
        }
        let machines = &live.machines;
        let settings = &live.settings;
        // §AR-rhei-panta.5: every look at this pass's ready set — the scan, the
        // held-ticket pass, and the halt report that explains what it refused —
        // resolves artifacts under the roots the loaded plan gives its tickets.
        let roots =
            ReadySetRoots { workspace_root: &workspace_root, task_roots: &loaded.task_roots };
        let ready = narrow_to_rhei_scope(
            find_runnable_tasks(&loaded.rhei, &machines.set, &roots, &HashSet::new()),
            &rhei_scope,
        );
        if ready.is_empty() {
            if !opts.dry_run() {
                // Interactive TUI: stay alive only when human gates are the
                // remaining blocker, so unrelated stuck work still reaches the
                // normal halt/error path. §FS-rhei-run-tui.1.5.5
                if opts.waits_for_human_gates(frontend.is_tui)
                    && should_wait_for_human_gate(&loaded.rhei, &machines.set, &rhei_scope)
                {
                    if !awaiting_gate_announced {
                        run_info!("{}", awaiting_gate_notice(frontend.is_tui));
                        awaiting_gate_announced = true;
                    }
                    // Sliced, so Ctrl+C ends the wait instead of the wait
                    // outlasting the operator. §FS-rhei-run.3.2
                    interruptible_sleep(Duration::from_millis(500));
                    continue;
                }
                if let Some(deadline) = earliest_pending_agent_deadline(
                    &loaded.rhei,
                    &machines.set,
                    settings,
                    opts,
                    &rhei_scope,
                )
                {
                    let sleep_secs = deadline.saturating_sub(current_unix_secs()).max(1);
                    if awaiting_deadline_announced != Some(deadline) {
                        run_info!(
                            "No ready tasks; sleeping {}s until the next scheduled attempt.",
                            sleep_secs
                        );
                        awaiting_deadline_announced = Some(deadline);
                    }
                    // A poll deadline is minutes away; the token must not wait
                    // it out. §FS-rhei-run.3.2
                    interruptible_sleep(
                        Duration::from_secs(sleep_secs).min(Duration::from_millis(500)),
                    );
                    continue;
                }
            }
            // Nothing schedulable on the first pass: without this the loop
            // exits having explained nothing, and a dry run reported success
            // on a project the real run halts on. §FS-rhei-run.4
            if pass == 0 {
                // The same roots the ready-set scan just used, so a halt line
                // names the file it looked for. §AR-rhei-panta.5
                let (lines, needs_human) =
                    halted_task_report(&loaded.rhei, &machines.set, &rhei_scope, input, &roots);
                if !lines.is_empty() {
                    run_info!("\nNothing to schedule. Why each remaining ticket is not moving:");
                    for line in &lines {
                        run_info!("  {line}");
                    }
                }
                halted_needs_human = needs_human;
            }
            break;
        }
        // Made progress this pass; re-arm the gate-wait notice for any later gate.
        awaiting_gate_announced = false;

        pass += 1;
        // A ticket only counts as newly out of the running when it stepped out
        // here: a pass may continue past one, and only when it learned
        // something. §FS-rhei-run.3
        let stalled_before_pass = stalled_tasks.len();
        let unpromptable_before_pass = unpromptable_tasks.len();
        let terminal_count = terminal_task_count(&loaded.rhei, &machines.set);
        sink.emit(RunEvent::PassStarted {
            pass,
            ready: ready.iter().map(|t| t.id.to_string()).collect(),
        });
        run_info!(
            "\nPass {}: {} ready, {} terminal, {} total.",
            pass,
            ready.len(),
            terminal_count,
            total_task_count(&loaded.rhei)
        );
        run_info!("Ready: {}", format_ready_tasks(&ready));
        // A ticket someone already claimed is ready but unschedulable; saying
        // nothing made it look like it was not ready at all. §FS-rhei-run.3
        let held =
            narrow_to_rhei_scope(find_held_tasks(&loaded.rhei, &machines.set, &roots), &rhei_scope);
        if !held.is_empty() {
            run_info!("Held by an assignee, so not scheduled: {}", format_held_tasks(&held));
        }
        // §FS-rhei-supervision.3.4: a dry run is what an author reads to
        // understand a machine, and the barrier is otherwise invisible in it.
        if opts.dry_run() {
            for line in format_supervisor_holds(&loaded.rhei, &machines.set, &rhei_scope) {
                run_info!("{line}");
            }
        }

        // Collect tasks that can be advanced autonomously.
        let plan_title = loaded.rhei.title.clone();
        let mut agent_tasks: Vec<(String, String, String, ResolvedAgent)> = Vec::new();
        let mut program_tasks: Vec<(String, String, String, ResolvedProgram)> = Vec::new();
        // The `bool` is whether this advance fires in agent mode with no
        // invocation left to run, so the release edge answers to
        // §FS-rhei-supervision.3.6 before it fires.
        let mut callback_tasks: Vec<(String, String, String, bool)> = Vec::new();

        for task in &ready {
            let task_id_str = task.id.to_string();
            // A ticket that already stalled in this pass is out of the running
            // until the next one, whichever mode is driving. §FS-rhei-run.3
            if stalled_tasks.contains(&task_id_str) {
                continue;
            }
            // The ticket's own machine governs its advance. §DA-per-rhei-state-machines
            let machine = machines.for_task(&task.id);
            let current_state_raw = task.state.as_str().to_string();
            let current_state = normalized_state_name(&current_state_raw, machine);

            // Check for gating state.
            if machine.states.get(&current_state).map(|d| d.gating).unwrap_or(false) {
                run_info!(
                    "Task {} is in gating state '{}'. Waiting for human action.",
                    task_id_str,
                    current_state
                );
                continue;
            }

            let state_def = machine
                .states
                .get(&current_state)
                .ok_or_else(|| miette!(
                    help = internal_error_help(),
                    "state '{}' missing from loaded machine", current_state
                ))?;

            if state_def.program.is_some() {
                if opts.no_program() {
                    callback_tasks.push((task_id_str, current_state_raw, current_state, false));
                    continue;
                }

                if let Some(resolved) = resolve_program(machine, &current_state, settings, opts)? {
                    program_tasks.push((task_id_str, current_state_raw, current_state, resolved));
                }
            } else {
                let invocations = resolve_agent_invocations_for_task(
                    machine,
                    &current_state,
                    settings,
                    opts,
                    Some(task),
                )?;
                if invocations.is_empty() {
                    // Unrestricted project runs use this scheduler so a
                    // member admitted later can introduce agent work. Keep
                    // the existing callback-only behavior for a project
                    // whose current task has no agent configured.
                    let project_callback_fallback =
                        loaded.is_panta_project() && opts.rhei_scope().is_empty();
                    if opts.no_agent() || project_callback_fallback {
                        callback_tasks.push((task_id_str, current_state_raw, current_state, false));
                        continue;
                    }
                    // Surface every remediation slot from the resolution order:
                    // `defaults.agent`, the state's
                    // `agent`, `models.<id>.default_agent`, and `--agent`.
                    // Mention the resolved model id when one is set so
                    // operators can locate `models.<id>.default_agent`.

                    // §FS-rhei-agents.1.4: Explain unresolved agent slots.
                    let resolved_model = state_def
                        .model
                        .clone()
                        .or_else(|| settings.defaults.model.clone())
                        .or_else(|| settings.model.clone());
                    // The settings file the project resolves, so an operator
                    // on the deprecated home is not sent to a path that would
                    // shadow it. §FS-rhei-agents.1.1
                    let project_settings = settings.project_settings_file.relative_path();
                    let model_remediation = match &resolved_model {
                        Some(id) => format!(
                            "models.{id}.default_agent in {}/{}",
                            workspace_root.display(),
                            project_settings
                        ),
                        None => "models.<id>.default_agent (in settings.json)".to_string(),
                    };
                    let header = match &resolved_model {
                        Some(id) => format!("no agent configured for model '{id}'."),
                        None => "no agent configured.".to_string(),
                    };
                    return Err(miette!(
                        help = run_report_help(),
                        "{header}\n\nSet one of:\n  \u{2022} defaults.agent in {}/{} or ~/.config/rhei/settings.json\n  \u{2022} the state's `agent:` in states.yaml\n  \u{2022} {model_remediation}\n  \u{2022} --agent <AGENT> on the rhei run command line (e.g. rhei run {} --agent claude-code)\n\nBuilt-in agents: claude-code, codex, gemini, cursor, kilocode, pi",
                        workspace_root.display(),
                        project_settings,
                        input.display()
                    ));
                }

                // The whole completion condition decides this, not the
                // declared outputs alone: an invocation that wrote its outputs
                // but not the result has not finished. §FS-rhei-agents.3.2
                let pending = agent_invocations_to_spawn(
                    &loaded,
                    &workspace_root,
                    task,
                    machine,
                    &current_state,
                    state_def,
                    invocations,
                );

                if pending.is_empty() {
                    callback_tasks.push((task_id_str, current_state_raw, current_state, true));
                    continue;
                }

                // Orchestrator Completion Authority: every invocation that
                // `rhei run` will actually spawn must resolve to a finite
                // timeout so that a non-returning agent cannot block forever.
                // Invocations whose outputs already exist have been filtered
                // out above and do not need a timeout.

                // §FS-rhei-agents.3.1 §FS-rhei-agents.3.2: Require timeout.
                if !opts.dry_run() {
                    for resolved in &pending {
                        ensure_orchestrator_timeout(resolved, &current_state)?;
                    }
                }

                for resolved in pending {
                    if resolved_provider_deadline(
                        &loaded.rhei,
                        &machines.set,
                        &resolved,
                        current_unix_secs(),
                    )
                    .is_some()
                    {
                        continue;
                    }
                    agent_tasks.push((
                        task_id_str.clone(),
                        current_state_raw.clone(),
                        current_state.clone(),
                        resolved,
                    ));
                }
            }
        }

        let mut advanced_any = false;
        let run_programs_in_worker_pool = max_parallel != 1;

        // Handle callback-only tasks first (fast, synchronous).
        for (task_id_str, current_state_raw, current_state, judge_release) in &callback_tasks {
            let loaded = load_plan(input)?;
            let target_id = parse_task_id(task_id_str);
            let machine = machines.for_task_str(task_id_str);
            let callback_paths = machines.callbacks_for_str(task_id_str);
            let task = match find_task_by_id(&loaded.rhei.tasks, &target_id) {
                Some(t) => t,
                None => continue,
            };
            if let Some(to_state) = manual_initial_terminal_transition(task, &loaded.rhei, machine)? {
                // A dry run reports and keeps scanning; only a real run must
                // stop before touching the task. §FS-rhei-run.4
                if opts.dry_run() {
                    let line = format_dry_run_manual_only(task_id_str, current_state, &to_state);
                    run_info!("{}", line);
                    manual_only_dry_run.push(line);
                    continue;
                }
                return Err(miette!(
                    help = run_report_help(),
                    "Task {} is in manual-only initial state '{}' with terminal transition to '{}'; \
                     use `rhei next`, do the task, then `rhei complete` instead of `rhei run`.",
                    task_id_str,
                    current_state,
                    to_state
                ));
            }
            let next_to = find_next_transition(task, &loaded.rhei, machine)?;
            let Some(to_state) = next_to else { continue };

            // This advance fires the release edge with no visit behind it — the
            // outputs a held visit left routed it here — so it answers the same
            // test, in a dry run too. §FS-rhei-supervision.3.6
            let withheld = withhold_empty_supervising_advance(
                *judge_release,
                &workspace_root,
                &loaded,
                &machines.set,
                &target_id,
                current_state,
            );

            if opts.dry_run() {
                let line = format_dry_run_advance(
                    withheld.is_some(),
                    task_id_str,
                    current_state_raw,
                    current_state,
                    &to_state,
                    machine,
                );
                run_info!("{}", line);
                continue;
            }

            if let Some(warning) = withheld {
                run_warn!("{}", warning);
                // Did not move; the rest of the pass must look elsewhere, and
                // the run's own stall bound ends it. §FS-rhei-run.3
                stalled_tasks.insert(task_id_str.clone());
                continue;
            }

            if record_poll_self_loop_if_needed(
                &loaded,
                input,
                machine,
                task,
                current_state,
                &to_state,
            )? {
                run_info!(
                    "Task {} poll self-loop scheduled next attempt from '{}'",
                    task_id_str,
                    current_state_raw
                );
                advanced_any = true;
                callback_transitions_made += 1;
                continue;
            }

            let task_ids_before: BTreeSet<String> =
                loaded.rhei.tasks.iter().map(|existing| existing.id.to_string()).collect();
            let route = loaded.task_route(task_id_str, input);
            // Callback-only advancement: no subprocess ran here, so a terminal
            // edge records the engine's own account unless a callback already
            // wrote a result, which wins. §FS-rhei-run.3
            match execute_callback_only_transition(
                TransitionFiles { task_file: &route.task_file, metadata_file: &route.metadata_file, metadata_id: &route.metadata_id, artifact_root: &route.execution_root, artifact_id: task_id_str },
                callback_paths,
                machine,
                &route.local_id,
                current_state,
                &to_state,
                opts.no_callbacks(),
                &runtime_dir,
            ) {
                Ok(effective_to) => {
                    run_info!(
                        "Task {} transitioned: '{}' \u{2192} '{}'",
                        task_id_str,
                        current_state_raw,
                        effective_to
                    );
                    advanced_any = true;
                    callback_transitions_made += 1;
                    let reloaded = load_plan(input)?;
                    let discovered = newly_discovered_tasks(&task_ids_before, &reloaded.rhei.tasks);
                    if !discovered.is_empty() {
                        run_info!(
                            "  Workspace expanded: discovered {} new task(s): {}",
                            discovered.len(),
                            discovered.join(", ")
                        );
                    }
                }
                Err(err) => {
                    run_warn!("warning: failed to advance Task {}: {}", task_id_str, err);
                }
            }
        }

        // Tickets a same-state claimant pushed to a later pass. They are still
        // claimable work, so the pass must not end while one is waiting on a
        // sibling that has since stalled. §FS-rhei-run.3
        let mut deferred_tasks: BTreeSet<String> = BTreeSet::new();

        let program_tasks = {
            let mut filtered: Vec<(String, String, String, ResolvedProgram)> = Vec::new();
            let mut state_claimant: HashMap<String, String> = HashMap::new();
            let mut deferred: BTreeSet<String> = BTreeSet::new();
            for entry in program_tasks {
                let is_concurrent = machines
                    .for_task_str(&entry.0)
                    .states
                    .get(&entry.2)
                    .map(|d| d.concurrent)
                    .unwrap_or(false);
                if is_concurrent {
                    filtered.push(entry);
                    continue;
                }
                match state_claimant.get(&entry.2) {
                    Some(claimant) if claimant == &entry.0 => filtered.push(entry),
                    Some(_) => {
                        deferred.insert(entry.0);
                    }
                    None => {
                        state_claimant.insert(entry.2.clone(), entry.0.clone());
                        filtered.push(entry);
                    }
                }
            }
            if !deferred.is_empty() {
                let deferred_vec: Vec<String> = deferred.iter().cloned().collect();
                run_info!(
                    "Deferred {} task(s) in non-concurrent states to a later pass: {}",
                    deferred_vec.len(),
                    deferred_vec.join(", ")
                );
                sink.emit(RunEvent::TasksDeferred { pass, tasks: deferred_vec });
            }
            deferred_tasks.extend(deferred);
            filtered
        };

        if !program_tasks.is_empty() && !run_programs_in_worker_pool {
            if opts.dry_run() {
                for (task_id_str, current_state_raw, current_state, resolved) in &program_tasks {
                    let loaded = load_plan(input)?;
                    let target_id = parse_task_id(task_id_str);
                    let machine = machines.for_task_str(task_id_str);
                    if let Some(task) = find_task_by_id(&loaded.rhei.tasks, &target_id) {
                        // A dry run prints the edge; how it matched decides
                        // nothing it prints. §FS-rhei-run.4
                        if let Some(route) = find_program_exit_transition(
                            machine,
                            loaded.rhei.metadata.as_ref(),
                            task,
                            current_state,
                            0,
                        )? {
                            run_info!(
                                "{}",
                                format_dry_run_transition(
                                    task_id_str,
                                    current_state_raw,
                                    &route.to,
                                    machine,
                                )
                            );
                        }
                    }
                    let _ = resolved;
                }
                sink.emit(RunEvent::PassEnded { pass, progressed: false });
                break;
            }

            let mut progress = AgentPassProgress {
                advanced_any: &mut advanced_any,
                agents_spawned: &mut agents_spawned,
                programs_spawned: &mut programs_spawned,
                stalled_tasks: &mut stalled_tasks,
                unpromptable_tasks: &mut unpromptable_tasks,
            };
            run_sequential_program_work_items(
                &program_tasks,
                &plan_title,
                input,
                machines,
                settings,
                opts,
                &workspace_root,
                &sink,
                &mut progress,
            )?;
        }

        if agent_tasks.is_empty() && (program_tasks.is_empty() || !run_programs_in_worker_pool) {
            if !advanced_any {
                if opts.dry_run() {
                    sink.emit(RunEvent::PassEnded { pass, progressed: false });
                    break;
                }
                // Nothing left that has not already stalled: the pass is over.
                // If it moved anything at all, the stalled tickets earn a fresh
                // pass rather than ending the run. §FS-rhei-run.3
                if progress_since_stall_reset && !stalled_tasks.is_empty() {
                    stalled_tasks.clear();
                    progress_since_stall_reset = false;
                    sink.emit(RunEvent::PassEnded { pass, progressed: false });
                    continue;
                }
                if let Some(deadline) = earliest_pending_agent_deadline(
                    &loaded.rhei,
                    &machines.set,
                    settings,
                    opts,
                    &rhei_scope,
                ) {
                    let sleep_secs = deadline.saturating_sub(current_unix_secs()).max(1);
                    if awaiting_deadline_announced != Some(deadline) {
                        run_info!(
                            "No ready tasks; sleeping {}s until the next scheduled attempt.",
                            sleep_secs
                        );
                        awaiting_deadline_announced = Some(deadline);
                    }
                    stalled_tasks.clear();
                    progress_since_stall_reset = false;
                    interruptible_sleep(
                        Duration::from_secs(sleep_secs).min(Duration::from_millis(500)),
                    );
                    continue;
                }
                run_info!("No program, agent, or callback-only tasks could advance.");
                sink.emit(RunEvent::PassEnded { pass, progressed: false });
                break;
            }
            progress_since_stall_reset = true;
            sink.emit(RunEvent::PassEnded { pass, progressed: true });
            continue;
        }

        // Enforce concurrent-state scheduling: for states without
        // `concurrent: true`, at most one task may be active in that state
        // per pass. Fanout invocations from the same task (via `all_targets`
        // / `all_models`) are always kept together. Deferred tasks are
        // naturally re-considered on the next pass.
        let agent_tasks = {
            let mut filtered: Vec<(String, String, String, ResolvedAgent)> = Vec::new();
            let mut state_claimant: HashMap<String, String> = HashMap::new();
            let mut deferred: BTreeSet<String> = BTreeSet::new();
            for entry in agent_tasks {
                if unpromptable_tasks.contains(&entry.0) {
                    continue;
                }
                let is_concurrent = machines
                    .for_task_str(&entry.0)
                    .states
                    .get(&entry.2)
                    .map(|d| d.concurrent)
                    .unwrap_or(false);
                if is_concurrent {
                    filtered.push(entry);
                    continue;
                }
                match state_claimant.get(&entry.2) {
                    Some(claimant) if claimant == &entry.0 => filtered.push(entry),
                    Some(_) => {
                        deferred.insert(entry.0);
                    }
                    None => {
                        state_claimant.insert(entry.2.clone(), entry.0.clone());
                        filtered.push(entry);
                    }
                }
            }
            if !deferred.is_empty() {
                let deferred_vec: Vec<String> = deferred.iter().cloned().collect();
                run_info!(
                    "Deferred {} task(s) in non-concurrent states to a later pass: {}",
                    deferred_vec.len(),
                    deferred_vec.join(", ")
                );
                sink.emit(RunEvent::TasksDeferred { pass, tasks: deferred_vec });
            }
            deferred_tasks.extend(deferred);
            filtered
        };

        // Determine which task ids to schedule this pass. `--parallel`
        // counts tasks; fanout invocations for a selected task stay together.
        let task_limit = if max_parallel == 0 { usize::MAX } else { max_parallel };
        let mut selected_task_ids = HashSet::new();
        let mut batch: Vec<(String, String, String, ResolvedAgent)> = Vec::new();
        for entry in &agent_tasks {
            if selected_task_ids.contains(&entry.0) {
                batch.push(entry.clone());
            } else if selected_task_ids.len() < task_limit {
                selected_task_ids.insert(entry.0.clone());
                batch.push(entry.clone());
            }
        }
        let batch_size = batch.len();
        let snapshot_override_selection =
            select_snapshot_override_run_invocation(machines, opts, &agent_tasks)?;

        if opts.dry_run() {
            if run_programs_in_worker_pool {
                for (task_id_str, current_state_raw, current_state, resolved) in &program_tasks {
                    let loaded = load_plan(input)?;
                    let target_id = parse_task_id(task_id_str);
                    let machine = machines.for_task_str(task_id_str);
                    if let Some(task) = find_task_by_id(&loaded.rhei.tasks, &target_id) {
                        // A dry run prints the edge; how it matched decides
                        // nothing it prints. §FS-rhei-run.4
                        if let Some(route) = find_program_exit_transition(
                            machine,
                            loaded.rhei.metadata.as_ref(),
                            task,
                            current_state,
                            0,
                        )? {
                            run_info!(
                                "{}",
                                format_dry_run_transition(
                                    task_id_str,
                                    current_state_raw,
                                    &route.to,
                                    machine,
                                )
                            );
                        }
                    }
                    let _ = resolved;
                }
            }
            for (task_id_str, current_state_raw, current_state, resolved) in &batch {
                let loaded = load_plan(input)?;
                let target_id = parse_task_id(task_id_str);
                let machine = machines.for_task_str(task_id_str);
                if let Some(task) = find_task_by_id(&loaded.rhei.tasks, &target_id) {
                    if let Some(to_state) = find_next_transition(task, &loaded.rhei, machine)? {
                        run_info!(
                            "{}",
                            format_dry_run_agent_transition(
                                task_id_str,
                                current_state_raw,
                                &to_state,
                                resolved,
                                machine,
                            )
                        );
                    }
                }
                let _ = current_state;
            }
            sink.emit(RunEvent::PassEnded { pass, progressed: false });
            break;
        }

        // Spawn agents (sequential or parallel).
        if batch_size == 1 && (program_tasks.is_empty() || !run_programs_in_worker_pool) {
            // Sequential: spawn one agent at a time. Every way out of this
            // ticket's turn lands on the shared pass tail below, so one ticket
            // giving up never skips the decision about the pass. §FS-rhei-run.3
            let mut progress = AgentPassProgress {
                advanced_any: &mut advanced_any,
                agents_spawned: &mut agents_spawned,
                programs_spawned: &mut programs_spawned,
                stalled_tasks: &mut stalled_tasks,
                unpromptable_tasks: &mut unpromptable_tasks,
            };
            run_sequential_agent_invocation(
                &batch[0],
                input,
                machines,
                settings,
                opts,
                &workspace_root,
                &runtime_dir,
                &run_id,
                snapshot_override_selection.as_ref(),
                &sink,
                intervene.as_ref(),
                &mut progress,
            )?;
        } else {
            let mut progress = AgentPassProgress {
                advanced_any: &mut advanced_any,
                agents_spawned: &mut agents_spawned,
                programs_spawned: &mut programs_spawned,
                stalled_tasks: &mut stalled_tasks,
                unpromptable_tasks: &mut unpromptable_tasks,
            };
            run_agent_worker_pool(
                &batch,
                &program_tasks,
                run_programs_in_worker_pool,
                task_limit,
                frontend_parallel,
                pass,
                input,
                live,
                opts,
                &workspace_root,
                &runtime_dir,
                &run_id,
                snapshot_override_selection.as_ref(),
                &sink,
                intervene.as_ref(),
                &mut progress,
                identity,
            )?;
            report_guard.machines = live.machines.set.clone();
        }

        sink.emit(RunEvent::PassEnded { pass, progressed: advanced_any });

        if advanced_any {
            progress_since_stall_reset = true;
            continue;
        }
        // Nothing moved, but a stalled ticket's siblings are still claimable:
        // keep going while one has not been tried. Requiring a *new* stall
        // bounds it — each turn takes one more ticket out. §FS-rhei-run.3
        let newly_stalled = stalled_tasks.len() > stalled_before_pass
            || unpromptable_tasks.len() > unpromptable_before_pass;
        let claimable = |id: &String| {
            !stalled_tasks.contains(id) && !unpromptable_tasks.contains(id)
        };
        let more_claimable = agent_tasks.iter().any(|entry| claimable(&entry.0))
            || program_tasks.iter().any(|entry| claimable(&entry.0))
            || deferred_tasks.iter().any(claimable);
        if newly_stalled && more_claimable {
            continue;
        }
        // Every claimable ticket has now advanced or stalled. A pass that moved
        // something earns the stalled ones another try; one that moved nothing
        // is where the run ends. §FS-rhei-run.3
        if progress_since_stall_reset && !stalled_tasks.is_empty() {
            stalled_tasks.clear();
            progress_since_stall_reset = false;
            continue;
        }
        let loaded = load_plan(input)?;
        if let Some(deadline) = earliest_pending_agent_deadline(
            &loaded.rhei,
            &machines.set,
            settings,
            opts,
            &rhei_scope,
        ) {
            let sleep_secs = deadline.saturating_sub(current_unix_secs()).max(1);
            if awaiting_deadline_announced != Some(deadline) {
                run_info!(
                    "No ready tasks; sleeping {}s until the next scheduled attempt.",
                    sleep_secs
                );
                awaiting_deadline_announced = Some(deadline);
            }
            stalled_tasks.clear();
            progress_since_stall_reset = false;
            interruptible_sleep(
                Duration::from_secs(sleep_secs).min(Duration::from_millis(500)),
            );
            continue;
        }
        break;
    }

    // Read once, here, and used for every statement the run makes about
    // itself: a signal arriving later — while the TUI is parked on its
    // finished screen — did not cut this loop short. §FS-rhei-run.3.2
    let interrupted_run = interrupted_by_signal();

    // Say plainly that the run stopped, so the summary below is not read as a
    // finished run. §FS-rhei-run.3.2
    if interrupted_run {
        run_warn!(
            "\nRun interrupted: no further work was scheduled, and interrupted \
             invocations left their tickets in the state they were worked in."
        );
    }

    // Print summary.
    let (terminal_count, total_tasks) = if opts.dry_run() {
        // Spec §Dry-Run Output: final line reads "Dry run complete - no
        // agents were spawned." Programs are also skipped under --dry-run,
        // but the wording matches the agent-spec example so existing
        // tooling that greps for this exact phrase keeps working.
        run_info!("\nDry run complete - no agents were spawned.");
        if !manual_only_dry_run.is_empty() {
            return Err(manual_only_dry_run_error(&manual_only_dry_run));
        }
        // The real run halts here; so must the prediction of it.
        // §FS-rhei-run.4
        if halted_needs_human {
            return Err(dry_run_halt_error());
        }
        (0usize, 0usize)
    } else if agents_spawned == 0 && programs_spawned == 0 {
        if callback_transitions_made == 0 {
            let loaded = load_plan(input)?;
            run_info!(
                "{}",
                no_advancement_summary(&loaded.rhei, &live.machines.set, &rhei_scope)
            );
            (0usize, 0usize)
        } else {
            let loaded = load_plan(input)?;
            let terminal_count = terminal_task_count(&loaded.rhei, &live.machines.set);
            let total_tasks = total_task_count(&loaded.rhei);
            // An interrupted run did not complete; saying so twice — once as
            // a warning and once as "Run complete" — is worse than either.
            // §FS-rhei-run.3.2
            if interrupted_run {
                run_info!(
                    "\nRun interrupted after {} callback transition(s); {}/{} tasks in terminal state.",
                    callback_transitions_made,
                    terminal_count,
                    total_tasks
                );
            } else {
                run_info!(
                    "\nRun complete: {} callback transition(s), {}/{} tasks in terminal state.",
                    callback_transitions_made,
                    terminal_count,
                    total_tasks
                );
            }
            run_info!("Final states: {}", format_state_counts(&loaded.rhei));
            let mut tasks = Vec::new();
            collect_plan_tasks(&loaded.rhei.tasks, &mut tasks);
            for task in tasks {
                run_info!("  - {} [{}]", format_task_label(task), task.state);
            }
            (terminal_count, total_tasks)
        }
    } else {
        let loaded = load_plan(input)?;
        let terminal_count = terminal_task_count(&loaded.rhei, &live.machines.set);
        let total_tasks = total_task_count(&loaded.rhei);
        // §FS-rhei-run.3.2: the run stopped; it did not complete.
        if interrupted_run {
            run_info!(
                "\nRun interrupted after {} agent(s), {} program(s) spawned; {}/{} tasks in terminal state.",
                agents_spawned,
                programs_spawned,
                terminal_count,
                total_tasks
            );
        } else {
            run_info!(
                "\nRun complete: {} agent(s), {} program(s) spawned, {}/{} tasks in terminal state.",
                agents_spawned,
                programs_spawned,
                terminal_count,
                total_tasks
            );
        }
        run_info!("Final states: {}", format_state_counts(&loaded.rhei));
        let mut tasks = Vec::new();
        collect_plan_tasks(&loaded.rhei.tasks, &mut tasks);
        for task in tasks {
            run_info!("  - {} [{}]", format_task_label(task), task.state);
        }
        (terminal_count, total_tasks)
    };

    let accounting = if opts.dry_run() {
        None
    } else {
        // §FS-rhei-cost-accounting.7: RunFinished carries available run totals.
        match load_plan(input) {
            Ok(loaded) => match regenerate_accounting_indexes(&workspace_root, &loaded.rhei) {
                Ok(summary) => summary,
                Err(err) => {
                    run_warn!("  warning: failed to finalize accounting rollups: {}", err);
                    None
                }
            },
            Err(_) => None,
        }
    };

    sink.emit(RunEvent::RunFinished {
        summary: RunSummary {
            agents_spawned,
            programs_spawned,
            terminal_tasks: terminal_count,
            total_tasks,
            // §FS-rhei-run-report.2.1: this run's own rollup is the strip's
            // headline; the workspace lifetime total rides beside it.
            accounting: accounting.as_ref().map(|rollup| rollup.run.clone()),
            workspace_accounting: accounting.and_then(|rollup| rollup.workspace).map(Box::new),
        },
    });
    // The loop reached the point where it writes a report, so the finished
    // surface keeps its operator. §FS-rhei-run-tui.1.5.7
    subprocess_guard.finished();
    frontend.write_frozen_dashboard();
    drop(diag_guard);
    drop(sink);
    drop(frontend);

    // §FS-rhei-run-report.1/.3: write the durable report (skipped under --dry-run,
    // §3.5), print the console summary or `Report:` pointer (§3.4), then disarm the
    // guard so its fallback only fires on an early error.
    emit_run_report(
        input,
        &live.machines.set,
        &summary_sink,
        &runtime_dir,
        RunStats {
            agents_spawned,
            programs_spawned,
            callback_only: callback_transitions_made,
            duration: Some(run_started.elapsed()),
            dashboard: frozen_dashboard_relative_path(
                dashboard_enabled,
                &runtime_dir,
                &workspace_root,
            ),
            run_id,
            started_at: Some(run_started_wall),
            workspace_root: workspace_root.clone(),
            command,
            parallel: max_parallel,
            mode: "agent",
            initial_states,
            dry_run: opts.dry_run(),
            interrupted: interrupted_run,
        },
    );
    report_guard.disarm();

    // An interrupted run is not a halt: it was told to stop, and the exit code
    // already names the signal. `interrupted_run`, not the token, so the halt
    // decision and the report cannot disagree. §FS-rhei-run.3.2
    if interrupted_run {
        return Ok(());
    }

    if !opts.dry_run() {
        let loaded = load_plan(input)?;
        // §FS-rhei-panta.6.1: a narrowed run halts on in-scope work only —
        // out-of-scope tickets left non-terminal are not a failure.
        if scoped_unfinished_task_exists(&loaded.rhei, &live.machines.set, &rhei_scope)
            && !remaining_work_is_only_gating_or_poll_blocked(
                &loaded.rhei,
                &live.machines.set,
                &rhei_scope,
            )
        {
            return Err(miette!(
                help = nothing_claimable_help(),
                "rhei run halted with non-terminal tasks remaining and no further advancement possible"
            ));
        }
    }

    Ok(())
}
