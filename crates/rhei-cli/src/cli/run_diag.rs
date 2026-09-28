// Ambient run-diagnostics sink.
//
// Leaf helpers on the `rhei run` path (failure transitions, snapshot
// emit/preload, agent/MCP setup) historically wrote diagnostics straight to
// stdout/stderr. During a live TUI run those writes race the terminal that
// ratatui owns and shatter the rendered frame — the corruption is permanent
// because ratatui's diff renderer never repairs cells it believes are
// unchanged. The frontend already carries an `EventSink` that fans diagnostics
// to the journal panel (TUI) or to stdout/stderr per level (non-TTY), but these
// helpers are buried under dozens of call sites and tests, so threading the
// sink as a parameter everywhere is impractical.
//
// Instead a run installs its frontend sink here for the run's duration via a
// RAII guard, and the leaf helpers emit through `emit_run_diag`. When no run
// is active (unit tests, other subcommands) the helpers fall back to direct
// prints, preserving prior behavior exactly.

static RUN_DIAG_SINK: std::sync::RwLock<Option<std::sync::Arc<dyn rhei_tui::EventSink>>> =
    std::sync::RwLock::new(None);

/// Installs the active run's frontend sink as the ambient diagnostic sink and
/// removes it on drop, so leaf diagnostics reach the journal/stdout sink instead
/// of the terminal for the lifetime of a `run_*_mode` call. §FS-rhei-run-tui.1.8
struct RunDiagGuard;

impl RunDiagGuard {
    fn install(sink: std::sync::Arc<dyn rhei_tui::EventSink>) -> Self {
        *RUN_DIAG_SINK.write().expect("run diag sink lock poisoned") = Some(sink);
        // A degraded root guard warns from `rhei-core`, which is under the same
        // hazard as every leaf helper here. §FS-rhei-recover.4.1
        rhei_core::root_access::set_warning_sink(Some(Box::new(|text| {
            emit_run_diag(rhei_tui::MessageLevel::Warn, text.to_owned())
        })));
        RunDiagGuard
    }
}

impl Drop for RunDiagGuard {
    fn drop(&mut self) {
        *RUN_DIAG_SINK.write().expect("run diag sink lock poisoned") = None;
        rhei_core::root_access::set_warning_sink(None);
    }
}

/// Route a run diagnostic through the active frontend sink, or fall back to a
/// direct print when no run is active. Info goes to stdout, Warn/Error to
/// stderr — matching both `StdoutSink` and the prior `println!`/`eprintln!`
/// split, so non-TTY/CI output is byte-for-byte preserved.
fn emit_run_diag(level: rhei_tui::MessageLevel, text: String) {
    if let Some(sink) = RUN_DIAG_SINK.read().expect("run diag sink lock poisoned").as_ref() {
        sink.emit(rhei_tui::RunEvent::Message { level, text });
        return;
    }
    match level {
        rhei_tui::MessageLevel::Info => println!("{text}"),
        rhei_tui::MessageLevel::Warn | rhei_tui::MessageLevel::Error => eprintln!("{text}"),
    }
}

/// Run diagnostic at info level (was `println!`). Routes to the journal under a
/// TUI, or to stdout otherwise.
macro_rules! diag_info {
    ($($arg:tt)*) => {
        emit_run_diag(rhei_tui::MessageLevel::Info, format!($($arg)*))
    };
}

/// Run diagnostic at warn level (was `eprintln!`). Routes to the journal under a
/// TUI, or to stderr otherwise.
macro_rules! diag_warn {
    ($($arg:tt)*) => {
        emit_run_diag(rhei_tui::MessageLevel::Warn, format!($($arg)*))
    };
}
