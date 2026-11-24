// An answer to a brief: whether the document is well formed, whether every
// answer fits the question it answers, which route it takes, and the text it
// is recorded as. Pure, like the rest of the module: the caller reads the
// document and the brief, and performs the transition.

// §FS-rhei-gate-brief.6 §FS-rhei-gate-brief.5 §FS-rhei-gate-brief.7.2

use std::collections::HashSet;

use indexmap::IndexMap;
use serde_json::Value;

use super::brief::Brief;
use super::questions::unknown_field_problems;
use super::types::{
    GateAnswer, GateError, GateErrorKind, Question, QuestionKind, RouteChoice,
    GATE_DOCUMENT_VERSION,
};

/// The implicit optional input every state has: the ticket's latest answer
/// file. §FS-rhei-gate-brief.7.3
pub const GATE_ANSWER_INPUT: &str = "gate-answer";

const ANSWER_FIELDS: [&str; 10] = [
    "version",
    "request_digest",
    "answers",
    "ticket",
    "state",
    "visit",
    "to",
    "route",
    "answered_by",
    "answered_at",
];

fn invalid(question: Option<&str>, message: impl Into<String>) -> GateError {
    GateError::new(GateErrorKind::Invalid, question, message)
}

/// Read an answer document, every problem with its shape reported together as
/// `invalid`. §FS-rhei-gate-brief.6
pub fn parse_answer(text: &str) -> Result<GateAnswer, Vec<GateError>> {
    let value: Value = serde_json::from_str(text)
        .map_err(|err| vec![invalid(None, format!("the answer is not JSON: {err}"))])?;
    let Some(object) = value.as_object() else {
        return Err(vec![invalid(None, "the answer must be a JSON object")]);
    };
    let mut problems = Vec::new();
    unknown_field_problems(object, &ANSWER_FIELDS, "answer", &mut problems);
    let mut errors: Vec<GateError> = problems.into_iter().map(|text| invalid(None, text)).collect();
    match object.get("version") {
        Some(version) if version.as_u64() == Some(GATE_DOCUMENT_VERSION) => {}
        Some(version) => errors.push(invalid(
            None,
            format!("`version` is {version}; this rhei reads version {GATE_DOCUMENT_VERSION}"),
        )),
        None => errors.push(invalid(None, "the answer has no `version`")),
    }
    match object.get("request_digest") {
        Some(Value::String(digest)) if !digest.trim().is_empty() => {}
        Some(_) => errors.push(invalid(None, "`request_digest` must be the brief's digest")),
        None => errors.push(invalid(
            None,
            "the answer has no `request_digest`; copy `digest` from `rhei gate show --json`",
        )),
    }
    match object.get("answers") {
        Some(Value::Object(_)) => {}
        Some(_) => errors.push(invalid(None, "`answers` must be an object of question ids")),
        None => errors.push(invalid(None, "the answer has no `answers`")),
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    serde_json::from_value(value).map_err(|err| vec![invalid(None, err.to_string())])
}

/// Every reason `answer` does not answer `brief`: a digest that differs is
/// `stale`; an id the brief does not ask, a missing required answer, or a value
/// that does not fit its question is `invalid`. §FS-rhei-gate-brief.6.1
pub fn answer_problems(brief: &Brief, answer: &GateAnswer) -> Vec<GateError> {
    let mut errors = Vec::new();
    if answer.request_digest != brief.digest {
        errors.push(GateError::new(
            GateErrorKind::Stale,
            None,
            format!(
                "the brief changed since it was read: the answer carries {}, the brief is now {}; \
                 read it again with `rhei gate show`",
                answer.request_digest, brief.digest
            ),
        ));
    }
    for id in answer.answers.keys() {
        if !brief.questions.iter().any(|question| question.id == *id) {
            errors.push(invalid(Some(id), format!("`{id}` is not a question this brief asks")));
        }
    }
    for question in &brief.questions {
        match given(&answer.answers, &question.id) {
            None if question.required => errors.push(invalid(
                Some(&question.id),
                format!("`{}` is required: {}", question.id, question.prompt),
            )),
            None => {}
            Some(value) => {
                if let Some(problem) = value_problem(question, value) {
                    errors.push(invalid(Some(&question.id), problem));
                }
            }
        }
    }
    errors
}

/// The answer to `id`, with `null` read as absent. §FS-rhei-gate-brief.6.1
fn given<'a>(answers: &'a IndexMap<String, Value>, id: &str) -> Option<&'a Value> {
    answers.get(id).filter(|value| !value.is_null())
}

fn value_problem(question: &Question, value: &Value) -> Option<String> {
    let id = &question.id;
    let options = || question.option_values().collect::<Vec<_>>().join(", ");
    match question.kind {
        QuestionKind::Boolean if !value.is_boolean() => {
            Some(format!("`{id}` is a boolean question; answer true or false"))
        }
        QuestionKind::Boolean => None,
        QuestionKind::Choice => match value.as_str() {
            Some(chosen) if question.option_values().any(|option| option == chosen) => None,
            Some(chosen) => Some(format!("`{id}`: '{chosen}' is not one of {}", options())),
            None => Some(format!("`{id}` is a choice question; answer one of {}", options())),
        },
        QuestionKind::Multi => {
            let Some(items) = value.as_array() else {
                return Some(format!(
                    "`{id}` is a multi question; answer an array of {}",
                    options()
                ));
            };
            let mut seen = HashSet::new();
            for item in items {
                let Some(chosen) = item.as_str() else {
                    return Some(format!(
                        "`{id}` holds {item}; every value is one of {}",
                        options()
                    ));
                };
                if !question.option_values().any(|option| option == chosen) {
                    return Some(format!("`{id}`: '{chosen}' is not one of {}", options()));
                }
                if !seen.insert(chosen) {
                    return Some(format!("`{id}` holds '{chosen}' twice"));
                }
            }
            (question.required && items.is_empty())
                .then(|| format!("`{id}` is required; pick at least one of {}", options()))
        }
        QuestionKind::Text => match value.as_str() {
            Some(text) if question.required && text.trim().is_empty() => {
                Some(format!("`{id}` is required and the answer is empty"))
            }
            Some(_) => None,
            None => Some(format!("`{id}` is a text question; answer a string")),
        },
    }
}

/// Where a valid answer leads: the first route whose `when` matches, or the
/// `destination` answer at a gate without routes. No match is `unrouted`.
/// §FS-rhei-gate-brief.5
pub fn pick_destination(
    brief: &Brief,
    answers: &IndexMap<String, Value>,
) -> Result<(String, Option<RouteChoice>), GateError> {
    if brief.routes.is_empty() {
        return given(answers, "destination")
            .and_then(Value::as_str)
            .map(|to| (to.to_string(), None))
            .ok_or_else(|| invalid(Some("destination"), "`destination` is required"));
    }
    for (index, route) in brief.routes.iter().enumerate() {
        let matches = route.when.iter().all(|(id, expected)| {
            let question = brief.questions.iter().find(|question| question.id == *id);
            question.zip(given(answers, id)).is_some_and(|(question, value)| {
                route_value_matches(question.kind, expected, value)
            })
        });
        if matches {
            let when = route.when.clone();
            return Ok((route.to.clone(), Some(RouteChoice { index, when })));
        }
    }
    Err(GateError::new(
        GateErrorKind::Unrouted,
        None,
        "no route matches the answers, and the gate declares no fallback route",
    ))
}

/// One `when` entry against one answer. §FS-rhei-gate-brief.5.1
fn route_value_matches(kind: QuestionKind, expected: &Value, value: &Value) -> bool {
    match kind {
        QuestionKind::Boolean | QuestionKind::Choice => expected == value,
        QuestionKind::Multi => {
            let held: HashSet<&str> =
                value.as_array().into_iter().flatten().filter_map(Value::as_str).collect();
            match expected {
                Value::String(one) => held.contains(one.as_str()),
                Value::Array(set) => {
                    let wanted: HashSet<&str> = set.iter().filter_map(Value::as_str).collect();
                    wanted.len() == set.len() && wanted == held
                }
                _ => false,
            }
        }
        QuestionKind::Text => false,
    }
}

/// The answer as the transition records it: one line per answered question,
/// `<prompt>: <label or value>`, then the answer file. §FS-rhei-gate-brief.7.2
pub fn render_answers(
    brief: &Brief,
    answers: &IndexMap<String, Value>,
    answer_path: &str,
) -> String {
    let mut lines = Vec::new();
    for question in &brief.questions {
        let Some(value) = given(answers, &question.id) else {
            continue;
        };
        let label = |chosen: &str| {
            question
                .options
                .iter()
                .flatten()
                .find(|option| option.value == chosen)
                .and_then(|option| option.label.clone())
                .unwrap_or_else(|| chosen.to_string())
        };
        let rendered = match (question.kind, value) {
            (QuestionKind::Boolean, Value::Bool(true)) => "yes".to_string(),
            (QuestionKind::Boolean, _) => "no".to_string(),
            (QuestionKind::Multi, Value::Array(items)) => {
                items.iter().filter_map(Value::as_str).map(label).collect::<Vec<_>>().join(", ")
            }
            (QuestionKind::Choice, Value::String(chosen)) => label(chosen),
            (_, Value::String(text)) => text.trim().to_string(),
            (_, other) => other.to_string(),
        };
        if rendered.is_empty() {
            continue;
        }
        lines.push(format!("{}: {rendered}", question.prompt.trim()));
    }
    lines.push(format!("Answer: {answer_path}"));
    lines.join("\n")
}
