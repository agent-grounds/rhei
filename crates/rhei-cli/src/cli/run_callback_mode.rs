
/// Callback-only execution mode (legacy behavior, used with --no-agent).
fn run_callback_mode(
    input: &Path,
    machines: &ExecutionMachines,
    opts: &RunOptions,
    max_parallel: usize,
    initial_warnings: &[String],
    identity: &RunIdentity,
) -> MietteResult<()> {
    use rhei_tui::{MessageLevel, RunEvent, RunSummary};

    // No `RunSubprocessGuard`: this mode spawns nothing supervised, so a guard
    // would own nothing. Anything supervised added here must install one.
    // §FS-rhei-run.3.2
    let workspace_root = run_execution_root(input);
    let runtime_dir = workspace_root.join("runtime");
    // §FS-rhei-run-report.3.1: run duration shown in the end-of-run summary.
    // §FS-rhei-run.2.7: one identity per run, so the report and the descriptor
    // name the same run.
    let run_started = identity.started;
    let run_started_wall = identity.started_wall;
    let run_id = identity.id.clone();
    let command = current_command_line();
    let initial = load_plan(input)?;
    let initial_total_tasks = total_task_count(&initial.rhei);
    let initial_states = collect_initial_states(&initial.rhei, &machines.set);
    // §FS-rhei-run-report.1: declared before the frontend so it drops after the
    // terminal is restored; disarmed on the happy path (see end of run).
    let mut report_guard = RunReportGuard {
        input,
        machines: &machines.set,
        runtime_dir: runtime_dir.clone(),
        run_started,
        run_started_wall,
        run_id: run_id.clone(),
        workspace_root: workspace_root.clone(),
        command: command.clone(),
        parallel: max_parallel,
        mode: "callback",
        initial_states: initial_states.clone(),
        dry_run: opts.dry_run(),
        summary: None,
        armed: true,
    };
    let frontend_parallel = max_parallel.max(1).min(u16::MAX as usize) as u16;
    // Callback-only mode installs no subprocess guard, so nothing raises this
    // and the surface leaves on a signal alone. §FS-rhei-run-tui.1.5.7
    let frontend = start_run_frontend(
        &workspace_root,
        input,
        machines,
        opts,
        frontend_parallel,
        initial_total_tasks,
        &RunShutdown::default(),
        identity,
    );
    let sink = frontend.sink.clone();
    // Route leaf-helper diagnostics through the frontend for the run's duration
    // instead of letting them write straight to the terminal and corrupt the
    // TUI. §FS-rhei-run-tui.1.8
    let diag_guard = RunDiagGuard::install(sink.clone());
    // Held past the frontend drop so the end-of-run summary can read activity
    // after the TUI restores the terminal. §FS-rhei-run-report.3
    let summary_sink = frontend.summary.clone();
    report_guard.summary = Some(summary_sink.clone());
    let dashboard_enabled = frontend.dashboard.is_some();
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

    let initial_terminal_count = terminal_task_count(&initial.rhei, &machines.set);
    run_info!(
        "Running {} '{}' with {} task(s) ({} terminal at start).",
        if workspace::is_workspace(input) { "workspace" } else { "plan" },
        initial.rhei.title,
        initial_total_tasks,
        initial_terminal_count
    );
    run_info!("Initial states: {}", format_state_counts(&initial.rhei));

    let mut transitions_made = 0u32;
    let mut pass = 0u32;
    let mut visited_ready_states = BTreeSet::<(String, String)>::new();
    // One-time notice so the gate-wait below does not spam the journal each tick.
    let mut awaiting_gate_announced = false;
    // Manual-only tasks reported by a dry run; the command still exits
    // non-zero once the scan is complete. §FS-rhei-run.4
    let mut manual_only_dry_run: Vec<String> = Vec::new();
    // Set when the pass loop found nothing to schedule and at least one
    // remaining ticket needs a human. A dry run must end the same way the real
    // run does, or it is not a prediction. §FS-rhei-run.4
    let mut halted_needs_human = false;
    // §FS-rhei-panta.6.1: `--rhei` narrows candidates, not prior resolution.
    let rhei_scope = rhei_scope_set(opts.rhei_scope());
    if rhei_scope.is_some() {
        // Repeat the pre-launch scope report inside the journal so a TUI run
        // sees it too. §FS-rhei-panta.6
        run_info!("Scope: narrowed to {}", scope_label(&rhei_scope));
    }

    loop {
        // Callback-only advancement spawns no supervised subprocess, but the
        // run still stops when the operator asks it to. §FS-rhei-run.3.2
        if interrupt_requested() {
            // Through the journal, not stderr: when no subprocess is in flight
            // the operator may still be looking at a live TUI.
            // §FS-rhei-run-tui.1.8
            if let Some(notice) = take_interruption_announcement() {
                run_warn!("{notice}");
            }
            break;
        }
        let loaded = load_plan(input)?;
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
                // Callback-only interactive TUI runs use the same human-gate
                // surface as agent mode; keep it alive only when gates are the
                // remaining blocker. §FS-rhei-run-tui.1.5.5
                if opts.waits_for_human_gates(frontend.is_tui)
                    && should_wait_for_human_gate(&loaded.rhei, &machines.set, &rhei_scope)
                {
                    if !awaiting_gate_announced {
                        run_info!("{}", awaiting_gate_notice(frontend.is_tui));
                        awaiting_gate_announced = true;
                    }
                    interruptible_sleep(Duration::from_millis(500));
                    continue;
                }
                if let Some(deadline) =
                    earliest_pending_poll_deadline(&loaded.rhei, &machines.set, &rhei_scope)
                {
                    let sleep_secs = deadline.saturating_sub(current_unix_secs()).max(1);
                    run_info!(
                        "No ready tasks; sleeping {}s until the next poll attempt.",
                        sleep_secs
                    );
                    interruptible_sleep(Duration::from_secs(sleep_secs));
                    continue;
                }
            }
            // Nothing schedulable and nothing advanced: without this the loop
            // exits having explained nothing, and the dry run reported success
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
        let terminal_count = terminal_task_count(&loaded.rhei, &machines.set);
        sink.emit(RunEvent::PassStarted {
            pass,
            ready: ready.iter().map(|task| task.id.to_string()).collect(),
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

        let mut advanced_any = false;
        let mut stalled_ready_tasks = Vec::new();

        for task in &ready {
            let task_id_str = task.id.to_string();
            // The ticket's own machine and callback base govern its advance.
            // §DA-per-rhei-state-machines
            let machine = machines.for_task(&task.id);
            let callback_paths = machines.callbacks_for_str(&task_id_str);
            let current_state_raw = task.state.as_str();
            let current_state = normalized_state_name(current_state_raw, machine);
            let visit_key = (task_id_str.clone(), current_state_raw.to_string());
            if visited_ready_states.contains(&visit_key) {
                stalled_ready_tasks.push(format!(
                    "{} (already visited '{}')",
                    format_task_label(task),
                    current_state_raw
                ));
                continue;
            }
            if let Some(to_state) = manual_initial_terminal_transition(task, &loaded.rhei, machine)? {
                // A dry run reports and keeps scanning; only a real run must
                // stop before touching the task. §FS-rhei-run.4
                if opts.dry_run() {
                    let line = format_dry_run_manual_only(&task_id_str, &current_state, &to_state);
                    run_info!("{}", line);
                    manual_only_dry_run.push(line);
                    continue;
                }
                return Err(miette!(
                    help = nothing_claimable_help(),
                    "Task {} is in manual-only initial state '{}' with terminal transition to '{}'; \
                     use `rhei next`, do the task, then `rhei complete` instead of `rhei run`.",
                    task_id_str,
                    current_state,
                    to_state
                ));
            }
            let next_to = find_next_transition(task, &loaded.rhei, machine)?;

            let Some(to_state) = next_to else {
                stalled_ready_tasks.push(format_task_label(task));
                continue;
            };

            if opts.dry_run() {
                run_info!(
                    "{}",
                    format_dry_run_transition(&task_id_str, current_state_raw, &to_state, machine)
                );
                continue;
            }

            visited_ready_states.insert(visit_key);
            if record_poll_self_loop_if_needed(
                &loaded,
                input,
                machine,
                task,
                &current_state,
                &to_state,
            )? {
                run_info!(
                    "Task {} poll self-loop scheduled next attempt from '{}'",
                    task_id_str,
                    current_state_raw
                );
                transitions_made += 1;
                advanced_any = true;
                break;
            }

            let task_ids_before: BTreeSet<String> =
                loaded.rhei.tasks.iter().map(|existing| existing.id.to_string()).collect();
            let route = loaded.task_route(&task_id_str, input);
            // Callback-only advancement: no subprocess ran here, so a terminal
            // edge records the engine's own account unless a callback already
            // wrote a result, which wins. §FS-rhei-run.3
            match execute_callback_only_transition(
                TransitionFiles { task_file: &route.task_file, metadata_file: &route.metadata_file, metadata_id: &route.metadata_id, artifact_root: &route.execution_root, artifact_id: &task_id_str },
                callback_paths,
                machine,
                &route.local_id,
                &current_state,
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
                    run_info!("  {}", format_task_label(task));
                    if is_terminal_state(&effective_to, machine) {
                        run_info!("  Result: reached terminal state '{}'.", effective_to);
                    } else {
                        run_info!("  Result: now in '{}'.", effective_to);
                    }
                    let reloaded = load_plan(input)?;
                    let discovered = newly_discovered_tasks(&task_ids_before, &reloaded.rhei.tasks);
                    if !discovered.is_empty() {
                        run_info!(
                            "  Workspace expanded: discovered {} new task(s): {}",
                            discovered.len(),
                            discovered.join(", ")
                        );
                    }
                    transitions_made += 1;
                    advanced_any = true;
                    break;
                }
                Err(err) => {
                    run_warn!("warning: failed to advance Task {}: {}", task_id_str, err);
                    continue;
                }
            }
        }

        if !stalled_ready_tasks.is_empty() && !advanced_any {
            run_info!(
                "No forward transition available for ready task(s): {}",
                stalled_ready_tasks.join(", ")
            );
        }

        sink.emit(RunEvent::PassEnded { pass, progressed: advanced_any });

        if opts.dry_run() || !advanced_any {
            break;
        }
    }

    // Read once, here, and used for every statement the run makes about
    // itself: a signal arriving later — while the TUI is parked on its
    // finished screen — did not cut this loop short. §FS-rhei-run.3.2
    let interrupted_run = interrupted_by_signal();

    let (terminal_count, total_tasks) = if opts.dry_run() {
        run_info!("\nDry run complete \u{2014} no changes were made.");
        if !manual_only_dry_run.is_empty() {
            return Err(manual_only_dry_run_error(&manual_only_dry_run));
        }
        // The real run halts here; so must the prediction of it.
        // §FS-rhei-run.4
        if halted_needs_human {
            return Err(dry_run_halt_error());
        }
        (0usize, 0usize)
    } else if transitions_made == 0 {
        let loaded = load_plan(input)?;
        run_info!("{}", no_advancement_summary(&loaded.rhei, &machines.set, &rhei_scope));
        (0usize, 0usize)
    } else {
        let loaded = load_plan(input)?;
        let terminal_count = terminal_task_count(&loaded.rhei, &machines.set);
        let total_tasks = total_task_count(&loaded.rhei);
        // §FS-rhei-run.3.2: the run stopped; it did not complete.
        if interrupted_run {
            run_info!(
                "\nRun interrupted after {} transition(s) made; {}/{} tasks in terminal state.",
                transitions_made,
                terminal_count,
                total_tasks
            );
        } else {
            run_info!(
                "\nRun complete: {} transition(s) made, {}/{} tasks in terminal state.",
                transitions_made,
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

    sink.emit(RunEvent::RunFinished {
        summary: RunSummary {
            agents_spawned: 0,
            programs_spawned: 0,
            terminal_tasks: terminal_count,
            total_tasks,
            accounting: None,
            workspace_accounting: None,
        },
    });
    frontend.write_frozen_dashboard();
    drop(diag_guard);
    drop(sink);
    drop(frontend);

    // §FS-rhei-run-report.1/.3: durable report (skipped under --dry-run, §3.5) +
    // console summary. Callback mode spawns no agents/programs; its advances are
    // callback-only. Disarm the guard so its fallback only fires on early error.
    emit_run_report(
        input,
        &machines.set,
        &summary_sink,
        &runtime_dir,
        RunStats {
            agents_spawned: 0,
            programs_spawned: 0,
            callback_only: transitions_made,
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
            mode: "callback",
            initial_states,
            dry_run: opts.dry_run(),
            interrupted: interrupted_run,
        },
    );
    report_guard.disarm();

    // An interrupted run is not a halt. The same reading the report used, so
    // the halt decision and the run's own account of itself cannot disagree.
    // §FS-rhei-run.3.2
    if interrupted_run {
        return Ok(());
    }

    if !opts.dry_run() {
        let loaded = load_plan(input)?;
        // §FS-rhei-panta.6.1: a narrowed run halts on in-scope work only —
        // out-of-scope tickets left non-terminal are not a failure.
        if scoped_unfinished_task_exists(&loaded.rhei, &machines.set, &rhei_scope)
            && !remaining_work_is_only_gating_or_poll_blocked(&loaded.rhei, &machines.set, &rhei_scope)
        {
            return Err(miette!(
                help = nothing_claimable_help(),
                "rhei run halted with non-terminal tasks remaining and no further advancement possible"
            ));
        }
    }

    Ok(())
}
