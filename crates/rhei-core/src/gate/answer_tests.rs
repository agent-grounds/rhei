// An answer checked against a brief built in memory: its shape, its values,
// the route it takes, and the text it is recorded as.

// §FS-rhei-gate-brief.6 §FS-rhei-gate-brief.5 §FS-rhei-gate-brief.7.2

use super::*;
use indexmap::IndexMap;
use serde_json::{json, Value};

fn question(id: &str, kind: QuestionKind, required: bool, options: &[&str]) -> Question {
    Question {
        id: id.into(),
        kind,
        prompt: format!("Ask {id}?"),
        required,
        options: (!options.is_empty()).then(|| {
            options
                .iter()
                .map(|value| QuestionOption {
                    value: (*value).into(),
                    label: Some(value.to_uppercase()),
                    description: None,
                })
                .collect()
        }),
        help: None,
    }
}

fn route(to: &str, when: Value) -> Route {
    serde_json::from_value(json!({ "to": to, "when": when })).unwrap()
}

fn destinations() -> Vec<Destination> {
    ["completed", "drafting"]
        .iter()
        .map(|state| Destination {
            state: (*state).into(),
            description: None,
            condition: None,
            terminal: *state == "completed",
        })
        .collect()
}

/// A brief with four questions, one of each kind, and three routes.
fn routed_brief() -> Brief {
    let block = GateDef {
        questions: Some(vec![
            question("approve", QuestionKind::Boolean, true, &[]),
            question("scope", QuestionKind::Choice, true, &["small", "large"]),
            question("areas", QuestionKind::Multi, false, &["docs", "code", "tests"]),
            question("note", QuestionKind::Text, false, &[]),
        ]),
        routes: vec![
            route("completed", json!({ "approve": true, "areas": ["code", "tests"] })),
            route("completed", json!({ "approve": true, "areas": "docs" })),
            route("drafting", json!({ "approve": false })),
        ],
        ..GateDef::default()
    };
    brief_of(Some(&block))
}

fn brief_of(block: Option<&GateDef>) -> Brief {
    let destinations = destinations();
    Brief::build(BriefParts {
        ticket: "plan.1",
        ticket_title: "Ship",
        state: "review",
        visit: 1,
        block,
        block_title: None,
        block_summary: None,
        block_display: None,
        request: None,
        destinations: &destinations,
    })
}

fn answer(brief: &Brief, answers: Value) -> GateAnswer {
    parse_answer(
        &json!({ "version": 1, "request_digest": brief.digest, "answers": answers }).to_string(),
    )
    .unwrap()
}

fn map(answers: Value) -> IndexMap<String, Value> {
    serde_json::from_value(answers).unwrap()
}

// §FS-rhei-gate-brief.6
#[test]
fn every_problem_with_the_document_shape_is_reported_together() {
    let errors = parse_answer(r#"{ "version": 2, "answers": [], "extra": true }"#).unwrap_err();
    let messages: Vec<&str> = errors.iter().map(|error| error.message.as_str()).collect();
    assert_eq!(errors.len(), 4, "{messages:?}");
    assert!(errors.iter().all(|error| error.kind == GateErrorKind::Invalid));
    assert!(messages.iter().any(|message| message.contains("unknown field `extra`")));
    assert!(messages.iter().any(|message| message.contains("version 1")));
    assert!(messages.iter().any(|message| message.contains("request_digest")));
    assert!(messages.iter().any(|message| message.contains("`answers` must be an object")));
    assert_eq!(parse_answer("not json").unwrap_err()[0].kind, GateErrorKind::Invalid);
}

// §FS-rhei-gate-brief.6.1 §FS-rhei-gate-brief.6.2
#[test]
fn every_value_problem_and_a_stale_digest_are_reported_together() {
    let brief = routed_brief();
    let mut sent = answer(
        &brief,
        json!({ "scope": "medium", "areas": ["docs", "docs"], "note": 3, "who": "me" }),
    );
    sent.request_digest = "sha256:old".into();
    let errors = answer_problems(&brief, &sent);
    let by_question = |id: &str| {
        errors.iter().find(|error| error.question.as_deref() == Some(id)).map(|error| error.kind)
    };
    assert_eq!(errors.iter().filter(|error| error.kind == GateErrorKind::Stale).count(), 1);
    assert_eq!(by_question("who"), Some(GateErrorKind::Invalid));
    assert_eq!(by_question("approve"), Some(GateErrorKind::Invalid), "required, missing");
    assert_eq!(by_question("scope"), Some(GateErrorKind::Invalid), "not an option");
    assert_eq!(by_question("areas"), Some(GateErrorKind::Invalid), "a value twice");
    assert_eq!(by_question("note"), Some(GateErrorKind::Invalid), "not a string");
    assert_eq!(errors.len(), 6, "{errors:?}");
}

// §FS-rhei-gate-brief.6.1
#[test]
fn null_is_absent_and_optional_questions_may_be_left_out() {
    let brief = routed_brief();
    let sent = answer(&brief, json!({ "approve": false, "scope": "small", "areas": null }));
    assert!(answer_problems(&brief, &sent).is_empty());
    let blank = answer(&brief_of(None), json!({ "destination": "completed", "reason": "  " }));
    let errors = answer_problems(&brief_of(None), &blank);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].question.as_deref(), Some("reason"));
}

// §FS-rhei-gate-brief.5 §FS-rhei-gate-brief.5.1
#[test]
fn the_first_matching_route_wins_and_multi_matches_by_member_or_set() {
    let brief = routed_brief();
    let pick = |answers: Value| pick_destination(&brief, &map(answers));
    let (to, route) = pick(json!({ "approve": true, "areas": ["tests", "code"] })).unwrap();
    assert_eq!((to.as_str(), route.unwrap().index), ("completed", 0));
    let (_, route) = pick(json!({ "approve": true, "areas": ["code", "docs"] })).unwrap();
    assert_eq!(route.unwrap().index, 1, "a string matches a member");
    let (to, route) = pick(json!({ "approve": false })).unwrap();
    assert_eq!((to.as_str(), route.unwrap().index), ("drafting", 2));
    let unrouted = pick(json!({ "approve": true, "areas": ["code"] })).unwrap_err();
    assert_eq!(unrouted.kind, GateErrorKind::Unrouted);
    let unanswered = pick(json!({ "approve": true })).unwrap_err();
    assert_eq!(unanswered.kind, GateErrorKind::Unrouted, "an unanswered question matches nothing");
}

// §FS-rhei-gate-brief.5 §FS-rhei-gate-brief.4.1
#[test]
fn a_gate_without_routes_goes_where_the_destination_answer_says() {
    let brief = brief_of(None);
    let picked = pick_destination(&brief, &map(json!({ "destination": "drafting" }))).unwrap();
    assert_eq!(picked, ("drafting".to_string(), None));
}

// §FS-rhei-gate-brief.7.2
#[test]
fn the_result_names_each_answer_by_its_prompt_and_label_then_the_file() {
    let brief = routed_brief();
    let answers = map(json!({
        "approve": true, "scope": "large", "areas": ["code", "tests"], "note": " Keep it. "
    }));
    let rendered = render_answers(&brief, &answers, "runtime/gates/plan.1/review.1.answer.json");
    assert_eq!(
        rendered,
        "Ask approve?: yes\nAsk scope?: LARGE\nAsk areas?: CODE, TESTS\nAsk note?: Keep it.\n\
         Answer: runtime/gates/plan.1/review.1.answer.json"
    );
    let short = render_answers(&brief, &map(json!({ "approve": false, "note": "" })), "a.json");
    assert_eq!(short, "Ask approve?: no\nAnswer: a.json", "an empty answer is not a line");
}

// A multi-line text answer, or a prompt with a break in it, is still one line
// of the result; the recorded answer keeps it as written. §FS-rhei-gate-brief.7.2
#[test]
fn line_breaks_fold_so_each_answered_question_is_one_line() {
    let mut brief = routed_brief();
    brief.questions[3].prompt = "Anything\n  else?".into();
    let answers = map(json!({ "approve": true, "note": "line one\r\n\n  line two\u{2028}three " }));
    let rendered = render_answers(&brief, &answers, "a.json");
    assert_eq!(
        rendered,
        "Ask approve?: yes\nAnything else?: line one line two three\nAnswer: a.json"
    );
    assert_eq!(rendered.lines().count(), 3, "two answers and the file");
}
