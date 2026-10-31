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
use std::time::{Instant, SystemTime};

use serde_json::{json, Map, Value};

use crate::rhei_tui::event::{
    AccountingRunSummary, AgentStream, MessageLevel, RunEvent, RunProvenance, RunSummary,
    UsageReport, UsageSummary,
};
use crate::rhei_tui::run_stop::RunStop;

mod bounds;
mod outcome;
mod rfc3339;
mod seal;

use bounds::{bound_report, bound_value, ceiling_policy_value};
use outcome::{decode_outcome, put_outcome};
pub use rfc3339::{format_rfc3339, parse_rfc3339};

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
        RunEvent::RunStarted { run_id, workspace, parallel, total_tasks, provenance } => {
            put("event", json!("run_started"));
            put("schema", json!(SCHEMA_VERSION));
            put("run_id", json!(run_id));
            put("workspace", json!(workspace.display().to_string()));
            put("parallel", json!(parallel));
            put("total_tasks", json!(total_tasks));
            // §FS-rhei-run-json.2.1: always present, additions under schema 1.
            put("rhei_build", json!(provenance.rhei_build));
            put("checkout_head_at_start", json!(provenance.checkout_head_at_start));
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
            configuration_fallbacks,
            reverted,
            ..
        } => {
            put("event", json!("slot_released"));
            put("slot", json!(slot));
            put("task", json!(task));
            put("from", json!(from));
            put("to", json!(to));
            put("log_path", json!(record_path(workspace, log_path)));
            put_outcome(outcome, &mut put);
            put("exit_code", json!(exit_code));
            put("duration_ms", json!(duration_ms));
            // Present only when the invocation carried one. §FS-rhei-run-json.2.1
            if !configuration_fallbacks.is_empty() {
                put("configuration_fallbacks", json!(configuration_fallbacks));
            }
            // §FS-rhei-run-json.2.1
            if let Some(location) = reverted {
                put("reverted", json!(location));
            }
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
            // Beside the summary, `null` on a dry run. §FS-rhei-run-json.2.1
            put("evidence_manifest", summary.evidence_manifest.clone().unwrap_or(Value::Null));
        }
        // These two do not move the schema version: a reader that does not
        // know them skips them like any other unrecognized `event`.
        // §FS-rhei-run-json.2.1 §FS-rhei-run-json.2.2
        RunEvent::BudgetSnapshot { bounds } => {
            put("event", json!("budget_snapshot"));
            put("bounds", Value::Array(bounds.iter().map(bound_value).collect()));
            // Absent where nothing was delegated. §FS-rhei-run-json.2.1
            if let Some(policy) = ceiling_policy_value(bounds) {
                put("ceiling_policy", policy);
            }
        }
        RunEvent::BudgetHalt { task, reason_code, bound, renews_at, remedy } => {
            put("event", json!("budget_halt"));
            put("task", json!(task));
            put("reason_code", json!(reason_code));
            let Value::Object(fields) = bound_value(bound) else { unreachable!("object") };
            for (key, value) in fields {
                put(&key, value);
            }
            if let Some(policy) = ceiling_policy_value(std::slice::from_ref(bound)) {
                put("ceiling_policy", policy);
            }
            put("renews_at", json!(renews_at));
            put("remedy", json!(remedy));
        }
        RunEvent::SealHalt { .. } => seal::put_seal_halt(event, &mut put),
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
    let mut value = json!({
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
    });
    // Inserted rather than written above, so the key is *absent* — not null —
    // on a run that did not select the option. §FS-rhei-run-json.2.1
    if let Some(stop) = summary.stop.as_ref() {
        value["stop"] = stop.to_value();
    }
    value
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
        "run_started" => RunEvent::RunStarted {
            run_id: text("run_id"),
            workspace: path("workspace"),
            parallel: num("parallel") as u16,
            total_tasks: num("total_tasks") as usize,
            provenance: RunProvenance::from_fields(v),
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
            configuration_fallbacks: list("configuration_fallbacks"),
            reverted: opt_text("reverted"),
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
        "budget_snapshot" => RunEvent::BudgetSnapshot {
            bounds: v
                .get("bounds")
                .and_then(Value::as_array)
                .map(|bounds| {
                    let policy = v.get("ceiling_policy");
                    bounds.iter().filter_map(|bound| bound_report(bound, policy)).collect()
                })
                .unwrap_or_default(),
        },
        "budget_halt" => RunEvent::BudgetHalt {
            task: text("task"),
            reason_code: text("reason_code"),
            bound: bound_report(v, v.get("ceiling_policy"))?,
            renews_at: v.get("renews_at").and_then(Value::as_str).map(str::to_owned),
            remedy: text("remedy"),
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
        "run_finished" => RunEvent::RunFinished {
            summary: RunSummary {
                evidence_manifest: v.get("evidence_manifest").filter(|m| m.is_object()).cloned(),
                ..decode_summary(v.get("summary"))
            },
        },
        _ => return seal::decode_seal_halt(kind, v),
    })
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
        stop: value.get("stop").and_then(RunStop::from_value),
        evidence_manifest: None,
    }
}

#[cfg(test)]
mod tests;
