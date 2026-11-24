//! The gate form: answering a briefed gate from the run TUI. One control per
//! question, nothing preselected; `Enter` asks the gate sink for a dry run that
//! names the destination, and `Enter` again answers. Both go through the sink
//! to the core `rhei gate answer` runs, so the form validates nothing itself.
//! §FS-rhei-run-tui.1.5.5 §FS-rhei-viz.5.1.2

use crossterm::event::KeyCode;

use crate::rhei_tui::event::MessageLevel;
use crate::rhei_viz_model::{GateAnswerError, GateAnswerReply, GateBrief, GateQuestion};

use super::input::InputAction;
use super::state::UiState;

/// One question's answer as the form holds it.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Given {
    Yes(bool),
    One(String),
    /// The picked options, in the order the question offers them.
    Many(Vec<String>),
    Text(String),
}

impl Given {
    fn to_json(&self) -> serde_json::Value {
        match self {
            Self::Yes(yes) => serde_json::Value::Bool(*yes),
            Self::One(value) | Self::Text(value) => serde_json::Value::String(value.clone()),
            Self::Many(values) => values.iter().cloned().map(serde_json::Value::String).collect(),
        }
    }
}

/// Where the form stands.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Stage {
    Editing,
    /// The dry run accepted the answers and named where they lead.
    Confirm {
        to: String,
    },
}

/// A briefed gate being answered. It keeps the brief it drew, so the digest it
/// posts is the one the operator read. §FS-rhei-viz.5.1.2
pub(super) struct GateForm {
    pub(super) task: String,
    pub(super) brief: GateBrief,
    pub(super) focus: usize,
    /// Each question's answer; `None` is unanswered, and is never sent.
    pub(super) given: Vec<Option<Given>>,
    /// The option cursor of each `choice` or `multi` question.
    pub(super) cursor: Vec<usize>,
    pub(super) stage: Stage,
    /// The last refusal, shown until the next submission.
    pub(super) errors: Vec<GateAnswerError>,
}

impl GateForm {
    pub(super) fn new(task: String, brief: GateBrief) -> Self {
        let count = brief.questions.len();
        Self {
            task,
            brief,
            focus: 0,
            given: vec![None; count],
            cursor: vec![0; count],
            stage: Stage::Editing,
            errors: Vec::new(),
        }
    }

    /// The answer document the form posts. §FS-rhei-gate-brief.7.1
    pub(super) fn document(&self) -> String {
        let answers: serde_json::Map<String, serde_json::Value> = self
            .brief
            .questions
            .iter()
            .zip(&self.given)
            .filter_map(|(question, given)| Some((question.id.clone(), given.as_ref()?.to_json())))
            .collect();
        serde_json::json!({
            "version": self.brief.version,
            "request_digest": self.brief.digest,
            "answers": answers,
        })
        .to_string()
    }

    fn question(&self) -> Option<&GateQuestion> {
        self.brief.questions.get(self.focus)
    }

    fn move_focus(&mut self, forward: bool) {
        let last = self.brief.questions.len().saturating_sub(1);
        self.focus =
            if forward { (self.focus + 1).min(last) } else { self.focus.saturating_sub(1) };
    }

    /// One key on the focused question, read by its kind. §FS-rhei-run-tui.1.5.5
    fn edit(&mut self, code: KeyCode) {
        let Some(question) = self.question() else { return };
        let kind = question.kind.clone();
        let options: Vec<String> =
            question.options().iter().map(|option| option.value.clone()).collect();
        let (given, cursor) = (&mut self.given[self.focus], &mut self.cursor[self.focus]);
        match (kind.as_str(), code) {
            ("boolean", KeyCode::Char('y' | 'Y')) => *given = Some(Given::Yes(true)),
            ("boolean", KeyCode::Char('n' | 'N')) => *given = Some(Given::Yes(false)),
            ("boolean", KeyCode::Char(' ')) => {
                *given = Some(Given::Yes(!matches!(given, Some(Given::Yes(true)))));
            }
            ("choice", KeyCode::Left | KeyCode::Right | KeyCode::Char(' '))
                if !options.is_empty() =>
            {
                // The first press picks the highlighted option; later ones move.
                if given.is_some() {
                    *cursor = step(*cursor, options.len(), code);
                }
                *given = Some(Given::One(options[*cursor].clone()));
            }
            ("choice", KeyCode::Char(digit)) => {
                if let Some(index) = option_index(digit, options.len()) {
                    *cursor = index;
                    *given = Some(Given::One(options[index].clone()));
                }
            }
            ("multi", KeyCode::Left | KeyCode::Right) if !options.is_empty() => {
                *cursor = step(*cursor, options.len(), code);
            }
            ("multi", KeyCode::Char(key)) => {
                let index =
                    if key == ' ' { Some(*cursor) } else { option_index(key, options.len()) };
                if let Some(index) = index.filter(|index| *index < options.len()) {
                    *cursor = index;
                    *given = toggled(given.take(), &options, index);
                }
            }
            ("boolean" | "choice" | "multi", _) => {}
            (_, KeyCode::Char(ch)) => {
                let mut text = match given.take() {
                    Some(Given::Text(text)) => text,
                    _ => String::new(),
                };
                text.push(ch);
                *given = Some(Given::Text(text));
            }
            (_, KeyCode::Backspace) => {
                if let Some(Given::Text(mut text)) = given.take() {
                    text.pop();
                    *given = (!text.is_empty()).then_some(Given::Text(text));
                }
            }
            _ => {}
        }
    }
}

/// The cursor one option left or right, held at the ends.
fn step(cursor: usize, len: usize, code: KeyCode) -> usize {
    match code {
        KeyCode::Left => cursor.saturating_sub(1),
        KeyCode::Right => (cursor + 1).min(len.saturating_sub(1)),
        _ => cursor,
    }
}

/// The option a digit names, counting from 1.
fn option_index(key: char, len: usize) -> Option<usize> {
    let digit = key.to_digit(10)? as usize;
    (1..=len).contains(&digit).then(|| digit - 1)
}

/// A `multi` answer with option `index` flipped, kept in the question's order;
/// none picked is unanswered.
fn toggled(given: Option<Given>, options: &[String], index: usize) -> Option<Given> {
    let mut picked = match given {
        Some(Given::Many(picked)) => picked,
        _ => Vec::new(),
    };
    let value = &options[index];
    if let Some(at) = picked.iter().position(|seen| seen == value) {
        picked.remove(at);
    } else {
        picked.push(value.clone());
    }
    picked.sort_by_key(|seen| options.iter().position(|option| option == seen));
    (!picked.is_empty()).then_some(Given::Many(picked))
}

/// Keys while the form is open. §FS-rhei-run-tui.1.5.5
pub(super) fn handle_key(state: &mut UiState, code: KeyCode) -> InputAction {
    let Some(form) = state.gate_form.as_mut() else {
        return InputAction::Continue;
    };
    if matches!(form.stage, Stage::Confirm { .. }) {
        match code {
            KeyCode::Enter => submit(state, false),
            KeyCode::Esc => form.stage = Stage::Editing,
            _ => {}
        }
        return InputAction::Continue;
    }
    match code {
        KeyCode::Esc => state.gate_form = None,
        KeyCode::Enter => submit(state, true),
        KeyCode::Up | KeyCode::BackTab => form.move_focus(false),
        KeyCode::Down | KeyCode::Tab => form.move_focus(true),
        KeyCode::Delete => form.given[form.focus] = None,
        other => form.edit(other),
    }
    InputAction::Continue
}

/// Post the answers through the gate sink: a dry run first, which names the
/// destination, then the answer itself. A refusal stays in the form with the
/// answers kept and is echoed in the journal. §FS-rhei-viz.5.1.2
fn submit(state: &mut UiState, dry_run: bool) {
    let Some(form) = state.gate_form.as_ref() else { return };
    let (task, from, document) = (form.task.clone(), form.brief.state.clone(), form.document());
    let reply = match &state.gate {
        Some(sink) => sink.answer_gate(&task, &from, &document, dry_run),
        None => Err("gate answers are not available".to_string()),
    }
    .and_then(|value| {
        serde_json::from_value::<GateAnswerReply>(value)
            .map_err(|err| format!("unreadable reply: {err}"))
    });
    let reply = match reply {
        Ok(reply) => reply,
        Err(reason) => {
            let error = GateAnswerError { kind: "refused".into(), question: None, message: reason };
            return refuse(state, &task, vec![error]);
        }
    };
    if !reply.ok {
        return refuse(state, &task, reply.errors);
    }
    let to = reply.to.unwrap_or_default();
    if dry_run {
        if let Some(form) = state.gate_form.as_mut() {
            form.errors.clear();
            form.stage = Stage::Confirm { to };
        }
        return;
    }
    state.gate_form = None;
    state.push_journal(MessageLevel::Info, format!("⮞ gate {task}: {from}→{to} · answered"));
}

fn refuse(state: &mut UiState, task: &str, errors: Vec<GateAnswerError>) {
    let reasons: Vec<String> = errors
        .iter()
        .map(|error| match &error.question {
            Some(question) => format!("{question}: {}", error.message),
            None => error.message.clone(),
        })
        .collect();
    state.push_journal(
        MessageLevel::Warn,
        format!("gate {task} answer refused: {}", reasons.join("; ")),
    );
    if let Some(form) = state.gate_form.as_mut() {
        form.errors = errors;
        form.stage = Stage::Editing;
    }
}

/// Keep an open form on the brief the plan now shows: a ticket that left its
/// gate closes it, and a brief whose digest changed redraws it, keeping each
/// answer whose question is still asked the same way. §FS-rhei-viz.5.1.1
pub(super) fn sync(state: &mut UiState) {
    let Some(form) = state.gate_form.as_mut() else { return };
    let Some(brief) = state.plan.gates.get(&form.task).filter(|brief| brief.authored()) else {
        state.gate_form = None;
        return;
    };
    if brief.digest == form.brief.digest {
        return;
    }
    let mut next = GateForm::new(form.task.clone(), brief.clone());
    for (index, question) in next.brief.questions.iter().enumerate() {
        let kept = form
            .brief
            .questions
            .iter()
            .position(|old| old.id == question.id && old.kind == question.kind);
        if let Some(old) = kept {
            next.given[index] = form.given[old].clone();
            next.cursor[index] = form.cursor[old].min(question.options().len().saturating_sub(1));
        }
    }
    next.focus = form.focus.min(next.brief.questions.len().saturating_sub(1));
    *form = next;
}

/// Open the form for the selected task when its gate's brief asks authored
/// questions and the sink can answer them. §FS-rhei-run-tui.1.5.5
pub(super) fn open(state: &mut UiState) -> bool {
    let answers = state.gate.as_ref().is_some_and(|sink| sink.answers_gates());
    let gating = state
        .selected_task()
        .and_then(|task| state.machine_state(&task.state))
        .is_some_and(|st| st.gating);
    let Some(task) = state.selected.clone().filter(|_| gating) else { return false };
    let Some(brief) = state.plan.gates.get(&task).filter(|brief| brief.authored() && answers)
    else {
        return false;
    };
    state.gate_form = Some(GateForm::new(task, brief.clone()));
    true
}
