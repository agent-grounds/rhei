// The gate module's own rules, exercised without a machine or a file: request
// reading, question and route checks, templates, and the brief. The machine's
// refusal of a bad block is tested beside the machine's other load checks.

// §FS-rhei-gate-brief

use super::*;
use serde_json::json;

fn approve_question() -> Question {
    Question {
        id: "approve".into(),
        kind: QuestionKind::Boolean,
        prompt: "Ship it?".into(),
        required: true,
        options: None,
        help: None,
    }
}

fn route(to: &str, when: serde_json::Value) -> Route {
    serde_json::from_value(json!({ "to": to, "when": when })).unwrap()
}

fn sample_request() -> serde_json::Value {
    json!({
        "version": 1,
        "title": "Rule on 3 disputed entries",
        "summary": "What was done.",
        "attachments": ["runtime/agora/charter.md"],
        "questions": [
            { "id": "d1", "kind": "choice", "prompt": "Cache per plan or per root?",
              "options": [ { "value": "plan", "label": "Per plan" }, { "value": "root" } ] },
            { "id": "approve", "kind": "boolean", "prompt": "Accept the verdict?" },
            { "id": "note", "kind": "text", "prompt": "Reasoning", "required": false }
        ]
    })
}

// §FS-rhei-gate-brief.3
#[test]
fn a_valid_request_reads_with_its_routes() {
    let routes = vec![
        route("shipping", json!({ "approve": true, "d1": "root" })),
        route("revising", json!({})),
    ];
    let request =
        parse_request(&sample_request().to_string(), false, &routes).expect("valid request");
    assert_eq!(request.title, "Rule on 3 disputed entries");
    assert_eq!(request.questions.len(), 3);
    assert!(request.questions[1].required, "required defaults to true");
    assert!(!request.questions[2].required);
}

// §FS-rhei-gate-brief.3 §FS-rhei-gate-brief.2
#[test]
fn every_problem_in_a_request_is_reported_together() {
    let text = json!({
        "version": 2,
        "title": "",
        "display": "/etc/passwd",
        "attachments": ["../outside.md"],
        "extra": true,
        "questions": [
            { "id": "Bad", "kind": "choice", "prompt": "x", "options": [ { "value": "a" } ] },
            { "id": "reason", "kind": "text", "prompt": "" },
            { "id": "flag", "kind": "boolean", "prompt": "x", "options": [] },
            { "id": "flag", "kind": "dropdown", "prompt": "x", "colour": "red" }
        ]
    })
    .to_string();
    let problems = parse_request(&text, false, &[]).unwrap_err();
    let joined = problems.join("\n");
    for expected in [
        "unknown field `extra`",
        "`version` is 2",
        "`title` is empty",
        "`display` '/etc/passwd' is absolute",
        "`attachments[0]` '../outside.md' escapes",
        "`questions[0].id` 'Bad' must match",
        "needs at least 2",
        "`questions[1].id` 'reason' is reserved",
        "`questions[1].prompt` is empty",
        "`questions[2].options` is not allowed on a boolean question",
        "`questions[3].id` 'flag' is used by an earlier question",
        "`questions[3].kind` 'dropdown' is not one of",
        "unknown field `colour`",
    ] {
        assert!(joined.contains(expected), "missing {expected:?} in:\n{joined}");
    }
}

// §FS-rhei-gate-brief.3
#[test]
fn a_request_may_not_set_a_display_the_block_owns() {
    let mut request = sample_request();
    request["display"] = json!("runtime/agora/verdict.md");
    let problems = parse_request(&request.to_string(), true, &[]).unwrap_err();
    assert!(problems[0].contains("set by the state machine's `gate:` block"), "{problems:?}");
    assert!(parse_request(&request.to_string(), false, &[]).is_ok());
}

// §FS-rhei-gate-brief.2
#[test]
fn routes_are_checked_against_the_request_questions() {
    let routes = vec![
        route("a", json!({ "missing": true })),
        route("b", json!({ "note": "x" })),
        route("c", json!({ "d1": "elsewhere" })),
        route("d", json!({ "approve": "yes" })),
    ];
    let problems = parse_request(&sample_request().to_string(), false, &routes).unwrap_err();
    let joined = problems.join("\n");
    assert!(joined.contains("`routes[0].when.missing` names no question"), "{joined}");
    assert!(joined.contains("`routes[1].when.note` names a text question"), "{joined}");
    assert!(joined.contains("`routes[2].when.d1` expects 'elsewhere'"), "{joined}");
    assert!(joined.contains("`routes[3].when.approve` must be true or false"), "{joined}");
}

#[test]
fn non_json_requests_are_refused_with_the_parse_error() {
    let problems = parse_request("{ not json", false, &[]).unwrap_err();
    assert!(problems[0].starts_with("the request is not JSON"), "{problems:?}");
}

// §FS-rhei-gate-brief.5.1
#[test]
fn multi_routes_take_one_option_or_a_set_of_them() {
    let pick = Question {
        id: "pick".into(),
        kind: QuestionKind::Multi,
        prompt: "Pick".into(),
        required: true,
        options: Some(vec![
            QuestionOption { value: "a".into(), label: None, description: None },
            QuestionOption { value: "b".into(), label: None, description: None },
        ]),
        help: None,
    };
    let fine = vec![route("x", json!({ "pick": "a" })), route("y", json!({ "pick": ["a", "b"] }))];
    assert!(route_when_problems(&fine, std::slice::from_ref(&pick)).is_empty());
    let bad = vec![route("x", json!({ "pick": ["a", "a"] })), route("y", json!({ "pick": 3 }))];
    assert_eq!(route_when_problems(&bad, &[pick]).len(), 2);
}

// §FS-rhei-gate-brief.1.1
#[test]
fn templates_allow_only_what_a_gate_resolves() {
    assert!(
        template_problems(GateField::Request, "runtime/gates/{task_id}-{state}.json").is_empty()
    );
    let path = template_problems(GateField::Display, "runtime/{visit_count}/{task_title}.md");
    assert_eq!(path.len(), 2, "{path:?}");
    assert!(template_problems(GateField::Title, "Rule on {task_title} ({meta.owner})").is_empty());
    assert_eq!(template_problems(GateField::Summary, "{model} said {meta.}").len(), 2);
    // Brace content that is not identifier-shaped is literal text.
    assert!(template_problems(GateField::Summary, "a JSON {\"k\": 1} example").is_empty());
    let rendered = render_template("{task_id}: {meta.missing} {state}", |name| match name {
        "task_id" => Some("agora.1".into()),
        "state" => Some("judge".into()),
        _ => None,
    });
    assert_eq!(rendered, "agora.1: {meta.missing} judge");
}

// §FS-rhei-gate-brief.1.2
#[test]
fn a_gate_block_is_checked_against_its_edges_and_questions() {
    let gate: GateDef = serde_yaml::from_str(
        r#"
display: /abs/verdict.md
request: runtime/{agent}.json
questions:
  - { id: approve, kind: boolean, prompt: "Ship?" }
routes:
  - { to: nowhere, when: { approve: true } }
  - { to: shipping, when: { other: 1 } }
"#,
    )
    .unwrap();
    let problems = gate_def_problems(&gate, &["shipping", "revising"]).join("\n");
    assert!(problems.contains("both `request` and `questions`"), "{problems}");
    assert!(problems.contains("`display` '/abs/verdict.md' is absolute"), "{problems}");
    assert!(problems.contains("uses {agent}"), "{problems}");
    assert!(problems.contains("`routes[0].to` 'nowhere' is not a state"), "{problems}");
    // Under `request`, `when` waits for the request on entry.
    assert!(!problems.contains("routes[1].when"), "{problems}");

    let static_gate: GateDef = serde_yaml::from_str(
        "questions:\n  - { id: approve, kind: boolean, prompt: \"Ship?\" }\nroutes:\n  - { to: shipping, when: { other: 1 } }\n",
    )
    .unwrap();
    let problems = gate_def_problems(&static_gate, &["shipping"]).join("\n");
    assert!(problems.contains("`routes[0].when.other` names no question"), "{problems}");

    let empty: GateDef = serde_yaml::from_str("questions: []\n").unwrap();
    assert!(gate_def_problems(&empty, &[]).join("\n").contains("`questions` is empty"));
}

#[test]
fn unknown_block_fields_are_refused_by_the_shape() {
    let refused = serde_yaml::from_str::<GateDef>("titel: oops\n");
    assert!(refused.is_err());
}

fn destinations_fixture() -> Vec<Destination> {
    vec![
        Destination {
            state: "completed".into(),
            description: Some("Done".into()),
            condition: None,
            terminal: true,
        },
        Destination {
            state: "revising".into(),
            description: None,
            condition: None,
            terminal: false,
        },
    ]
}

fn parts<'a>(
    block: Option<&'a GateDef>,
    request: Option<Result<GateRequest, Vec<String>>>,
    destinations: &'a [Destination],
) -> BriefParts<'a> {
    BriefParts {
        ticket: "agora.1",
        ticket_title: "Discuss",
        state: "judge",
        visit: 1,
        block,
        block_title: block.and_then(|block| block.title.clone()),
        block_summary: None,
        block_display: block.and_then(|block| block.display.clone()),
        request,
        destinations,
    }
}

// §FS-rhei-gate-brief.4.1
#[test]
fn a_gate_without_a_block_asks_destination_and_reason() {
    let destinations = destinations_fixture();
    let brief = Brief::build(parts(None, None, &destinations));
    assert_eq!(brief.source, BriefSource::Synthesized);
    let ids: Vec<&str> = brief.questions.iter().map(|q| q.id.as_str()).collect();
    assert_eq!(ids, ["destination", "reason"]);
    assert_eq!(brief.questions[0].prompt, "Where should agora.1 go?");
    let options = brief.questions[0].options.as_ref().unwrap();
    assert_eq!(options[0].value, "completed");
    assert_eq!(options[0].description.as_deref(), Some("Done"));
    assert_eq!(brief.title, "Discuss");
    assert_eq!(brief.required(), 2);
    assert_eq!(brief.authored_required(), 0);
    assert!(brief.digest.starts_with("sha256:") && brief.digest.len() == 7 + 64);
}

// §FS-rhei-gate-brief.4 §FS-rhei-gate-brief.4.3
#[test]
fn static_questions_with_routes_ask_only_what_was_authored() {
    let destinations = destinations_fixture();
    let block = GateDef {
        title: Some("Ship it".into()),
        questions: Some(vec![approve_question()]),
        routes: vec![route("completed", json!({ "approve": true })), route("revising", json!({}))],
        ..GateDef::default()
    };
    let brief = Brief::build(parts(Some(&block), None, &destinations));
    assert_eq!(brief.source, BriefSource::Questions);
    assert_eq!(brief.questions.len(), 1);
    assert_eq!(brief.authored_required(), 1);
    assert_eq!(brief.title, "Ship it");
    assert_eq!(brief.routes.len(), 2);
}

// §FS-rhei-gate-brief.4.2
#[test]
fn a_broken_request_degrades_to_the_synthesized_brief() {
    let destinations = destinations_fixture();
    let block = GateDef {
        display: Some("runtime/verdict.md".into()),
        request: Some("runtime/gates/agora.1.request.json".into()),
        routes: vec![route("completed", json!({ "approve": true }))],
        ..GateDef::default()
    };
    let broken =
        Some(Err(
            vec!["the request file runtime/gates/agora.1.request.json does not exist".into()],
        ));
    let brief = Brief::build(parts(Some(&block), broken, &destinations));
    assert_eq!(brief.source, BriefSource::Synthesized);
    assert!(brief.routes.is_empty(), "a degraded brief asks the destination");
    let ids: Vec<&str> = brief.questions.iter().map(|q| q.id.as_str()).collect();
    assert_eq!(ids, ["destination", "reason"]);
    assert_eq!(brief.problems.len(), 1);
    assert_eq!(
        brief.display.as_deref(),
        Some("runtime/verdict.md"),
        "the block's display still applies"
    );
    assert_eq!(brief.authored_required(), 0);

    let unread = Brief::build(parts(Some(&block), None, &destinations));
    assert_eq!(unread.source, BriefSource::Synthesized);
}

// §FS-rhei-gate-brief.4 §FS-rhei-gate-brief.4.3 §FS-rhei-gate-brief.4.5
#[test]
fn a_request_brief_takes_title_display_and_attachments_from_the_request() {
    let destinations = destinations_fixture();
    let block = GateDef {
        title: Some("Block title".into()),
        request: Some("runtime/gates/r.json".into()),
        ..GateDef::default()
    };
    let mut document = sample_request();
    document["display"] = json!("runtime/agora/verdict.md");
    let request = parse_request(&document.to_string(), false, &[]).unwrap();
    let brief = Brief::build(parts(Some(&block), Some(Ok(request.clone())), &destinations));
    assert_eq!(brief.source, BriefSource::Request);
    assert_eq!(brief.title, "Rule on 3 disputed entries");
    assert_eq!(brief.display.as_deref(), Some("runtime/agora/verdict.md"));
    assert_eq!(brief.attachments, ["runtime/agora/charter.md"]);
    let ids: Vec<&str> = brief.questions.iter().map(|q| q.id.as_str()).collect();
    assert_eq!(
        ids,
        ["d1", "approve", "note", "destination"],
        "no routes: rhei asks the destination"
    );
    assert_eq!(brief.questions[0].options.as_ref().unwrap()[1].label.as_deref(), Some("root"));
    assert_eq!(brief.authored_required(), 2);

    // The digest moves with what is asked, and only with it.
    let again = Brief::build(parts(Some(&block), Some(Ok(request.clone())), &destinations));
    assert_eq!(brief.digest, again.digest);
    let mut next_visit = parts(Some(&block), Some(Ok(request)), &destinations);
    next_visit.visit = 2;
    assert_ne!(Brief::build(next_visit).digest, brief.digest);
}

// §FS-rhei-gate-brief.6 §FS-rhei-gate-brief.7.1
#[test]
fn an_answer_document_round_trips_with_and_without_recording_fields() {
    let sent: GateAnswer = serde_json::from_value(json!({
        "version": 1, "request_digest": "sha256:00", "answers": { "approve": true }
    }))
    .unwrap();
    assert_eq!(sent.route, None);
    let recorded: GateAnswer = serde_json::from_value(json!({
        "version": 1, "request_digest": "sha256:00", "answers": { "destination": "completed" },
        "ticket": "agora.1", "state": "judge", "visit": 1, "to": "completed", "route": null,
        "answered_by": "someone", "answered_at": "2026-11-24T12:00:00Z"
    }))
    .unwrap();
    assert_eq!(recorded.route, Some(None));
    let written = serde_json::to_value(&recorded).unwrap();
    assert!(written["route"].is_null() && written.get("route").is_some());
    assert!(serde_json::from_value::<GateAnswer>(json!({
        "version": 1, "request_digest": "x", "answers": {}, "extra": 1
    }))
    .is_err());
    let error = GateError::new(GateErrorKind::NotAGate, None, "not a gate");
    assert_eq!(serde_json::to_value(&error).unwrap()["kind"], "not_a_gate");
}

fn machine(gate_state: &str) -> Result<crate::state_machine::StateMachine, String> {
    let yaml = format!(
        r#"
name: gated
version: 1.0
states:
  pending: {{ description: "not started" }}
  judge:
{gate_state}
  shipping: {{ description: "Ship the change" }}
  revising: {{ description: "Revise it" }}
  completed: {{ description: "done", final: true }}
transitions:
  - from: pending
    to: judge
  - from: judge
    to: shipping
  - from: judge
    to: revising
    condition: "test -f runtime/notes.md"
  - from: shipping
    to: completed
  - from: revising
    to: judge
  - from: "*"
    to: completed
"#
    );
    crate::state_machine::StateMachine::from_yaml_str(&yaml).map_err(|err| err.to_string())
}

// §FS-rhei-gate-brief.1.2 §FS-rhei-states.1.3
#[test]
fn a_machine_with_a_valid_gate_block_loads() {
    let loaded = machine(
        r#"    gating: true
    gate:
      title: "Ship {task_title}?"
      display: runtime/{task_id}/verdict.md
      questions:
        - { id: approve, kind: boolean, prompt: "Ship it?" }
      routes:
        - { to: shipping, when: { approve: true } }
        - { to: revising }"#,
    )
    .expect("valid gate loads");
    let gate = loaded.states["judge"].gate.as_ref().expect("block kept");
    assert_eq!(gate.routes.len(), 2);
    let targets = destinations(&loaded, "judge");
    let names: Vec<&str> = targets.iter().map(|d| d.state.as_str()).collect();
    assert_eq!(names, ["shipping", "revising", "completed"], "exact before wildcard, deduplicated");
    assert_eq!(targets[1].condition.as_deref(), Some("test -f runtime/notes.md"));
    assert_eq!(targets[0].description.as_deref(), Some("Ship the change"));
    assert!(targets[2].terminal);
}

// §FS-rhei-gate-brief.1.2
#[test]
fn a_machine_reports_every_gate_problem_at_once() {
    let error = machine(
        r#"    gating: true
    gate:
      request: /abs/request.json
      routes:
        - { to: pending }"#,
    )
    .unwrap_err();
    assert!(
        error.contains("state 'judge' gate: `request` '/abs/request.json' is absolute"),
        "{error}"
    );
    assert!(error.contains("`routes[0].to` 'pending' is not a state"), "{error}");
    assert!(error.contains("declared: shipping, revising, completed"), "{error}");
}

// §FS-rhei-gate-brief.1.2 §FS-rhei-states.1.3
#[test]
fn a_gate_block_on_a_state_that_is_not_gating_is_refused() {
    let error = machine("    gate: { title: x }").unwrap_err();
    assert!(error.contains("declares `gate:` but is not `gating: true`"), "{error}");
    let misspelt = machine("    gating: true\n    gate: { titel: x }").unwrap_err();
    assert!(misspelt.contains("titel"), "{misspelt}");
}

// §FS-rhei-gate-brief.7.3 §FS-rhei-states.4.2
#[test]
fn every_state_may_branch_on_the_implicit_gate_answer() {
    let branching =
        "    gating: true\n    instructions: |\n      {if input.gate-answer.exists}\n      \
                     Read {gate_answer}.\n      {endif}";
    assert!(machine(branching).is_ok(), "{:?}", machine(branching).err());
    let typo =
        machine("    gating: true\n    instructions: \"{if input.gate-answers.exists}x{endif}\"")
            .unwrap_err();
    assert!(typo.contains("'gate-answers' is not a declared input"), "{typo}");
}
