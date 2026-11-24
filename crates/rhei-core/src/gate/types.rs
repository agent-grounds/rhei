// The documents a gate brief is made of: the `gate:` block a states file
// declares, the request the state before the gate writes, and the answer a
// caller sends. Shapes only; what makes one valid is in the sibling parts.

// §FS-rhei-gate-brief

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The only document version a reader of these files accepts. §FS-rhei-gate-brief
pub const GATE_DOCUMENT_VERSION: u64 = 1;

/// The `gate:` block of a `gating: true` state. §FS-rhei-gate-brief.1
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GateDef {
    /// One-line title template; the request's title wins over it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Prose template saying what was done and what is asked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Path template of the file the person reads; an implicit required input.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
    /// Path template of the request file; an implicit required input.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<String>,
    /// Static questions. `Some(vec![])` is kept distinct from absent so the
    /// validator can refuse an empty list. §FS-rhei-gate-brief.1.2
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub questions: Option<Vec<Question>>,
    /// Answers to destinations, first match wins. §FS-rhei-gate-brief.5
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub routes: Vec<Route>,
}

/// The four kinds a question may be. §FS-rhei-gate-brief.3.1
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum QuestionKind {
    Boolean,
    Choice,
    Multi,
    Text,
}

impl QuestionKind {
    /// Every kind's spelling, in the order a diagnostic lists them.
    pub const ALL: [&'static str; 4] = ["boolean", "choice", "multi", "text"];

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "boolean" => Some(Self::Boolean),
            "choice" => Some(Self::Choice),
            "multi" => Some(Self::Multi),
            "text" => Some(Self::Text),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Boolean => "boolean",
            Self::Choice => "choice",
            Self::Multi => "multi",
            Self::Text => "text",
        }
    }

    /// Whether the kind picks from `options`.
    pub fn takes_options(self) -> bool {
        matches!(self, Self::Choice | Self::Multi)
    }
}

fn default_required() -> bool {
    true
}

/// One question, static or from a request. §FS-rhei-gate-brief.3.1
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Question {
    pub id: String,
    pub kind: QuestionKind,
    pub prompt: String,
    #[serde(default = "default_required")]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<QuestionOption>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
}

impl Question {
    /// The question as a brief prints it: every option labelled, its value
    /// when the author gave no label. §FS-rhei-gate-brief.3.2
    pub fn labelled(&self) -> Self {
        let mut question = self.clone();
        if let Some(options) = question.options.as_mut() {
            for option in options {
                if option.label.as_deref().is_none_or(|label| label.trim().is_empty()) {
                    option.label = Some(option.value.clone());
                }
            }
        }
        question
    }

    /// The option values, in order; empty for a kind that takes none.
    pub fn option_values(&self) -> impl Iterator<Item = &str> {
        self.options.iter().flatten().map(|option| option.value.as_str())
    }
}

/// One option of a `choice` or `multi` question. §FS-rhei-gate-brief.3.2
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct QuestionOption {
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// One route from answers to a destination. §FS-rhei-gate-brief.5
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    pub to: String,
    /// Question id to expected value; empty is the fallback that always matches.
    #[serde(default)]
    pub when: IndexMap<String, Value>,
}

/// The request file the state before a gate writes. §FS-rhei-gate-brief.3
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GateRequest {
    pub version: u64,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<String>,
    pub questions: Vec<Question>,
}

/// The route an answer took, as recorded: its index among the brief's routes
/// and the `when` it matched. §FS-rhei-gate-brief.7.1
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RouteChoice {
    pub index: usize,
    pub when: IndexMap<String, Value>,
}

/// The answer a caller sends, and the file rhei records it as: the recording
/// fields are optional, so the recorded file is an answer document too.
/// §FS-rhei-gate-brief.6 §FS-rhei-gate-brief.7.1
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GateAnswer {
    pub version: u64,
    pub request_digest: String,
    pub answers: IndexMap<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visit: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    /// `Some(None)` is a recorded `null`: the destination came from the
    /// `destination` question rather than a route.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "nullable_route")]
    pub route: Option<Option<RouteChoice>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answered_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answered_at: Option<String>,
}

/// Keeps a present `null` apart from an absent field for [`GateAnswer::route`].
mod nullable_route {
    use super::RouteChoice;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(
        value: &Option<Option<RouteChoice>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match value {
            Some(inner) => inner.serialize(serializer),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Option<RouteChoice>>, D::Error> {
        Option::<RouteChoice>::deserialize(deserializer).map(Some)
    }
}

/// Why an answer was refused. §FS-rhei-gate-brief.6.2
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GateErrorKind {
    /// Shape, ids, kinds, a missing required answer, or an option value.
    Invalid,
    /// The digest differs from the current brief's.
    Stale,
    /// The ticket is not in `--from`.
    Conflict,
    /// No route matches.
    Unrouted,
    /// The ticket's state is not a gating state.
    NotAGate,
    /// The transition path refused the edge.
    Refused,
}

/// One refusal of an answer, as `rhei gate answer --json` lists it.
/// §FS-rhei-gate-cmd.3.2
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct GateError {
    pub kind: GateErrorKind,
    pub question: Option<String>,
    pub message: String,
}

impl GateError {
    pub fn new(kind: GateErrorKind, question: Option<&str>, message: impl Into<String>) -> Self {
        Self { kind, question: question.map(str::to_string), message: message.into() }
    }
}
