use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::rhei_viz_model::VizModel;

use crate::rhei_tui::event::{EventSink, RunEvent};

mod files;
mod gate;
mod http;
mod intervene;
mod state;

use files::{derive_auto_links, handle_log, handle_open};
pub use gate::GateTransitionSink;
use gate::{handle_gate_answer, handle_gate_transition};
use http::{escape_json_string, read_http_request, write_not_found, write_response};
use intervene::handle_intervene;
pub use intervene::InterveneSink;
use state::{task_accounting_for_tasks, DashboardLink, DashboardState, TaskAccounting};

const RECENT_LIMIT: usize = 200;
const SLOT_TRAFFIC_LIMIT: usize = 60;

/// Closure invoked on every `/snapshot` request: the host builds the full
/// [`VizModel`] via `rhei-viz` (so the dashboard never parses plans); a `None`
/// from a transient reload failure keeps the previous model. §AR-rhei-viz-flow.3
pub type PlanLoader = Arc<dyn Fn() -> Option<VizModel> + Send + Sync>;

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
    /// Whether `/answer-gate` reaches the core `rhei gate answer` runs. §FS-rhei-viz.5.1.2
    gate_answer: bool,
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
        // §AR-rhei-viz-flow.7: the one route into an agent — deliver a message to its stdin.
        "/intervene" => handle_intervene(&mut stream, intervene, &request.body),
        // §FS-rhei-viz.5.1.3: explicit human transition out of a plain gating state.
        "/transition-gate" => handle_gate_transition(&mut stream, gate_transition, &request.body),
        // §FS-rhei-viz.5.1.2: an answer to a briefed gate, dry run or real.
        "/answer-gate" => handle_gate_answer(&mut stream, gate_transition, &request.body),
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
            let capabilities = DashboardCapabilities {
                gate_transition: gate_transition.is_some(),
                gate_answer: gate_transition.is_some_and(|sink| sink.answers_gates()),
            };
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

fn now_ms() -> u128 {
    system_time_ms(SystemTime::now())
}

fn system_time_ms(value: SystemTime) -> u128 {
    value.duration_since(UNIX_EPOCH).unwrap_or_default().as_millis()
}

#[cfg(test)]
mod gate_tests;
#[cfg(test)]
mod tests;
