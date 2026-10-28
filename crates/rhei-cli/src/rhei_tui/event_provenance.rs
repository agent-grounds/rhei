//! Where a run came from: the rhei build it ran on and the commit its
//! checkout started on, carried by `run_started`, the run descriptor and every
//! invocation record. §FS-rhei-run-headless.2

use serde_json::Value;

/// The rhei build a run ran on: its version, and the commit it was built from
/// with whether its sources differed, each `null` when unknown.
/// §FS-rhei-run-headless.2 §FS-rhei-version.2
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RheiBuild {
    pub version: String,
    pub commit: Option<String>,
    pub dirty: Option<bool>,
}

/// The commit HEAD named when a run took its locks, and the repository's
/// remote reduced for publication, `null` when none reduces.
/// §FS-rhei-run-headless.2 §FS-rhei-summary.4
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CheckoutHead {
    pub repo: Option<String>,
    pub commit: String,
}

/// What a run says about where it came from, on the descriptor, the stream's
/// head and every invocation record it writes. §FS-rhei-run-json.2.1
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunProvenance {
    pub rhei_build: RheiBuild,
    pub checkout_head_at_start: Option<CheckoutHead>,
}

impl RunProvenance {
    /// Its two `run_started` fields read back; a stream from an older rhei
    /// carries neither and reads as unknown. §FS-rhei-run-json.2.1
    pub(crate) fn from_fields(record: &Value) -> RunProvenance {
        RunProvenance {
            rhei_build: record
                .get("rhei_build")
                .and_then(|build| serde_json::from_value(build.clone()).ok())
                .unwrap_or_default(),
            checkout_head_at_start: record
                .get("checkout_head_at_start")
                .and_then(|head| serde_json::from_value(head.clone()).ok()),
        }
    }
}
