//! Waiting out an access another process's open handle refuses.
//!
//! Windows refuses to replace a file - and refuses to read one while it is being
//! replaced - for as long as another process holds it open. Every file several
//! rhei processes read and replace by pathname meets that refusal, so the one
//! test for it and the one bound on waiting it out live here, where the plan
//! writers in `rhei-cli` and the budget witness index in this crate both reach
//! them. §AR-agent-orchestrator-workflow.3.3.1.1

use std::time::{Duration, Instant};

/// How long an access refused by another open handle is retried before the
/// refusal is reported. §AR-agent-orchestrator-workflow.3.3.1.1
pub const PATIENCE: Duration = Duration::from_secs(2);

/// The pause between two attempts at a refused access.
const PAUSE: Duration = Duration::from_millis(20);

/// Whether a failed access was refused because another handle holds the file,
/// judged by what the error says rather than by the platform.
///
/// `PermissionDenied` is a refusal everywhere: Windows reports a rename over an
/// open file as os error 5. Windows's sharing and lock violations (os errors 32
/// and 33) read as no `ErrorKind` of their own, so their raw codes count too -
/// on Windows only, because on Unix those numbers are `EPIPE` and `EDOM`.
// §AR-agent-orchestrator-workflow.3.3.1.1
fn refused(error: &std::io::Error) -> bool {
    const ERROR_SHARING_VIOLATION: i32 = 32;
    const ERROR_LOCK_VIOLATION: i32 = 33;
    error.kind() == std::io::ErrorKind::PermissionDenied
        || (cfg!(windows)
            && matches!(error.raw_os_error(), Some(ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION)))
}

/// One bounded wait on a refused access, started before its first attempt.
///
/// For a caller whose attempt hands back state the next attempt needs - the
/// staged file a refused replacement returns - and so cannot be a closure;
/// [`wait_out`] is the shape for one that can.
// §AR-agent-orchestrator-workflow.3.3.1.1
pub struct Wait {
    deadline: Instant,
}

impl Wait {
    /// Start the bound before the first attempt.
    pub fn start() -> Self {
        Self { deadline: Instant::now() + PATIENCE }
    }

    /// Whether the attempt that failed with `error` is tried again: only a
    /// refusal, and only within the bound. Pauses before saying yes.
    pub fn again(&self, error: &std::io::Error) -> bool {
        if refused(error) && Instant::now() < self.deadline {
            std::thread::sleep(PAUSE);
            return true;
        }
        false
    }
}

/// Run `attempt` until it does not fail with a refusal or the bound runs out,
/// and return what its last attempt met.
///
/// A failure that is not a refusal is returned after that one attempt.
// §AR-agent-orchestrator-workflow.3.3.1.1
pub fn wait_out<T>(mut attempt: impl FnMut() -> std::io::Result<T>) -> std::io::Result<T> {
    let wait = Wait::start();
    loop {
        match attempt() {
            Err(error) if wait.again(&error) => {}
            outcome => return outcome,
        }
    }
}
