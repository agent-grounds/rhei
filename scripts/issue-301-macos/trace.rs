//! Disposable pass/event evidence for §FS-rhei-validate.5 and §FS-rhei-errors.1.2.
//! No env flag means no trace; only the named E2E's child receives that flag.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

static PASS: AtomicU64 = AtomicU64::new(1);
static ADMISSION: AtomicU64 = AtomicU64::new(0);

/// Per-process output prevents parallel watch tests mixing evidence (§FS-rhei-validate.5).
struct Trace {
    file: File,
    stderr: PathBuf,
    seq: u64,
    origin: Instant,
    overhead_ns: u128,
}

/// Initialize outside watch roots, preserving raw path spellings (§FS-rhei-validate.5).
fn trace() -> Option<&'static Mutex<Trace>> {
    static TRACE: OnceLock<Option<Mutex<Trace>>> = OnceLock::new();
    TRACE
        .get_or_init(|| {
            let directory = std::env::var_os("ISSUE_301_TRACE_DIR")?;
            let path = PathBuf::from(directory).join(format!("trace-{}.jsonl", std::process::id()));
            let origin = Instant::now();
            let file =
                OpenOptions::new().write(true).create_new(true).open(path).expect("trace file");
            let stderr = std::env::var_os("ISSUE_301_STDERR").expect("trace stderr").into();
            Some(Mutex::new(Trace { file, stderr, seq: 0, origin, overhead_ns: 0 }))
        })
        .as_ref()
}

/// Prior records' cumulative I/O cost excludes the still-running record (§FS-rhei-validate.5).
fn write(trace: &mut Trace, mut value: serde_json::Value, started: Instant) -> u64 {
    trace.seq += 1;
    value["seq"] = trace.seq.into();
    value["pid"] = std::process::id().into();
    value["elapsed_ns"] = trace.origin.elapsed().as_nanos().to_string().into();
    value["overhead_before_ns"] = trace.overhead_ns.to_string().into();
    value["unix_us"] = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_micros()
        .to_string()
        .into();
    writeln!(trace.file, "{value}").expect("write diagnostic trace");
    trace.overhead_ns += started.elapsed().as_nanos();
    trace.seq
}

/// The original error rendering and watch decisions remain in their caller (§FS-rhei-validate.5).
pub(super) fn record(value: serde_json::Value) -> u64 {
    let started = Instant::now();
    let Some(trace) = trace() else { return 0 };
    write(&mut trace.lock().expect("trace lock"), value, started)
}

/// Record the actual filter result; debounce never replaces the admission (§FS-rhei-validate.5).
pub(super) fn event(stage: &str, event: &notify::Event, accepted: bool) {
    let seq = record(serde_json::json!({
        "type": "event", "stage": stage, "kind": format!("{:?}", event.kind),
        "paths": event.paths, "accepted": accepted,
    }));
    if stage == "receive" && accepted {
        ADMISSION.store(seq, Ordering::Relaxed);
    }
}

/// Preserve registration spellings alongside aliases for later inspection (§FS-rhei-validate.5).
pub(super) fn root(path: &std::path::Path, mode: notify::RecursiveMode) {
    if trace().is_some() {
        record(serde_json::json!({
            "type": "root", "path": path, "canonical_path": std::fs::canonicalize(path).ok(),
            "mode": format!("{mode:?}"),
        }));
    }
}

/// Bound an unchanged rendering, without reformatting its error (§FS-rhei-validate.5).
pub(super) fn begin() -> u64 {
    let pass = PASS.fetch_add(1, Ordering::Relaxed);
    boundary("pass_begin", pass);
    pass
}

/// A killed process can leave this record missing; analysis marks it incomplete (§FS-rhei-validate.5).
pub(super) fn end(pass: u64) {
    boundary("pass_end", pass);
}

/// File length delimits bytes written by the original eprintln (§FS-rhei-errors.1.2).
fn boundary(kind: &str, pass: u64) {
    let started = Instant::now();
    let Some(trace) = trace() else { return };
    let mut trace = trace.lock().expect("trace lock");
    let offset = std::fs::metadata(&trace.stderr).expect("diagnostic stderr metadata").len();
    write(
        &mut trace,
        serde_json::json!({
            "type": kind, "pass": pass, "stderr_bytes": offset,
            "admitting_event": ADMISSION.load(Ordering::Relaxed),
        }),
        started,
    );
}
