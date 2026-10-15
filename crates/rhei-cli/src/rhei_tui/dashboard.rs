use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::rhei_viz_model::VizModel;

use crate::rhei_tui::event::{EventSink, RunEvent};

mod state;

use state::{task_accounting_for_tasks, DashboardLink, DashboardState, TaskAccounting};

const RECENT_LIMIT: usize = 200;
const SLOT_TRAFFIC_LIMIT: usize = 60;

/// Closure invoked on every `/snapshot` request: the host builds the full
/// [`VizModel`] via `rhei-viz` (so the dashboard never parses plans); a `None`
/// from a transient reload failure keeps the previous model. §AR-rhei-viz-flow.3
pub type PlanLoader = Arc<dyn Fn() -> Option<VizModel> + Send + Sync>;

/// The single inbound, state-changing-adjacent path (§AR-rhei-viz-flow.7): deliver a message
/// to the stdin of the agent running one task, and **nothing else**. There is
/// no path from the loopback server to plan-state mutation. The host
/// (`rhei-cli`) implements delivery, the agent-capability gate, and the durable
/// audit trail; the server is a thin pass-through.
pub trait InterveneSink: Send + Sync {
    /// Deliver `message` to the selected running agent. `slot` disambiguates
    /// concurrent fanout invocations of the same task; `task_id` remains a
    /// fallback for single-invocation callers. `Ok(())` on delivery;
    /// `Err(reason)` when the agent is not interactively reachable (one-shot
    /// agent, closed stdin, or no matching running task).
    fn deliver(
        &self,
        task_id: Option<&str>,
        slot: Option<crate::rhei_tui::event::Slot>,
        message: &str,
    ) -> Result<(), String>;

    /// Whether the named running task — optionally a specific fanout `slot` — has
    /// a writable agent stdin registered, so a `/intervene` message can be
    /// delivered now. Gates the live composer; defaults to `false`. §FS-rhei-viz.5
    fn reachable(&self, _task_id: &str, _slot: Option<crate::rhei_tui::event::Slot>) -> bool {
        false
    }
}

/// Live dashboard transport for explicit human gate transitions. §FS-rhei-viz.5.1
/// The host (`rhei-cli`) owns validation, callbacks, compare-and-swap writes,
/// and audit entries; the loopback server only transports the request.
pub trait GateTransitionSink: Send + Sync {
    /// Transition `task_id` from `from` to `to`, returning the effective target
    /// state after callbacks, or a human-readable rejection reason.
    ///
    /// `result` is the operator's own account of the decision, carried through
    /// the move exactly as `rhei transition --result` carries it. It is
    /// optional on every hop and blank-is-none: the host refuses a terminal
    /// release that has neither a message nor a result already on disk, and the
    /// surfaces render that refusal rather than inventing one for the human.
    // §FS-rhei-viz.5.1 §FS-rhei-states.3.3
    fn transition_gate(
        &self,
        task_id: &str,
        from: &str,
        to: &str,
        result: Option<&str>,
    ) -> Result<String, String>;
}

/// Per-task runtime overlay carried alongside the [`VizModel`] base in the live
/// `/snapshot`: which slot (if any) is running the task, the active invocation's
/// template values, whether it was deferred this pass, and its compact
/// accounting rollups. Absent on a static render, so the supplementary surfaces
/// degrade by field-presence. §AR-rhei-viz-flow.4.
#[derive(Clone, Serialize)]
pub struct TaskRuntime {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_slot: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template_context: Option<crate::rhei_viz_model::TemplateContext>,
    /// Whether this slot's agent can take a live `/intervene` message now (stdin
    /// held open via `intervene_stdin`). The Flow composer renders only when
    /// `true`, so an unreachable agent is flagged up front. §FS-rhei-viz.5
    pub intervene: bool,
    pub deferred_this_pass: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accounting: Option<TaskAccounting>,
}

/// The live `/snapshot` payload: the [`VizModel`] static base (plan_title,
/// plan_state, about, tasks, machine) plus the runtime overlay
/// (`DashboardState`, auto links, and per-task runtime). §AR-rhei-viz-flow.4.
#[derive(Serialize)]
struct SnapshotPayload<'a> {
    #[serde(flatten)]
    base: VizModel,
    #[serde(flatten)]
    state: &'a DashboardState,
    capabilities: DashboardCapabilities,
    auto_links: Vec<DashboardLink>,
    task_runtime: BTreeMap<String, TaskRuntime>,
}

#[derive(Serialize)]
struct DashboardCapabilities {
    gate_transition: bool,
}

struct HttpRequest {
    path: String,
    query: String,
    body: Vec<u8>,
}

pub struct DashboardSink {
    url: String,
    state: Arc<Mutex<DashboardState>>,
    stop: Arc<AtomicBool>,
    join: Mutex<Option<JoinHandle<()>>>,
    /// The discovery file (`runtime/dashboard.json`) this run published its
    /// loopback URL into, so `rhei intervene` can reach the live server. Removed
    /// when the dashboard finishes. §AR-rhei-viz-flow.7
    addr_file: Option<PathBuf>,
}

impl DashboardSink {
    /// Backwards-compatible constructor: no plan loader, so the Flow view shows
    /// an empty plan.
    pub fn start(workspace: PathBuf, parallel: u16, total_tasks: usize) -> io::Result<Self> {
        Self::start_with_plan(workspace, parallel, total_tasks, None)
    }

    /// Constructor with an optional plan loader. The loader is called on every
    /// `/snapshot` request and must be cheap (it parses a markdown file and
    /// builds the [`VizModel`]). Pass `None` to serve an empty plan.
    pub fn start_with_plan(
        workspace: PathBuf,
        parallel: u16,
        total_tasks: usize,
        plan_loader: Option<PlanLoader>,
    ) -> io::Result<Self> {
        Self::start_with_plan_and_intervene(workspace, parallel, total_tasks, plan_loader, None)
    }

    /// Constructor that also wires the `/intervene` delivery channel (§AR-rhei-viz-flow.7).
    /// When `intervene` is `None`, `POST /intervene` reports the agent as not
    /// reachable.
    pub fn start_with_plan_and_intervene(
        workspace: PathBuf,
        parallel: u16,
        total_tasks: usize,
        plan_loader: Option<PlanLoader>,
        intervene: Option<Arc<dyn InterveneSink>>,
    ) -> io::Result<Self> {
        Self::start_with_plan_intervene_and_gate(
            workspace,
            parallel,
            total_tasks,
            plan_loader,
            intervene,
            None,
        )
    }

    pub fn start_with_plan_intervene_and_gate(
        workspace: PathBuf,
        parallel: u16,
        total_tasks: usize,
        plan_loader: Option<PlanLoader>,
        intervene: Option<Arc<dyn InterveneSink>>,
        gate_transition: Option<Arc<dyn GateTransitionSink>>,
    ) -> io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let url = format!("http://{}", listener.local_addr()?);
        let state =
            Arc::new(Mutex::new(DashboardState::new(workspace, parallel.max(1), total_tasks)));
        let stop = Arc::new(AtomicBool::new(false));
        // The loader is set once at construction and never mutated, so no
        // mutex is required around it. `last_plan` does need a mutex because
        // it caches a value that's overwritten each request.
        let plan = plan_loader;
        let last_plan = Arc::new(Mutex::new(None));

        let thread_state = Arc::clone(&state);
        let thread_stop = Arc::clone(&stop);
        // Publish the loopback URL so a separate `rhei intervene` process can
        // reach this run's server. Best-effort: a failure to write only costs
        // headless intervention, not the run. §AR-rhei-viz-flow.7
        let addr_file = {
            let workspace = match state.lock() {
                Ok(s) => s.workspace.clone(),
                Err(p) => p.into_inner().workspace.clone(),
            };
            publish_dashboard_addr(&workspace, &url)
        };
        let handle = thread::spawn(move || {
            serve(listener, thread_state, plan, last_plan, thread_stop, intervene, gate_transition)
        });

        Ok(Self { url, state, stop, join: Mutex::new(Some(handle)), addr_file })
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    /// Freeze the live surface into a self-contained static page under `runtime/`
    /// by inlining the final superset snapshot into the one static renderer — the
    /// frozen page equals the live one minus polling. §AR-rhei-viz-flow.5.3
    pub fn write_frozen_dashboard(&self) -> io::Result<PathBuf> {
        let snapshot = self.fetch_snapshot_body()?;
        let snapshot = String::from_utf8(snapshot).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("dashboard snapshot was not valid UTF-8: {err}"),
            )
        })?;
        let path = self.frozen_dashboard_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        // `snapshot` is already valid JSON; wrap it as a one-plan bundle so the
        // asset's static path renders it (the selector stays hidden).
        let boot = format!("{{\"plan\":{snapshot}}}");
        fs::write(&path, crate::rhei_viz_model::render_inline(&boot))?;
        Ok(path)
    }

    pub fn finish(&self) {
        self.stop.store(true, Ordering::SeqCst);
        // Drop the discovery file first so a late `rhei intervene` sees the run
        // is gone rather than dialing a closing socket.
        if let Some(path) = &self.addr_file {
            let _ = fs::remove_file(path);
        }
        let mut guard = match self.join.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if let Some(handle) = guard.take() {
            let _ = handle.join();
        }
    }

    fn frozen_dashboard_path(&self) -> PathBuf {
        let workspace = match self.state.lock() {
            Ok(s) => s.workspace.clone(),
            Err(p) => p.into_inner().workspace.clone(),
        };
        PathBuf::from(workspace).join("runtime/dashboard.html")
    }

    fn fetch_snapshot_body(&self) -> io::Result<Vec<u8>> {
        let addr = self.url.strip_prefix("http://").unwrap_or(self.url.as_str());
        let mut stream = TcpStream::connect(addr)?;
        stream
            .write_all(b"GET /snapshot HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")?;
        let mut response = Vec::new();
        stream.read_to_end(&mut response)?;
        let split = response
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "malformed HTTP response"))?;
        if !response.starts_with(b"HTTP/1.1 200 OK\r\n") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "dashboard snapshot request failed",
            ));
        }
        Ok(response[split + 4..].to_vec())
    }
}

impl Drop for DashboardSink {
    fn drop(&mut self) {
        self.finish();
    }
}

impl EventSink for DashboardSink {
    fn emit(&self, event: RunEvent) {
        let mut state = match self.state.lock() {
            Ok(s) => s,
            Err(p) => p.into_inner(),
        };
        state.apply(&event);
    }
}

fn serve(
    listener: TcpListener,
    state: Arc<Mutex<DashboardState>>,
    plan: Option<PlanLoader>,
    last_plan: Arc<Mutex<Option<VizModel>>>,
    stop: Arc<AtomicBool>,
    intervene: Option<Arc<dyn InterveneSink>>,
    gate_transition: Option<Arc<dyn GateTransitionSink>>,
) {
    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                let _ = stream.set_nonblocking(false);
                handle_client(
                    stream,
                    &state,
                    plan.as_ref(),
                    &last_plan,
                    intervene.as_ref(),
                    gate_transition.as_ref(),
                );
            }
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(50));
            }
            Err(_) => break,
        }
    }
}

fn handle_client(
    mut stream: TcpStream,
    state: &Arc<Mutex<DashboardState>>,
    plan: Option<&PlanLoader>,
    last_plan: &Arc<Mutex<Option<VizModel>>>,
    intervene: Option<&Arc<dyn InterveneSink>>,
    gate_transition: Option<&Arc<dyn GateTransitionSink>>,
) {
    let Some(request) = read_http_request(&mut stream) else {
        return;
    };

    match request.path.as_str() {
        // §AR-rhei-viz-flow.2: the one Flow asset, served verbatim; its JS polls /snapshot.
        "/" => write_response(
            &mut stream,
            "text/html; charset=utf-8",
            crate::rhei_viz_model::live_asset().as_bytes(),
        ),
        // §FS-rhei-viz.11: open an artifact or log file in the operator's editor.
        "/open" => handle_open(&mut stream, state, &request.query),
        // §AR-rhei-viz-flow.6: tail a running task's durable agent log for the live terminal.
        "/log" => handle_log(&mut stream, state, &request.query),
        // §AR-rhei-viz-flow.7: the one mutation boundary — deliver a message to an agent's stdin.
        "/intervene" => handle_intervene(&mut stream, intervene, &request.body),
        // §FS-rhei-viz.5.1: explicit human transition out of a gating state.
        "/transition-gate" => handle_gate_transition(&mut stream, gate_transition, &request.body),
        "/snapshot" => {
            let snapshot_state = match state.lock() {
                Ok(s) => s.clone(),
                Err(p) => p.into_inner().clone(),
            };
            // Rebuild the plan view. If the loader is missing or returns None
            // (transient parse error during a write), reuse the last good model
            // rather than blanking the surface (§FS-rhei-viz.7.1).
            let fresh = plan.and_then(|loader| loader());
            let mut base = if let Some(model) = fresh {
                if let Ok(mut last) = last_plan.lock() {
                    *last = Some(model.clone());
                }
                model
            } else {
                last_plan.lock().ok().and_then(|g| g.clone()).unwrap_or_default()
            };

            // §FS-rhei-viz.9: promote the derived plan state to `active` when a
            // top-level task is assigned to a running slot (the runtime signal
            // the pure derivation in `rhei-viz` cannot see).
            let active_tasks: HashSet<&str> = snapshot_state
                .slots
                .iter()
                .filter(|slot| slot.active)
                .filter_map(|slot| slot.task.as_deref())
                .collect();
            if base.tasks.iter().any(|t| t.depth == 0 && active_tasks.contains(t.id.as_str())) {
                base.plan_state = Some("active".to_string());
            }

            // Per-task runtime overlay: in_slot, deferred-this-pass, and the
            // compact accounting rollups. §FS-rhei-cost-accounting.10.
            let deferred_set: HashSet<&str> =
                snapshot_state.deferred.iter().map(|s| s.as_str()).collect();
            let accounting_by_task =
                task_accounting_for_tasks(&base.tasks, &snapshot_state.invocations);
            let mut task_runtime = BTreeMap::new();
            for task in &base.tasks {
                let active_slot = snapshot_state.slots.iter().enumerate().find(|(_, slot)| {
                    slot.active && slot.task.as_deref() == Some(task.id.as_str())
                });
                let in_slot = active_slot.map(|(i, _)| i as u16);
                let template_context =
                    active_slot.and_then(|(_, slot)| slot.template_context.clone());
                // Capability gate: the composer is offered only when the running
                // slot's agent holds a writable stdin the sink can reach, so the
                // gate and delivery share one registry. §FS-rhei-viz.5
                let intervene_reachable = match (in_slot, intervene) {
                    (Some(slot), Some(sink)) => sink.reachable(&task.id, Some(slot)),
                    _ => false,
                };
                let deferred = deferred_set.contains(task.id.as_str());
                let accounting = accounting_by_task.get(&task.id).cloned();
                if in_slot.is_some() || deferred || accounting.is_some() {
                    task_runtime.insert(
                        task.id.clone(),
                        TaskRuntime {
                            in_slot,
                            template_context,
                            intervene: intervene_reachable,
                            deferred_this_pass: deferred,
                            accounting,
                        },
                    );
                }
            }

            let auto_links = derive_auto_links(&snapshot_state.workspace);
            let capabilities = DashboardCapabilities { gate_transition: gate_transition.is_some() };
            let payload = SnapshotPayload {
                base,
                state: &snapshot_state,
                capabilities,
                auto_links,
                task_runtime,
            };
            match serde_json::to_vec(&payload) {
                Ok(body) => write_response(&mut stream, "application/json", &body),
                Err(err) => write_response(
                    &mut stream,
                    "application/json",
                    format!(r#"{{"error":"{}"}}"#, escape_json_string(&err.to_string())).as_bytes(),
                ),
            }
        }
        "/accounting/invocations" => {
            // §FS-rhei-cost-accounting.10: Invocation details use a separate endpoint.
            let snapshot_state = match state.lock() {
                Ok(s) => s.clone(),
                Err(p) => p.into_inner().clone(),
            };
            match serde_json::to_vec(&snapshot_state.invocations) {
                Ok(body) => write_response(&mut stream, "application/json", &body),
                Err(err) => write_response(
                    &mut stream,
                    "application/json",
                    format!(r#"{{"error":"{}"}}"#, escape_json_string(&err.to_string())).as_bytes(),
                ),
            }
        }
        _ => write_not_found(&mut stream),
    }
}

/// Publish the live dashboard's loopback URL to `runtime/dashboard.json` so a
/// separate `rhei intervene` process can discover and message this run; returns
/// the written path (removed on shutdown), or `None` on write failure. §AR-rhei-viz-flow.7
fn publish_dashboard_addr(workspace: &str, url: &str) -> Option<PathBuf> {
    let dir = Path::new(workspace).join("runtime");
    if fs::create_dir_all(&dir).is_err() {
        return None;
    }
    let path = dir.join("dashboard.json");
    let body = serde_json::json!({ "url": url, "pid": std::process::id() });
    match fs::write(&path, body.to_string()) {
        Ok(()) => Some(path),
        Err(_) => None,
    }
}

fn derive_auto_links(workspace: &str) -> Vec<DashboardLink> {
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
fn encode_url_path(path: &str) -> String {
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

fn read_http_request(stream: &mut TcpStream) -> Option<HttpRequest> {
    const MAX_HEADER_BYTES: usize = 64 * 1024;

    let mut raw = Vec::new();
    let header_end = loop {
        if let Some(pos) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos;
        }
        if raw.len() >= MAX_HEADER_BYTES {
            return None;
        }

        let mut buf = [0u8; 8192];
        let n = stream.read(&mut buf).ok()?;
        if n == 0 {
            return None;
        }
        raw.extend_from_slice(&buf[..n]);
    };

    let headers = String::from_utf8_lossy(&raw[..header_end]);
    let mut request_line = headers.lines().next().unwrap_or_default().split_whitespace();
    let method = request_line.next().unwrap_or_default();
    let target = request_line.next().unwrap_or("/");
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let content_length = if method.eq_ignore_ascii_case("POST") {
        headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                if name.eq_ignore_ascii_case("content-length") {
                    value.trim().parse().ok()
                } else {
                    None
                }
            })
            .unwrap_or(0)
    } else {
        0
    };

    // POST bodies may arrive after the first read or exceed the 8 KiB request
    // buffer; read the declared body before dispatching mutation routes. §AR-rhei-viz-flow.7.
    let body_start = header_end + 4;
    let mut body = raw.get(body_start..).unwrap_or_default().to_vec();
    if body.len() < content_length {
        let already = body.len();
        body.resize(content_length, 0);
        stream.read_exact(&mut body[already..]).ok()?;
    } else {
        body.truncate(content_length);
    }

    Some(HttpRequest { path: path.to_string(), query: query.to_string(), body })
}

fn write_response(stream: &mut TcpStream, content_type: &str, body: &[u8]) {
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(body);
}

fn write_not_found(stream: &mut TcpStream) {
    write_status(stream, "404 Not Found", b"not found");
}

fn write_status(stream: &mut TcpStream, status: &str, body: &[u8]) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(body);
}

/// §FS-rhei-viz.5: resolve a workspace-relative artifact/log path and open it
/// in the operator's editor. The request never mutates plan state.
fn handle_open(stream: &mut TcpStream, state: &Arc<Mutex<DashboardState>>, query: &str) {
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
fn handle_log(stream: &mut TcpStream, state: &Arc<Mutex<DashboardState>>, query: &str) {
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

/// §AR-rhei-viz-flow.7: deliver a `POST /intervene` message to one agent's stdin via the
/// host's [`InterveneSink`]. This route never transitions a task, writes the
/// plan, or mutates task metadata — it only hands bytes to the sink, which owns
/// delivery and the durable audit trail.
fn handle_intervene(
    stream: &mut TcpStream,
    intervene: Option<&Arc<dyn InterveneSink>>,
    body: &[u8],
) {
    #[derive(serde::Deserialize)]
    struct Req {
        #[serde(default)]
        task_id: String,
        #[serde(default)]
        slot: Option<crate::rhei_tui::event::Slot>,
        #[serde(default)]
        message: String,
    }
    let Ok(req) = serde_json::from_slice::<Req>(body) else {
        return write_status(stream, "400 Bad Request", b"invalid intervene body");
    };
    if req.message.trim().is_empty() {
        return reply_intervene(stream, false, "empty message");
    }
    if req.task_id.is_empty() && req.slot.is_none() {
        return reply_intervene(stream, false, "missing task_id");
    }
    let Some(sink) = intervene else {
        return reply_intervene(stream, false, "intervene is not available on this surface");
    };
    let task_id = (!req.task_id.is_empty()).then_some(req.task_id.as_str());
    match sink.deliver(task_id, req.slot, &req.message) {
        Ok(()) => reply_intervene(stream, true, ""),
        Err(reason) => reply_intervene(stream, false, &reason),
    }
}

fn reply_intervene(stream: &mut TcpStream, ok: bool, error: &str) {
    let body = if ok {
        serde_json::json!({ "ok": true })
    } else {
        serde_json::json!({ "ok": false, "error": error })
    };
    match serde_json::to_vec(&body) {
        Ok(bytes) => write_response(stream, "application/json", &bytes),
        Err(_) => write_response(stream, "application/json", br#"{"ok":false}"#),
    }
}

/// §FS-rhei-viz.5.1: transport one explicit human gate transition to the host.
/// The host owns validation and plan writes; this route never rewrites the plan
/// directly.
fn handle_gate_transition(
    stream: &mut TcpStream,
    gate_transition: Option<&Arc<dyn GateTransitionSink>>,
    body: &[u8],
) {
    #[derive(serde::Deserialize)]
    struct Req {
        #[serde(default)]
        task_id: String,
        #[serde(default)]
        from: String,
        #[serde(default)]
        to: String,
        /// The operator's account of the decision. Optional so an older page
        /// (or a non-terminal release with nothing to say) still posts a valid
        /// body. §FS-rhei-viz.5.1
        #[serde(default)]
        result: Option<String>,
    }
    let Ok(req) = serde_json::from_slice::<Req>(body) else {
        return write_status(stream, "400 Bad Request", b"invalid gate transition body");
    };
    if req.task_id.trim().is_empty() {
        return reply_gate_transition(stream, false, "", "missing task_id");
    }
    if req.from.trim().is_empty() {
        return reply_gate_transition(stream, false, "", "missing from");
    }
    if req.to.trim().is_empty() {
        return reply_gate_transition(stream, false, "", "missing to");
    }
    let Some(sink) = gate_transition else {
        return reply_gate_transition(
            stream,
            false,
            "",
            "gate transitions are not available on this surface",
        );
    };
    // Blank is none: a field the operator left empty must not become a result
    // entry that says nothing. §FS-rhei-viz.5.1 §FS-rhei-states.3.3
    let result = req.result.as_deref().map(str::trim).filter(|message| !message.is_empty());
    match sink.transition_gate(req.task_id.trim(), req.from.trim(), req.to.trim(), result) {
        Ok(effective_to) => reply_gate_transition(stream, true, &effective_to, ""),
        Err(reason) => reply_gate_transition(stream, false, "", &reason),
    }
}

fn reply_gate_transition(stream: &mut TcpStream, ok: bool, to: &str, error: &str) {
    let body = if ok {
        serde_json::json!({ "ok": true, "to": to })
    } else {
        serde_json::json!({ "ok": false, "error": error })
    };
    match serde_json::to_vec(&body) {
        Ok(bytes) => write_response(stream, "application/json", &bytes),
        Err(_) => write_response(stream, "application/json", br#"{"ok":false}"#),
    }
}

/// True when an absolute, host-supplied log path sits within the workspace root
/// and contains no `..` traversal.
fn log_within_workspace(workspace: &str, log_path: &str) -> bool {
    !log_path.contains("..") && Path::new(log_path).starts_with(Path::new(workspace))
}

/// Read the durable log from `from`, capped per request so polling stays light.
/// A `from` past the current length (file rotated/truncated) restarts at 0.
fn read_log_tail(path: &Path, from: u64) -> (String, u64) {
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

/// Find the first `key=value` pair in a query string and percent-decode its value.
fn query_param(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        (k == key).then(|| percent_decode(v))
    })
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => match (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                (Some(h), Some(l)) => {
                    out.push(h * 16 + l);
                    i += 3;
                }
                _ => {
                    out.push(b'%');
                    i += 1;
                }
            },
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_val(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Join a workspace-relative path to the workspace root, rejecting absolute
/// paths and any `..` traversal so a click can never reach outside the run.
fn resolve_within_workspace(workspace: &str, rel: &str) -> Option<PathBuf> {
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

fn launch_editor(path: &Path) -> io::Result<()> {
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
fn resolve_editor() -> String {
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

fn now_ms() -> u128 {
    system_time_ms(SystemTime::now())
}

fn system_time_ms(value: SystemTime) -> u128 {
    value.duration_since(UNIX_EPOCH).unwrap_or_default().as_millis()
}

fn escape_json_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests;
