//! Gate briefs: what a gating state shows a person, what it asks, and which
//! answer leads where.
//!
//! The documents are in [`types`]; [`definition`] checks a states file's
//! `gate:` block, [`request`] reads a request file, and [`brief`] builds the
//! brief one visit puts to a person. Everything here is pure: resolving
//! templates against a ticket and reading files is the caller's.
//!
//! §FS-rhei-gate-brief

pub mod brief;
pub mod definition;
pub mod questions;
pub mod request;
pub mod template;
pub mod types;

pub use brief::{
    brief_digest, destination_question, destinations, is_synthesized, reason_question, Brief,
    BriefParts, BriefSource, Destination,
};
pub use definition::gate_def_problems;
pub use questions::{question_list_problems, route_when_problems, RESERVED_QUESTION_IDS};
pub use request::parse_request;
pub use template::{relative_path_problem, render_template, template_problems, GateField};
pub use types::{
    GateAnswer, GateDef, GateError, GateErrorKind, GateRequest, Question, QuestionKind,
    QuestionOption, Route, RouteChoice, GATE_DOCUMENT_VERSION,
};

#[cfg(test)]
mod tests;
