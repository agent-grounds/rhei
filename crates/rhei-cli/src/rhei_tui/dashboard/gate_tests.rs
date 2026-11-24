//! The gate routes as a client sees them: `/answer-gate` hands the answer
//! document to the host and returns what `rhei gate answer --json` prints, and
//! the snapshot says whether it can and carries each brief. §FS-rhei-viz.5.1.2 §FS-rhei-viz.8

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use super::tests::{fetch_snapshot_json, model, post_json};
use super::{DashboardSink, GateTransitionSink, PlanLoader};
use crate::rhei_viz_model::GateBrief;

/// `(task, from, answer document, dry_run)` for one answer the sink received.
type Answered = (String, String, serde_json::Value, bool);

/// A sink that answers gates by recording what it was asked and replying with
/// a fixed document, as the host's core would.
struct AnsweringSink {
    answered: Mutex<Vec<Answered>>,
    reply: Result<serde_json::Value, String>,
}

impl GateTransitionSink for AnsweringSink {
    fn transition_gate(
        &self,
        _: &str,
        _: &str,
        to: &str,
        _: Option<&str>,
    ) -> Result<String, String> {
        Ok(to.to_string())
    }

    fn answers_gates(&self) -> bool {
        true
    }

    fn answer_gate(
        &self,
        task_id: &str,
        from: &str,
        answers: &str,
        dry_run: bool,
    ) -> Result<serde_json::Value, String> {
        let document = serde_json::from_str(answers).expect("the route forwards a JSON document");
        self.answered.lock().unwrap().push((task_id.into(), from.into(), document, dry_run));
        self.reply.clone()
    }
}

/// A sink from before answers: it releases, and says nothing about answering.
struct ReleaseOnlySink;

impl GateTransitionSink for ReleaseOnlySink {
    fn transition_gate(
        &self,
        _: &str,
        _: &str,
        to: &str,
        _: Option<&str>,
    ) -> Result<String, String> {
        Ok(to.to_string())
    }
}

fn start(sink: Arc<dyn GateTransitionSink>, loader: Option<PlanLoader>) -> DashboardSink {
    DashboardSink::start_with_plan_intervene_and_gate(
        PathBuf::from("/tmp/ws"),
        1,
        1,
        loader,
        None,
        Some(sink),
    )
    .expect("start dashboard")
}

fn answering(reply: Result<serde_json::Value, String>) -> Arc<AnsweringSink> {
    Arc::new(AnsweringSink { answered: Mutex::new(Vec::new()), reply })
}

const ANSWER: &str = r#"{"task_id":" 1 ","from":"review","dry_run":true,
    "answers":{"version":1,"request_digest":"sha256:abc","answers":{"approve":true}}}"#;

/// The route forwards the document and the dry run, and replies with the
/// host's document as it came, refusals included. §FS-rhei-viz.5.1.2
#[test]
fn answer_gate_forwards_the_document_and_returns_the_hosts_reply() {
    let reply = serde_json::json!({"version": 1, "ok": true, "dry_run": true, "ticket": "1",
        "from": "review", "to": "completed", "route": {"index": 0, "when": {"approve": true}},
        "answer_path": "runtime/gates/1/review.1.answer.json", "result": "approve: yes", "errors": []});
    let sink = answering(Ok(reply.clone()));
    let dashboard = start(sink.clone(), None);

    assert_eq!(post_json(&dashboard, "/answer-gate", ANSWER), reply);
    let answered = sink.answered.lock().unwrap().clone();
    assert_eq!(answered.len(), 1);
    let (task, from, document, dry_run) = &answered[0];
    assert_eq!((task.as_str(), from.as_str(), *dry_run), ("1", "review", true));
    assert_eq!(document["request_digest"], "sha256:abc");
    assert_eq!(document["answers"]["approve"], true);

    let refused = serde_json::json!({"version": 1, "ok": false, "dry_run": false, "ticket": "1",
        "from": "review", "to": null, "route": null, "answer_path": null, "result": null,
        "errors": [{"kind": "invalid", "question": "approve", "message": "`approve` is required"}]});
    let dashboard = start(answering(Ok(refused.clone())), None);
    assert_eq!(post_json(&dashboard, "/answer-gate", ANSWER), refused);
}

/// Nothing reaches a host that cannot answer, a body without a document, or
/// a sink that refuses outright; each says why. §FS-rhei-viz.5.1.2
#[test]
fn answer_gate_says_why_it_cannot_answer() {
    let release_only = start(Arc::new(ReleaseOnlySink), None);
    let reply = post_json(&release_only, "/answer-gate", ANSWER);
    assert_eq!(reply["ok"], false);
    assert_eq!(reply["error"], "gate answers are not available on this surface");

    let sink = answering(Err("task '1' not found in the plan".into()));
    let dashboard = start(sink.clone(), None);
    let no_document = post_json(&dashboard, "/answer-gate", r#"{"task_id":"1","from":"review"}"#);
    assert_eq!(no_document["ok"], false);
    assert!(no_document["error"].as_str().unwrap().contains("missing answers"), "{no_document}");
    let no_from = post_json(&dashboard, "/answer-gate", r#"{"task_id":"1","answers":{}}"#);
    assert_eq!(no_from["error"], "missing from");
    assert!(sink.answered.lock().unwrap().is_empty(), "nothing reached the host");

    let failed = post_json(&dashboard, "/answer-gate", ANSWER);
    assert_eq!(failed["ok"], false);
    assert_eq!(failed["error"], "task '1' not found in the plan");
}

/// The snapshot carries each brief the loader built and says whether answers
/// are wired, apart from releases. §FS-rhei-viz.8
#[test]
fn snapshot_carries_briefs_and_the_answer_capability() {
    let brief = GateBrief {
        version: 1,
        ticket: "1".into(),
        title: "Review the draft".into(),
        state: "review".into(),
        source: "request".into(),
        digest: "sha256:abc".into(),
        ..GateBrief::default()
    };
    let loader: PlanLoader = Arc::new(move || {
        let mut plan = model("active", Vec::new());
        plan.gates.insert("1".into(), brief.clone());
        Some(plan)
    });
    let dashboard = start(answering(Err(String::new())), Some(loader));
    let snapshot = fetch_snapshot_json(&dashboard);
    assert_eq!(snapshot["capabilities"]["gate_transition"], true);
    assert_eq!(snapshot["capabilities"]["gate_answer"], true);
    assert_eq!(snapshot["gates"]["1"]["digest"], "sha256:abc");
    assert_eq!(
        snapshot["gates"]["1"]["since"],
        serde_json::Value::Null,
        "an unknown since stays null"
    );

    let release_only = start(Arc::new(ReleaseOnlySink), None);
    let snapshot = fetch_snapshot_json(&release_only);
    assert_eq!(snapshot["capabilities"]["gate_transition"], true);
    assert_eq!(snapshot["capabilities"]["gate_answer"], false);
    assert!(snapshot.get("gates").is_none(), "no gate, no briefs");
}
