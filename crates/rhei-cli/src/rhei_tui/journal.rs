use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::rhei_tui::event::{EventSink, RunEvent, TaskOutcome};

/// Append-only writer for `runtime/transitions.log`.
///
/// Each `SlotAssigned` produces one line; its paired `SlotReleased` produces
/// a second line with `exit=`, `duration=`, and `outcome=` key/value pairs.
/// Journal errors never abort a run — they are logged to stderr and swallowed.
pub struct JournalSink {
    inner: Mutex<Inner>,
    workspace_root: PathBuf,
}

struct Inner {
    writer: Option<BufWriter<File>>,
    path: PathBuf,
}

impl JournalSink {
    /// Open (or create) `<workspace_root>/runtime/transitions.log` in append
    /// mode. Missing parent directories are created.
    pub fn open(workspace_root: impl AsRef<Path>) -> std::io::Result<Self> {
        let workspace_root = workspace_root.as_ref().to_path_buf();
        let runtime_dir = workspace_root.join("runtime");
        fs::create_dir_all(&runtime_dir)?;
        let path = runtime_dir.join("transitions.log");
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok(Self {
            workspace_root,
            inner: Mutex::new(Inner { writer: Some(BufWriter::new(file)), path }),
        })
    }

    fn write_line(&self, line: &str) {
        let mut guard = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if let Some(writer) = guard.writer.as_mut() {
            if let Err(err) = writer.write_all(line.as_bytes()) {
                eprintln!("warning: journal write failed: {}", err);
                guard.writer = None;
                return;
            }
            if let Err(err) = writer.flush() {
                eprintln!("warning: journal flush failed: {}", err);
                guard.writer = None;
            }
        }
    }

    /// Resolve a path as workspace-relative when inside the workspace.
    fn format_path(&self, path: &Path) -> String {
        match path.strip_prefix(&self.workspace_root) {
            Ok(rel) => rel.display().to_string(),
            Err(_) => path.display().to_string(),
        }
    }

    /// Path of the underlying journal file (primarily for tests).
    pub fn path(&self) -> PathBuf {
        let guard = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard.path.clone()
    }
}

impl EventSink for JournalSink {
    fn emit(&self, event: RunEvent) {
        match event {
            RunEvent::SlotAssigned { task, from, to, log_path, wall_clock, .. } => {
                let ts = format_rfc3339(wall_clock);
                let log = self.format_path(&log_path);
                // `from == to` means "started work in `to`" with no
                // transition; render it differently so a reader of the
                // journal isn't fooled into thinking the state machine
                // declared a `to → to` self-loop.
                let move_str =
                    if from == to { format!("start@{to}") } else { format!("{from}\u{2192}{to}") };
                let line = format!("{ts}  {task}  {move_str}  {log}\n");
                self.write_line(&line);
            }
            RunEvent::SlotReleased {
                task,
                from,
                to,
                log_path,
                outcome,
                wall_clock,
                exit_code,
                duration_ms,
                ..
            } => {
                let ts = format_rfc3339(wall_clock);
                let log = self.format_path(&log_path);
                // The journal's outcome vocabulary. §FS-rhei-run-tui.1.7
                let outcome_str = match &outcome {
                    TaskOutcome::Completed => "completed",
                    TaskOutcome::Failed(_) => "failed",
                    TaskOutcome::Waiting => "waiting",
                    TaskOutcome::ProviderLimited { .. } => "provider_limited",
                    TaskOutcome::Cancelled => "cancelled",
                    TaskOutcome::TimedOut => "timeout",
                    TaskOutcome::Interrupted => "interrupted",
                    // Only a reader of someone else's stream holds one of
                    // these; it is written as it was read. §FS-rhei-run-json.2.2
                    TaskOutcome::Unrecognized(name) => name.as_str(),
                };
                let mut meta_parts: Vec<String> = Vec::new();
                if let Some(code) = exit_code {
                    meta_parts.push(format!("exit={code}"));
                }
                meta_parts.push(format!("duration={}", format_duration(duration_ms)));
                meta_parts.push(format!("outcome={outcome_str}"));
                if let TaskOutcome::ProviderLimited { provider, next_attempt_at } = &outcome {
                    meta_parts.push(format!("provider={provider}"));
                    meta_parts.push(format!("next_attempt_at={next_attempt_at}"));
                }
                let meta = meta_parts.join(",");
                let move_str =
                    if from == to { format!("end@{to}") } else { format!("{from}\u{2192}{to}") };
                let line = format!("{ts}  {task}  {move_str}  {log}  {meta}\n");
                self.write_line(&line);
            }
            RunEvent::UsageReported { task, invocation_id, usage, .. } => {
                let cost = usage
                    .cost_micro
                    .or(usage.priced_cost_micro)
                    .map(format_cost_micro)
                    .unwrap_or_else(|| "unpriced".to_string());
                let line = format!(
                    "{}  {}  usage  invocation={} agent={} cost={} coverage={:?}\n",
                    format_rfc3339(SystemTime::now()),
                    task,
                    invocation_id,
                    usage.agent,
                    cost,
                    usage.coverage
                );
                self.write_line(&line);
            }
            _ => {}
        }
    }
}

fn format_cost_micro(value: u64) -> String {
    format!("{}.{:06}", value / 1_000_000, value % 1_000_000)
}

fn format_rfc3339(t: SystemTime) -> String {
    let secs = t.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let (year, month, day, hour, minute, second) = civil_from_epoch_secs(secs);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", year, month, day, hour, minute, second)
}

/// Convert a Unix timestamp (UTC) to (year, month, day, hour, minute, second).
///
/// Uses the algorithm from Howard Hinnant's date library (public domain). Valid
/// for all year values representable in i32; we only need recent timestamps.
fn civil_from_epoch_secs(secs: i64) -> (i32, u32, u32, u32, u32, u32) {
    let days = secs.div_euclid(86_400);
    let time_of_day = secs.rem_euclid(86_400) as u32;
    let hour = time_of_day / 3600;
    let minute = (time_of_day % 3600) / 60;
    let second = time_of_day % 60;

    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = y + if m <= 2 { 1 } else { 0 };
    (y as i32, m, d, hour, minute, second)
}

fn format_duration(millis: u64) -> String {
    let total_secs = millis / 1000;
    if total_secs >= 60 {
        let minutes = total_secs / 60;
        let seconds = total_secs % 60;
        format!("{minutes}m{seconds}s")
    } else if total_secs > 0 {
        let frac = (millis % 1000) / 100;
        if frac == 0 {
            format!("{total_secs}s")
        } else {
            format!("{total_secs}.{frac}s")
        }
    } else {
        format!("{millis}ms")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rhei_tui::event::{EventSink, RunEvent, TaskOutcome};
    use std::time::{Duration, Instant, SystemTime};

    fn fixed_time() -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000)
    }

    #[test]
    fn writes_assigned_and_released_lines() {
        let tmp = tempfile::tempdir().unwrap();
        let sink = JournalSink::open(tmp.path()).unwrap();

        let log_rel = tmp.path().join("runtime/logs/task-1-pending.log");
        sink.emit(RunEvent::SlotAssigned {
            slot: 0,
            task: "task-1".to_string(),
            from: "draft".to_string(),
            to: "pending".to_string(),
            agent: None,
            template_context: None,
            log_path: log_rel.clone(),
            started_at: Instant::now(),
            wall_clock: fixed_time(),
        });
        sink.emit(RunEvent::SlotReleased {
            slot: 0,
            task: "task-1".to_string(),
            from: "draft".to_string(),
            to: "pending".to_string(),
            log_path: log_rel,
            outcome: TaskOutcome::Completed,
            finished_at: Instant::now(),
            wall_clock: fixed_time() + Duration::from_millis(3_490),
            exit_code: Some(0),
            duration_ms: 3_490,
        });

        let contents = std::fs::read_to_string(sink.path()).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("task-1  draft\u{2192}pending"));
        assert!(lines[0].ends_with("runtime/logs/task-1-pending.log"));
        assert!(lines[1].contains("exit=0"));
        assert!(lines[1].contains("duration=3.4s"));
        assert!(lines[1].contains("outcome=completed"));
    }

    #[test]
    fn appends_on_second_open() {
        let tmp = tempfile::tempdir().unwrap();
        {
            let sink = JournalSink::open(tmp.path()).unwrap();
            sink.emit(RunEvent::SlotAssigned {
                slot: 0,
                task: "t1".to_string(),
                from: "a".to_string(),
                to: "b".to_string(),
                agent: None,
                template_context: None,
                log_path: tmp.path().join("runtime/logs/t1.log"),
                started_at: Instant::now(),
                wall_clock: fixed_time(),
            });
        }
        {
            let sink = JournalSink::open(tmp.path()).unwrap();
            sink.emit(RunEvent::SlotAssigned {
                slot: 0,
                task: "t2".to_string(),
                from: "a".to_string(),
                to: "b".to_string(),
                agent: None,
                template_context: None,
                log_path: tmp.path().join("runtime/logs/t2.log"),
                started_at: Instant::now(),
                wall_clock: fixed_time(),
            });
        }
        let contents = std::fs::read_to_string(tmp.path().join("runtime/transitions.log")).unwrap();
        assert_eq!(contents.lines().count(), 2);
    }

    /// The durable invocation ledger preserves the provider and UTC retry
    /// instant while recording no authored transition. §FS-rhei-run-tui.1.7
    #[test]
    fn provider_limit_release_is_auditable_without_a_transition() {
        let tmp = tempfile::tempdir().unwrap();
        let sink = JournalSink::open(tmp.path()).unwrap();
        sink.emit(RunEvent::SlotReleased {
            slot: 0,
            task: "task-1".to_string(),
            from: "working".to_string(),
            to: "working".to_string(),
            log_path: tmp.path().join("runtime/logs/task-1-working.log"),
            outcome: TaskOutcome::ProviderLimited {
                provider: "openai".to_string(),
                next_attempt_at: "2026-09-16T20:21:00Z".to_string(),
            },
            finished_at: Instant::now(),
            wall_clock: fixed_time(),
            exit_code: Some(1),
            duration_ms: 10,
        });

        let contents = std::fs::read_to_string(sink.path()).unwrap();
        assert!(contents.contains("end@working"), "{contents}");
        assert!(contents.contains("outcome=provider_limited"), "{contents}");
        assert!(contents.contains("provider=openai"), "{contents}");
        assert!(contents.contains("next_attempt_at=2026-09-16T20:21:00Z"), "{contents}");
    }
}
