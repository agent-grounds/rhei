//! Keep root refusal remedies with their causes across I/O conversions. §FS-rhei-errors.6

use std::{fmt, io};

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
