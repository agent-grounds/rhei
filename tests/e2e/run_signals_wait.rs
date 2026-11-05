//! The bounded wait for `rhei run` to end, and what it says when it runs out.
//! §REQ-cross-platform.7 §FS-rhei-run-tui.1.8

use super::*;

/// Wait for `rhei run` to exit, rather than blocking forever if it does not.
pub(super) fn wait_for_exit(
    child: &mut std::process::Child,
    timeout: std::time::Duration,
) -> std::process::ExitStatus {
    wait_for_exit_reporting(child, timeout, String::new)
}

/// [`wait_for_exit`], whose failure also carries what `report` says.
///
/// `report` runs before the run is killed, because the kill ends what the run
/// started and whether those were still alive is what the report is for
/// (§REQ-cross-platform.7). The words a search for this failure looks for stay
/// first, on a line of their own.
pub(super) fn wait_for_exit_reporting(
    child: &mut std::process::Child,
    timeout: std::time::Duration,
    report: impl FnOnce() -> String,
) -> std::process::ExitStatus {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait().expect("try_wait") {
            Some(status) => return status,
            None if std::time::Instant::now() >= deadline => {
                let message = format!("rhei run did not exit within {timeout:?}\n{}", report());
                let _ = child.kill();
                let _ = child.wait();
                panic!("{}", message.trim_end());
            }
            None => std::thread::sleep(std::time::Duration::from_millis(20)),
        }
    }
}

/// What a run that outlived its lost terminal leaves to read while it still can
/// be read: whether what it started is alive, what that means, and its stderr.
///
/// That is what tells a print that never failed from an exit that got stuck, which
/// the bare panic of agent-grounds/rhei#554 could not. §REQ-cross-platform.7
pub(super) fn lost_terminal_report(dir: &Path, agent: &str, grandchild: &str) -> String {
    let agent_alive = pid_is_alive(agent);
    let grandchild_alive = pid_is_alive(grandchild);
    let reading = match (agent_alive, grandchild_alive) {
        (true, true) => {
            "the run never began to terminate them, so no print has failed: none came, \
             or it landed before the terminal hung up"
        }
        (false, false) => "the run began to terminate them and got stuck on its way out",
        _ => {
            "the run began to terminate them and got no further, or one ended by itself; \
             `kill -0` also answers for a zombie the run has not reaped"
        }
    };
    format!(
        "agent {agent} alive: {agent_alive}; grandchild {grandchild} alive: {grandchild_alive}\n\
         {reading}\nstderr:\n{}",
        read_run_stderr(dir)
    )
}

/// A wait that runs out reads the run before killing it and keeps the words a
/// search looks for. One live process stands in for the run and what it started,
/// so the same report reads the other way once the wait has killed it.
// §REQ-cross-platform.7 §FS-rhei-run-tui.1.8
#[test]
fn a_wait_that_runs_out_reports_what_it_read_before_the_kill() {
    use std::process::Stdio;

    let dir = unique_temp_dir("run-wait-report");
    fs::write(dir.join("run.err"), "what the run said\n").expect("write run stderr");
    let mut stand_in = KillOnDrop(
        Command::new("sleep")
            .arg("300")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("sleep should start"),
    );
    let pid = stand_in.id().to_string();

    let waited = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        wait_for_exit_reporting(&mut stand_in, std::time::Duration::from_millis(50), || {
            lost_terminal_report(&dir, &pid, &pid)
        })
    }));

    let panic = waited.expect_err("a child that does not exit should run the wait out");
    let message = panic.downcast_ref::<String>().expect("a formatted failure message");
    assert!(message.starts_with("rhei run did not exit within 50ms\n"), "{message}");
    assert!(message.contains("never began to terminate"), "read after the kill: {message}");
    assert!(message.contains("what the run said"), "the run's stderr is missing: {message}");

    assert!(
        stand_in.try_wait().expect("try_wait").is_some(),
        "the wait should still kill what it gave up on"
    );
    let after_the_kill = lost_terminal_report(&dir, &pid, &pid);
    assert!(after_the_kill.contains("got stuck on its way out"), "{after_the_kill}");
}
