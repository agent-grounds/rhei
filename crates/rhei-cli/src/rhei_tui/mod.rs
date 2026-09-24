//! Live visualization and transition journal for `rhei run`.
//!
//! §FS-rhei-run-tui: Live visualization and transition journal behavior.

mod dashboard;
mod event;
mod event_json;
mod event_log;
mod frontend;
mod journal;
mod json;
mod run_stop;
mod stdout;
mod tui;

pub use dashboard::{DashboardSink, GateTransitionSink, InterveneSink, PlanLoader};
pub use event::{
    bound_journal_line, summarize_usage_summaries, AccountingRunSummary, AgentStream, BoundReport,
    DimensionStatus, DimensionSummary, EventSink, MessageLevel, NullSink, PricingStatus, RunEvent,
    RunSummary, Slot, TaskOutcome, Tee, UsageCoverage, UsageReport, UsageStatus, UsageSummary,
};
pub use event_json::{
    decode as decode_event, encode as encode_event, format_rfc3339, parse_rfc3339, SCHEMA_VERSION,
};
pub use event_log::{event_log_path, EventLogReader, EventLogSink};
pub use frontend::{select_frontend, Frontend, FrontendKind};
pub use journal::JournalSink;
pub use json::JsonSink;
pub use run_stop::{IdleBlocker, RunStop, StopReason};
pub use stdout::StdoutSink;
pub use tui::{StopRequested, TuiContext, TuiSink};
