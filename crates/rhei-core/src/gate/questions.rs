// What makes a question, and a route's `when`, valid. One reading serves the
// static questions of a states file and the questions of a request file, so
// the two cannot drift; it walks JSON so that every problem in a request is
// found, not only the first one a deserializer stops at.

// §FS-rhei-gate-brief.3.1 §FS-rhei-gate-brief.5.1

use std::collections::HashSet;
use std::sync::OnceLock;

use regex::Regex;
use serde_json::{Map, Value};

use super::types::{Question, QuestionKind, Route};

/// The ids rhei asks itself, which no author may use. §FS-rhei-gate-brief.4.1
pub const RESERVED_QUESTION_IDS: [&str; 2] = ["destination", "reason"];

const QUESTION_FIELDS: [&str; 6] = ["id", "kind", "prompt", "required", "options", "help"];
const OPTION_FIELDS: [&str; 3] = ["value", "label", "description"];

fn question_id_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"^[a-z][a-z0-9_-]*$").expect("question id pattern is valid"))
}

/// Every problem with a list of questions, each prefixed with where it is
/// (`<at>[<index>]`). An empty list is a problem: a gate that names questions
/// asks at least one. §FS-rhei-gate-brief.3.1
pub fn question_list_problems(questions: &[Value], at: &str) -> Vec<String> {
    let mut problems = Vec::new();
    if questions.is_empty() {
        problems.push(format!("`{at}` is empty; ask at least one question"));
    }
    let mut seen = HashSet::new();
    for (index, question) in questions.iter().enumerate() {
        question_problems(question, &format!("{at}[{index}]"), &mut seen, &mut problems);
    }
    problems
}

/// The same checks over typed questions, as a states file declares them.
pub fn typed_question_problems(questions: &[Question], at: &str) -> Vec<String> {
    let values: Vec<Value> = questions
        .iter()
        .map(|question| serde_json::to_value(question).unwrap_or(Value::Null))
        .collect();
    question_list_problems(&values, at)
}

fn question_problems(
    question: &Value,
    at: &str,
    seen: &mut HashSet<String>,
    problems: &mut Vec<String>,
) {
    let Some(object) = question.as_object() else {
        problems.push(format!("`{at}` must be an object"));
        return;
    };
    unknown_field_problems(object, &QUESTION_FIELDS, at, problems);
    match object.get("id") {
        Some(Value::String(id)) => {
            if !question_id_pattern().is_match(id) {
                problems.push(format!("`{at}.id` '{id}' must match ^[a-z][a-z0-9_-]*$"));
            } else if RESERVED_QUESTION_IDS.contains(&id.as_str()) {
                problems.push(format!(
                    "`{at}.id` '{id}' is reserved: rhei asks `destination` and `reason` itself"
                ));
            } else if !seen.insert(id.clone()) {
                problems.push(format!("`{at}.id` '{id}' is used by an earlier question"));
            }
        }
        Some(_) => problems.push(format!("`{at}.id` must be a string")),
        None => problems.push(format!("`{at}` has no `id`")),
    }
    let kind = match object.get("kind") {
        Some(Value::String(kind)) => {
            let parsed = QuestionKind::parse(kind);
            if parsed.is_none() {
                problems.push(format!(
                    "`{at}.kind` '{kind}' is not one of {}",
                    QuestionKind::ALL.join(", ")
                ));
            }
            parsed
        }
        Some(_) => {
            problems.push(format!("`{at}.kind` must be a string"));
            None
        }
        None => {
            problems.push(format!("`{at}` has no `kind`"));
            None
        }
    };
    match object.get("prompt") {
        Some(Value::String(prompt)) if !prompt.trim().is_empty() => {}
        Some(Value::String(_)) => problems.push(format!("`{at}.prompt` is empty")),
        Some(_) => problems.push(format!("`{at}.prompt` must be a string")),
        None => problems.push(format!("`{at}` has no `prompt`")),
    }
    if object.get("required").is_some_and(|value| !value.is_boolean()) {
        problems.push(format!("`{at}.required` must be true or false"));
    }
    if object.get("help").is_some_and(|value| !value.is_string()) {
        problems.push(format!("`{at}.help` must be a string"));
    }
    option_problems(object.get("options"), kind, at, problems);
}

fn option_problems(
    options: Option<&Value>,
    kind: Option<QuestionKind>,
    at: &str,
    problems: &mut Vec<String>,
) {
    let Some(kind) = kind else {
        return;
    };
    let Some(options) = options else {
        if kind.takes_options() {
            problems.push(format!("`{at}` is a {} question and needs `options`", kind.as_str()));
        }
        return;
    };
    if !kind.takes_options() {
        problems.push(format!("`{at}.options` is not allowed on a {} question", kind.as_str()));
        return;
    }
    let Some(options) = options.as_array() else {
        problems.push(format!("`{at}.options` must be an array"));
        return;
    };
    let least = if kind == QuestionKind::Choice { 2 } else { 1 };
    if options.len() < least {
        problems.push(format!(
            "`{at}.options` has {} option(s); a {} question needs at least {least}",
            options.len(),
            kind.as_str()
        ));
    }
    let mut values = HashSet::new();
    for (index, option) in options.iter().enumerate() {
        let at = format!("{at}.options[{index}]");
        let Some(object) = option.as_object() else {
            problems.push(format!("`{at}` must be an object"));
            continue;
        };
        unknown_field_problems(object, &OPTION_FIELDS, &at, problems);
        match object.get("value") {
            Some(Value::String(value)) if value.trim().is_empty() => {
                problems.push(format!("`{at}.value` is empty"))
            }
            Some(Value::String(value)) => {
                if !values.insert(value.clone()) {
                    problems.push(format!("`{at}.value` '{value}' is used by an earlier option"));
                }
            }
            Some(_) => problems.push(format!("`{at}.value` must be a string")),
            None => problems.push(format!("`{at}` has no `value`")),
        }
        for field in ["label", "description"] {
            if object.get(field).is_some_and(|value| !value.is_string()) {
                problems.push(format!("`{at}.{field}` must be a string"));
            }
        }
    }
}

/// One problem per field of `object` that `known` does not name.
pub(super) fn unknown_field_problems(
    object: &Map<String, Value>,
    known: &[&str],
    at: &str,
    problems: &mut Vec<String>,
) {
    for key in object.keys() {
        if !known.contains(&key.as_str()) {
            problems.push(format!(
                "`{at}` has unknown field `{key}`; known fields are {}",
                known.join(", ")
            ));
        }
    }
}

/// Every problem with the routes' `when` against `questions`: a key that names
/// no question or a `text` one, or an expected value of the wrong type or
/// outside the options. §FS-rhei-gate-brief.5.1
pub fn route_when_problems(routes: &[Route], questions: &[Question]) -> Vec<String> {
    let mut problems = Vec::new();
    for (index, route) in routes.iter().enumerate() {
        for (key, expected) in &route.when {
            let at = format!("routes[{index}].when.{key}");
            let Some(question) = questions.iter().find(|question| question.id == *key) else {
                problems.push(format!("`{at}` names no question the gate asks"));
                continue;
            };
            if let Some(problem) = expected_value_problem(question, expected) {
                problems.push(format!("`{at}` {problem}"));
            }
        }
    }
    problems
}

fn expected_value_problem(question: &Question, expected: &Value) -> Option<String> {
    let in_options = |value: &str| question.option_values().any(|option| option == value);
    match question.kind {
        QuestionKind::Text => Some("names a text question, which a route cannot match on".into()),
        QuestionKind::Boolean => {
            (!expected.is_boolean()).then(|| "must be true or false for a boolean question".into())
        }
        QuestionKind::Choice => match expected {
            Value::String(value) if in_options(value) => None,
            Value::String(value) => Some(format!("expects '{value}', which is not an option")),
            _ => Some("must be one option value for a choice question".into()),
        },
        QuestionKind::Multi => {
            let values: Vec<&Value> = match expected {
                Value::Array(values) => values.iter().collect(),
                single => vec![single],
            };
            let mut seen = HashSet::new();
            for value in values {
                let Some(value) = value.as_str() else {
                    return Some("must be an option value or an array of them".into());
                };
                if !in_options(value) {
                    return Some(format!("expects '{value}', which is not an option"));
                }
                if !seen.insert(value) {
                    return Some(format!("expects '{value}' twice"));
                }
            }
            None
        }
    }
}
