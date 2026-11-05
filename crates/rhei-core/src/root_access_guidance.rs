//! Keep root refusal remedies with their causes across I/O conversions. §FS-rhei-errors.6

use std::{
    fmt, io,
    path::{Path, PathBuf},
};

/// Root access must retain its I/O kind and display text for existing callers,
/// while diagnostic adapters can recover the cause's next action. §FS-rhei-errors.6
#[derive(Debug)]
struct Refusal {
    message: String,
    help: String,
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Refusal {}

/// Attach guidance where a root-access refusal is raised. §FS-rhei-errors.6
pub(super) fn refusal(kind: io::ErrorKind, message: String, help: String) -> io::Error {
    io::Error::new(kind, Refusal { message, help })
}

/// Return guidance carried by a root-access refusal, without guessing from its
/// display text; ordinary filesystem errors retain error-kind guidance. §FS-rhei-errors.6
pub fn refusal_help(error: &io::Error) -> Option<&str> {
    error.get_ref()?.downcast_ref::<Refusal>().map(|cause| cause.help.as_str())
}

/// A raw filesystem error otherwise loses the child path during discovery.
/// Keep that subject and the original error together. §FS-rhei-errors.6
#[derive(Debug)]
struct PathFailure {
    path: PathBuf,
    error: io::Error,
}

impl fmt::Display for PathFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cannot access '{}': {}", self.path.display(), self.error)
    }
}

impl std::error::Error for PathFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

/// Attach the measured path once, retaining more specific nested context and
/// root-interlock guidance when either is already present. §FS-rhei-errors.6
pub(super) fn with_path(path: &Path, error: io::Error) -> io::Error {
    if error_path(&error).is_some() || refusal_help(&error).is_some() {
        return error;
    }
    io::Error::new(error.kind(), PathFailure { path: path.to_path_buf(), error })
}

/// The actual filesystem subject, which can differ from the supplied project
/// when discovery fails on a member or marker. §FS-rhei-errors.6
pub fn error_path(error: &io::Error) -> Option<&Path> {
    error.get_ref()?.downcast_ref::<PathFailure>().map(|cause| cause.path.as_path())
}
