// The brief one visit of one ticket to a gate puts to a person: which
// questions it asks and from where, the questions rhei adds itself, the
// degraded brief of a broken request, and the digest an answer carries back.
// Pure: the caller resolves templates and reads files, and hands in results.

// §FS-rhei-gate-brief.4

use serde::Serialize;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use super::types::{GateDef, GateRequest, Question, QuestionKind, QuestionOption, Route};
use crate::state_machine::StateMachine;

/// Where a brief's questions came from. §FS-rhei-gate-brief.4
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BriefSource {
    Request,
    Questions,
    Synthesized,
}

impl BriefSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Request => "request",
            Self::Questions => "questions",
            Self::Synthesized => "synthesized",
        }
    }
}

/// One move out of a gate: the edge that governs it and where it leads.
/// §FS-rhei-gate-cmd.2.1
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Destination {
    pub state: String,
    pub description: Option<String>,
    pub condition: Option<String>,
    #[serde(rename = "final")]
    pub terminal: bool,
}

/// One entry per distinct target of `state`'s declared outgoing edges, exact
/// and wildcard alike, in the order the machine first declares each. An entry
/// is described by the edge that governs the pair: the first exact edge to
/// the target, else the first wildcard one (§FS-rhei-transition-cmd.3).
/// §FS-rhei-gate-brief.4.1 §FS-rhei-gate-cmd.2.1
pub fn destinations(machine: &StateMachine, state: &str) -> Vec<Destination> {
    let mut out: Vec<Destination> = Vec::new();
    let leaving = || {
        machine.transitions().iter().filter(|rule| machine.transition_matches_source(rule, state))
    };
    for declared in leaving() {
        let target = declared.to.0.as_str();
        if out.iter().any(|known| known.state == target) {
            continue;
        }
        let rule =
            leaving().find(|rule| rule.to.0 == target && rule.from.0 != "*").unwrap_or(declared);
        let target_def = machine.states.get(target);
        let condition = rule.condition.clone().or_else(|| {
            rule.exit_code.as_ref().map(|code| format!("exit code {}", exit_code_text(code)))
        });
        out.push(Destination {
            state: target.to_string(),
            description: target_def.and_then(|def| def.description.clone()),
            condition,
            terminal: target_def.is_some_and(|def| def.terminal),
        });
    }
    out
}

fn exit_code_text(code: &serde_yaml::Value) -> String {
    match code {
        serde_yaml::Value::Sequence(codes) => {
            codes.iter().map(exit_code_text).collect::<Vec<_>>().join(", ")
        }
        serde_yaml::Value::Number(number) => number.to_string(),
        serde_yaml::Value::String(text) => text.clone(),
        other => serde_yaml::to_string(other).unwrap_or_default().trim().to_string(),
    }
}

/// The `destination` question rhei asks at a gate without routes.
/// §FS-rhei-gate-brief.4.1
pub fn destination_question(ticket: &str, destinations: &[Destination]) -> Question {
    Question {
        id: "destination".to_string(),
        kind: QuestionKind::Choice,
        prompt: format!("Where should {ticket} go?"),
        required: true,
        options: Some(
            destinations
                .iter()
                .map(|destination| QuestionOption {
                    value: destination.state.clone(),
                    label: Some(destination.state.clone()),
                    description: destination.description.clone(),
                })
                .collect(),
        ),
        help: None,
    }
}

/// The `reason` question rhei asks at a gate with no authored questions.
/// §FS-rhei-gate-brief.4.1
pub fn reason_question() -> Question {
    Question {
        id: "reason".to_string(),
        kind: QuestionKind::Text,
        prompt: "Why?".to_string(),
        required: true,
        options: None,
        help: None,
    }
}

/// What the caller resolved and read for one brief. §FS-rhei-gate-brief.4
pub struct BriefParts<'a> {
    /// Project-qualified ticket id.
    pub ticket: &'a str,
    pub ticket_title: &'a str,
    /// Canonical gate state name.
    pub state: &'a str,
    pub visit: u64,
    pub block: Option<&'a GateDef>,
    /// The block's `title`, `summary` and `display`, rendered.
    pub block_title: Option<String>,
    pub block_summary: Option<String>,
    pub block_display: Option<String>,
    /// The block's request, read and checked: `None` when the block names no
    /// request, `Err` with every problem when it is missing or invalid.
    pub request: Option<Result<GateRequest, Vec<String>>>,
    pub destinations: &'a [Destination],
}

/// The effective brief of one visit. §FS-rhei-gate-brief.4
#[derive(Debug, Clone, PartialEq)]
pub struct Brief {
    pub title: String,
    pub summary: Option<String>,
    /// Relative path of the display file, from the block or the request.
    pub display: Option<String>,
    pub attachments: Vec<String>,
    pub source: BriefSource,
    /// Effective questions in order, synthesized ones included, labelled.
    pub questions: Vec<Question>,
    pub routes: Vec<Route>,
    /// Every reason the brief degraded. §FS-rhei-gate-brief.4.2
    pub problems: Vec<String>,
    pub digest: String,
}

impl Brief {
    /// Build the brief: the authored questions from the request or the block,
    /// the synthesized ones, and the degraded brief of a broken request.
    /// §FS-rhei-gate-brief.4.1 §FS-rhei-gate-brief.4.2 §FS-rhei-gate-brief.4.3
    pub fn build(parts: BriefParts<'_>) -> Self {
        let mut problems = Vec::new();
        let mut request = None;
        let (source, authored, routes) = match parts.block {
            None => (BriefSource::Synthesized, Vec::new(), Vec::new()),
            Some(block) => match (&parts.request, &block.questions) {
                (Some(Ok(read)), _) => {
                    request = Some(read);
                    (BriefSource::Request, read.questions.clone(), block.routes.clone())
                }
                (Some(Err(found)), _) => {
                    problems.extend(found.iter().cloned());
                    (BriefSource::Synthesized, Vec::new(), Vec::new())
                }
                // A block that names a request nobody read is degraded too, so
                // a caller's omission never passes for a gate that asks nothing.
                (None, _) if block.request.is_some() => {
                    problems.push("the request file was not read".to_string());
                    (BriefSource::Synthesized, Vec::new(), Vec::new())
                }
                (None, Some(questions)) if !questions.is_empty() => {
                    (BriefSource::Questions, questions.clone(), block.routes.clone())
                }
                (None, _) => (BriefSource::Synthesized, Vec::new(), block.routes.clone()),
            },
        };
        let mut questions: Vec<Question> = authored.iter().map(Question::labelled).collect();
        if routes.is_empty() {
            questions.push(destination_question(parts.ticket, parts.destinations));
        }
        if authored.is_empty() {
            questions.push(reason_question());
        }
        let title = [
            request.map(|read| read.title.clone()),
            parts.block_title.clone(),
            Some(parts.ticket_title.to_string()),
        ]
        .into_iter()
        .flatten()
        .map(|title| title.trim().to_string())
        .find(|title| !title.is_empty())
        .unwrap_or_else(|| parts.ticket.to_string());
        let summary = request
            .and_then(|read| read.summary.clone())
            .or(parts.block_summary.clone())
            .filter(|summary| !summary.trim().is_empty());
        let display =
            parts.block_display.clone().or_else(|| request.and_then(|read| read.display.clone()));
        let attachments = request.map(|read| read.attachments.clone()).unwrap_or_default();
        let digest = brief_digest(
            parts.ticket,
            parts.state,
            parts.visit,
            &title,
            display.as_deref(),
            &questions,
            &routes,
        );
        Self { title, summary, display, attachments, source, questions, routes, problems, digest }
    }

    /// How many questions the brief requires an answer to. §FS-rhei-gate-cmd.4
    pub fn required(&self) -> usize {
        self.questions.iter().filter(|question| question.required).count()
    }

    /// How many required questions an author asked, as opposed to the ones
    /// rhei synthesized; a gate with any is answered, not transitioned.
    /// §FS-rhei-gate-brief.8
    pub fn authored_required(&self) -> usize {
        match self.source {
            BriefSource::Synthesized => 0,
            BriefSource::Request | BriefSource::Questions => self
                .questions
                .iter()
                .filter(|question| question.required && !is_synthesized(question))
                .count(),
        }
    }
}

/// Whether a question is one rhei asks itself; ids are reserved, so the id
/// alone decides. §FS-rhei-gate-brief.4.1
pub fn is_synthesized(question: &Question) -> bool {
    super::questions::RESERVED_QUESTION_IDS.contains(&question.id.as_str())
}

/// `sha256:<hex>` over the canonical JSON of what the brief asks.
/// §FS-rhei-gate-brief.4.5
pub fn brief_digest(
    ticket: &str,
    state: &str,
    visit: u64,
    title: &str,
    display: Option<&str>,
    questions: &[Question],
    routes: &[Route],
) -> String {
    let document = json!({
        "ticket": ticket,
        "state": state,
        "visit": visit,
        "title": title,
        "display": display,
        "questions": questions,
        "routes": routes,
    });
    let canonical = canonical_json(&document);
    let bytes = serde_json::to_vec(&canonical).unwrap_or_default();
    let hash = Sha256::digest(&bytes);
    hash.iter().fold(String::from("sha256:"), |mut out, byte| {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
        out
    })
}

/// The same value with every object's keys sorted, whatever map order serde
/// was built with, so the digest does not depend on a feature flag.
fn canonical_json(value: &Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut keys: Vec<&String> = object.keys().collect();
            keys.sort();
            let mut sorted = Map::new();
            for key in keys {
                sorted.insert(key.clone(), canonical_json(&object[key]));
            }
            Value::Object(sorted)
        }
        Value::Array(items) => Value::Array(items.iter().map(canonical_json).collect()),
        other => other.clone(),
    }
}
