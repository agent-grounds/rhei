//! Controlled descriptor reads and process cleanup for recoverable outages.
//! §FS-rhei-run-headless.5.3

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;
use std::process::{Child, Output, Stdio};
use std::time::Duration;

use nix::sys::signal::{kill, Signal};
use nix::unistd::{mkfifo, Pid};

use super::super::headless_support::wait_until;
use super::{descriptor_pid, run_lock, Workspace};

/// Restore modes before Workspace's cleanup, including during unwinding.
/// §FS-rhei-run-headless.5.3
pub(super) struct RestoreRunLock<'a>(pub(super) &'a Workspace);

impl Drop for RestoreRunLock<'_> {
    fn drop(&mut self) {
        let _ = fs::set_permissions(run_lock(self.0), fs::Permissions::from_mode(0o644));
    }
}

/// SIGSTOP prevents terminal publication until recovery has been observed.
/// Drop resumes on success and on assertion failure, before workspace cleanup.
/// §FS-rhei-run-headless.5.3
pub(super) struct PausedOwner(Pid);

impl PausedOwner {
    pub(super) fn new(ws: &Workspace) -> Self {
        let pid = Pid::from_raw(descriptor_pid(ws));
        kill(pid, Signal::SIGSTOP).expect("pause run owner");
        Self(pid)
    }
}

impl Drop for PausedOwner {
    fn drop(&mut self) {
        let _ = kill(self.0, Signal::SIGCONT);
    }
}

/// Each FIFO rendezvous establishes that attach reached a descriptor read.
/// Answering with the real, unchanged descriptor lets production classify the
/// lock normally. The *following* read proves the previous probe completed.
/// Only attach reads these FIFOs: the owner is paused, and listing precedes us.
/// This uses Unix FIFOs on both Linux and macOS, without tracing or test hooks
/// in production. §FS-rhei-run-headless.5.3
pub(super) struct DescriptorGate {
    path: PathBuf,
    saved: PathBuf,
    staging: PathBuf,
    body: Vec<u8>,
}

impl DescriptorGate {
    pub(super) fn new(ws: &Workspace) -> Self {
        let path = ws.root.join("runtime/run.json");
        let saved = path.with_extension("saved");
        let staging = path.with_extension("fifo");
        let body = fs::read(&path).expect("original descriptor");
        fs::rename(&path, &saved).expect("preserve original descriptor");
        let gate = Self { path, saved, staging, body };
        gate.replace_fifo();
        gate
    }

    fn replace_fifo(&self) {
        mkfifo(&self.staging, nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR)
            .expect("descriptor FIFO");
        fs::rename(&self.staging, &self.path).expect("publish next descriptor FIFO");
    }

    /// Nonblocking writer open succeeds only when attach has opened the reader.
    /// A deadline turns an unexpected read sequence into a fixture failure.
    pub(super) fn reader(&self) -> File {
        let mut writer = None;
        wait_until("attach's next descriptor read", Duration::from_secs(10), || {
            match OpenOptions::new()
                .write(true)
                .custom_flags(nix::libc::O_NONBLOCK)
                .open(&self.path)
            {
                Ok(file) => {
                    writer = Some(file);
                    true
                }
                Err(err) if err.raw_os_error() == Some(nix::libc::ENXIO) => false,
                Err(err) => panic!("open descriptor FIFO: {err}"),
            }
        });
        writer.expect("attach opened the FIFO")
    }

    pub(super) fn serve(&mut self) {
        let writer = self.reader();
        self.answer(writer);
    }

    pub(super) fn answer(&mut self, mut writer: File) {
        // Publish the next FIFO *before* delivering EOF on this one, so the
        // next read cannot reopen the old FIFO or consume two answers at once.
        self.replace_fifo();
        writer.write_all(&self.body).expect("serve unchanged descriptor");
    }

    pub(super) fn restore(&self, mut writer: File) {
        fs::rename(&self.saved, &self.path).expect("restore original descriptor");
        writer.write_all(&self.body).expect("finish last gated descriptor read");
    }
}

impl Drop for DescriptorGate {
    fn drop(&mut self) {
        // WaitingAttach is dropped first, so no reader survives a failure.
        if self.saved.exists() {
            let _ = fs::rename(&self.saved, &self.path);
        }
        let _ = fs::remove_file(&self.staging);
    }
}

/// Own attach until its bounded result is collected; kill and reap on failure.
/// §FS-rhei-run-headless.5.3
pub(super) struct WaitingAttach(Option<Child>);

impl WaitingAttach {
    pub(super) fn new(ws: &Workspace) -> Self {
        Self(Some(
            ws.rhei_command(&["attach", "--wait"])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("start attach"),
        ))
    }

    pub(super) fn assert_waiting(&mut self) {
        assert!(
            self.0.as_mut().expect("owned attach").try_wait().expect("inspect attach").is_none(),
            "attach returned before the controlled owner was released"
        );
    }

    pub(super) fn finish(&mut self) -> Output {
        wait_until("attach's result", Duration::from_secs(60), || {
            self.0.as_mut().expect("owned attach").try_wait().expect("inspect attach").is_some()
        });
        self.0.take().expect("owned attach").wait_with_output().expect("collect attach result")
    }
}

impl Drop for WaitingAttach {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
