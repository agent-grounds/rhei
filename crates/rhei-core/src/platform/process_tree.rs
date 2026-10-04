//! A child process and everything it starts, stopped as one.
//!
//! A bounded callback is a shell running a script that starts a network
//! client; stopping only the shell leaves the client holding the transition.
//! So the child is spawned as the root of a tree of its own and expiry stops
//! the tree. The two halves differ, and the difference is declared: on Linux
//! and macOS the tree is a process group that is asked with `SIGTERM`, given a
//! grace, then killed; on Windows it is a Job Object, and terminating it ends
//! every process in it at once, with no grace, because there is no `SIGTERM`
//! to ask with. §FS-rhei-transitions.4.10 §REQ-cross-platform.2

use std::io;
use std::process::{Child, Command, ExitStatus};
#[cfg(unix)]
use std::time::{Duration, Instant};

/// How often the grace checks whether the tree has gone.
#[cfg(unix)]
const GRACE_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// A child spawned as the root of a process tree of its own.
pub struct ProcessTree {
    child: Child,
    /// `None` when the job could not be made or joined; the tree is then only
    /// its direct child, which is what is stopped.
    #[cfg(windows)]
    job: Option<windows::Job>,
}

impl ProcessTree {
    /// Spawn `command` as the root of a new process tree.
    ///
    /// On Unix the child leads a new process group, which its descendants
    /// inherit. On Windows it is started suspended, assigned to a new Job
    /// Object, and only then resumed, so nothing it starts can escape the job
    /// by starting before the assignment.
    // §FS-rhei-transitions.4.10
    pub fn spawn(command: &mut Command) -> io::Result<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt as _;
            command.process_group(0);
            Ok(Self { child: command.spawn()? })
        }
        #[cfg(windows)]
        {
            windows::spawn(command)
        }
    }

    /// The tree's root.
    pub fn child(&mut self) -> &mut Child {
        &mut self.child
    }

    /// Stop the whole tree and reap its root.
    ///
    /// On Unix: `SIGTERM` to the group, then up to `grace` for the group to
    /// empty — the root reaped and no member left — then `SIGKILL` to a group
    /// that did not empty, so a descendant that outlived the `SIGTERM` is reached. On
    /// Windows the job is terminated at once and `grace` is not used.
    // §FS-rhei-transitions.4.10 §REQ-cross-platform.2
    pub fn stop(&mut self, grace: std::time::Duration) -> io::Result<ExitStatus> {
        #[cfg(unix)]
        {
            self.stop_group(grace)
        }
        #[cfg(windows)]
        {
            let _ = grace;
            match &self.job {
                Some(job) => job.terminate(),
                None => {
                    let _ = self.child.kill();
                }
            }
            self.child.wait()
        }
    }

    #[cfg(unix)]
    fn stop_group(&mut self, grace: Duration) -> io::Result<ExitStatus> {
        let pgid = self.child.id() as i32;
        let _ = signal_group(pgid, GroupSignal::Terminate);
        let deadline = Instant::now() + grace;
        let mut status = None;
        let mut failure = None;
        let mut emptied = false;
        loop {
            if status.is_none() {
                match self.child.try_wait() {
                    Ok(reaped) => status = reaped,
                    // A failed reap is a reason to stop waiting, never to leave
                    // the group alive: the kill below still runs.
                    Err(err) => {
                        failure = Some(err);
                        break;
                    }
                }
            }
            // The root is reaped first, so a zombie root does not keep the
            // group looking alive for the whole grace.
            if status.is_some() && group_is_empty(pgid) {
                emptied = true;
                break;
            }
            if Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(GRACE_POLL_INTERVAL);
        }
        // An emptied group with its leader reaped no longer holds `pgid`, so the
        // id is free and a `SIGKILL` could reach whoever takes it next.
        if !emptied {
            let _ = signal_group(pgid, GroupSignal::Kill);
        }
        // A root that moved itself to another group with `setpgid` is no longer
        // reached through `pgid`; it is killed by name.
        if status.is_none() {
            let _ = self.child.kill();
        }
        if let Some(err) = failure {
            return Err(err);
        }
        match status {
            Some(status) => Ok(status),
            None => self.child.wait(),
        }
    }
}

/// The signal [`signal_group`] sends.
#[cfg(unix)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupSignal {
    /// `SIGTERM`: ask the group to stop.
    Terminate,
    /// `SIGKILL`: end the group now.
    Kill,
}

/// Send `signal` to every process in the group `pgid`. A group that no longer
/// exists is not an error: it is already what the caller wanted.
///
/// The one way rhei signals a process tree on Unix, whether it is a bounded
/// callback's or a supervised agent's or program's.
// §FS-rhei-transitions.4.10 §FS-rhei-run.3.2
#[cfg(unix)]
pub fn signal_group(pgid: i32, signal: GroupSignal) -> io::Result<()> {
    use nix::errno::Errno;
    use nix::sys::signal::{killpg, Signal};
    use nix::unistd::Pid;
    let signal = match signal {
        GroupSignal::Terminate => Signal::SIGTERM,
        GroupSignal::Kill => Signal::SIGKILL,
    };
    match killpg(Pid::from_raw(pgid), signal) {
        Ok(()) | Err(Errno::ESRCH) => Ok(()),
        Err(errno) => Err(io::Error::from(errno)),
    }
}

/// Whether no process is left in the group `pgid`.
#[cfg(unix)]
fn group_is_empty(pgid: i32) -> bool {
    use nix::errno::Errno;
    use nix::sys::signal::killpg;
    use nix::unistd::Pid;
    matches!(killpg(Pid::from_raw(pgid), None), Err(Errno::ESRCH))
}

#[cfg(windows)]
mod windows {
    use super::ProcessTree;
    use std::io;
    use std::os::windows::io::AsRawHandle as _;
    use std::os::windows::process::CommandExt as _;
    use std::process::{Child, Command};
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
    };
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, TerminateJobObject,
    };
    use windows_sys::Win32::System::Threading::{
        OpenThread, ResumeThread, CREATE_SUSPENDED, THREAD_SUSPEND_RESUME,
    };

    /// An owned Job Object handle. Closing it leaves the job's processes
    /// running: only an expiry ends them, never a callback that finished.
    pub(super) struct Job(HANDLE);

    impl Job {
        fn create() -> io::Result<Self> {
            // SAFETY: both pointers may be null: default security, no name.
            let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if handle.is_null() {
                return Err(io::Error::last_os_error());
            }
            Ok(Self(handle))
        }

        fn assign(&self, child: &Child) -> io::Result<()> {
            // SAFETY: both handles are open for the duration of the call.
            let assigned =
                unsafe { AssignProcessToJobObject(self.0, child.as_raw_handle() as HANDLE) };
            if assigned == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }

        pub(super) fn terminate(&self) {
            // SAFETY: the handle is open until `drop`. A failure leaves nothing
            // further to try; the caller reaps the root either way.
            unsafe { TerminateJobObject(self.0, 1) };
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            // SAFETY: the handle was opened by `create` and is closed once.
            unsafe { CloseHandle(self.0) };
        }
    }

    /// Start suspended, join the job, then resume: std cannot hand back the
    /// primary thread's handle, so it is found by enumerating the process's
    /// threads. A job that cannot be made or joined — an enclosing job that
    /// forbids nesting — leaves the tree as only its direct child.
    pub(super) fn spawn(command: &mut Command) -> io::Result<ProcessTree> {
        command.creation_flags(CREATE_SUSPENDED);
        let mut child = command.spawn()?;
        let job = Job::create().and_then(|job| job.assign(&child).map(|()| job)).ok();
        if let Err(err) = resume(child.id()) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(err);
        }
        Ok(ProcessTree { child, job })
    }

    /// Resume every thread of the suspended process `pid`.
    fn resume(pid: u32) -> io::Result<()> {
        // SAFETY: the snapshot handle is checked and closed below, and the
        // entry is sized as the API requires before the first call.
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
            if snapshot == INVALID_HANDLE_VALUE {
                return Err(io::Error::last_os_error());
            }
            let mut entry: THREADENTRY32 = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
            let mut resumed = false;
            let mut more = Thread32First(snapshot, &mut entry) != 0;
            while more {
                if entry.th32OwnerProcessID == pid {
                    let thread = OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID);
                    if !thread.is_null() {
                        resumed |= ResumeThread(thread) != u32::MAX;
                        CloseHandle(thread);
                    }
                }
                more = Thread32Next(snapshot, &mut entry) != 0;
            }
            CloseHandle(snapshot);
            if resumed {
                Ok(())
            } else {
                Err(io::Error::other(format!("could not resume the suspended process {pid}")))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::python;
    use std::path::Path;
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    /// Whether `pid` is still running.
    fn alive(pid: u32) -> bool {
        #[cfg(unix)]
        {
            nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), None).is_ok()
        }
        #[cfg(windows)]
        {
            let filter = format!("PID eq {pid}");
            Command::new("tasklist")
                .args(["/FI", &filter, "/NH", "/FO", "CSV"])
                .output()
                .map(|out| String::from_utf8_lossy(&out.stdout).contains(&format!("\"{pid}\"")))
                .unwrap_or(false)
        }
    }

    /// Spawn `code` under Python as a tree, with `arg` as its one argument.
    fn spawn_python(code: &str, arg: &Path) -> ProcessTree {
        let mut cmd = Command::new(python());
        cmd.arg("-c").arg(code).arg(arg);
        cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        ProcessTree::spawn(&mut cmd).expect("spawn the tree")
    }

    fn wait_for_pid(path: &Path) -> u32 {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(pid) =
                std::fs::read_to_string(path).ok().and_then(|s| s.trim().parse().ok())
            {
                return pid;
            }
            assert!(Instant::now() < deadline, "the tree never recorded its grandchild");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Stopping the tree reaches the grandchild its root started: a process
    /// group on Unix, a Job Object on Windows. §FS-rhei-transitions.4.10
    #[test]
    fn stopping_a_tree_reaches_a_grandchild() {
        let dir = tempfile::tempdir().expect("tempdir");
        let pid_file = dir.path().join("grandchild.pid");
        let mut tree = spawn_python(
            "import pathlib,subprocess,sys; \
             c = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(30)']); \
             pathlib.Path(sys.argv[1]).write_text(str(c.pid)); c.wait()",
            &pid_file,
        );
        let grandchild = wait_for_pid(&pid_file);

        tree.stop(Duration::from_secs(2)).expect("stop the tree");

        let deadline = Instant::now() + Duration::from_secs(10);
        while alive(grandchild) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(!alive(grandchild), "grandchild {grandchild} outlived its tree");
    }

    /// The grace waits for the group, not only its root: a grandchild that
    /// traps `SIGTERM` gets to clean up after the root has already exited.
    /// §FS-rhei-transitions.4.10
    #[cfg(unix)]
    #[test]
    fn the_grace_waits_for_the_whole_group() {
        let dir = tempfile::tempdir().expect("tempdir");
        let marker = dir.path().join("cleaned");
        let mut tree = spawn_python(
            "import pathlib,subprocess,sys,time; \
             p = pathlib.Path(sys.argv[1]); \
             subprocess.Popen([sys.executable, '-c', \
               'import pathlib,signal,sys,time\\n' \
               'p = pathlib.Path(sys.argv[1])\\n' \
               'def stop(s, f):\\n    time.sleep(0.5); p.write_text(\"done\"); sys.exit(0)\\n' \
               'signal.signal(signal.SIGTERM, stop)\\n' \
               'p.with_suffix(\".ready\").write_text(\"1\")\\n' \
               'time.sleep(30)\\n', str(p)]); \
             time.sleep(30)",
            &marker,
        );
        let ready = marker.with_extension("ready");
        let deadline = Instant::now() + Duration::from_secs(10);
        while !ready.exists() {
            assert!(Instant::now() < deadline, "the grandchild never installed its handler");
            std::thread::sleep(Duration::from_millis(50));
        }

        tree.stop(Duration::from_secs(5)).expect("stop the tree");

        assert!(marker.exists(), "the grandchild's SIGTERM handler should have had its grace");
    }
}
