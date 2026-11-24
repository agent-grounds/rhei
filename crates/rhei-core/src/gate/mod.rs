//! Gate briefs: what a gating state shows a person, what it asks, and which
//! answer leads where.
//!
//! The documents are in [`types`]; [`definition`] checks a states file's
//! `gate:` block, [`request`] reads a request file, [`brief`] builds the
//! brief one visit puts to a person, and [`answer`] checks an answer to it and
//! picks where it leads. Everything here is pure: resolving templates against
//! a ticket, reading files and moving the ticket are the caller's.
//!
//! §FS-rhei-gate-brief

pub mod answer;
pub mod brief;
pub mod definition;
pub mod questions;
pub mod request;
pub mod template;
pub mod types;

pub use answer::{
    answer_problems, parse_answer, pick_destination, render_answers, GATE_ANSWER_INPUT,
};
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
mod answer_tests;
#[cfg(test)]
mod tests;
