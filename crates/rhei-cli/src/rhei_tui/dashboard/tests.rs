use super::*;
use crate::rhei_tui::event::{
    DimensionStatus, DimensionSummary, PricingStatus, RunSummary, TaskOutcome, UsageCoverage,
    UsageReport, UsageStatus, UsageSummary,
};
use crate::rhei_viz_model::{Machine, TaskRow, TemplateContext, VizModel};
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime};

fn empty_state() -> DashboardState {
    DashboardState::new(PathBuf::from("/tmp/ws"), 1, 1)
}

fn assigned(from: &str, to: &str) -> RunEvent {
    RunEvent::SlotAssigned {
        slot: 0,
        task: "1".to_string(),
        from: from.to_string(),
        to: to.to_string(),
        agent: None,
        template_context: None,
        log_path: PathBuf::from("/tmp/ws/runtime/logs/1.log"),
        started_at: Instant::now(),
        wall_clock: SystemTime::now(),
    }
}

fn task_row(id: &str, title: &str, state: &str, parent: Option<&str>, depth: u8) -> TaskRow {
    TaskRow {
        id: id.to_string(),
        title: title.to_string(),
        parent: parent.map(str::to_string),
        depth,
        state: state.to_string(),
        visit_count: None,
        prior: Vec::new(),
        history: Vec::new(),
    }
}

/// Build a `VizModel` for a loader closure, as `rhei-viz` would. `plan_state`
/// is the pure derivation; the dashboard promotes it to `active` when a root is
/// running.
fn model(plan_state: &str, tasks: Vec<TaskRow>) -> VizModel {
    VizModel {
        plan_title: Some("Demo Plan".to_string()),
        plan_state: Some(plan_state.to_string()),
        about: None,
        tasks,
        machine: Machine { name: "rhei".to_string(), states: Vec::new() },
    }
}

fn fetch_snapshot_json(dashboard: &DashboardSink) -> serde_json::Value {
    let addr = dashboard.url().strip_prefix("http://").expect("loopback url");
    let mut stream = TcpStream::connect(addr).expect("connect to dashboard");
    stream
        .write_all(b"GET /snapshot HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .expect("write request");
    let mut response = String::new();
    stream.read_to_string(&mut response).expect("read response");
    let body = response.split("\r\n\r\n").nth(1).expect("response body");
    serde_json::from_str(body).expect("snapshot json")
}

fn fetch_status_line(dashboard: &DashboardSink, target: &str) -> String {
    let addr = dashboard.url().strip_prefix("http://").expect("loopback url");
    let mut stream = TcpStream::connect(addr).expect("connect to dashboard");
    let request = format!("GET {target} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).expect("write request");
    let mut response = String::new();
    stream.read_to_string(&mut response).expect("read response");
    response.lines().next().unwrap_or_default().to_string()
}

fn fetch_body(dashboard: &DashboardSink, target: &str) -> String {
    let addr = dashboard.url().strip_prefix("http://").expect("loopback url");
    let mut stream = TcpStream::connect(addr).expect("connect to dashboard");
    let request = format!("GET {target} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).expect("write request");
    let mut response = String::new();
    stream.read_to_string(&mut response).expect("read response");
    response.split("\r\n\r\n").nth(1).unwrap_or_default().to_string()
}

fn fetch_body_split_headers(dashboard: &DashboardSink, target: &str) -> String {
    let addr = dashboard.url().strip_prefix("http://").expect("loopback url");
    let mut stream = TcpStream::connect(addr).expect("connect to dashboard");
    let head = format!("GET {target} HTTP/1.1\r\nHost: local");
    stream.write_all(head.as_bytes()).expect("write first header chunk");
    stream.flush().expect("flush first header chunk");
    std::thread::sleep(Duration::from_millis(25));
    stream.write_all(b"host\r\nConnection: close\r\n\r\n").expect("write second header chunk");
    let mut response = String::new();
    stream.read_to_string(&mut response).expect("read response");
    response.split("\r\n\r\n").nth(1).unwrap_or_default().to_string()
}

fn post_json(dashboard: &DashboardSink, target: &str, body: &str) -> serde_json::Value {
    let addr = dashboard.url().strip_prefix("http://").expect("loopback url");
    let mut stream = TcpStream::connect(addr).expect("connect to dashboard");
    let request = format!(
        "POST {target} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).expect("write request");
    let mut response = String::new();
    stream.read_to_string(&mut response).expect("read response");
    let payload = response.split("\r\n\r\n").nth(1).unwrap_or_default();
    serde_json::from_str(payload).expect("intervene json")
}

fn post_json_split(dashboard: &DashboardSink, target: &str, body: &str) -> serde_json::Value {
    let addr = dashboard.url().strip_prefix("http://").expect("loopback url");
    let mut stream = TcpStream::connect(addr).expect("connect to dashboard");
    let head = format!(
        "POST {target} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).expect("write headers");
    stream.flush().expect("flush headers");
    std::thread::sleep(Duration::from_millis(25));
    stream.write_all(body.as_bytes()).expect("write body");
    let mut response = String::new();
    stream.read_to_string(&mut response).expect("read response");
    let payload = response.split("\r\n\r\n").nth(1).unwrap_or_default();
    serde_json::from_str(payload).expect("intervene json")
}

/// A stub [`InterveneSink`] that records deliveries (or always fails) so the
/// route can be tested without spawning a real agent.
struct StubIntervene {
    delivered: Mutex<Vec<(String, String)>>,
    fail_reason: Option<String>,
}
impl InterveneSink for StubIntervene {
    fn deliver(
        &self,
        task_id: Option<&str>,
        _slot: Option<crate::rhei_tui::event::Slot>,
        message: &str,
    ) -> Result<(), String> {
        if let Some(reason) = &self.fail_reason {
            return Err(reason.clone());
        }
        self.delivered
            .lock()
            .unwrap()
            .push((task_id.unwrap_or("").to_string(), message.to_string()));
        Ok(())
    }
}

/// `(task, from, to, result)` for one recorded gate request. The result is
/// recorded so the route's blank-is-none trimming is observable.
// §FS-rhei-viz.5.1
type GateRequest = (String, String, String, Option<String>);

/// A stub [`GateTransitionSink`] that records transition requests without
/// touching plan files.
struct StubGateTransition {
    requested: Mutex<Vec<GateRequest>>,
    fail_reason: Option<String>,
}

impl GateTransitionSink for StubGateTransition {
    fn transition_gate(
        &self,
        task_id: &str,
        from: &str,
        to: &str,
        result: Option<&str>,
    ) -> Result<String, String> {
        if let Some(reason) = &self.fail_reason {
            return Err(reason.clone());
        }
        self.requested.lock().unwrap().push((
            task_id.to_string(),
            from.to_string(),
            to.to_string(),
            result.map(str::to_string),
        ));
        Ok(to.to_string())
    }
}

// §FS-rhei-viz §11: only workspace-relative paths may be opened.
#[test]
fn resolve_within_workspace_rejects_escapes() {
    assert_eq!(
        resolve_within_workspace("/tmp/ws", "runtime/specs/task-7.md"),
        Some(PathBuf::from("/tmp/ws/runtime/specs/task-7.md"))
    );
    assert!(resolve_within_workspace("/tmp/ws", "./runtime/x.md").is_some());
    assert!(resolve_within_workspace("/tmp/ws", "../etc/passwd").is_none());
    assert!(resolve_within_workspace("/tmp/ws", "/etc/passwd").is_none());
    assert!(resolve_within_workspace("/tmp/ws", "a/../../b").is_none());
    assert!(resolve_within_workspace("/tmp/ws", "").is_none());
}

#[test]
fn percent_decode_and_query_param() {
    assert_eq!(percent_decode("runtime%2Fspecs%2Ftask-7.md"), "runtime/specs/task-7.md");
    assert_eq!(percent_decode("a+b"), "a b");
    assert_eq!(percent_decode("plain"), "plain");
    assert_eq!(query_param("path=runtime%2Fx.md&n=1", "path").as_deref(), Some("runtime/x.md"));
    assert_eq!(query_param("n=1", "path"), None);
}

// §FS-rhei-viz §11: the open route rejects missing and escaping paths with 400.
// Valid paths are not exercised here because they would launch a real editor.
#[test]
fn open_route_rejects_invalid_paths() {
    let dashboard = DashboardSink::start_with_plan(PathBuf::from("/tmp/ws"), 1, 1, None)
        .expect("start dashboard");
    assert!(fetch_status_line(&dashboard, "/open").contains("400"));
    assert!(fetch_status_line(&dashboard, "/open?path=..%2Fetc%2Fpasswd").contains("400"));
    assert!(fetch_status_line(&dashboard, "/open?path=%2Fetc%2Fpasswd").contains("400"));
}

// AR §2: the dashboard serves the one self-contained Flow asset at `/`, which
// polls `/snapshot` (its boot payload is left `null`).
#[test]
fn root_serves_the_self_contained_flow_asset() {
    let dashboard = DashboardSink::start_with_plan(PathBuf::from("/tmp/ws"), 1, 1, None)
        .expect("start dashboard");
    let html = fetch_body(&dashboard, "/");
    assert!(html.contains("Rhei · Flow"));
    assert!(html.contains("/*__BOOT__*/null"), "live asset leaves BOOT null so its JS polls");
    assert!(!html.contains("<script src="));
    assert!(!html.contains("<link rel=\"stylesheet\""));
}

#[test]
fn request_parser_accepts_split_headers() {
    let dashboard = DashboardSink::start_with_plan(PathBuf::from("/tmp/ws"), 1, 1, None)
        .expect("start dashboard");
    let html = fetch_body_split_headers(&dashboard, "/");

    assert!(html.contains("Rhei · Flow"));
    assert!(html.contains("/*__BOOT__*/null"));
}

/// Per `RunEvent::SlotAssigned`'s contract, `from == to` means the engine
/// started a worker in an autonomous state — not a transition.
#[test]
fn same_state_assignment_records_no_transition() {
    let mut state = empty_state();
    state.apply(&assigned("fetch", "fetch"));

    assert!(state.slots[0].active);
    assert_eq!(state.slots[0].state.as_deref(), Some("fetch"));
    assert!(
        state.slots[0].transition.is_none(),
        "from == to must not produce a transition; got {:?}",
        state.slots[0].transition
    );
    let last = state.recent.last().expect("recent line");
    assert!(
        last.text.contains("started in fetch"),
        "expected 'started in fetch'; got {:?}",
        last.text
    );
    assert!(
        !last.text.contains("fetch->fetch") && !last.text.contains("fetch→fetch"),
        "must not render a same-state arrow; got {:?}",
        last.text
    );
}

/// A real cross-state assignment must record both the `transition` string and a
/// `from->to` recent line.
#[test]
fn cross_state_assignment_records_arrow_transition() {
    let mut state = empty_state();
    state.apply(&assigned("draft", "pending"));

    assert_eq!(state.slots[0].transition.as_deref(), Some("draft->pending"));
    let last = state.recent.last().expect("recent line");
    assert!(last.text.contains("draft->pending"), "expected 'draft->pending'; got {:?}", last.text);
}

#[test]
fn url_path_encodes_unsafe_bytes_and_preserves_slashes() {
    assert_eq!(encode_url_path("/Users/me/project"), "/Users/me/project");
    assert_eq!(encode_url_path("/path with spaces/x"), "/path%20with%20spaces/x");
    assert_eq!(encode_url_path("/has#hash?and"), "/has%23hash%3Fand");
    assert_eq!(encode_url_path("/caf\u{00e9}"), "/caf%C3%A9");
}

/// `/snapshot` is the superset: the `VizModel` base (flat `tasks`, `machine`,
/// `about`) plus the runtime overlay (`task_runtime`, slots, deferred).
/// AR §4, §FS-rhei-viz §8.
#[test]
fn snapshot_endpoint_exposes_base_model_and_runtime_overlay() {
    let loader: PlanLoader = Arc::new(|| {
        Some(model(
            "pending",
            vec![
                task_row("1", "Root", "pending", None, 0),
                task_row("1.1", "Child", "agent-review", Some("1"), 1),
                task_row("2", "Deferred", "pending", None, 0),
            ],
        ))
    });
    let dashboard = DashboardSink::start_with_plan(PathBuf::from("/tmp/ws"), 2, 3, Some(loader))
        .expect("start dashboard");

    dashboard.emit(RunEvent::PassStarted { pass: 1, ready: vec!["2".to_string()] });
    dashboard.emit(RunEvent::TasksDeferred { pass: 1, tasks: vec!["2".to_string()] });
    dashboard.emit(RunEvent::SlotAssigned {
        slot: 1,
        task: "1.1".to_string(),
        from: "agent-review".to_string(),
        to: "agent-review".to_string(),
        agent: Some("codex".to_string()),
        template_context: Some(TemplateContext {
            target_slug: Some("codex-openai-gpt-5".to_string()),
            model: Some("gpt-5".to_string()),
            ..TemplateContext::default()
        }),
        log_path: PathBuf::from("/tmp/ws/runtime/logs/1.1.log"),
        started_at: Instant::now(),
        wall_clock: SystemTime::now(),
    });

    let snapshot = fetch_snapshot_json(&dashboard);
    assert_eq!(snapshot["plan_title"], "Demo Plan");
    // A running *child* (not a root) does not promote the plan to active.
    assert_eq!(snapshot["plan_state"], "pending");
    assert_eq!(snapshot["tasks"].as_array().expect("tasks").len(), 3);

    let child = snapshot["tasks"]
        .as_array()
        .expect("tasks")
        .iter()
        .find(|task| task["id"] == "1.1")
        .expect("child task");
    assert_eq!(child["title"], "Child");
    assert_eq!(child["parent"], "1");
    assert_eq!(child["depth"], 1);
    assert_eq!(child["state"], "agent-review");

    // Runtime overlay is keyed separately so the base `tasks` stay identical in
    // shape to a static render.
    assert_eq!(snapshot["task_runtime"]["1.1"]["in_slot"], 1);
    assert_eq!(
        snapshot["task_runtime"]["1.1"]["template_context"]["target_slug"],
        "codex-openai-gpt-5"
    );
    assert_eq!(snapshot["task_runtime"]["1.1"]["template_context"]["model"], "gpt-5");
    assert_eq!(snapshot["task_runtime"]["2"]["deferred_this_pass"], true);
}

#[test]
fn snapshot_marks_running_root_active() {
    let loader: PlanLoader = Arc::new(|| {
        Some(model(
            "pending",
            vec![task_row("1", "Root", "work", None, 0), task_row("2", "Next", "pending", None, 0)],
        ))
    });
    let dashboard = DashboardSink::start_with_plan(PathBuf::from("/tmp/ws"), 1, 2, Some(loader))
        .expect("start dashboard");

    dashboard.emit(RunEvent::SlotAssigned {
        slot: 0,
        task: "1".to_string(),
        from: "work".to_string(),
        to: "work".to_string(),
        agent: None,
        template_context: None,
        log_path: PathBuf::from("/tmp/ws/runtime/logs/1.log"),
        started_at: Instant::now(),
        wall_clock: SystemTime::now(),
    });

    assert_eq!(fetch_snapshot_json(&dashboard)["plan_state"], "active");
}

#[test]
fn snapshot_does_not_mark_released_slot_active() {
    let loader: PlanLoader = Arc::new(|| {
        Some(model(
            "pending",
            vec![
                task_row("1", "Root", "completed", None, 0),
                task_row("2", "Next", "pending", None, 0),
            ],
        ))
    });
    let dashboard = DashboardSink::start_with_plan(PathBuf::from("/tmp/ws"), 1, 2, Some(loader))
        .expect("start dashboard");

    dashboard.emit(RunEvent::SlotAssigned {
        slot: 0,
        task: "1".to_string(),
        from: "work".to_string(),
        to: "work".to_string(),
        agent: None,
        template_context: None,
        log_path: PathBuf::from("/tmp/ws/runtime/logs/1.log"),
        started_at: Instant::now(),
        wall_clock: SystemTime::now(),
    });
    dashboard.emit(RunEvent::SlotReleased {
        slot: 0,
        task: "1".to_string(),
        from: "work".to_string(),
        to: "completed".to_string(),
        log_path: PathBuf::from("/tmp/ws/runtime/logs/1.log"),
        outcome: TaskOutcome::Completed,
        finished_at: Instant::now(),
        wall_clock: SystemTime::now(),
        exit_code: Some(0),
        duration_ms: 10,
    });

    let snapshot = fetch_snapshot_json(&dashboard);
    assert_eq!(snapshot["plan_state"], "pending");
    assert!(snapshot["task_runtime"].get("1").is_none(), "released task has no runtime overlay");
}

// AR §6: /log tails the running task's durable log from a byte offset and
// returns the new bytes plus the next offset.
#[test]
fn log_endpoint_tails_running_task_log_from_offset() {
    let temp = tempfile::tempdir().expect("tempdir");
    let log_path = temp.path().join("runtime/logs/task-1-in-progress.log");
    fs::create_dir_all(log_path.parent().unwrap()).unwrap();
    fs::write(&log_path, "line one\nline two\n").unwrap();

    let dashboard = DashboardSink::start_with_plan(temp.path().to_path_buf(), 1, 1, None)
        .expect("start dashboard");
    dashboard.emit(RunEvent::SlotAssigned {
        slot: 0,
        task: "1".to_string(),
        from: "in-progress".to_string(),
        to: "in-progress".to_string(),
        agent: Some("codex".to_string()),
        template_context: None,
        log_path: log_path.clone(),
        started_at: Instant::now(),
        wall_clock: SystemTime::now(),
    });

    let first: serde_json::Value =
        serde_json::from_str(&fetch_body(&dashboard, "/log?task=1&from=0")).expect("json");
    assert!(first["data"].as_str().unwrap().contains("line one"));
    let next = first["next"].as_u64().unwrap();
    assert!(next > 0);

    // Append more and tail from the recorded offset — only the delta returns.
    fs::write(&log_path, "line one\nline two\nline three\n").unwrap();
    let second: serde_json::Value =
        serde_json::from_str(&fetch_body(&dashboard, &format!("/log?task=1&from={next}")))
            .expect("json");
    assert_eq!(second["data"], "line three\n");

    // Unknown task → empty tail, not an error.
    let none: serde_json::Value =
        serde_json::from_str(&fetch_body(&dashboard, "/log?task=99&from=0")).expect("json");
    assert_eq!(none["data"], "");
}

// AR §7: POST /intervene delivers the message to the host sink and reports the
// outcome; it never touches plan state.
#[test]
fn intervene_delivers_to_sink_and_reports_outcome() {
    use std::sync::Mutex as StdMutex;
    let sink = Arc::new(StubIntervene { delivered: StdMutex::new(Vec::new()), fail_reason: None });
    let dashboard = DashboardSink::start_with_plan_and_intervene(
        PathBuf::from("/tmp/ws"),
        1,
        1,
        None,
        Some(sink.clone() as Arc<dyn InterveneSink>),
    )
    .expect("start dashboard");

    let ok = post_json(&dashboard, "/intervene", r#"{"task_id":"1","message":"focus on tests"}"#);
    assert_eq!(ok["ok"], true);
    assert_eq!(
        sink.delivered.lock().unwrap().as_slice(),
        &[("1".to_string(), "focus on tests".to_string())]
    );

    // Empty message is rejected without reaching the sink.
    let empty = post_json(&dashboard, "/intervene", r#"{"task_id":"1","message":"  "}"#);
    assert_eq!(empty["ok"], false);
    assert_eq!(sink.delivered.lock().unwrap().len(), 1);
}

#[test]
fn intervene_reads_split_and_large_request_body() {
    use std::sync::Mutex as StdMutex;
    let sink = Arc::new(StubIntervene { delivered: StdMutex::new(Vec::new()), fail_reason: None });
    let dashboard = DashboardSink::start_with_plan_and_intervene(
        PathBuf::from("/tmp/ws"),
        1,
        1,
        None,
        Some(sink.clone() as Arc<dyn InterveneSink>),
    )
    .expect("start dashboard");

    let message = "x".repeat(9000);
    let body = format!(r#"{{"task_id":"1","message":"{message}"}}"#);
    let ok = post_json_split(&dashboard, "/intervene", &body);

    assert_eq!(ok["ok"], true);
    assert_eq!(sink.delivered.lock().unwrap().as_slice(), &[("1".to_string(), message)]);
}

#[test]
fn intervene_reports_unreachable_agent() {
    use std::sync::Mutex as StdMutex;
    let sink = Arc::new(StubIntervene {
        delivered: StdMutex::new(Vec::new()),
        fail_reason: Some("agent not interactively reachable".to_string()),
    });
    let dashboard = DashboardSink::start_with_plan_and_intervene(
        PathBuf::from("/tmp/ws"),
        1,
        1,
        None,
        Some(sink as Arc<dyn InterveneSink>),
    )
    .expect("start dashboard");

    let res = post_json(&dashboard, "/intervene", r#"{"task_id":"7","message":"hi"}"#);
    assert_eq!(res["ok"], false);
    assert_eq!(res["error"], "agent not interactively reachable");
}

#[test]
fn intervene_without_sink_reports_unavailable() {
    let dashboard = DashboardSink::start_with_plan(PathBuf::from("/tmp/ws"), 1, 1, None)
        .expect("start dashboard");
    let res = post_json(&dashboard, "/intervene", r#"{"task_id":"1","message":"hi"}"#);
    assert_eq!(res["ok"], false);
}

// §FS-rhei-viz.5.1: POST /transition-gate transports an explicit human gate
// transition to the host sink and reports the effective target state.
#[test]
fn gate_transition_delivers_to_sink_and_reports_outcome() {
    use std::sync::Mutex as StdMutex;
    let sink =
        Arc::new(StubGateTransition { requested: StdMutex::new(Vec::new()), fail_reason: None });
    let dashboard = DashboardSink::start_with_plan_intervene_and_gate(
        PathBuf::from("/tmp/ws"),
        1,
        1,
        None,
        None,
        Some(sink.clone() as Arc<dyn GateTransitionSink>),
    )
    .expect("start dashboard");

    let ok = post_json(
        &dashboard,
        "/transition-gate",
        r#"{"task_id":"1","from":"human-gate","to":"completed"}"#,
    );
    assert_eq!(ok["ok"], true);
    assert_eq!(ok["to"], "completed");

    // The operator's result rides the same request; a whitespace-only field is
    // no field at all. §FS-rhei-viz.5.1 §FS-rhei-states.3.3
    let with_result = post_json(
        &dashboard,
        "/transition-gate",
        r#"{"task_id":"1","from":"human-gate","to":"completed","result":"  Reviewed and shipped.  "}"#,
    );
    assert_eq!(with_result["ok"], true);
    let blank = post_json(
        &dashboard,
        "/transition-gate",
        r#"{"task_id":"1","from":"human-gate","to":"completed","result":"   "}"#,
    );
    assert_eq!(blank["ok"], true);

    assert_eq!(
        sink.requested.lock().unwrap().as_slice(),
        &[
            ("1".to_string(), "human-gate".to_string(), "completed".to_string(), None),
            (
                "1".to_string(),
                "human-gate".to_string(),
                "completed".to_string(),
                Some("Reviewed and shipped.".to_string())
            ),
            ("1".to_string(), "human-gate".to_string(), "completed".to_string(), None),
        ]
    );

    let snapshot = fetch_snapshot_json(&dashboard);
    assert_eq!(snapshot["capabilities"]["gate_transition"], true);
}

#[test]
fn gate_transition_reports_unavailable_without_sink() {
    let dashboard = DashboardSink::start_with_plan(PathBuf::from("/tmp/ws"), 1, 1, None)
        .expect("start dashboard");
    let res = post_json(
        &dashboard,
        "/transition-gate",
        r#"{"task_id":"1","from":"human-gate","to":"completed"}"#,
    );
    assert_eq!(res["ok"], false);

    let snapshot = fetch_snapshot_json(&dashboard);
    assert_eq!(snapshot["capabilities"]["gate_transition"], false);
}

// AR §7: the dashboard publishes its loopback URL to runtime/dashboard.json so a
// separate `rhei intervene` process can discover and message the live run, and
// removes the file when the run ends.
#[test]
fn dashboard_publishes_and_cleans_up_discovery_file() {
    let temp = tempfile::tempdir().expect("tempdir");
    let addr_file = temp.path().join("runtime/dashboard.json");
    let dashboard = DashboardSink::start_with_plan(temp.path().to_path_buf(), 1, 1, None)
        .expect("start dashboard");

    let raw = fs::read_to_string(&addr_file).expect("discovery file written");
    let value: serde_json::Value = serde_json::from_str(&raw).expect("valid json");
    assert_eq!(value["url"], dashboard.url());
    assert!(value["pid"].as_u64().is_some());

    dashboard.finish();
    assert!(!addr_file.exists(), "discovery file removed on shutdown");
}

// §FS-rhei-viz §5: the snapshot surfaces per-slot intervene capability so the
// Flow composer is gated on what the agent can actually take. A slot whose agent
// holds stdin open reports `intervene: true`; a one-shot agent reports `false`.
#[test]
fn snapshot_marks_intervene_capability_per_slot() {
    struct CapabilityStub;
    impl InterveneSink for CapabilityStub {
        fn deliver(
            &self,
            _task_id: Option<&str>,
            _slot: Option<crate::rhei_tui::event::Slot>,
            _message: &str,
        ) -> Result<(), String> {
            Ok(())
        }
        fn reachable(&self, task_id: &str, _slot: Option<crate::rhei_tui::event::Slot>) -> bool {
            task_id == "1"
        }
    }

    let loader: PlanLoader = Arc::new(|| {
        Some(model(
            "active",
            vec![
                task_row("1", "Streaming", "work", None, 0),
                task_row("2", "OneShot", "work", None, 0),
            ],
        ))
    });
    let dashboard = DashboardSink::start_with_plan_and_intervene(
        PathBuf::from("/tmp/ws"),
        2,
        2,
        Some(loader),
        Some(Arc::new(CapabilityStub) as Arc<dyn InterveneSink>),
    )
    .expect("start dashboard");

    for (slot, task) in [(0u16, "1"), (1u16, "2")] {
        dashboard.emit(RunEvent::SlotAssigned {
            slot,
            task: task.to_string(),
            from: "work".to_string(),
            to: "work".to_string(),
            agent: Some("custom".to_string()),
            template_context: None,
            log_path: PathBuf::from(format!("/tmp/ws/runtime/logs/{task}.log")),
            started_at: Instant::now(),
            wall_clock: SystemTime::now(),
        });
    }

    let snapshot = fetch_snapshot_json(&dashboard);
    // The agent holding stdin open is messageable; the one-shot agent is not.
    assert_eq!(snapshot["task_runtime"]["1"]["intervene"], true);
    assert_eq!(snapshot["task_runtime"]["2"]["intervene"], false);
}

fn dashboard_usage(
    invocation_id: &str,
    coverage: UsageCoverage,
    pricing_status: PricingStatus,
    cost_micro: Option<u64>,
    priced_cost_micro: Option<u64>,
) -> UsageSummary {
    let measured = DimensionSummary {
        value: Some(1),
        status: DimensionStatus::Measured,
        missing_count: 0,
        measured_count: 1,
    };
    UsageSummary {
        invocation_id: invocation_id.to_string(),
        state: "work".to_string(),
        agent: "codex".to_string(),
        provider: Some("openai".to_string()),
        model: Some("gpt-test".to_string()),
        total: measured.clone(),
        input_total: measured.clone(),
        input_cached_read: measured.clone(),
        input_cache_write: measured.clone(),
        output_total: measured.clone(),
        output_cached_read: measured.clone(),
        output_cache_write: measured,
        cost_micro,
        priced_cost_micro,
        currency: Some("USD".to_string()),
        coverage,
        status: UsageStatus::Measured,
        pricing_status,
    }
}

#[test]
fn dashboard_mixed_priced_and_unpriced_rollup_is_partial() {
    let mut state = empty_state();
    state.apply(&RunEvent::UsageReported {
        slot: None,
        task: "1".to_string(),
        invocation_id: "priced".to_string(),
        report: UsageReport::Final,
        usage: dashboard_usage(
            "priced",
            UsageCoverage::Complete,
            PricingStatus::Priced,
            Some(100),
            Some(100),
        ),
    });
    state.apply(&RunEvent::UsageReported {
        slot: None,
        task: "2".to_string(),
        invocation_id: "unpriced".to_string(),
        report: UsageReport::Final,
        usage: dashboard_usage(
            "unpriced",
            UsageCoverage::Unpriced,
            PricingStatus::Unpriced,
            None,
            None,
        ),
    });

    let accounting = state.accounting.expect("accounting");
    assert_eq!(accounting.coverage, UsageCoverage::Partial);
    assert_eq!(accounting.pricing_status, PricingStatus::PartialPrice);
    assert_eq!(accounting.cost_micro, None);
    assert_eq!(accounting.priced_cost_micro, Some(100));
}

/// `/snapshot` carries the compact per-task accounting rollups (direct +
/// subtree) in the runtime overlay. §FS-rhei-cost-accounting §6, §10.
#[test]
fn snapshot_task_runtime_carries_accounting_rollups() {
    let loader: PlanLoader = Arc::new(|| {
        Some(model(
            "active",
            vec![
                task_row("1", "Root", "in-progress", None, 0),
                task_row("1.1", "Child", "in-progress", Some("1"), 1),
            ],
        ))
    });
    let dashboard = DashboardSink::start_with_plan(PathBuf::from("/tmp/ws"), 1, 2, Some(loader))
        .expect("start dashboard");
    dashboard.emit(RunEvent::UsageReported {
        slot: Some(0),
        task: "1.1".to_string(),
        invocation_id: "i1".to_string(),
        report: UsageReport::Final,
        usage: dashboard_usage(
            "i1",
            UsageCoverage::Complete,
            PricingStatus::Priced,
            Some(50),
            Some(50),
        ),
    });

    let snapshot = fetch_snapshot_json(&dashboard);
    // The child has direct cost; the root has it only in its subtree rollup.
    assert!(snapshot["task_runtime"]["1.1"]["accounting"]["direct"].is_object());
    assert!(snapshot["task_runtime"]["1"]["accounting"]["subtree"].is_object());
}

/// Dashboard-enabled runs leave an inspectable static artifact behind after the
/// loopback server exits — produced by the *same* static renderer. §FS-rhei-viz
/// §7.1, AR §5.3.
#[test]
fn frozen_dashboard_writes_self_contained_final_artifact() {
    let temp = tempfile::tempdir().expect("tempdir");
    let loader: PlanLoader =
        Arc::new(|| Some(model("completed", vec![task_row("1", "Done", "completed", None, 0)])));
    let dashboard = DashboardSink::start_with_plan(temp.path().to_path_buf(), 1, 1, Some(loader))
        .expect("start dashboard");
    dashboard.emit(RunEvent::RunFinished {
        summary: RunSummary {
            agents_spawned: 0,
            programs_spawned: 1,
            terminal_tasks: 1,
            total_tasks: 1,
            accounting: None,
            workspace_accounting: None,
            stop: None,
        },
    });

    let path = dashboard.write_frozen_dashboard().expect("write frozen dashboard");
    assert_eq!(path, temp.path().join("runtime/dashboard.html"));
    let html = fs::read_to_string(path).expect("frozen html");
    // The freeze inlines the final snapshot into the one asset; no placeholder
    // remains and the page is self-contained.
    assert!(html.contains("\"plan_title\":\"Done\"") || html.contains("Done"));
    assert!(html.contains("const BOOT = {"));
    assert!(!html.contains("/*__BOOT__*/null"), "frozen page must not poll");
    assert!(!html.contains("<script src="));
}

/// Temporary plan reload failures keep serving the last good model. §FS-rhei-viz
/// §7.1.
#[test]
fn snapshot_keeps_last_good_model() {
    let calls = Arc::new(AtomicUsize::new(0));
    let loader_calls = Arc::clone(&calls);
    let loader: PlanLoader = Arc::new(move || {
        if loader_calls.fetch_add(1, Ordering::SeqCst) == 0 {
            Some(model("draft", vec![task_row("1", "Root", "draft", None, 0)]))
        } else {
            None
        }
    });
    let dashboard = DashboardSink::start_with_plan(PathBuf::from("/tmp/ws"), 1, 1, Some(loader))
        .expect("start dashboard");

    let first = fetch_snapshot_json(&dashboard);
    let second = fetch_snapshot_json(&dashboard);

    assert_eq!(first["plan_title"], "Demo Plan");
    assert_eq!(second["plan_title"], "Demo Plan");
    assert_eq!(second["plan_state"], "draft");
    assert_eq!(second["tasks"].as_array().expect("tasks").len(), 1);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}
