// Reading a request file. Every problem is collected before any is reported,
// because the agent that wrote the file fixes it from this one list.

// §FS-rhei-gate-brief.3 §FS-rhei-gate-brief.2

use serde_json::Value;

use super::questions::{question_list_problems, route_when_problems, unknown_field_problems};
use super::template::relative_path_problem;
use super::types::{GateRequest, Route, GATE_DOCUMENT_VERSION};

const REQUEST_FIELDS: [&str; 6] =
    ["version", "title", "summary", "display", "attachments", "questions"];

/// Parse and check a request against [§FS-rhei-gate-brief.3], and its routes
/// against its questions. `block_display` says whether the machine's block
/// declares its own `display`, in which case the request may not. Whether the
/// files it names exist is the caller's to check.
pub fn parse_request(
    text: &str,
    block_display: bool,
    routes: &[Route],
) -> Result<GateRequest, Vec<String>> {
    let value: Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(err) => return Err(vec![format!("the request is not JSON: {err}")]),
    };
    let mut problems = Vec::new();
    let Some(object) = value.as_object() else {
        return Err(vec!["the request must be a JSON object".to_string()]);
    };
    unknown_field_problems(object, &REQUEST_FIELDS, "request", &mut problems);
    match object.get("version") {
        Some(version) if version.as_u64() == Some(GATE_DOCUMENT_VERSION) => {}
        Some(version) => {
            problems.push(format!("`version` is {version}; this rhei reads version 1"))
        }
        None => problems.push("the request has no `version`; write \"version\": 1".to_string()),
    }
    match object.get("title") {
        Some(Value::String(title)) if title.trim().is_empty() => {
            problems.push("`title` is empty".into())
        }
        Some(Value::String(title)) if title.contains('\n') => {
            problems.push("`title` must be one line".into())
        }
        Some(Value::String(_)) => {}
        Some(_) => problems.push("`title` must be a string".into()),
        None => problems.push("the request has no `title`".into()),
    }
    if object.get("summary").is_some_and(|summary| !summary.is_string()) {
        problems.push("`summary` must be a string".into());
    }
    match object.get("display") {
        None => {}
        Some(Value::String(_)) if block_display => problems.push(
            "`display` is set by the state machine's `gate:` block; remove it from the request"
                .into(),
        ),
        Some(Value::String(path)) => {
            if let Some(problem) = relative_path_problem(path) {
                problems.push(format!("`display` '{path}' {problem}"));
            }
        }
        Some(_) => problems.push("`display` must be a string".into()),
    }
    match object.get("attachments") {
        None => {}
        Some(Value::Array(paths)) => {
            for (index, path) in paths.iter().enumerate() {
                match path.as_str() {
                    Some(path) => {
                        if let Some(problem) = relative_path_problem(path) {
                            problems.push(format!("`attachments[{index}]` '{path}' {problem}"));
                        }
                    }
                    None => problems.push(format!("`attachments[{index}]` must be a string")),
                }
            }
        }
        Some(_) => problems.push("`attachments` must be an array of paths".into()),
    }
    match object.get("questions") {
        Some(Value::Array(questions)) => {
            problems.extend(question_list_problems(questions, "questions"))
        }
        Some(_) => problems.push("`questions` must be an array".into()),
        None => problems.push("the request has no `questions`".into()),
    }
    if !problems.is_empty() {
        // Routes are checked against questions that are themselves valid, so a
        // broken question is reported once, not again through every route.
        return Err(problems);
    }
    let request: GateRequest = serde_json::from_value(value)
        .map_err(|err| vec![format!("the request does not read as a request: {err}")])?;
    let route_problems = route_when_problems(routes, &request.questions);
    if !route_problems.is_empty() {
        return Err(route_problems);
    }
    Ok(request)
}
