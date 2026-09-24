// How a run gets its surfaces: the frontend it renders through, the loopback
// control server, the live-action sinks, and the identity it publishes.
//
// Separated from the option surface next door because these are different
// concerns with different readers — one is what the operator types, the other
// is what the run wires up once those choices are made.

// §AR-source-file-size.3 §FS-rhei-run-tui.1.4 §FS-rhei-run-headless.2

struct ActiveRunFrontend {
    sink: Arc<dyn rhei_tui::EventSink>,
    /// True when an interactive TUI is the active frontend. The run loop uses
    /// this to keep itself alive while a human gate is pending, so the operator
    /// can resolve the gate in the UI and have the run continue (§FS-rhei-run-tui.1.5.5).
    is_tui: bool,
    dashboard: Option<Arc<rhei_tui::DashboardSink>>,
    /// Accumulates per-task driver/duration for the end-of-run console summary.
    /// §FS-rhei-run-report.3
    summary: Arc<SummarySink>,
    /// The intervene registry, present only when the dashboard is live. The run
    /// loop registers each running agent's stdin here so `/intervene` can reach
    /// it. AR §7.
    intervene: Option<Arc<RunInterveneSink>>,
    /// Whether to publish the dashboard URL as a run link. False when the
    /// operator asked for no dashboard but the run still needs its control
    /// server. §FS-rhei-run-headless.4
    announces_dashboard: bool,
    _frontend: Option<rhei_tui::Frontend>,
}

/// The words a run uses while it waits at a human gate.
///
/// A driving surface points at itself; a detached run points at the commands
/// that reach it, because the operator reading this in `run.log` has no screen.
// §FS-rhei-run-headless.1.2
fn awaiting_gate_notice(frontend_is_tui: bool) -> &'static str {
    if frontend_is_tui {
        "Waiting for human gate decisions — resolve a gate in the UI, or press Ctrl+C to stop."
    } else if is_headless_child() {
        "Waiting for human gate decisions — release one with `rhei attach` or the browser \
         dashboard, or end the run with `rhei stop`."
    } else {
        "Waiting for human gate decisions — release one with `rhei transition`, or press \
         Ctrl+C to stop."
    }
}

struct RunGateTransitionSink {
    input: PathBuf,
    machines: ExecutionMachines,
    no_callbacks: bool,
}

impl RunGateTransitionSink {
    fn new(input: PathBuf, machines: ExecutionMachines, no_callbacks: bool) -> Self {
        Self { input, machines, no_callbacks }
    }
}

impl rhei_tui::GateTransitionSink for RunGateTransitionSink {
    fn transition_gate(
        &self,
        task_id: &str,
        from: &str,
        to: &str,
        result: Option<&str>,
    ) -> Result<String, String> {
        // A gate decision lands on one ticket: its own machine and callback
        // base execute the human transition. §DA-per-rhei-state-machines
        transition_dashboard_gate(
            &self.input,
            self.machines.for_task_str(task_id),
            self.machines.callbacks_for_str(task_id),
            task_id,
            from,
            to,
            result,
            self.no_callbacks,
        )
        .map_err(|err| err.to_string())
    }
}

impl ActiveRunFrontend {
    fn announce_dashboard(&self) {
        if !self.announces_dashboard {
            return;
        }
        if let Some(dashboard) = &self.dashboard {
            self.sink.emit(rhei_tui::RunEvent::RunLink {
                label: "Dashboard".to_string(),
                url: dashboard.url().to_string(),
            });
        }
    }

    fn write_frozen_dashboard(&self) {
        let Some(dashboard) = &self.dashboard else {
            return;
        };
        match dashboard.write_frozen_dashboard() {
            Ok(path) => self.sink.emit(rhei_tui::RunEvent::Message {
                level: rhei_tui::MessageLevel::Info,
                text: format!("Final dashboard: {}", path.display()),
            }),
            Err(err) => self.sink.emit(rhei_tui::RunEvent::Message {
                level: rhei_tui::MessageLevel::Warn,
                text: format!("warning: could not write final dashboard: {err}"),
            }),
        }
    }
}

/// The surface a dry run renders through.
///
/// A dry run writes no event log and publishes no descriptor — it is
/// side-effect-free — but the frontend the caller asked for is still the
/// frontend it gets. Hardcoding stdout here put prose on the record stream of
/// `rhei run --json --dry-run`, which the record contract says can never
/// happen.
// §FS-rhei-run-json.1 §FS-rhei-run-json.4
fn dry_run_sink(workspace_root: &Path, opts: &RunOptions) -> Arc<dyn rhei_tui::EventSink> {
    match opts.frontend_kind() {
        rhei_tui::FrontendKind::Json { agent_output } => {
            Arc::new(rhei_tui::JsonSink::new(agent_output, workspace_root))
        }
        _ => Arc::new(rhei_tui::StdoutSink::new()),
    }
}

/// Carry the initial validation report into the selected frontend after the
/// run has started. Validator entries are bodies; run messages include the
/// complete human-readable warning prefix.
// §FS-rhei-run.3 §FS-rhei-run-json.1 §FS-rhei-run-tui.1.1
fn emit_initial_validation_warnings(
    sink: &Arc<dyn rhei_tui::EventSink>,
    warnings: &[String],
) {
    for warning in warnings {
        sink.emit(rhei_tui::RunEvent::Message {
            level: rhei_tui::MessageLevel::Warn,
            text: format!("warning: {warning}"),
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn start_run_frontend(
    workspace_root: &Path,
    plan_input: &Path,
    machines: &ExecutionMachines,
    opts: &RunOptions,
    parallel: u16,
    total_tasks: usize,
    shutdown: &RunShutdown,
    identity: &RunIdentity,
) -> ActiveRunFrontend {
    announce_line_output_for_until_idle(opts);
    if opts.dry_run() {
        return ActiveRunFrontend {
            sink: dry_run_sink(workspace_root, opts),
            is_tui: false,
            dashboard: None,
            summary: Arc::new(SummarySink::new()),
            intervene: None,
            announces_dashboard: false,
            _frontend: None,
        };
    }

    // The loader re-reads the plan and builds the full `VizModel` via `rhei-viz`,
    // so the TUI render thread and dashboard share one run model and the same
    // intervene/gate boundaries; neither parses plans itself. §FS-rhei-run-tui.1.5
    let plan_path = plan_input.to_path_buf();
    let loader_machines = machines.set.clone();
    let loader: rhei_tui::PlanLoader =
        Arc::new(move || load_plan_for_dashboard(&plan_path, &loader_machines));
    // AR §7: the intervene registry the run loop registers agents into.
    let registry = Arc::new(RunInterveneSink::new(workspace_root.join("runtime")));
    let gate = Arc::new(RunGateTransitionSink::new(
        plan_input.to_path_buf(),
        machines.clone(),
        opts.no_callbacks(),
    ));

    let tui_context = rhei_tui::TuiContext::driving(
        workspace_root.to_path_buf(),
        Some(loader.clone()),
        Some(registry.clone() as Arc<dyn rhei_tui::InterveneSink>),
        Some(gate.clone() as Arc<dyn rhei_tui::GateTransitionSink>),
        // This run's own shutdown fact, not the thread's: it is asked from
        // inside the run's unwind, after the guard has handed its thread-local
        // ownership back. §FS-rhei-run-tui.1.5.7
        {
            let shutdown = shutdown.clone();
            Arc::new(move || shutdown.is_raised())
        },
    );
    let frontend = rhei_tui::select_frontend(
        workspace_root,
        opts.frontend_kind(),
        parallel,
        total_tasks,
        tui_context,
    );

    let dashboard = if opts.dashboard_enabled(frontend.is_tui) {
        match rhei_tui::DashboardSink::start_with_plan_intervene_and_gate(
            workspace_root.to_path_buf(),
            parallel,
            total_tasks,
            Some(loader.clone()),
            Some(registry.clone() as Arc<dyn rhei_tui::InterveneSink>),
            Some(gate.clone() as Arc<dyn rhei_tui::GateTransitionSink>),
        ) {
            Ok(sink) => Some(Arc::new(sink)),
            Err(err) => {
                frontend.sink.emit(rhei_tui::RunEvent::Message {
                    level: rhei_tui::MessageLevel::Warn,
                    text: format!("warning: could not start dashboard: {err}"),
                });
                None
            }
        }
    } else {
        None
    };

    // The run loop registers running agents' stdin into the registry so both the
    // TUI composer and the dashboard `/intervene` can reach them. Wire it
    // whenever a live surface is present.
    let intervene: Option<Arc<RunInterveneSink>> =
        (frontend.is_tui || dashboard.is_some()).then(|| registry.clone());

    // The summary sink is always teed in so the end-of-run console summary can
    // render per-task driver/duration regardless of dashboard state.
    // §FS-rhei-run-report.3
    let summary = Arc::new(SummarySink::new());
    let mut inner: Vec<Arc<dyn rhei_tui::EventSink>> = vec![frontend.sink.clone(), summary.clone()];
    if let Some(dashboard) = &dashboard {
        inner.push(dashboard.clone());
    }
    let sink: Arc<dyn rhei_tui::EventSink> = Arc::new(rhei_tui::Tee::new(inner));

    // Published once the control URL is known, so a reader learns where to
    // reach this run in the same breath it learns the run exists.
    // §FS-rhei-run-headless.2
    publish_run_descriptor(&RunDescriptor {
        id: identity.id.clone(),
        pid: std::process::id(),
        status: RunStatus::Running,
        workspace: workspace_root.to_path_buf(),
        plan: plan_input.to_path_buf(),
        state_machine: machines.state_machine_override.clone(),
        control_url: dashboard.as_ref().map(|d| d.url().to_string()),
        started_at: rhei_tui::format_rfc3339(identity.started_wall),
        headless: identity.headless,
        parallel: parallel as usize,
        log: identity.headless.then(|| run_console_log_path(workspace_root)),
        events: rhei_tui::event_log_path(workspace_root),
        exit_code: None,
    });

    let is_tui = frontend.is_tui;
    ActiveRunFrontend {
        sink,
        is_tui,
        dashboard,
        summary,
        intervene,
        announces_dashboard: opts.announces_dashboard(),
        _frontend: Some(frontend),
    }
}

#[allow(clippy::too_many_arguments)]
fn transition_dashboard_gate(
    input: &Path,
    machine: &rhei_validator::StateMachine,
    callback_paths: &CallbackPaths,
    task_id_str: &str,
    from: &str,
    to: &str,
    result: Option<&str>,
    no_callbacks: bool,
) -> MietteResult<String> {
    let loaded = load_plan(input)?;
    let task = find_task_by_id_str(&loaded.rhei.tasks, task_id_str)
        .ok_or_else(|| {
            miette!(
                help = format!(
                    "list the task ids in this plan with: rhei list {}",
                    shell_quote(&input.display().to_string())
                ),
                "task '{}' not found in the plan",
                task_id_str
            )
        })?;
    let current_state = normalized_state_name(task.state.as_str(), machine);
    if current_state != from {
        return Err(miette!(
            help = format!(
                "someone moved the task since you looked. Re-read its current state with: \
                 rhei list {}",
                shell_quote(&input.display().to_string())
            ),
            "conflict: Task {} is in state '{}', expected '{}'",
            task_id_str,
            task.state,
            from
        ));
    }
    if !machine.states.get(&current_state).map(|def| def.gating).unwrap_or(false) {
        return Err(miette!(
            help = "only human-gate states are released this way. Advance a non-gating state \
                    with `rhei transition` or let `rhei run` drive it. See which states gate \
                    with: rhei states",
            "Task {} is in state '{}', which is not a gating state",
            task_id_str,
            current_state
        ));
    }
    let explicit_transition =
        machine.transitions().iter().any(|rule| rule.from.0 == from && rule.to.0 == to);
    if !explicit_transition {
        return Err(miette!(
            help = format!(
                "the state machine declares no '{from}' -> '{to}' edge. List the edges \
                 leaving '{from}' with: rhei states"
            ),
            "transition from '{}' to '{}' is not an explicit human-gate transition",
            from,
            to
        ));
    }

    // The operator's account rides the move, as `transition --result` does; blank
    // is none, and the shared path refuses a terminal release with none.
    // §FS-rhei-viz.5.1 §FS-rhei-run.3 §FS-rhei-states.3.3
    let result = result.map(str::trim).filter(|message| !message.is_empty());
    let route = loaded.task_route(task_id_str, input);
    execute_transition(
        TransitionFiles {
            task_file: &route.task_file,
            metadata_file: &route.metadata_file,
            metadata_id: &route.metadata_id,
            artifact_root: &route.execution_root,
            artifact_id: task_id_str,
        },
        callback_paths,
        machine,
        &route.local_id,
        from,
        to,
        result,
        no_callbacks,
    )
}

/// Re-read the plan from disk and build the dashboard's [`VizModel`] via
/// `rhei-viz` (flatten the resolved machine, derive plan state, classify).
/// Called on every `/snapshot` request, so failures must be non-fatal — return
/// `None` and let the dashboard fall back to the last good model. AR §5.2.
fn load_plan_for_dashboard(
    plan_path: &Path,
    machines: &rhei_validator::MachineSet,
) -> Option<rhei_viz_model::VizModel> {
    // §FS-rhei-recover.4: a watch refresh reports the interlock instead of hiding it as no graph.
    let _guards = rhei_core::root_access::for_input(plan_path)
        .map_err(|err| eprintln!("{err}")).ok()?;
    let loaded = load_plan(plan_path).ok()?;
    // Any directory input — workspace or Panta project — is its own execution
    // root; per-task roots route each ticket's history to its owning rhei,
    // which is where a project run writes its ledgers. §AR-rhei-panta.5
    let default_root = execution_workspace_root(plan_path);
    rhei_viz::build_set_with_history_roots(
        &loaded.rhei,
        machines,
        &default_root,
        &loaded.task_roots,
    ).map_err(|err| eprintln!("{err}")).ok()
}

impl
    From<(
        StandaloneExecutionFlags,
        AgentExecutionFlags,
        ProgramExecutionFlags,
        SnapshotExecutionFlags,
    )> for RunOptions
{
    fn from(
        (standalone, agent, program, snapshot): (
            StandaloneExecutionFlags,
            AgentExecutionFlags,
            ProgramExecutionFlags,
            SnapshotExecutionFlags,
        ),
    ) -> Self {
        Self { standalone, agent, program, snapshot, price_book: builtin_price_book() }
    }
}
