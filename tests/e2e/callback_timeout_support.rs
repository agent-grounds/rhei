//! The harness the callback-bound scenarios share: a `rhei` spawn the test
//! itself bounds, and the hung callback's grandchild, found and cleaned up.
//!
//! Until the bound exists rhei ignores `callback_timeout` and waits for a hung
//! callback, so every spawn here carries a wall-clock limit of the harness's
//! own: a case that fails costs its fixture's sleep, never a hung job.
//! §FS-rhei-transitions.4.10

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use super::*;

/// How long the harness lets one `rhei` spawn run before it stops it.
pub const HARNESS_LIMIT: Duration = Duration::from_secs(60);

/// How long a hung callback's grandchild sleeps: far past every bound the
/// scenarios set, well inside [`HARNESS_LIMIT`].
pub const GRANDCHILD_SLEEP_SECS: u64 = 25;

/// A spawn that returned in less than this did not wait for the grandchild.
pub const WELL_BEFORE_THE_SLEEP: Duration = Duration::from_secs(18);

/// One ticket at the gate every machine here starts from.
pub const PLAN: &str = r#"# Rhei: Callback bound

## Tasks

### Task 1: Move under a bound
**State:** gate
"#;

/// The callback the ticket describes: a script that starts a network client and
/// waits on it, where the client never returns. The grandchild inherits the
/// callback's stdout, as `gh` under `bash` does, and records its pid so the
/// test can tell whether the bound reached it.
pub const HANG: &str = r#"import pathlib
import subprocess
import sys

sys.stdin.read()
child = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(SLEEP)'])
pathlib.Path(__file__).with_suffix('.pid').write_text(str(child.pid))
child.wait()
"#;

/// A fixture with its `SLEEP` placeholder filled in.
pub fn sleep_for(script: &str) -> String {
    script.replace("SLEEP", &GRANDCHILD_SLEEP_SECS.to_string())
}

/// One scenario's plan and machine, in a directory of their own.
pub struct Case {
    pub dir: TestDir,
    pub plan: PathBuf,
    pub machine: PathBuf,
}

impl Case {
    pub fn new(prefix: &str, machine_text: &str) -> Self {
        let dir = unique_temp_dir(prefix);
        let plan = write_fixture_file(&dir, "plan.rhei.md", PLAN);
        let machine = write_fixture_file(&dir, "states.yaml", machine_text);
        Case { dir, plan, machine }
    }

    pub fn ledger(&self) -> Option<String> {
        fs::read_to_string(self.dir.join("runtime/state-transitions.log")).ok()
    }

    pub fn transition(&self, to: &str, extra: &[&str]) -> Bounded {
        let current = current_state(&self.plan);
        let mut args = vec!["--task", "1", "--from", current.as_str(), "--to", to];
        args.extend_from_slice(extra);
        rhei_bounded("transition", &self.plan, &self.machine, &args)
    }
}

pub fn current_state(plan: &Path) -> String {
    let text = fs::read_to_string(plan).expect("read plan");
    text.lines()
        .find_map(|line| line.strip_prefix("**State:** "))
        .map(|state| state.trim().to_string())
        .expect("plan should carry a state")
}

pub struct Bounded {
    pub run: CliRun,
    pub elapsed: Duration,
}

impl Bounded {
    pub fn output(&self) -> String {
        format!("stdout:\n{}\nstderr:\n{}", self.run.stdout, self.run.stderr)
    }

    pub fn assert_failed_with(&self, expected: &str) {
        assert!(
            !self.run.status.success(),
            "the callback should have been stopped by its bound and the move refused \
             (returned after {:?})\n{}",
            self.elapsed,
            self.output()
        );
        assert!(
            self.run.stderr.contains(expected),
            "the failure should read `{expected}`\n{}",
            self.output()
        );
        assert!(
            self.elapsed < WELL_BEFORE_THE_SLEEP,
            "rhei returned after {:?}, which is the hung grandchild's sleep, not the bound\n{}",
            self.elapsed,
            self.output()
        );
    }
}

/// Run `rhei <subcommand> <plan> <args>` with the harness's own wall-clock
/// limit. Output goes to files rather than pipes, so a descendant still holding
/// one cannot keep the harness waiting either.
pub fn rhei_bounded(subcommand: &str, plan: &Path, machine: &Path, args: &[&str]) -> Bounded {
    static SPAWNS: AtomicUsize = AtomicUsize::new(0);
    let dir = plan.parent().expect("plan should have a directory");
    let n = SPAWNS.fetch_add(1, Ordering::SeqCst);
    let out_path = dir.join(format!("rhei-{n}.stdout"));
    let err_path = dir.join(format!("rhei-{n}.stderr"));
    let mut cmd = rhei_command(dir.join(".home"));
    cmd.arg("--state-machine").arg(machine).arg(subcommand).arg(plan).args(args);
    cmd.stdin(Stdio::null())
        .stdout(fs::File::create(&out_path).expect("stdout file"))
        .stderr(fs::File::create(&err_path).expect("stderr file"));
    // Its own group, so the harness can stop it with everything it started.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let start = Instant::now();
    let mut child = cmd.spawn().expect("rhei should start");
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll rhei") {
            break status;
        }
        if start.elapsed() > HARNESS_LIMIT {
            stop_tree(&mut child);
            panic!(
                "rhei {subcommand} did not return within {}s\nstdout:\n{}\nstderr:\n{}",
                HARNESS_LIMIT.as_secs(),
                stdout_from_file(&out_path),
                stderr_from_file(&err_path)
            );
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let elapsed = start.elapsed();
    Bounded {
        run: CliRun {
            status,
            stdout: stdout_from_file(&out_path),
            stderr: stderr_from_file(&err_path),
        },
        elapsed,
    }
}

pub fn stop_tree(child: &mut Child) {
    let pid = child.id().to_string();
    #[cfg(unix)]
    let _ = std::process::Command::new("kill").args(["-KILL", "--", &format!("-{pid}")]).status();
    #[cfg(windows)]
    let _ = std::process::Command::new("taskkill").args(["/T", "/F", "/PID", &pid]).status();
    let _ = child.kill();
    let _ = child.wait();
}

pub fn alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
    #[cfg(windows)]
    {
        let filter = format!("PID eq {pid}");
        std::process::Command::new("tasklist")
            .args(["/FI", &filter, "/NH", "/FO", "CSV"])
            .output()
            .map(|out| String::from_utf8_lossy(&out.stdout).contains(&format!("\"{pid}\"")))
            .unwrap_or(false)
    }
}

/// The hung callback's grandchild, found through the pid file it wrote. Dropping
/// it kills whatever is left, so a failing case does not leave a sleeper behind.
pub struct Grandchild {
    pub pid_file: PathBuf,
}

impl Grandchild {
    pub fn pid(&self) -> Option<u32> {
        fs::read_to_string(&self.pid_file).ok()?.trim().parse().ok()
    }

    pub fn assert_stopped(&self) {
        let pid = self.pid().expect("the callback should have recorded its grandchild");
        let deadline = Instant::now() + Duration::from_secs(10);
        while alive(pid) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(!alive(pid), "the callback's grandchild {pid} outlived the bound");
    }
}

impl Drop for Grandchild {
    fn drop(&mut self) {
        let Some(pid) = self.pid() else { return };
        if !alive(pid) {
            return;
        }
        #[cfg(unix)]
        let _ = std::process::Command::new("kill").args(["-KILL", &pid.to_string()]).status();
        #[cfg(windows)]
        let _ =
            std::process::Command::new("taskkill").args(["/F", "/PID", &pid.to_string()]).status();
    }
}

pub fn hang_fixture(dir: &Path, name: &str) -> (PathBuf, Grandchild) {
    let script = write_fixture_file(dir, name, &sleep_for(HANG));
    let pid_file = script.with_extension("pid");
    (script, Grandchild { pid_file })
}
