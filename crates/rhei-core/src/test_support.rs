//! What more than one module's tests need.

use std::process::{Command, Stdio};

/// The interpreter tests write their child processes in.
///
/// A callback is a command line for the platform's own shell, and the two
/// shells share almost no vocabulary: `printf`, `true`, and `$VAR` are `sh`,
/// not `cmd`. Python is on both, so what a child process *does* can be pinned
/// once instead of twice.
pub(crate) fn python() -> &'static str {
    static PYTHON: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();
    PYTHON.get_or_init(|| {
        let candidates = if cfg!(windows) { ["python", "python3"] } else { ["python3", "python"] };
        for candidate in candidates {
            let runs = Command::new(candidate)
                .arg("-c")
                .arg("pass")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|status| status.success())
                .unwrap_or(false);
            if runs {
                return candidate;
            }
        }
        panic!(
            "these tests run their child processes under Python: put `python3` or `python` on PATH"
        )
    })
}
