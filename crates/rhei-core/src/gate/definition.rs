// What makes a `gate:` block valid when its machine loads. The machine's own
// validation names the state and decides whether the state may carry a block
// at all; this part checks the block against the edges that leave it.

// §FS-rhei-gate-brief.1.2

use super::questions::{route_when_problems, typed_question_problems};
use super::template::{relative_path_problem, template_problems, GateField};
use super::types::GateDef;

/// Every problem with `gate`, given the targets of the edges its state
/// declares (exact and wildcard). §FS-rhei-gate-brief.1.2
pub fn gate_def_problems(gate: &GateDef, targets: &[&str]) -> Vec<String> {
    let mut problems = Vec::new();
    if gate.request.is_some() && gate.questions.is_some() {
        problems.push(
            "declares both `request` and `questions`; questions come from one or the other".into(),
        );
    }
    for (field, path) in [(GateField::Display, &gate.display), (GateField::Request, &gate.request)]
    {
        if let Some(path) = path {
            if let Some(problem) = relative_path_problem(path) {
                problems.push(format!("`{}` '{path}' {problem}", field.name()));
            }
        }
    }
    let templated = [
        (GateField::Title, &gate.title),
        (GateField::Summary, &gate.summary),
        (GateField::Display, &gate.display),
        (GateField::Request, &gate.request),
    ];
    for (field, text) in templated {
        if let Some(text) = text {
            problems.extend(template_problems(field, text));
        }
    }
    let questions_valid = match &gate.questions {
        Some(questions) => {
            let found = typed_question_problems(questions, "questions");
            let valid = found.is_empty();
            problems.extend(found);
            valid
        }
        None => true,
    };
    for (index, route) in gate.routes.iter().enumerate() {
        if !targets.contains(&route.to.as_str()) {
            let declared = if targets.is_empty() { "none".to_string() } else { targets.join(", ") };
            problems.push(format!(
                "`routes[{index}].to` '{}' is not a state this state has a declared edge to \
                 (declared: {declared})",
                route.to
            ));
        }
    }
    // Under `request` the questions do not exist yet: the keys are checked on
    // entry, against the request. §FS-rhei-gate-brief.2
    if gate.request.is_none() && questions_valid {
        let questions = gate.questions.as_deref().unwrap_or_default();
        problems.extend(route_when_problems(&gate.routes, questions));
    }
    problems
}
