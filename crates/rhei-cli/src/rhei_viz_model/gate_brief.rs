//! The brief of a ticket waiting at a gate, as the surfaces carry it: the
//! document `rhei gate show --json` prints, field for field, mirrored here so
//! the model stays free of the core types. The host builds it with the code
//! `rhei gate show` runs; the surfaces only render it. §FS-rhei-viz.8 §FS-rhei-viz.5.1.1

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// The `source` of a brief whose questions rhei synthesized: no `gate:` block,
/// a block that asks nothing, or a degraded brief. §FS-rhei-gate-brief.4.1
pub const SYNTHESIZED_SOURCE: &str = "synthesized";

/// One ticket's brief at its gate. §FS-rhei-gate-cmd.2.1
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GateBrief {
    pub version: u64,
    pub ticket: String,
    pub ticket_title: String,
    pub title: String,
    pub state: String,
    pub visit: u64,
    pub since: Option<String>,
    pub plan: String,
    pub root: String,
    pub source: String,
    pub summary: Option<String>,
    pub display: Option<GateFile>,
    #[serde(default)]
    pub attachments: Vec<GateFile>,
    #[serde(default)]
    pub questions: Vec<GateQuestion>,
    #[serde(default)]
    pub destinations: Vec<GateDestination>,
    #[serde(default)]
    pub routes: Vec<GateRoute>,
    pub digest: String,
    pub answer_path: String,
    #[serde(default)]
    pub problems: Vec<String>,
}

impl GateBrief {
    /// Whether an author asked these questions, so the gate is answered rather
    /// than released. §FS-rhei-viz.5.1.2 §FS-rhei-viz.5.1.3
    pub fn authored(&self) -> bool {
        self.source != SYNTHESIZED_SOURCE
    }
}

/// A file the brief shows, and whether a person can open it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GateFile {
    pub path: String,
    pub absolute: String,
    pub readable: bool,
    pub problem: Option<String>,
}

/// One question the gate asks. `kind` is `boolean`, `choice`, `multi` or `text`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GateQuestion {
    pub id: String,
    pub kind: String,
    pub prompt: String,
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<GateOption>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
}

impl GateQuestion {
    /// The options a `choice` or `multi` question offers; none for the others.
    pub fn options(&self) -> &[GateOption] {
        self.options.as_deref().unwrap_or_default()
    }
}

/// One option of a `choice` or `multi` question.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GateOption {
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl GateOption {
    /// What a person reads for this option: its label, else its value.
    pub fn shown(&self) -> &str {
        self.label.as_deref().unwrap_or(&self.value)
    }
}

/// One state the gate can lead to, with the edge that governs it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GateDestination {
    pub state: String,
    pub description: Option<String>,
    pub condition: Option<String>,
    #[serde(rename = "final")]
    pub terminal: bool,
}

/// One route, in the order routes are tried: the destination and the answers
/// it matches; an empty `when` always matches.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GateRoute {
    pub to: String,
    #[serde(default)]
    pub when: IndexMap<String, serde_json::Value>,
}

/// The part of a `rhei gate answer --json` document a surface acts on: whether
/// it was accepted, where it leads, and each refusal. §FS-rhei-gate-cmd.3.2
#[derive(Clone, Debug, Default, Deserialize)]
pub struct GateAnswerReply {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub ticket: String,
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub to: Option<String>,
    #[serde(default)]
    pub errors: Vec<GateAnswerError>,
}

/// One refusal: its kind, the question it names if any, and why.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct GateAnswerError {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub question: Option<String>,
    #[serde(default)]
    pub message: String,
}
