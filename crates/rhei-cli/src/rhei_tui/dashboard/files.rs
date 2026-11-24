//! The run's workspace as the dashboard reaches it: links to its folders, an
//! artifact opened in the operator's editor, and a running agent's log tailed
//! for the live terminal. No path leaves the workspace root. §FS-rhei-viz.11 §AR-rhei-viz-flow.6

use std::fs;
use std::io::{self, Read, Seek, SeekFrom};
use std::net::TcpStream;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use super::http::{query_param, write_response, write_status};
use super::state::{DashboardLink, DashboardState};

pub(super) fn derive_auto_links(workspace: &str) -> Vec<DashboardLink> {
    let mut out = Vec::new();
    let encoded_root = encode_url_path(workspace);
    let push = |out: &mut Vec<DashboardLink>, label: &str, suffix: &str| {
        let url = if suffix.is_empty() {
            format!("file://{encoded_root}")
        } else {
            // `suffix` is a fixed ASCII relative path; encoding is a no-op
            // but keeps the construction symmetric.
            format!("file://{encoded_root}/{}", encode_url_path(suffix))
        };
        out.push(DashboardLink { label: label.to_string(), url, source: "workspace" });
    };
    push(&mut out, "Workspace", "");
    push(&mut out, "Runtime logs", "runtime/logs");
    push(&mut out, "Runtime results", "runtime/results");
    // Surface the intervention audit trail once it exists, so the one mutation
    // boundary is reachable from the dashboard. §AR-rhei-viz-flow.7
    if Path::new(workspace).join("runtime/interventions.log").is_file() {
        push(&mut out, "Interventions", "runtime/interventions.log");
    }
    out
}

/// Percent-encode a filesystem path for embedding in a `file://` URL.
///
/// Preserves RFC 3986 path-safe bytes (unreserved + sub-delims + `:` `@` `/`)
/// and percent-encodes everything else, including spaces, `#`, `?`, and any
/// non-ASCII byte. Operates on raw bytes so non-UTF-8 paths are still
/// representable.
pub(super) fn encode_url_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for byte in path.bytes() {
        let safe = matches!(
            byte,
            b'A'..=b'Z'
                | b'a'..=b'z'
                | b'0'..=b'9'
                | b'-'
                | b'.'
                | b'_'
                | b'~'
                | b'!'
                | b'$'
                | b'&'
                | b'\''
                | b'('
                | b')'
                | b'*'
                | b'+'
                | b','
                | b';'
                | b'='
                | b':'
                | b'@'
                | b'/'
        );
        if safe {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push_str(&format!("{:02X}", byte));
        }
    }
    out
}

/// §FS-rhei-viz.5: resolve a workspace-relative artifact/log path and open it
/// in the operator's editor. The request never mutates plan state.
pub(super) fn handle_open(stream: &mut TcpStream, state: &Arc<Mutex<DashboardState>>, query: &str) {
    let Some(rel) = query_param(query, "path") else {
        return write_status(stream, "400 Bad Request", b"missing path");
    };
    let workspace = match state.lock() {
        Ok(s) => s.workspace.clone(),
        Err(p) => p.into_inner().workspace.clone(),
    };
    let Some(abs) = resolve_within_workspace(&workspace, &rel) else {
        return write_status(stream, "400 Bad Request", b"invalid path");
    };
    match launch_editor(&abs) {
        Ok(()) => write_status(stream, "204 No Content", b""),
        Err(_) => write_status(stream, "500 Internal Server Error", b"could not launch editor"),
    }
}

/// Tail a running task's durable agent log for the live terminal:
/// `GET /log?task&from=<offset>` returns new bytes + next offset; the host-
/// supplied path is rejected unless within the workspace root. §AR-rhei-viz-flow.6
pub(super) fn handle_log(stream: &mut TcpStream, state: &Arc<Mutex<DashboardState>>, query: &str) {
    let task = query_param(query, "task");
    let from = query_param(query, "from").and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);

    let (workspace, log_path) = {
        let s = match state.lock() {
            Ok(s) => s,
            Err(p) => p.into_inner(),
        };
        let path = task.as_deref().and_then(|t| {
            // Prefer the slot actively running this task; fall back to any slot
            // that holds its (most recent) log path.
            s.slots
                .iter()
                .find(|sl| sl.active && sl.task.as_deref() == Some(t))
                .or_else(|| s.slots.iter().find(|sl| sl.task.as_deref() == Some(t)))
                .and_then(|sl| sl.log_path.clone())
        });
        (s.workspace.clone(), path)
    };

    let Some(log_path) = log_path else {
        // No durable log yet (task not running, or no log recorded). Empty tail.
        return write_response(stream, "application/json", br#"{"next":0,"data":""}"#);
    };
    if !log_within_workspace(&workspace, &log_path) {
        return write_status(stream, "400 Bad Request", b"invalid log path");
    }

    let (data, next) = read_log_tail(Path::new(&log_path), from);
    let body = serde_json::json!({ "next": next, "data": data });
    match serde_json::to_vec(&body) {
        Ok(bytes) => write_response(stream, "application/json", &bytes),
        Err(_) => write_response(stream, "application/json", br#"{"next":0,"data":""}"#),
    }
}

/// True when an absolute, host-supplied log path sits within the workspace root
/// and contains no `..` traversal.
pub(super) fn log_within_workspace(workspace: &str, log_path: &str) -> bool {
    !log_path.contains("..") && Path::new(log_path).starts_with(Path::new(workspace))
}

/// Read the durable log from `from`, capped per request so polling stays light.
/// A `from` past the current length (file rotated/truncated) restarts at 0.
pub(super) fn read_log_tail(path: &Path, from: u64) -> (String, u64) {
    const MAX_CHUNK: u64 = 256 * 1024;
    let Ok(mut file) = fs::File::open(path) else {
        return (String::new(), from);
    };
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let start = if from > len { 0 } else { from };
    if file.seek(SeekFrom::Start(start)).is_err() {
        return (String::new(), start);
    }
    let to_read = (len - start).min(MAX_CHUNK) as usize;
    let mut buf = vec![0u8; to_read];
    let read = file.read(&mut buf).unwrap_or(0);
    buf.truncate(read);
    (String::from_utf8_lossy(&buf).into_owned(), start + read as u64)
}

/// Join a workspace-relative path to the workspace root, rejecting absolute
/// paths and any `..` traversal so a click can never reach outside the run.
pub(super) fn resolve_within_workspace(workspace: &str, rel: &str) -> Option<PathBuf> {
    let rel_path = Path::new(rel);
    if rel.is_empty() || rel_path.is_absolute() {
        return None;
    }
    for component in rel_path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            // ParentDir, RootDir, or a Windows prefix could escape the root.
            _ => return None,
        }
    }
    Some(Path::new(workspace).join(rel_path))
}

pub(super) fn launch_editor(path: &Path) -> io::Result<()> {
    let editor = resolve_editor();
    let mut parts = editor.split_whitespace();
    let program = parts.next().unwrap_or("xdg-open");
    let mut command = Command::new(program);
    for arg in parts {
        command.arg(arg);
    }
    command
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

/// Resolve the editor command: `RHEI_EDITOR`, then `VISUAL`, then `EDITOR`,
/// then a platform opener. §FS-rhei-viz.5
pub(super) fn resolve_editor() -> String {
    for key in ["RHEI_EDITOR", "VISUAL", "EDITOR"] {
        if let Ok(value) = std::env::var(key) {
            if !value.trim().is_empty() {
                return value;
            }
        }
    }
    if cfg!(target_os = "macos") {
        "open".to_string()
    } else if cfg!(target_os = "windows") {
        "cmd /C start".to_string()
    } else {
        "xdg-open".to_string()
    }
}
