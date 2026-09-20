//! The JSON form of a [`RunEvent`], in both directions.
//!
//! One module owns the wire contract of  so the `--json`
//! frontend, the durable `runtime/events.jsonl`, and the attach client that
//! replays it cannot drift apart. Serialization is exhaustive over `RunEvent`
//! on purpose: a new variant fails to compile here rather than silently
//! escaping the contract.
//!
//! `Instant` fields (`started_at`, `finished_at`) are process-local and have no
//! wire form. A decoded event carries a fresh `Instant`, which is correct for
//! everything a renderer does with them — measure elapsed time from now — and
//! meaningless to compare across processes, which nothing does.
// §FS-rhei-run-json.2

use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Map, Value};

use crate::rhei_tui::event::{
    AccountingRunSummary, AgentStream, MessageLevel, RunEvent, RunSummary, TaskOutcome,
    UsageReport, UsageSummary,
};

/// Wire version of the record contract. Moves only when a documented field is
/// removed or changes meaning; adding records or fields does not move it.
// §FS-rhei-run-json.2.2
pub const SCHEMA_VERSION: u64 = 1;

/// Render a path the way the transition journal does: workspace-relative when
/// it is inside the workspace, absolute otherwise.
///
/// The two must agree. One run wrote `runtime/logs/task-plan.1-pending.log`
/// into `transitions.log` and the absolute path of the same file into
/// `events.jsonl`, so a consumer reading both saw two names for one log — and
/// the absolute one leaks the machine's directory layout into a record stream
/// that is otherwise portable. `workspace` is `None` where the encoder has no
/// workspace to relativize against, and then a path is left as it is.
// §FS-rhei-run-json.2.1
fn record_path(workspace: Option<&Path>, path: &Path) -> String {
    workspace.and_then(|root| path.strip_prefix(root).ok()).unwrap_or(path).display().to_string()
}

/// Encode one event as the payload half of a record: the `event` discriminator
/// plus its own fields. The envelope (`seq`, `ts`) is added by [`encode`].
// §FS-rhei-run-json.2.1
fn payload(event: &RunEvent, workspace: Option<&Path>) -> Map<String, Value> {
    let mut map = Map::new();
    let mut put = |key: &str, value: Value| {
        map.insert(key.to_string(), value);
    };
    match event {
        RunEvent::Budget { event } => {
            return serde_json::to_value(event)
                .expect("budget event serializes")
                .as_object()
                .expect("budget event is an object")
                .clone();
        }
        RunEvent::RunStarted { run_id, workspace, parallel, total_tasks } => {
            put("event", json!("run_started"));
            put("schema", json!(SCHEMA_VERSION));
            put("run_id", json!(run_id));
            put("workspace", json!(workspace.display().to_string()));
            put("parallel", json!(parallel));
            put("total_tasks", json!(total_tasks));
        }
        RunEvent::PassStarted { pass, ready } => {
            put("event", json!("pass_started"));
            put("pass", json!(pass));
            put("ready", json!(ready));
        }
        RunEvent::SlotAssigned { slot, task, from, to, agent, log_path, .. } => {
            put("event", json!("slot_assigned"));
            put("slot", json!(slot));
            put("task", json!(task));
            put("from", json!(from));
            put("to", json!(to));
            put("agent", json!(agent));
            put("log_path", json!(record_path(workspace, log_path)));
        }
        RunEvent::SlotReleased {
            slot,
            task,
            from,
            to,
            log_path,
            outcome,
            exit_code,
            duration_ms,
            ..
        } => {
            put("event", json!("slot_released"));
            put("slot", json!(slot));
            put("task", json!(task));
            put("from", json!(from));
            put("to", json!(to));
            put("log_path", json!(record_path(workspace, log_path)));
            put("outcome", json!(outcome_name(outcome)));
            if let TaskOutcome::Failed(reason) = outcome {
                put("reason", json!(reason));
            }
            if let TaskOutcome::ProviderLimited { provider, next_attempt_at } = outcome {
                put("provider", json!(provider));
                put("next_attempt_at", json!(next_attempt_at));
            }
            put("exit_code", json!(exit_code));
            put("duration_ms", json!(duration_ms));
        }
        RunEvent::PassEnded { pass, progressed } => {
            put("event", json!("pass_ended"));
            put("pass", json!(pass));
            put("progressed", json!(progressed));
        }
        RunEvent::TasksDeferred { pass, tasks } => {
            put("event", json!("tasks_deferred"));
            put("pass", json!(pass));
            put("tasks", json!(tasks));
        }
        RunEvent::TaskOutputsMissing { task, state, entries } => {
            put("event", json!("task_outputs_missing"));
            put("task", json!(task));
            put("state", json!(state));
            put("entries", json!(entries));
        }
        RunEvent::UsageReported { slot, task, invocation_id, report, usage } => {
            put("event", json!("usage_reported"));
            put("slot", json!(slot));
            put("task", json!(task));
            put("invocation_id", json!(invocation_id));
            // §FS-rhei-run-json.2.1: the record names which report it carries.
            put("report", json!(report_name(*report)));
            put("usage", serde_json::to_value(usage).unwrap_or(Value::Null));
        }
        RunEvent::Message { level, text } => {
            put("event", json!("message"));
            put("level", json!(level_name(*level)));
            put("text", json!(text));
        }
        RunEvent::RunLink { label, url } => {
            put("event", json!("link"));
            put("label", json!(label));
            put("url", json!(url));
        }
        RunEvent::AgentOutput { slot, task, stream, line, .. } => {
            put("event", json!("agent_output"));
            put("slot", json!(slot));
            put("task", json!(task));
            put("stream", json!(stream_name(*stream)));
            put("line", json!(line));
        }
        RunEvent::RunFinished { summary } => {
            put("event", json!("run_finished"));
            put("summary", summary_value(summary));
        }
    }
    map
}

/// Encode one event as a complete record: envelope plus payload.
///
/// `seq` is `None` for records that are not cursor points — `agent_output`,
/// and only that. Numbering it would give the stdout stream and
/// `runtime/events.jsonl` two different sequences for the same run, so
/// `--since` on one would silently skip records of the other.
///
/// `workspace` is the run's root, against which the paths a record carries are
/// relativized.
// §FS-rhei-run-json.2 §FS-rhei-run-json.2.1 §FS-rhei-run-json.2.3
pub fn encode(
    seq: Option<u64>,
    event: &RunEvent,
    at: SystemTime,
    workspace: Option<&Path>,
) -> Value {
    let mut map = payload(event, workspace);
    // Inserted after the payload so the envelope keys win if a variant ever
    // names one, and read back in the same order by `decode`.
    if let Some(seq) = seq {
        map.insert("seq".to_string(), json!(seq));
    }
    map.insert("ts".to_string(), json!(format_rfc3339(at)));
    Value::Object(map)
}

/// The wall-clock the event itself carries, where it carries one. Events
/// without their own timestamp are stamped by the sink at emit time, which is
/// the same instant to the second precision §FS-rhei-run-json.2 records.
pub fn event_wall_clock(event: &RunEvent) -> Option<SystemTime> {
    match event {
        RunEvent::SlotAssigned { wall_clock, .. }
        | RunEvent::SlotReleased { wall_clock, .. }
        | RunEvent::AgentOutput { wall_clock, .. } => Some(*wall_clock),
        _ => None,
    }
}

/// Whether this event belongs in the durable log and the default `--json`
/// stream. Agent output is excluded: the per-task log is its durable form, and
/// including it would make the event log unbounded. §FS-rhei-run-json.2.3
pub fn is_structural(event: &RunEvent) -> bool {
    !matches!(event, RunEvent::AgentOutput { .. })
}

/// The `outcome` vocabulary of a `slot_released` record. §FS-rhei-run-json.2.1
fn outcome_name(outcome: &TaskOutcome) -> &str {
    match outcome {
        TaskOutcome::Completed => "completed",
        TaskOutcome::Failed(_) => "failed",
        TaskOutcome::Waiting => "waiting",
        TaskOutcome::ProviderLimited { .. } => "provider_limited",
        TaskOutcome::Cancelled => "cancelled",
        TaskOutcome::TimedOut => "timeout",
        TaskOutcome::Interrupted => "interrupted",
        // Written back exactly as it was read, which is what makes a reader
        // that re-encodes — `rhei attach --json` — pass the record on
        // unchanged. §FS-rhei-run-json.2.2
        TaskOutcome::Unrecognized(name) => name,
    }
}

fn level_name(level: MessageLevel) -> &'static str {
    match level {
        MessageLevel::Info => "info",
        MessageLevel::Warn => "warn",
        MessageLevel::Error => "error",
    }
}

// §FS-rhei-cost-accounting.7.1: the two reports, on the wire.
fn report_name(report: UsageReport) -> &'static str {
    match report {
        UsageReport::Streamed => "streamed",
        UsageReport::Final => "final",
    }
}

fn stream_name(stream: AgentStream) -> &'static str {
    match stream {
        AgentStream::Stdout => "stdout",
        AgentStream::Stderr => "stderr",
    }
}

fn summary_value(summary: &RunSummary) -> Value {
    json!({
        "agents_spawned": summary.agents_spawned,
        "programs_spawned": summary.programs_spawned,
        "terminal_tasks": summary.terminal_tasks,
        "total_tasks": summary.total_tasks,
        "accounting": summary
            .accounting
            .as_ref()
            .and_then(|a| serde_json::to_value(a).ok())
            .unwrap_or(Value::Null),
        // §FS-rhei-cost-accounting.6: the workspace lifetime total travels
        // beside the run's own, never in place of it.
        "workspace_accounting": summary
            .workspace_accounting
            .as_ref()
            .and_then(|a| serde_json::to_value(a).ok())
            .unwrap_or(Value::Null),
    })
}

// ---------------------------------------------------------------------------
// Decoding — the attach client's half of the contract
// ---------------------------------------------------------------------------

/// One decoded record: the event it carried, the sequence number it was
/// written under if it had one, and the timestamp it was written with.
///
/// `ts` is carried rather than re-derived because a replay must re-emit the
/// instant the run recorded, not the instant it was read back.
// §FS-rhei-run-json.2
pub struct DecodedRecord {
    pub seq: Option<u64>,
    pub ts: SystemTime,
    pub event: RunEvent,
}

/// Parse one record line back into a [`RunEvent`].
///
/// Returns `None` for a line that is not a record this build understands — a
/// blank line, a torn final line from a run still writing, or a record kind
/// added by a newer `rhei`. A reader skips those rather than failing: an
/// unreadable line is one missing update, not a broken surface.
// §FS-rhei-run-json.2
pub fn decode(line: &str) -> Option<DecodedRecord> {
    let value: Value = serde_json::from_str(line.trim()).ok()?;
    // Absent on `agent_output`, which carries no cursor. A record that names
    // `seq` as something other than a number is malformed, though.
    let seq = match value.get("seq") {
        Some(seq) => Some(seq.as_u64()?),
        None => None,
    };
    let kind = value.get("event")?.as_str()?;
    let wall_clock = value
        .get("ts")
        .and_then(Value::as_str)
        .and_then(parse_rfc3339)
        .unwrap_or_else(SystemTime::now);
    let event = decode_event(kind, &value, wall_clock)?;
    Some(DecodedRecord { seq, ts: wall_clock, event })
}

fn decode_event(kind: &str, v: &Value, wall_clock: SystemTime) -> Option<RunEvent> {
    let text = |key: &str| v.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
    let opt_text = |key: &str| v.get(key).and_then(Value::as_str).map(str::to_string);
    let list = |key: &str| {
        v.get(key)
            .and_then(Value::as_array)
            .map(|items| {
                items.iter().filter_map(Value::as_str).map(str::to_string).collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    let num = |key: &str| v.get(key).and_then(Value::as_u64).unwrap_or(0);
    let slot = || num("slot") as u16;
    let path = |key: &str| PathBuf::from(text(key));

    Some(match kind {
        "budget_snapshot" | "budget_reserved" | "budget_settled" | "budget_released"
        | "budget_breach" | "budget_halt" | "budget_started" | "budget_contained" => {
            RunEvent::Budget { event: Box::new(serde_json::from_value(v.clone()).ok()?) }
        }
        "run_started" => RunEvent::RunStarted {
            run_id: text("run_id"),
            workspace: path("workspace"),
            parallel: num("parallel") as u16,
            total_tasks: num("total_tasks") as usize,
        },
        "pass_started" => RunEvent::PassStarted { pass: num("pass") as u32, ready: list("ready") },
        "slot_assigned" => RunEvent::SlotAssigned {
            slot: slot(),
            task: text("task"),
            from: text("from"),
            to: text("to"),
            agent: opt_text("agent"),
            template_context: None,
            log_path: path("log_path"),
            started_at: Instant::now(),
            wall_clock,
        },
        "slot_released" => RunEvent::SlotReleased {
            slot: slot(),
            task: text("task"),
            from: text("from"),
            to: text("to"),
            log_path: path("log_path"),
            outcome: decode_outcome(
                v.get("outcome").and_then(Value::as_str)?,
                opt_text("reason"),
                opt_text("provider"),
                opt_text("next_attempt_at"),
            ),
            finished_at: Instant::now(),
            wall_clock,
            exit_code: v.get("exit_code").and_then(Value::as_i64).map(|code| code as i32),
            duration_ms: num("duration_ms"),
        },
        "pass_ended" => RunEvent::PassEnded {
            pass: num("pass") as u32,
            progressed: v.get("progressed").and_then(Value::as_bool).unwrap_or(false),
        },
        "tasks_deferred" => {
            RunEvent::TasksDeferred { pass: num("pass") as u32, tasks: list("tasks") }
        }
        "task_outputs_missing" => RunEvent::TaskOutputsMissing {
            task: text("task"),
            state: text("state"),
            entries: list("entries"),
        },
        "usage_reported" => RunEvent::UsageReported {
            slot: v.get("slot").and_then(Value::as_u64).map(|s| s as u16),
            task: text("task"),
            invocation_id: text("invocation_id"),
            // §FS-rhei-run-json.2.1: a record written before the field existed
            // decodes as `final`, the report a reader of one record expects.
            report: match v.get("report").and_then(Value::as_str) {
                Some("streamed") => UsageReport::Streamed,
                _ => UsageReport::Final,
            },
            usage: serde_json::from_value::<UsageSummary>(v.get("usage")?.clone()).ok()?,
        },
        "message" => RunEvent::Message {
            level: match v.get("level").and_then(Value::as_str).unwrap_or("info") {
                "warn" => MessageLevel::Warn,
                "error" => MessageLevel::Error,
                _ => MessageLevel::Info,
            },
            text: text("text"),
        },
        "link" => RunEvent::RunLink { label: text("label"), url: text("url") },
        "agent_output" => RunEvent::AgentOutput {
            slot: slot(),
            task: text("task"),
            stream: match v.get("stream").and_then(Value::as_str).unwrap_or("stdout") {
                "stderr" => AgentStream::Stderr,
                _ => AgentStream::Stdout,
            },
            line: text("line"),
            wall_clock,
        },
        "run_finished" => RunEvent::RunFinished { summary: decode_summary(v.get("summary")) },
        _ => return None,
    })
}

/// Read an `outcome` back off the stream, so an attached reader shows the run
/// the words the run itself wrote. A value this build has no variant for is
/// kept under its own name rather than rewritten to one that is: the vocabulary
/// is additive under one `schema`, a reader that stopped on a word it had not
/// met would turn that into an outage, and one that renamed it would report the
/// run as something it never said.
// §FS-rhei-run-json.2.1 §FS-rhei-run-json.2.2
fn decode_outcome(
    name: &str,
    reason: Option<String>,
    provider: Option<String>,
    next_attempt_at: Option<String>,
) -> TaskOutcome {
    match name {
        "completed" => TaskOutcome::Completed,
        "failed" => TaskOutcome::Failed(reason.unwrap_or_default()),
        "waiting" => TaskOutcome::Waiting,
        "provider_limited" => TaskOutcome::ProviderLimited {
            provider: provider.unwrap_or_default(),
            next_attempt_at: next_attempt_at.unwrap_or_default(),
        },
        "cancelled" => TaskOutcome::Cancelled,
        "timeout" => TaskOutcome::TimedOut,
        "interrupted" => TaskOutcome::Interrupted,
        other => TaskOutcome::Unrecognized(other.to_string()),
    }
}

fn decode_summary(value: Option<&Value>) -> RunSummary {
    let Some(value) = value else {
        return RunSummary::default();
    };
    let num = |key: &str| value.get(key).and_then(Value::as_u64).unwrap_or(0);
    RunSummary {
        agents_spawned: num("agents_spawned") as u32,
        programs_spawned: num("programs_spawned") as u32,
        terminal_tasks: num("terminal_tasks") as usize,
        total_tasks: num("total_tasks") as usize,
        accounting: value
            .get("accounting")
            .cloned()
            .and_then(|a| serde_json::from_value::<AccountingRunSummary>(a).ok()),
        workspace_accounting: value
            .get("workspace_accounting")
            .cloned()
            .and_then(|a| serde_json::from_value::<AccountingRunSummary>(a).ok())
            .map(Box::new),
    }
}

// ---------------------------------------------------------------------------
// Timestamps
// ---------------------------------------------------------------------------

/// UTC RFC 3339, second precision — the `ts` format of §FS-rhei-run-json.2.
pub fn format_rfc3339(at: SystemTime) -> String {
    let secs = at.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let (y, mo, d, h, mi, s) = civil_from_epoch(secs);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

/// Parse the format [`format_rfc3339`] writes. Deliberately strict: this reads
/// back only our own output, so a lenient parser would only hide corruption.
pub fn parse_rfc3339(text: &str) -> Option<SystemTime> {
    let bytes = text.as_bytes();
    if bytes.len() != 20 || bytes[19] != b'Z' {
        return None;
    }
    let field = |from: usize, to: usize| text.get(from..to)?.parse::<i64>().ok();
    let (year, month, day) = (field(0, 4)?, field(5, 7)?, field(8, 10)?);
    let (hour, minute, second) = (field(11, 13)?, field(14, 16)?, field(17, 19)?);
    let days = days_from_civil(year, month, day);
    let secs = days * 86_400 + hour * 3600 + minute * 60 + second;
    u64::try_from(secs).ok().map(|s| UNIX_EPOCH + std::time::Duration::from_secs(s))
}

/// Days since the Unix epoch for a civil date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_epoch(secs: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400) as u32;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (y + i64::from(m <= 2), m, d, tod / 3600, (tod % 3600) / 60, tod % 60)
}

#[cfg(test)]
mod tests;
