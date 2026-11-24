//! Black-box contract for reading a gate: `rhei gate show`, `list` and
//! `schema`, each against the schema it publishes. §FS-rhei-gate-cmd

use serde_json::{json, Value};

use super::gate_support::*;
use super::*;

fn schema(fx: &GateFixture, name: &str) -> Value {
    let result = fx.rhei(&["gate", "schema", name]);
    assert_success(&result);
    serde_json::from_str(&result.stdout).expect("a gate schema is JSON")
}

/// Every field of the brief, in the documented order, with what a request
/// written by the state before the gate puts in it. §FS-rhei-gate-cmd.2.1
#[test]
fn show_json_prints_the_brief_field_for_field() {
    let fx = gate_fixture("gate-show-json");
    fx.prepare_review("verdict.1", GOOD_REQUEST);
    fx.enter("verdict.1", "review");

    let result = fx.rhei(&["gate", "show", "verdict.1", "--json"]);
    assert_success(&result);
    assert_eq!(
        top_level_keys(&result.stdout),
        [
            "version",
            "ticket",
            "ticket_title",
            "title",
            "state",
            "visit",
            "since",
            "plan",
            "root",
            "source",
            "summary",
            "display",
            "attachments",
            "questions",
            "destinations",
            "routes",
            "digest",
            "answer_path",
            "problems",
        ]
    );
    let brief: Value = serde_json::from_str(&result.stdout).expect("brief JSON");
    assert_eq!(brief["version"], 1);
    assert_eq!(brief["ticket"], "verdict.1");
    assert_eq!(brief["ticket_title"], "Draft the verdict");
    // The request's title wins over the block's. §FS-rhei-gate-brief.4.3
    assert_eq!(brief["title"], "Rule on the draft");
    assert_eq!(brief["state"], "review");
    assert_eq!(brief["visit"], 1);
    assert_eq!(brief["source"], "request");
    assert_eq!(brief["summary"], "The draft is ready.");
    assert_eq!(brief["display"]["path"], "runtime/drafts/verdict.1.md");
    assert_eq!(brief["display"]["readable"], true);
    assert!(brief["display"]["absolute"]
        .as_str()
        .is_some_and(|path| Path::new(path).is_absolute()));
    assert_eq!(
        brief["attachments"][0]["readable"], false,
        "a missing attachment is reported, not refused"
    );
    assert_eq!(brief["attachments"][0]["problem"], "does not exist");
    assert_eq!(
        brief["questions"].as_array().map(Vec::len),
        Some(2),
        "a routed request asks only its own"
    );
    assert_eq!(brief["routes"][0], json!({ "to": "completed", "when": { "approve": true } }));
    assert_eq!(
        brief["destinations"],
        json!([
            { "state": "completed", "description": "Done and shipped", "condition": null, "final": true },
            { "state": "drafting", "description": "The agent drafts the verdict", "condition": null, "final": false },
            { "state": "cancelled", "description": null, "condition": null, "final": true },
        ])
    );
    let digest = brief["digest"].as_str().expect("digest");
    assert!(digest.starts_with("sha256:") && digest.len() == 71, "{digest}");
    assert_eq!(brief["answer_path"], "runtime/gates/verdict.1/1.answer.json");
    assert_eq!(brief["problems"], json!([]));
    assert_matches_schema(&schema(&fx, "show"), &brief, "the request brief");
    assert_eq!(result.stderr, "", "a document on stdout, nothing on stderr");
}

/// Reading is not a write: the digest is the same twice over, and nothing
/// under the root changes. §FS-rhei-gate-brief.4.5 §FS-rhei-gate-cmd.2
#[test]
fn show_reads_without_writing_and_its_digest_is_stable() {
    let fx = gate_fixture("gate-show-stable");
    fx.prepare_review("verdict.1", GOOD_REQUEST);
    fx.enter("verdict.1", "review");
    let plan_before = fs::read_to_string(fx.root().join("verdict.rhei.md")).expect("plan");

    let first = fx.gate_json(&["show", "verdict.1"]);
    let second = fx.gate_json(&["show", "verdict.1"]);
    assert_eq!(first["digest"], second["digest"]);
    assert_eq!(fs::read_to_string(fx.root().join("verdict.rhei.md")).expect("plan"), plan_before);
    assert!(!fx.root().join("runtime/gates/verdict.1").exists(), "show records no answer");
}

/// A person's form of the same brief ends with the command that answers it.
/// §FS-rhei-gate-cmd.2.2
#[test]
fn show_text_ends_with_the_answer_command() {
    let fx = gate_fixture("gate-show-text");
    fx.prepare_review("verdict.1", GOOD_REQUEST);
    fx.enter("verdict.1", "review");

    let result = fx.rhei(&["gate", "show", "verdict.1"]);
    assert_success(&result);
    assert!(result.stdout.starts_with("verdict.1: Rule on the draft\n"), "{}", result.stdout);
    for expected in
        ["approve (boolean, required): Ship it?", "UNREADABLE: does not exist", "completed [final]"]
    {
        assert!(result.stdout.contains(expected), "missing {expected:?}:\n{}", result.stdout);
    }
    assert!(
        result.stdout.ends_with("rhei gate answer verdict.1 --from review --answers <file|->\n"),
        "{}",
        result.stdout
    );
}

/// A gate with no `gate:` block still asks something: where to, and why.
/// §FS-rhei-gate-brief.4.1
#[test]
fn a_gate_without_a_block_asks_for_a_destination_and_a_reason() {
    let fx = gate_fixture("gate-show-synthesized");
    fx.enter("verdict.3", "parked");

    let brief = fx.gate_json(&["show", "verdict.3"]);
    assert_eq!(brief["source"], "synthesized");
    assert_eq!(brief["title"], "Park me", "no request and no block title: the ticket's");
    assert_eq!(brief["routes"], json!([]));
    assert_eq!(brief["display"], Value::Null);
    let questions = brief["questions"].as_array().expect("questions");
    assert_eq!(questions.len(), 2);
    assert_eq!(questions[0]["id"], "destination");
    assert_eq!(questions[0]["kind"], "choice");
    assert_eq!(questions[0]["prompt"], "Where should verdict.3 go?");
    let values: Vec<&str> = questions[0]["options"]
        .as_array()
        .expect("options")
        .iter()
        .map(|option| option["value"].as_str().expect("value"))
        .collect();
    assert_eq!(
        values,
        ["drafting", "completed", "cancelled"],
        "declaration order, `*` edges included"
    );
    assert_eq!(questions[0]["options"][0]["description"], "The agent drafts the verdict");
    assert_eq!(
        questions[1],
        json!({ "id": "reason", "kind": "text", "prompt": "Why?", "required": true })
    );
    assert_matches_schema(&schema(&fx, "show"), &brief, "the synthesized brief");
}

/// Static questions are the brief, with the block's templates rendered.
/// §FS-rhei-gate-brief.1 §FS-rhei-gate-brief.1.1
#[test]
fn static_questions_are_the_brief() {
    let fx = gate_fixture("gate-show-static");
    fx.enter("verdict.2", "vetting");

    let brief = fx.gate_json(&["show", "verdict.2"]);
    assert_eq!(brief["source"], "questions");
    assert_eq!(brief["summary"], "Vet the plan, in Verdict");
    let ids: Vec<&str> = brief["questions"]
        .as_array()
        .expect("questions")
        .iter()
        .map(|q| q["id"].as_str().expect("id"))
        .collect();
    assert_eq!(ids, ["approve", "note"]);
    assert_eq!(brief["routes"][1], json!({ "to": "drafting", "when": {} }));
}

/// A request that went missing after entry degrades the brief instead of
/// stranding the ticket: exit 0, the problem said. §FS-rhei-gate-brief.4.2
#[test]
fn a_request_gone_missing_degrades_the_brief() {
    let fx = gate_fixture("gate-show-degraded");
    fx.prepare_review("verdict.1", GOOD_REQUEST);
    fx.enter("verdict.1", "review");
    fs::remove_file(fx.root().join("runtime/gates/verdict.1.request.json"))
        .expect("remove the request");

    let brief = fx.gate_json(&["show", "verdict.1"]);
    assert_eq!(brief["source"], "synthesized");
    let ids: Vec<&str> = brief["questions"]
        .as_array()
        .expect("questions")
        .iter()
        .map(|q| q["id"].as_str().expect("id"))
        .collect();
    assert_eq!(ids, ["destination", "reason"]);
    let problems = brief["problems"].to_string();
    assert!(problems.contains("runtime/gates/verdict.1.request.json"), "{problems}");
    assert_matches_schema(&schema(&fx, "show"), &brief, "the degraded brief");
}

/// `show` reads gates only, and says which state the ticket is in.
/// §FS-rhei-gate-cmd.2
#[test]
fn show_refuses_a_ticket_not_at_a_gate() {
    let fx = gate_fixture("gate-show-not-a-gate");
    let result = fx.rhei(&["gate", "show", "verdict.4"]);
    assert_eq!(
        result.status.code(),
        Some(1),
        "stdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert_stderr_contains(&result, "Task verdict.4 is in state 'todo', which is not a gate");
    assert_stderr_contains(&result, "rhei gate list");
    assert_eq!(result.stdout, "");
}

/// Every waiting ticket, with what it asks and what it holds; plan order when
/// no entry time is known. §FS-rhei-gate-cmd.4
#[test]
fn list_names_every_gate_with_required_and_holds() {
    let fx = gate_fixture("gate-list");
    fx.prepare_review("verdict.1", GOOD_REQUEST);
    fx.enter("verdict.1", "review");
    fx.enter("verdict.3", "parked");

    let result = fx.rhei(&["gate", "list", "--json"]);
    assert_success(&result);
    assert_eq!(top_level_keys(&result.stdout), ["version", "root", "gates"]);
    let listed: Value = serde_json::from_str(&result.stdout).expect("list JSON");
    let gates = listed["gates"].as_array().expect("gates");
    let tickets: Vec<&str> =
        gates.iter().map(|gate| gate["ticket"].as_str().expect("ticket")).collect();
    assert_eq!(tickets, ["verdict.1", "verdict.3"]);
    assert_eq!(gates[0]["required"], 1);
    assert_eq!(gates[0]["holds"], 1, "verdict.4 follows verdict.1");
    assert_eq!(gates[0]["source"], "request");
    assert_eq!(gates[1]["required"], 2, "destination and reason");
    assert_eq!(gates[1]["holds"], 0);
    assert_matches_schema(&schema(&fx, "list"), &listed, "the gate list");

    let text = fx.rhei(&["gate", "list"]);
    assert_success(&text);
    assert_eq!(text.stdout.lines().count(), 2, "{}", text.stdout);
    assert!(
        text.stdout.starts_with("verdict.1  review  Rule on the draft  required 1  holds 1\n"),
        "{}",
        text.stdout
    );
}

/// No gate is an empty list, not an error. §FS-rhei-gate-cmd.4
#[test]
fn list_with_no_gate_is_empty_and_succeeds() {
    let fx = gate_fixture("gate-list-empty");
    let listed = fx.gate_json(&["list"]);
    assert_eq!(listed["gates"], json!([]));
    let text = fx.rhei(&["gate", "list"]);
    assert_success(&text);
    assert_eq!(text.stdout, "No ticket waits at a gate.\n");
}

/// Five schemas, by short name or all at once, each the published file.
/// §FS-rhei-gate-cmd.5 §FS-rhei-gate-brief.9
#[test]
fn schema_prints_the_five_gate_documents() {
    let fx = gate_fixture("gate-schema");
    let all = fx.rhei(&["gate", "schema"]);
    assert_success(&all);
    assert_eq!(top_level_keys(&all.stdout), ["version", "schemas"]);
    let document: Value = serde_json::from_str(&all.stdout).expect("schemas JSON");
    let names: Vec<&str> =
        document["schemas"].as_object().expect("schemas").keys().map(String::as_str).collect();
    let mut expected = ["answer", "list", "request", "result", "show"];
    expected.sort_unstable();
    assert_eq!(names, expected);
    for name in ["request", "answer", "show", "result", "list"] {
        let one = fx.rhei(&["gate", "schema", name]);
        assert_success(&one);
        let source =
            repo_root().join(format!("crates/rhei-cli/schemas/rhei.gate.{name}.v1.schema.json"));
        assert_eq!(one.stdout, fs::read_to_string(source).expect("published schema source"));
        let parsed: Value = serde_json::from_str(&one.stdout).expect("schema JSON");
        assert_eq!(parsed["$id"], format!("rhei.gate.{name}.v1"));
        assert_eq!(document["schemas"][name], parsed);
    }

    let unknown = fx.rhei(&["gate", "schema", "verdict"]);
    assert!(!unknown.status.success());
    assert_stderr_contains(&unknown, "unknown gate schema 'verdict'");
    assert_stderr_contains(&unknown, "request, answer, show, result, list");
}

/// The request and answer schemas accept the documented examples and close
/// every object, because rhei refuses what it does not know. §FS-rhei-gate-brief.9
#[test]
fn request_and_answer_schemas_accept_the_examples_and_refuse_unknown_fields() {
    let fx = gate_fixture("gate-schema-examples");
    let request = schema(&fx, "request");
    let example: Value = serde_json::from_str(GOOD_REQUEST).expect("request JSON");
    assert_matches_schema(&request, &example, "the example request");
    let mut extra = example.clone();
    extra["urgency"] = json!("high");
    assert_violates_schema(&request, &extra, "a request with an unknown field");
    let mut reserved = example;
    reserved["questions"][0]["id"] = json!("reason");
    assert_violates_schema(&request, &reserved, "a request asking a reserved id");

    let answer = schema(&fx, "answer");
    let caller = json!({ "version": 1, "request_digest": "sha256:00", "answers": { "approve": true, "note": null } });
    assert_matches_schema(&answer, &caller, "a caller's answer");
    let recorded = json!({
        "version": 1, "ticket": "verdict.1", "state": "review", "visit": 1, "to": "completed",
        "route": { "index": 0, "when": { "approve": true } }, "request_digest": "sha256:00",
        "answers": { "approve": true }, "answered_by": "someone", "answered_at": "2026-11-24T12:00:00Z"
    });
    assert_matches_schema(&answer, &recorded, "a recorded answer");
    assert_violates_schema(
        &answer,
        &json!({ "version": 1, "answers": {} }),
        "an answer without its digest",
    );
}
