/// Whether stdout has been handed to the JSON record stream for this process.
///
/// `--json` promises that stdout carries records and nothing else. That is a
/// process-wide fact once the frontend is chosen, and the run path has a few
/// human-prose writers that predate the frontend or outlive it; each consults
/// this rather than threading a flag through every caller.
// §FS-rhei-run-json.1
static JSON_RECORDS_OWN_STDOUT: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

fn reserve_stdout_for_json_records() {
    JSON_RECORDS_OWN_STDOUT.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// True when a human-oriented line must go to stderr because stdout is a
/// record stream. §FS-rhei-run-json.1
fn stdout_carries_json_records() -> bool {
    JSON_RECORDS_OWN_STDOUT.load(std::sync::atomic::Ordering::SeqCst)
}

/// Flags that control standalone execution behavior for `rhei run`.
#[derive(Args, Clone, Debug, Default)]
#[command(next_help_heading = "Standalone Execution")]
struct StandaloneExecutionFlags {
    /// Show what transitions would be made without executing them
    #[arg(long)]
    dry_run: bool,
    /// Skip execution of on_leave/on_enter callbacks
    #[arg(long)]
    no_callbacks: bool,
    /// Continue to the next task when an agent exits non-zero
    #[arg(long)]
    continue_on_error: bool,
    /// Maximum number of agents to run concurrently (0 = unlimited)
    #[arg(long, default_value_t = 1, add = ArgValueCompleter::new(complete_parallel))]
    parallel: usize,
    /// Override profile-derived and built-in pricing with a local price book
    #[arg(long, value_name = "PATH", add = ArgValueCompleter::new(complete_any_path))]
    prices: Option<PathBuf>,
    /// Narrow to the named rhei (repeatable; one id per flag). A rhei id
    /// is its file stem or directory name; default is the whole project
    #[arg(long = "rhei", value_name = "RHEI_ID", add = ArgValueCompleter::new(complete_rhei_id))]
    rhei: Vec<String>,
    /// Force TUI mode even when stdout is not detected as a TTY
    #[arg(long, conflicts_with = "no_tui")]
    tui: bool,
    /// Force plain stdout output even when stdout is a TTY
    #[arg(long)]
    no_tui: bool,
    /// Emit the run as a JSONL event stream on stdout (implies --no-tui).
    /// With --headless, describes the launcher's own output instead
    #[arg(long, conflicts_with = "tui")]
    json: bool,
    /// Include live agent output lines in the --json stream instead of
    /// leaving them to the per-task logs
    #[arg(long, requires = "json")]
    json_agent_output: bool,
    /// Detach the run into its own session and print its run id
    #[arg(long, conflicts_with_all = ["tui", "dry_run"])]
    headless: bool,
    /// Serve a loopback browser dashboard for this run
    #[arg(long, conflicts_with = "no_dashboard")]
    dashboard: bool,
    /// Disable the loopback browser dashboard
    #[arg(long)]
    no_dashboard: bool,
}

/// Flags that control agent-specific behavior for `rhei run`.
#[derive(Args, Clone, Debug, Default)]
#[command(next_help_heading = "Agent Execution")]
struct AgentExecutionFlags {
    /// Disable agent spawning; use callback-only advancement
    #[arg(long)]
    no_agent: bool,
    /// Override the agent dimension on ordinary single targets; `all_targets` is unchanged.
    /// §FS-rhei-run.2.2
    #[arg(long, value_name = "AGENT", add = ArgValueCompleter::new(complete_agent_name))]
    agent: Option<String>,
    /// Override the agent mode (named flag set) for this run
    #[arg(long, value_name = "MODE", add = ArgValueCompleter::new(complete_agent_mode))]
    agent_mode: Option<String>,
    /// Override the model dimension on ordinary single targets; fanout keeps its declared models.
    /// §FS-rhei-run.2.2
    #[arg(long, value_name = "MODEL", add = ArgValueCompleter::new(complete_model_name))]
    model: Option<String>,
}

/// Flags that control program-specific behavior for `rhei run`.
#[derive(Args, Clone, Debug, Default)]
#[command(next_help_heading = "Program Execution")]
struct ProgramExecutionFlags {
    /// Disable program spawning; use callback-only advancement for program states
    #[arg(long)]
    no_program: bool,
    /// Override the program timeout for this run
    #[arg(long, value_name = "DURATION", add = ArgValueCompleter::new(complete_duration))]
    program_timeout: Option<String>,
}

/// Flags that control snapshot inheritance overrides for `rhei run`.
///
/// §FS-rhei-run.2.3 §FS-rhei-snapshot-operations.2: Snapshot run flags.
#[derive(Args, Clone, Debug, Default)]
#[command(next_help_heading = "Snapshots")]
struct SnapshotExecutionFlags {
    /// Override the concrete source snapshot selected by an authored
    /// `snapshot.inherit:` after that state's constraints are applied.
    #[arg(long, value_name = "REF")]
    from_snapshot: Option<String>,
    /// Explicitly bypass authored source-selection and compatibility
    /// constraints for an ad-hoc debug run. Requires `--from-snapshot`.
    #[arg(long, requires = "from_snapshot")]
    override_inherit: bool,
    /// Select the task for an ambiguous snapshot override.
    #[arg(long = "task", value_name = "TASK_ID", add = ArgValueCompleter::new(complete_task_id))]
    snapshot_task: Option<String>,
    /// Select the fanout target for an ambiguous snapshot override.
    #[arg(long = "target", value_name = "SLUG")]
    snapshot_target: Option<String>,
}

/// Options for the `run` command.
struct RunOptions {
    standalone: StandaloneExecutionFlags,
    agent: AgentExecutionFlags,
    program: ProgramExecutionFlags,
    snapshot: SnapshotExecutionFlags,
    price_book: PriceBook,
}

impl RunOptions {
    fn dry_run(&self) -> bool {
        self.standalone.dry_run
    }

    fn no_callbacks(&self) -> bool {
        self.standalone.no_callbacks
    }

    fn continue_on_error(&self) -> bool {
        self.standalone.continue_on_error
    }

    fn parallel(&self) -> usize {
        self.standalone.parallel
    }

    fn prices_path(&self) -> Option<&Path> {
        self.standalone.prices.as_deref()
    }

    fn price_book(&self) -> &PriceBook {
        &self.price_book
    }

    fn select_price_book(&mut self, price_book: PriceBook) {
        self.price_book = price_book;
    }

    /// Rhei ids this invocation is narrowed to; empty means the whole project.
    /// §FS-rhei-panta.6
    fn rhei_scope(&self) -> &[String] {
        &self.standalone.rhei
    }

    /// Adopt the scope implied by the resolved target — the rhei a member-plan
    /// path pointed at — when `--rhei` did not already set one. §FS-rhei-panta.6
    fn narrow_to(&mut self, scope: Vec<String>) {
        self.standalone.rhei = scope;
    }

    /// Whether this invocation asks to detach. §FS-rhei-run-headless.1
    fn headless(&self) -> bool {
        self.standalone.headless
    }

    fn json(&self) -> bool {
        self.standalone.json
    }

    fn frontend_kind(&self) -> rhei_tui::FrontendKind {
        // Decided before TTY detection: a stream a program parses is never also
        // a screen. §FS-rhei-run-json.1
        if self.standalone.json {
            rhei_tui::FrontendKind::Json { agent_output: self.standalone.json_agent_output }
        } else if self.standalone.tui {
            rhei_tui::FrontendKind::Tui
        } else if self.standalone.no_tui {
            rhei_tui::FrontendKind::Stdout
        } else {
            rhei_tui::FrontendKind::Auto
        }
    }

    /// Whether the loopback **control server** runs. It is what an attached
    /// surface intervenes and releases gates through, so a detached run always
    /// serves it; `--no-dashboard` withholds the browser link, not the
    /// endpoints.
    // §FS-rhei-run-headless.4
    fn dashboard_enabled(&self, frontend_is_tui: bool) -> bool {
        if self.standalone.dashboard {
            true
        } else if self.standalone.no_dashboard {
            // A detached run with no control server could never be intervened
            // in, so the server stays; what the flag removes is the link.
            is_headless_child()
        } else {
            frontend_is_tui || is_headless_child()
        }
    }

    /// Whether the run points anyone at the browser dashboard. `--no-dashboard`
    /// on a detached run keeps the control server (above) but announces
    /// nothing, so no browser is invited to a surface the operator turned off.
    // §FS-rhei-run-headless.4
    fn announces_dashboard(&self) -> bool {
        !self.standalone.no_dashboard
    }

    /// Whether the run should stay alive for a pending human gate.
    ///
    /// An interactive surface has an operator in front of it; a detached run
    /// has one arriving later, through `rhei attach` or the dashboard. Only a
    /// plain non-interactive run has nobody at all.
    // §FS-rhei-run-headless.1.2 §FS-rhei-run-tui.1.5.7
    fn waits_for_human_gates(&self, frontend_is_tui: bool) -> bool {
        frontend_is_tui || is_headless_child()
    }

    fn no_agent(&self) -> bool {
        self.agent.no_agent
    }

    fn agent_override(&self) -> Option<&str> {
        self.agent.agent.as_deref()
    }

    fn agent_mode_override(&self) -> Option<&str> {
        self.agent.agent_mode.as_deref()
    }

    fn model_override(&self) -> Option<&str> {
        self.agent.model.as_deref()
    }

    fn no_program(&self) -> bool {
        self.program.no_program
    }

    fn program_timeout_override(&self) -> Option<&str> {
        self.program.program_timeout.as_deref()
    }

    fn snapshot_override_ref(&self) -> Option<&str> {
        self.snapshot.from_snapshot.as_deref()
    }

    fn override_inherit(&self) -> bool {
        self.snapshot.override_inherit
    }

    fn snapshot_task_selector(&self) -> Option<&str> {
        self.snapshot.snapshot_task.as_deref()
    }

    fn snapshot_target_selector(&self) -> Option<&str> {
        self.snapshot.snapshot_target.as_deref()
    }
}
