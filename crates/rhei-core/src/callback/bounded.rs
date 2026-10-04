//! A callback run held to its `callback_timeout`.
//!
//! The bound covers the whole run: writing the context to stdin, waiting for
//! the callback to exit, and reading its output to the end. Each of those is
//! where a callback can hang — one that never reads a large payload, one that
//! never exits, one that exits while a background child still holds its
//! stdout — so stdin and both output pipes are served on helper threads and the
//! caller's thread only waits, against one deadline. §FS-rhei-transitions.4.10

use super::{CallbackBound, CallbackError};
use crate::platform::process_tree::ProcessTree;
use std::io::{self, Read, Write};
use std::process::{Command, Output};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// The grace a stopped callback's tree gets between `SIGTERM` and `SIGKILL` on
/// Linux and macOS: the one `program_timeout` and `agent_timeout` use
/// (§FS-rhei-agents.7.3). Windows has no grace. §FS-rhei-transitions.4.10
const TERMINATE_GRACE: Duration = Duration::from_secs(10);

/// How long the output pipes are waited for once the tree is stopped. A
/// descendant that escaped the tree may still hold one open; after this the
/// readers are abandoned with what they captured. §FS-rhei-transitions.4.10
const DRAIN_WINDOW: Duration = Duration::from_secs(2);

/// How often the wait checks the child and the helper threads.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// How a bounded run ended.
pub(super) enum Outcome {
    /// The callback exited and its output closed within the bound.
    Finished(Output),
    /// The bound expired; the tree was stopped, and this is what it had written.
    Expired { stdout: Vec<u8>, stderr: Vec<u8> },
}

/// Run `cmd` under `bound`, delivering `payload` on its stdin when present.
// §FS-rhei-transitions.4.10
pub(super) fn run(
    cmd: &mut Command,
    payload: Option<Vec<u8>>,
    bound: &CallbackBound,
    command: &str,
) -> Result<Outcome, CallbackError> {
    let deadline = Instant::now() + bound.limit;
    let mut tree =
        ProcessTree::spawn(cmd).map_err(|e| CallbackError::SpawnFailed(command.to_string(), e))?;
    let child = tree.child();
    // Without a payload the stdin is dropped here, so reads see EOF at once.
    let writer = match (payload, child.stdin.take()) {
        (Some(payload), Some(stdin)) => {
            Some(std::thread::spawn(move || write_payload(stdin, &payload)))
        }
        _ => None,
    };
    let stdout = Pipe::read(child.stdout.take());
    let stderr = Pipe::read(child.stderr.take());

    let mut status = None;
    while Instant::now() < deadline {
        match tree.child().try_wait() {
            Ok(Some(exited)) => {
                status = Some(exited);
                break;
            }
            Ok(None) => std::thread::sleep(POLL_INTERVAL),
            Err(err) => {
                let _ = tree.stop(TERMINATE_GRACE);
                return Err(CallbackError::SpawnFailed(command.to_string(), err));
            }
        }
    }
    // Exiting is not the end of the run: the output has to close too, and a
    // background child that holds it open is still inside the bound.
    if let Some(status) = status {
        if settled(&[&stdout, &stderr], writer.as_ref(), deadline) {
            if let Some(Ok(Err(err))) = writer.map(JoinHandle::join) {
                return Err(CallbackError::StdinWriteFailed(command.to_string(), err));
            }
            return Ok(Outcome::Finished(Output {
                status,
                stdout: stdout.captured(),
                stderr: stderr.captured(),
            }));
        }
    }

    let _ = tree.stop(TERMINATE_GRACE);
    settled(&[&stdout, &stderr], None, Instant::now() + DRAIN_WINDOW);
    Ok(Outcome::Expired { stdout: stdout.captured(), stderr: stderr.captured() })
}

/// Write the payload and close stdin. `BrokenPipe` is the callback closing its
/// stdin early, which is not a failure: its exit and output are the truth.
fn write_payload(mut stdin: impl Write, payload: &[u8]) -> io::Result<()> {
    match stdin.write_all(payload) {
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        other => other,
    }
}

/// Whether every pipe has closed and the writer has finished by `deadline`.
fn settled(
    pipes: &[&Pipe],
    writer: Option<&JoinHandle<io::Result<()>>>,
    deadline: Instant,
) -> bool {
    loop {
        let done = pipes.iter().all(|pipe| pipe.closed())
            && writer.is_none_or(|writer| writer.is_finished());
        if done {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// One output pipe, read to its end on a thread of its own into a buffer the
/// caller can take at any time — so an abandoned reader still gives up what it
/// read.
struct Pipe {
    buffer: Arc<Mutex<Vec<u8>>>,
    reader: Option<JoinHandle<()>>,
}

impl Pipe {
    fn read(source: Option<impl Read + Send + 'static>) -> Self {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let reader = source.map(|mut source| {
            let buffer = Arc::clone(&buffer);
            std::thread::spawn(move || {
                let mut chunk = [0u8; 8192];
                loop {
                    match source.read(&mut chunk) {
                        Ok(0) => break,
                        Ok(n) => buffer
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .extend_from_slice(&chunk[..n]),
                        Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                        Err(_) => break,
                    }
                }
            })
        });
        Self { buffer, reader }
    }

    fn closed(&self) -> bool {
        self.reader.as_ref().is_none_or(JoinHandle::is_finished)
    }

    fn captured(&self) -> Vec<u8> {
        self.buffer.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }
}

#[cfg(test)]
mod tests {
    use super::super::{CallbackContext, CallbackExecutor, ShellCallbackExecutor};
    use super::*;
    use crate::ast::CallbackRef;
    use crate::test_support::python;
    use serde_json::{json, Value as JsonValue};
    use std::path::Path;

    /// Long past every bound below, so only the bound can end the run early.
    const HANG_SECS: u64 = 30;
    /// Well short of [`HANG_SECS`]: a run that returned in less than this was
    /// stopped by its bound rather than waited out.
    const STOPPED_WITHIN: Duration = Duration::from_secs(20);

    fn python_callback(code: &str) -> CallbackRef {
        CallbackRef(format!("cli:{} -c \"{code}\"", python()))
    }

    fn bounded(secs: u64, authored: &str) -> ShellCallbackExecutor {
        ShellCallbackExecutor::new(Some(CallbackBound {
            limit: Duration::from_secs(secs),
            authored: authored.to_string(),
        }))
    }

    fn ctx(payload: Option<&JsonValue>) -> CallbackContext<'_> {
        CallbackContext {
            task_id: "1",
            task_id_local: "1",
            from_state: "gate",
            to_state: "opening",
            firing_id: None,
            ledger_status: None,
            plan_path: Path::new("plan.rhei.md"),
            callback_cwd: Path::new("."),
            model: None,
            agent: None,
            context_json: payload,
        }
    }

    fn assert_expired(
        executor: &ShellCallbackExecutor,
        callback: &CallbackRef,
        payload: Option<&JsonValue>,
        authored: &str,
    ) {
        let started = Instant::now();
        let result = executor
            .execute(callback, &ctx(payload))
            .expect("an expired bound is a failed result, not an executor error");
        let elapsed = started.elapsed();
        assert!(!result.success, "an expired callback rejects");
        assert_eq!(
            result.error.as_deref(),
            Some(
                format!("exceeded callback_timeout {authored}; its process tree was stopped")
                    .as_str()
            )
        );
        assert!(
            elapsed < STOPPED_WITHIN,
            "the bound should have stopped the run, took {elapsed:?}"
        );
    }

    // §FS-rhei-transitions.4.10
    #[test]
    fn expiry_is_a_failed_result_naming_the_bound_as_authored() {
        let callback = python_callback(&format!("import time; time.sleep({HANG_SECS})"));
        // Authored in a longer spelling than its value, to show it is not re-rendered.
        assert_expired(&bounded(1, "0m1s"), &callback, None, "0m1s");
    }

    // §FS-rhei-transitions.4.10
    #[test]
    fn a_callback_that_never_reads_a_payload_larger_than_the_pipe_is_still_bounded() {
        let callback = python_callback(&format!("import time; time.sleep({HANG_SECS})"));
        let payload = json!({ "padding": "x".repeat(4 * 1024 * 1024) });
        assert_expired(&bounded(1, "1s"), &callback, Some(&payload), "1s");
    }

    // §FS-rhei-transitions.4.10
    #[test]
    fn a_callback_that_exits_while_a_background_child_holds_its_stdout_is_bounded() {
        let callback = python_callback(&format!(
            "import subprocess,sys; subprocess.Popen([sys.executable, '-c', 'import time; time.sleep({HANG_SECS})'])"
        ));
        assert_expired(&bounded(1, "1s"), &callback, None, "1s");
    }

    /// Inside its bound a callback reads, writes and exits exactly as it does
    /// unbounded: stdin delivered, JSON parsed, a non-zero exit a crash.
    // §FS-rhei-transitions.4.10
    #[test]
    fn within_its_bound_a_callback_behaves_as_unbounded() {
        let payload = json!({ "task": { "id": "99" } });
        let echo = python_callback(
            "import json,sys;t=json.load(sys.stdin)['task']['id'];sys.stdout.write(json.dumps({'success': True, 'data': {'id': t}}))",
        );
        let crash = python_callback("import sys;sys.stderr.write('boom');sys.exit(3)");
        for executor in [ShellCallbackExecutor::default(), bounded(30, "30s")] {
            let result = executor.execute(&echo, &ctx(Some(&payload))).expect("runs");
            assert!(result.success, "{:?}", result.error);
            assert_eq!(result.data, Some(json!({ "id": "99" })));

            let result = executor.execute(&crash, &ctx(None)).expect("runs");
            assert!(!result.success);
            assert_eq!(result.error.as_deref(), Some("callback crashed (exit 3): boom"));
        }
    }

    /// No bound is today's behaviour: a callback that takes longer than any
    /// bound here is waited for.
    // §FS-rhei-transitions.4.10
    #[test]
    fn without_a_bound_a_slow_callback_is_waited_for() {
        let callback = python_callback("import time; time.sleep(2)");
        let result = ShellCallbackExecutor::default().execute(&callback, &ctx(None)).expect("runs");
        assert!(result.success, "{:?}", result.error);
    }
}
