//! Whether a worker a dead run started is gone, read from what that run wrote
//! about it right after the spawn, and never guessed.
//!
//! The two halves differ, and the difference is declared: on Linux and macOS
//! the worker is its process group, gone only when signalling the group finds
//! nothing; on Windows it is the agent's own process, named by its pid and its
//! creation time, because a pid is reused only once its process has exited.
//! An id that names someone else by now can keep a worker refused, never let
//! one pass for gone. §FS-rhei-evidence.11.2 §REQ-cross-platform.2

/// One process still in a killed worker's group, as the refusal lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveProcess {
    pub pid: u32,
    /// Its command line, or its image where that is all the platform gives.
    pub command: String,
}

/// What reading a killed worker's group found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkerLiveness {
    /// Nothing of it is left.
    Gone,
    /// Something is, or rhei cannot tell that nothing is; the processes it
    /// could list. §FS-rhei-agents.8.1.1
    Running(Vec<LiveProcess>),
}

/// The group `pgid` as a killed run's group record names it: gone only when
/// [`super::process_tree::group_is_empty`] says so, and otherwise every
/// process `ps` lists in it.
// §FS-rhei-evidence.11.2
#[cfg(unix)]
pub fn group_liveness(pgid: i32) -> WorkerLiveness {
    if super::process_tree::group_is_empty(pgid) {
        return WorkerLiveness::Gone;
    }
    WorkerLiveness::Running(group_processes(pgid))
}

/// Every process `ps` lists in group `pgid`, or none where `ps` cannot run.
/// `-A` and the `pid`, `pgid` and `args` columns are common to procps and the
/// BSD `ps` of macOS. §REQ-cross-platform.2
#[cfg(unix)]
fn group_processes(pgid: i32) -> Vec<LiveProcess> {
    let listed = std::process::Command::new("ps")
        .args(["-A", "-o", "pid=", "-o", "pgid=", "-o", "args="])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output();
    let Ok(listed) = listed else { return Vec::new() };
    String::from_utf8_lossy(&listed.stdout)
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let pid = fields.next()?.parse().ok()?;
            let group: i32 = fields.next()?.parse().ok()?;
            let command = fields.collect::<Vec<_>>().join(" ");
            (group == pgid).then_some(LiveProcess { pid, command })
        })
        .collect()
}

/// The creation time of process `pid` as a `FILETIME` count of 100 ns ticks,
/// written beside the pid in the group record; `None` where it cannot be read.
// §FS-rhei-evidence.3.4
#[cfg(windows)]
pub fn process_created_at(pid: u32) -> Option<u64> {
    let process = windows::Process::open(pid).ok()?;
    process.created_at()
}

/// Process `pid` as a killed run's group record names it: gone when opening it
/// finds no process, or one created at another time than `created_at`.
/// "Access denied", and any other failure to open or query it, is not gone.
// §FS-rhei-evidence.11.2
#[cfg(windows)]
pub fn process_liveness(pid: u32, created_at: u64) -> WorkerLiveness {
    let unknown = || WorkerLiveness::Running(vec![LiveProcess { pid, command: String::new() }]);
    let process = match windows::Process::open(pid) {
        Ok(process) => process,
        Err(windows::OpenFailure::NoSuchProcess) => return WorkerLiveness::Gone,
        Err(windows::OpenFailure::Other) => return unknown(),
    };
    match process.created_at() {
        Some(created) if created != created_at => WorkerLiveness::Gone,
        Some(_) => WorkerLiveness::Running(vec![LiveProcess { pid, command: process.image() }]),
        None => unknown(),
    }
}

#[cfg(windows)]
mod windows {
    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, ERROR_INVALID_PARAMETER, FILETIME, HANDLE,
    };
    use windows_sys::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    /// Why a pid could not be opened: it names no process, or something else
    /// went wrong - access denied among them - which says nothing about that.
    pub(super) enum OpenFailure {
        NoSuchProcess,
        Other,
    }

    /// An open process handle, closed on drop.
    pub(super) struct Process(HANDLE);

    impl Process {
        pub(super) fn open(pid: u32) -> Result<Self, OpenFailure> {
            // SAFETY: plain call; a null handle is the documented failure.
            let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
            if !handle.is_null() {
                return Ok(Self(handle));
            }
            // SAFETY: read right after the failing call on this thread.
            match unsafe { GetLastError() } {
                ERROR_INVALID_PARAMETER => Err(OpenFailure::NoSuchProcess),
                _ => Err(OpenFailure::Other),
            }
        }

        pub(super) fn created_at(&self) -> Option<u64> {
            let zero = || FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
            let (mut created, mut exited, mut kernel, mut user) = (zero(), zero(), zero(), zero());
            // SAFETY: the handle is open and every pointer is to a local.
            let read = unsafe {
                GetProcessTimes(self.0, &mut created, &mut exited, &mut kernel, &mut user)
            };
            (read != 0).then(|| {
                (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime)
            })
        }

        /// The process's image path, or nothing where it cannot be read.
        pub(super) fn image(&self) -> String {
            let mut buffer = vec![0u16; 1024];
            let mut size = buffer.len() as u32;
            // SAFETY: the buffer holds `size` wide characters, as the call is told.
            let read = unsafe {
                QueryFullProcessImageNameW(
                    self.0,
                    PROCESS_NAME_WIN32,
                    buffer.as_mut_ptr(),
                    &mut size,
                )
            };
            if read == 0 {
                return String::new();
            }
            String::from_utf16_lossy(&buffer[..size as usize])
        }
    }

    impl Drop for Process {
        fn drop(&mut self) {
            // SAFETY: opened by `open` and closed once.
            unsafe { CloseHandle(self.0) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    /// A group with a process in it is running and lists that process; once it
    /// is killed and reaped, the group is gone. §FS-rhei-evidence.11.2
    #[cfg(unix)]
    #[test]
    fn a_group_is_running_while_a_process_is_left_in_it_and_gone_after() {
        use std::os::unix::process::CommandExt as _;
        let mut child = Command::new("sleep")
            .arg("30")
            .process_group(0)
            .stdin(Stdio::null())
            .spawn()
            .expect("spawn sleep");
        let pgid = child.id() as i32;
        match group_liveness(pgid) {
            WorkerLiveness::Running(processes) => assert!(
                processes.iter().any(|p| p.pid == child.id() && p.command.contains("sleep 30")),
                "the group's process is listed with its command line: {processes:?}"
            ),
            WorkerLiveness::Gone => panic!("a group with a live process is not gone"),
        }
        child.kill().expect("kill");
        child.wait().expect("reap");
        assert_eq!(group_liveness(pgid), WorkerLiveness::Gone);
    }

    /// A zombie still in the group keeps it from being gone: only `ESRCH` is.
    #[cfg(unix)]
    #[test]
    fn an_unreaped_zombie_keeps_its_group_from_being_gone() {
        use std::os::unix::process::CommandExt as _;
        let mut child = Command::new("true").process_group(0).spawn().expect("spawn true");
        let pgid = child.id() as i32;
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(
            matches!(group_liveness(pgid), WorkerLiveness::Running(_)),
            "an exited but unreaped leader is still in its group"
        );
        child.wait().expect("reap");
        assert_eq!(group_liveness(pgid), WorkerLiveness::Gone);
    }

    /// A running process is not gone while its creation time is the recorded
    /// one, and is gone - reused - when the record names another time, or once
    /// it has exited and nothing holds it open. §FS-rhei-evidence.11.2
    #[cfg(windows)]
    #[test]
    fn a_process_is_gone_only_when_absent_or_created_at_another_time() {
        let mut child = Command::new("ping")
            .args(["-n", "30", "127.0.0.1"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .spawn()
            .expect("spawn");
        let pid = child.id();
        let created = process_created_at(pid).expect("creation time of a live process");
        assert!(matches!(process_liveness(pid, created), WorkerLiveness::Running(_)));
        assert_eq!(process_liveness(pid, created + 1), WorkerLiveness::Gone, "a reused pid");
        child.kill().expect("kill");
        child.wait().expect("reap");
        // The `Child` holds the process open until it is dropped.
        drop(child);
        assert_eq!(process_liveness(pid, created), WorkerLiveness::Gone);
    }
}
