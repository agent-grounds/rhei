// §AR-rhei-viz-flow.7: the host side of the one mutation boundary.

// `RunInterveneSink` keeps a per-slot registry of open agent-stdin writers, delivers
// `/intervene` messages to selected stdin handles and nothing else, and records every
// delivery (and every failure reason) to a durable audit trail at `runtime/interventions.log`,
// mirroring successful messages into the task's agent transcript.
//
// There is no path from here to plan-state mutation: a delivery resolves a task,
// sends bytes to that task's child stdin, logs, and returns.

use std::sync::mpsc::{channel, Sender};

struct InterveneWrite {
    bytes: Vec<u8>,
    ack: Sender<Result<(), String>>,
}

/// One registered running invocation: a channel to its stdin writer thread, the
/// shared handle to its durable agent log (so a mirror line cannot interleave
/// with the agent's own output), and identifying metadata for the audit trail.
struct InterveneTarget {
    task_id: String,
    stdin_tx: Sender<InterveneWrite>,
    log_file: Arc<Mutex<fs::File>>,
    slot: rhei_tui::Slot,
    state: String,
    stdin_format: AgentStdinFormat,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct InterveneKey {
    task_id: String,
    slot: rhei_tui::Slot,
}

pub(crate) struct RunInterveneSink {
    runtime_dir: PathBuf,
    targets: Mutex<HashMap<InterveneKey, InterveneTarget>>,
    audit: Mutex<()>,
}

impl RunInterveneSink {
    pub(crate) fn new(runtime_dir: PathBuf) -> Self {
        Self { runtime_dir, targets: Mutex::new(HashMap::new()), audit: Mutex::new(()) }
    }

    /// Register a running task's stdin for intervention. The provided
    /// `child_stdin` is moved into a dedicated writer thread that holds it open
    /// for the process lifetime and writes any delivered messages; the thread
    /// exits (closing stdin) when the task is unregistered.
    pub(crate) fn register(
        &self,
        task_id: &str,
        slot: rhei_tui::Slot,
        state: &str,
        log_file: Arc<Mutex<fs::File>>,
        child_stdin: std::process::ChildStdin,
        stdin_format: AgentStdinFormat,
    ) {
        let (tx, rx) = channel::<InterveneWrite>();
        std::thread::spawn(move || {
            let mut stdin = child_stdin;
            while let Ok(write) = rx.recv() {
                let result = stdin
                    .write_all(&write.bytes)
                    .and_then(|()| stdin.flush())
                    .map_err(|err| err.to_string());
                let failed = result.is_err();
                let _ = write.ack.send(result);
                if failed {
                    break;
                }
            }
            // Sender dropped (task unregistered) or write failed: drop stdin,
            // closing the pipe.
        });
        if let Ok(mut targets) = self.targets.lock() {
            let key = InterveneKey { task_id: task_id.to_string(), slot };
            targets.insert(
                key,
                InterveneTarget {
                    task_id: task_id.to_string(),
                    stdin_tx: tx,
                    log_file,
                    slot,
                    state: state.to_string(),
                    stdin_format,
                },
            );
        }
    }

    /// Drop a task's registration once its agent exits. Dropping the stored
    /// sender ends the writer thread, which closes the child's stdin.
    pub(crate) fn unregister(&self, task_id: &str, slot: rhei_tui::Slot) {
        if let Ok(mut targets) = self.targets.lock() {
            targets.remove(&InterveneKey { task_id: task_id.to_string(), slot });
        }
    }

    /// Append one line to the durable audit trail. Serialized through `audit`
    /// so concurrent deliveries do not interleave.
    fn audit(&self, task_id: &str, slot: Option<rhei_tui::Slot>, state: &str, message: &str, outcome: &str) {
        let _guard = self.audit.lock();
        let path = self.runtime_dir.join("interventions.log");
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let slot = slot.map(|s| s.to_string()).unwrap_or_else(|| "-".to_string());
        // Keep the audit line single-row: collapse newlines in the message.
        let flat = message.replace(['\n', '\r'], " ");
        if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(&path) {
            let _ = writeln!(
                file,
                "{ts} slot={slot} task={task_id} state={state} bytes={len} outcome={outcome} message={flat}",
                ts = format_iso8601_utc(std::time::SystemTime::now()),
                len = message.len(),
            );
        }
    }
}

impl rhei_tui::InterveneSink for RunInterveneSink {
    /// Reachable while the writer thread still holds the child's stdin open (i.e.
    /// registered). Shares the registry `deliver` consults, so the composer gate
    /// and delivery never disagree. §FS-rhei-viz.5
    fn reachable(&self, task_id: &str, slot: Option<rhei_tui::Slot>) -> bool {
        let Ok(targets) = self.targets.lock() else {
            return false;
        };
        match slot {
            Some(slot) => {
                targets.contains_key(&InterveneKey { task_id: task_id.to_string(), slot })
            }
            None => targets.values().any(|target| target.task_id == task_id),
        }
    }

    fn deliver(
        &self,
        task_id: Option<&str>,
        slot: Option<rhei_tui::Slot>,
        message: &str,
    ) -> Result<(), String> {
        // Snapshot the target's identifying metadata + channel under the lock,
        // then release it before writing so a slow agent cannot block others.
        let targets = {
            let targets = self.targets.lock().map_err(|_| "intervene registry poisoned".to_string())?;
            let mut selected = Vec::new();
            if let Some(slot) = slot {
                if let Some(task_id) = task_id {
                    if let Some(target) =
                        targets.get(&InterveneKey { task_id: task_id.to_string(), slot })
                    {
                        selected.push(target);
                    }
                } else if let Some(target) = targets.values().find(|target| target.slot == slot) {
                    selected.push(target);
                }
            } else if let Some(task_id) = task_id {
                selected.extend(targets.values().filter(|target| target.task_id == task_id));
            }

            selected
                .into_iter()
                .map(|target| {
                    (
                        target.task_id.clone(),
                        target.stdin_tx.clone(),
                        target.log_file.clone(),
                        target.slot,
                        target.state.clone(),
                        target.stdin_format,
                    )
                })
                .collect::<Vec<_>>()
        };

        if targets.is_empty() {
            self.audit(task_id.unwrap_or("-"), slot, "", message, "unreachable");
            return Err(
                "agent not interactively reachable (one-shot agent, or task not running)"
                    .to_string(),
            );
        }
        if slot.is_none() && task_id.is_some() && targets.len() > 1 {
            self.audit(task_id.unwrap_or("-"), None, "", message, "ambiguous");
            return Err("multiple agents are running for this task; retry with slot".to_string());
        }

        let mut delivered = 0usize;
        let mut failed = 0usize;
        for (task_id, tx, log_file, slot, state, stdin_format) in targets {
            let bytes = stdin_message_bytes(stdin_format, message);
            let (ack_tx, ack_rx) = channel();
            if tx.send(InterveneWrite { bytes, ack: ack_tx }).is_err() {
                self.audit(&task_id, Some(slot), &state, message, "stdin-closed");
                failed += 1;
                continue;
            }
            match ack_rx.recv() {
                Ok(Ok(())) => {}
                Ok(Err(_)) | Err(_) => {
                    self.audit(&task_id, Some(slot), &state, message, "stdin-closed");
                    failed += 1;
                    continue;
                }
            }

            // Mirror into the durable transcript so the message appears inline
            // in the agent log and the live terminal (§FS-rhei-viz.5, item 1 of §AR-rhei-viz-flow.10).
            let _ = with_agent_log(&log_file, |f| {
                writeln!(
                    f,
                    "[intervene {}] {message}",
                    format_iso8601_utc(std::time::SystemTime::now())
                )
            });
            self.audit(&task_id, Some(slot), &state, message, "delivered");
            delivered += 1;
        }

        match (delivered, failed) {
            (0, _) => Err("agent stdin is closed".to_string()),
            (_, 0) => Ok(()),
            _ => Err(format!(
                "delivered to {delivered} agent(s), but {failed} active agent stdin stream(s) were closed"
            )),
        }
    }
}

#[cfg(test)]
mod intervene_tests {
    use super::*;
    use std::time::Duration;

    /// A sink for the writes a delivery makes, in a directory this harness
    /// empties on the way in.
    ///
    /// One directory per process, emptied on the way in: these files are never
    /// read back, so the cost of the pid is one leftover directory per `cargo
    /// test` run, and the cost of leaving it out is a second test binary — of
    /// which `--all-targets` runs several at once — deleting this one's log
    /// directory while a delivery is writing into it.
    fn temp_log(name: &str) -> Arc<Mutex<fs::File>> {
        static LOGS: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
        let dir = LOGS.get_or_init(|| {
            let dir = std::env::temp_dir()
                .join(format!("rhei-intervene-logs-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).expect("intervention log directory");
            dir
        });
        Arc::new(Mutex::new(
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(dir.join(format!("{name}.log")))
                .expect("open temp intervention log"),
        ))
    }

    fn fake_target(
        task_id: &str,
        slot: rhei_tui::Slot,
        name: &str,
    ) -> (InterveneTarget, std::sync::mpsc::Receiver<InterveneWrite>) {
        fake_target_with_format(task_id, slot, name, AgentStdinFormat::PlainLine)
    }

    fn fake_target_with_format(
        task_id: &str,
        slot: rhei_tui::Slot,
        name: &str,
        stdin_format: AgentStdinFormat,
    ) -> (InterveneTarget, std::sync::mpsc::Receiver<InterveneWrite>) {
        let (tx, rx) = channel::<InterveneWrite>();
        (
            InterveneTarget {
                task_id: task_id.to_string(),
                stdin_tx: tx,
                log_file: temp_log(name),
                slot,
                state: "review".to_string(),
                stdin_format,
            },
            rx,
        )
    }

    #[test]
    fn unregister_removes_only_the_finished_slot_for_a_task() {
        let sink = RunInterveneSink::new(std::env::temp_dir());
        let (first_target, first_rx) = fake_target("1", 0, "first");
        let (second_target, second_rx) = fake_target("1", 1, "second");

        {
            let mut targets = sink.targets.lock().expect("targets lock");
            targets.insert(InterveneKey { task_id: "1".to_string(), slot: 0 }, first_target);
            targets.insert(InterveneKey { task_id: "1".to_string(), slot: 1 }, second_target);
        }

        sink.unregister("1", 0);
        let writer = std::thread::spawn(move || {
            let write = second_rx
                .recv_timeout(Duration::from_millis(100))
                .expect("message reaches sibling");
            assert_eq!(write.bytes, b"still running\n".to_vec());
            write.ack.send(Ok(())).expect("ack write");
        });
        rhei_tui::InterveneSink::deliver(&sink, Some("1"), None, "still running")
            .expect("deliver to remaining slot");
        writer.join().expect("writer thread");

        assert!(matches!(
            first_rx.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty | std::sync::mpsc::TryRecvError::Disconnected)
        ));
    }

    #[test]
    fn task_only_delivery_rejects_ambiguous_fanout() {
        let sink = RunInterveneSink::new(std::env::temp_dir());
        let (first_target, first_rx) = fake_target("1", 0, "ambiguous-first");
        let (second_target, second_rx) = fake_target("1", 1, "ambiguous-second");

        {
            let mut targets = sink.targets.lock().expect("targets lock");
            targets.insert(InterveneKey { task_id: "1".to_string(), slot: 0 }, first_target);
            targets.insert(InterveneKey { task_id: "1".to_string(), slot: 1 }, second_target);
        }

        let err = rhei_tui::InterveneSink::deliver(&sink, Some("1"), None, "ambiguous")
            .expect_err("task-only delivery must not broadcast to fanout siblings");

        assert!(err.contains("retry with slot"));
        assert!(first_rx.try_recv().is_err());
        assert!(second_rx.try_recv().is_err());
    }

    #[test]
    fn slot_and_task_delivery_rejects_stale_exact_match() {
        let sink = RunInterveneSink::new(std::env::temp_dir());
        let (target, rx) = fake_target("1", 1, "stale-slot");

        {
            let mut targets = sink.targets.lock().expect("targets lock");
            targets.insert(InterveneKey { task_id: "1".to_string(), slot: 1 }, target);
        }

        let err = rhei_tui::InterveneSink::deliver(&sink, Some("1"), Some(0), "stale")
            .expect_err("stale slot should not fall back to task-only delivery");

        assert!(err.contains("not interactively reachable"));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn delivery_waits_for_writer_ack_before_reporting_success() {
        let sink = RunInterveneSink::new(std::env::temp_dir());
        let (target, rx) = fake_target("1", 0, "write-failure");

        {
            let mut targets = sink.targets.lock().expect("targets lock");
            targets.insert(InterveneKey { task_id: "1".to_string(), slot: 0 }, target);
        }

        let writer = std::thread::spawn(move || {
            let write = rx.recv_timeout(Duration::from_millis(100)).expect("message reaches writer");
            assert_eq!(write.bytes, b"closed\n".to_vec());
            write.ack.send(Err("broken pipe".to_string())).expect("ack write failure");
        });
        let err = rhei_tui::InterveneSink::deliver(&sink, Some("1"), Some(0), "closed")
            .expect_err("write failure must be reported");
        writer.join().expect("writer thread");

        assert!(err.contains("stdin is closed"));
    }

    #[test]
    fn claude_stream_json_targets_receive_json_intervention_messages() {
        let sink = RunInterveneSink::new(std::env::temp_dir());
        let (target, rx) =
            fake_target_with_format("1", 0, "claude-json", AgentStdinFormat::ClaudeCodeStreamJson);

        {
            let mut targets = sink.targets.lock().expect("targets lock");
            targets.insert(InterveneKey { task_id: "1".to_string(), slot: 0 }, target);
        }

        let writer = std::thread::spawn(move || {
            let write = rx.recv_timeout(Duration::from_millis(100)).expect("message reaches writer");
            let line = String::from_utf8(write.bytes).expect("utf8 json line");
            let json: serde_json::Value =
                serde_json::from_str(line.trim_end()).expect("valid stream-json message");
            assert_eq!(json["type"], "user");
            assert_eq!(json["message"]["role"], "user");
            assert_eq!(json["message"]["content"][0]["text"], "hello claude");
            write.ack.send(Ok(())).expect("ack write");
        });
        rhei_tui::InterveneSink::deliver(&sink, Some("1"), Some(0), "hello claude")
            .expect("deliver stream-json message");
        writer.join().expect("writer thread");
    }
}
