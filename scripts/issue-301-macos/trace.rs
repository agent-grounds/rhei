//! Disposable instrumentation for §FS-rhei-validate.5 and §FS-rhei-errors.1.2.
//! The original eprintln and event selection remain unchanged. Byte offsets
//! associate actual stderr bytes with passes without rendering the error again.

use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};

static SEQUENCE: AtomicU64 = AtomicU64::new(1);
static PASS: AtomicU64 = AtomicU64::new(1);
static ADMISSION: AtomicU64 = AtomicU64::new(0);

/// Record an observation outside the registered roots (§FS-rhei-validate.5).
pub(super) fn record(mut value: serde_json::Value) -> u64 {
    let seq = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    value["seq"] = seq.into();
    value["unix_us"] = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("diagnostic clock")
        .as_micros()
        .to_string()
        .into();
    let path = std::env::var_os("ISSUE_301_TRACE").expect("diagnostic trace path");
    let mut file = std::fs::OpenOptions::new().append(true).open(path).expect("diagnostic trace");
    writeln!(file, "{value}").expect("write diagnostic trace");
    seq
}

/// Record the actual logical filter result, including debounce (§FS-rhei-validate.5).
pub(super) fn event(stage: &str, event: &notify::Event, accepted: bool) {
    let seq = record(serde_json::json!({
        "type": "event", "stage": stage, "kind": format!("{:?}", event.kind),
        "paths": event.paths, "accepted": accepted,
    }));
    if stage == "receive" && accepted {
        ADMISSION.store(seq, Ordering::Relaxed);
    }
}

/// Record each successful OS registration, including later roots (§FS-rhei-validate.5).
pub(super) fn root(path: &std::path::Path, mode: notify::RecursiveMode) {
    record(serde_json::json!({"type": "root", "path": path, "mode": format!("{mode:?}")}));
}

/// Start the byte interval for one unchanged rendering pass (§FS-rhei-validate.5).
pub(super) fn begin() -> u64 {
    let pass = PASS.fetch_add(1, Ordering::Relaxed);
    boundary("pass_begin", pass);
    pass
}

/// End the byte interval for one unchanged rendering pass (§FS-rhei-validate.5).
pub(super) fn end(pass: u64) {
    boundary("pass_end", pass);
}

/// Inspect file length without reading or reformatting stderr (§FS-rhei-errors.1.2).
fn boundary(kind: &str, pass: u64) {
    let path = std::env::var_os("ISSUE_301_STDERR").expect("diagnostic stderr path");
    let offset = std::fs::metadata(path).expect("diagnostic stderr metadata").len();
    record(serde_json::json!({
        "type": kind, "pass": pass, "stderr_bytes": offset,
        "admitting_event": ADMISSION.load(Ordering::Relaxed),
    }));
}
