//! The run TUI's gate block: a briefed gate opens a form that answers through
//! the gate sink after a dry run, a plain gate keeps its release, and both
//! render the brief. §FS-rhei-run-tui.1.5.5 §FS-rhei-viz.5.1

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

use super::gate_form::{Given, Stage};
use super::input::handle_key_event;
use super::state::UiState;
use crate::rhei_tui::dashboard::GateTransitionSink;
use crate::rhei_tui::event::MessageLevel;
use crate::rhei_viz_model::{
    GateBrief, GateDestination, GateFile, GateOption, GateQuestion, Machine, MachineState, TaskRow,
    Transition, VizModel,
};

/// `(task, from, answer document, dry_run)` for one answer the sink received.
type Answered = (String, String, serde_json::Value, bool);

/// A gate sink that answers from a queue of replies, as the host's core would.
struct AnsweringGate {
    answered: Mutex<Vec<Answered>>,
    replies: Mutex<VecDeque<Result<serde_json::Value, String>>>,
}

impl GateTransitionSink for AnsweringGate {
    fn transition_gate(
        &self,
        _: &str,
        _: &str,
        to: &str,
        _: Option<&str>,
    ) -> Result<String, String> {
        Ok(to.to_string())
    }

    fn answers_gates(&self) -> bool {
        true
    }

    fn answer_gate(
        &self,
        task_id: &str,
        from: &str,
        answers: &str,
        dry_run: bool,
    ) -> Result<serde_json::Value, String> {
        let document = serde_json::from_str(answers).expect("the form posts a JSON document");
        self.answered.lock().unwrap().push((task_id.into(), from.into(), document, dry_run));
        self.replies.lock().unwrap().pop_front().unwrap_or_else(|| Err("no reply queued".into()))
    }
}

fn gate(replies: Vec<Result<serde_json::Value, String>>) -> Arc<AnsweringGate> {
    Arc::new(AnsweringGate {
        answered: Mutex::new(Vec::new()),
        replies: Mutex::new(replies.into()),
    })
}

fn moved(to: &str, dry_run: bool) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({"version": 1, "ok": true, "dry_run": dry_run, "ticket": "1",
        "from": "vetting", "to": to, "errors": []}))
}

fn machine_state(name: &str, gating: bool, terminal: bool, to: &[&str]) -> MachineState {
    MachineState {
        name: name.into(),
        description: None,
        instructions: None,
        visits: None,
        initial: false,
        terminal,
        gating,
        waiting_on: None,
        process: None,
        transitions: to
            .iter()
            .map(|to| Transition { to: (*to).into(), condition: None, wildcard: false })
            .collect(),
        inputs: vec![],
        outputs: vec![],
        template_context: Default::default(),
        template_contexts: vec![],
    }
}

fn question(id: &str, kind: &str, options: &[&str]) -> GateQuestion {
    GateQuestion {
        id: id.into(),
        kind: kind.into(),
        prompt: format!("What about {id}?"),
        required: id == "approve",
        options: (!options.is_empty()).then(|| {
            options
                .iter()
                .map(|value| GateOption { value: (*value).into(), ..Default::default() })
                .collect()
        }),
        help: None,
    }
}

/// The brief of ticket `1` at `vetting`, its display file under `root`.
fn brief(source: &str, root: &Path) -> GateBrief {
    GateBrief {
        version: 1,
        ticket: "1".into(),
        ticket_title: "Vet the proposal".into(),
        title: "Vet the proposal before it ships".into(),
        state: "vetting".into(),
        visit: 2,
        root: root.display().to_string(),
        source: source.into(),
        summary: Some("Read the draft, then decide.".into()),
        display: Some(GateFile {
            path: "draft.md".into(),
            absolute: root.join("draft.md").display().to_string(),
            readable: true,
            problem: None,
        }),
        questions: vec![
            question("approve", "boolean", &[]),
            question("scope", "choice", &["small", "large"]),
            question("tags", "multi", &["a", "b"]),
            question("note", "text", &[]),
        ],
        destinations: vec![
            GateDestination { state: "approved".into(), terminal: true, ..Default::default() },
            GateDestination { state: "rework".into(), ..Default::default() },
        ],
        digest: "d1".into(),
        ..GateBrief::default()
    }
}

fn state_at_gate(sink: Arc<AnsweringGate>, source: &str, root: &Path) -> UiState {
    std::fs::write(root.join("draft.md"), "First line of the draft\nsecond line\n")
        .expect("write draft");
    let mut state =
        UiState::with_context(PathBuf::from("/ws"), 1, 1, None, None, Some(sink), false);
    state.plan = VizModel {
        tasks: vec![TaskRow {
            id: "1".into(),
            title: "Vet the proposal".into(),
            parent: None,
            depth: 0,
            state: "vetting".into(),
            visit_count: None,
            prior: vec![],
            history: vec![],
        }],
        machine: Machine {
            name: "inbox".into(),
            states: vec![
                machine_state("vetting", true, false, &["approved", "rework"]),
                machine_state("approved", false, true, &[]),
                machine_state("rework", false, false, &[]),
            ],
        },
        gates: [("1".to_string(), brief(source, root))].into(),
        ..VizModel::default()
    };
    state.refresh_plan();
    assert!(state.select_task("1"));
    state
}

fn press(state: &mut UiState, code: KeyCode) {
    handle_key_event(state, code, KeyModifiers::NONE);
}

fn screen(state: &UiState) -> String {
    let mut terminal = Terminal::new(TestBackend::new(120, 48)).unwrap();
    terminal.draw(|f| super::render::draw(f, state)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| (0..buffer.area.width).map(|x| buffer.get(x, y).symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

fn last_journal(state: &UiState) -> (MessageLevel, String) {
    let entry = state.journal.back().expect("a journal line");
    (entry.level, entry.text.clone())
}

/// Every control takes its own keys, the dry run names the destination, and
/// only the confirmation answers. §FS-rhei-run-tui.1.5.5 §FS-rhei-viz.5.1.2
#[test]
fn a_briefed_gate_answers_after_a_dry_run_names_the_destination() {
    let dir = tempfile::tempdir().expect("tmpdir");
    let sink = gate(vec![moved("approved", true), moved("approved", false)]);
    let mut state = state_at_gate(sink.clone(), "request", dir.path());

    press(&mut state, KeyCode::Enter);
    assert!(
        state.gate_form.is_some() && !state.gate_active,
        "a briefed gate opens the form, not the release"
    );
    press(&mut state, KeyCode::Char('y'));
    press(&mut state, KeyCode::Down);
    press(&mut state, KeyCode::Right); // the first press picks the highlighted option
    press(&mut state, KeyCode::Right);
    press(&mut state, KeyCode::Tab);
    press(&mut state, KeyCode::Char('2'));
    press(&mut state, KeyCode::Left);
    press(&mut state, KeyCode::Char(' '));
    press(&mut state, KeyCode::Down);
    for ch in "ok y".chars() {
        press(&mut state, KeyCode::Char(ch));
    }
    press(&mut state, KeyCode::Backspace);
    press(&mut state, KeyCode::Backspace);
    let form = state.gate_form.as_ref().unwrap();
    assert_eq!(form.given[0], Some(Given::Yes(true)));
    assert_eq!(form.given[1], Some(Given::One("large".into())));
    assert_eq!(
        form.given[2],
        Some(Given::Many(vec!["a".into(), "b".into()])),
        "picks keep the offered order"
    );
    assert_eq!(form.given[3], Some(Given::Text("ok".into())));

    press(&mut state, KeyCode::Enter);
    assert_eq!(state.gate_form.as_ref().unwrap().stage, Stage::Confirm { to: "approved".into() });
    assert!(screen(&state).contains("this moves 1 to approved"), "{}", screen(&state));
    press(&mut state, KeyCode::Enter);

    assert!(state.gate_form.is_none(), "an answer that lands closes the block");
    let answered = sink.answered.lock().unwrap().clone();
    assert_eq!(answered.iter().map(|(_, _, _, dry)| *dry).collect::<Vec<_>>(), vec![true, false]);
    let (task, from, document, _) = &answered[1];
    assert_eq!((task.as_str(), from.as_str()), ("1", "vetting"));
    assert_eq!(
        document,
        &serde_json::json!({"version": 1, "request_digest": "d1",
            "answers": {"approve": true, "scope": "large", "tags": ["a", "b"], "note": "ok"}})
    );
    assert_eq!(
        last_journal(&state),
        (MessageLevel::Info, "⮞ gate 1: vetting→approved · answered".into())
    );
}

/// A refusal stays in the form with the answers kept, beside the question it
/// names, and is echoed in the journal; unanswered questions are never sent,
/// and Esc backs out of a confirmation without answering. §FS-rhei-run-tui.1.5.5
#[test]
fn a_refusal_keeps_the_form_and_its_answers() {
    let dir = tempfile::tempdir().expect("tmpdir");
    let refused = serde_json::json!({"ok": false, "errors": [
        {"kind": "invalid", "question": "approve", "message": "`approve` is required"}]});
    let sink =
        gate(vec![Ok(refused), Err("could not reach the run".into()), moved("rework", true)]);
    let mut state = state_at_gate(sink.clone(), "questions", dir.path());

    press(&mut state, KeyCode::Enter);
    press(&mut state, KeyCode::Down);
    press(&mut state, KeyCode::Char('1'));
    press(&mut state, KeyCode::Enter);
    let form = state.gate_form.as_ref().expect("a refusal keeps the form");
    assert_eq!(form.stage, Stage::Editing);
    assert_eq!(form.given[1], Some(Given::One("small".into())), "the answers are kept");
    assert_eq!(form.errors[0].question.as_deref(), Some("approve"));
    assert!(screen(&state).contains("✗ `approve` is required"), "{}", screen(&state));
    assert_eq!(
        last_journal(&state),
        (MessageLevel::Warn, "gate 1 answer refused: approve: `approve` is required".into())
    );
    assert_eq!(
        sink.answered.lock().unwrap()[0].2["answers"],
        serde_json::json!({"scope": "small"})
    );

    press(&mut state, KeyCode::Enter);
    let form = state.gate_form.as_ref().unwrap();
    assert_eq!(form.errors[0].message, "could not reach the run");
    assert!(screen(&state).contains("could not reach the run"));

    press(&mut state, KeyCode::Up);
    press(&mut state, KeyCode::Char('n'));
    press(&mut state, KeyCode::Enter);
    assert_eq!(state.gate_form.as_ref().unwrap().stage, Stage::Confirm { to: "rework".into() });
    press(&mut state, KeyCode::Esc);
    assert_eq!(state.gate_form.as_ref().unwrap().stage, Stage::Editing, "Esc returns to the form");
    press(&mut state, KeyCode::Delete);
    assert_eq!(
        state.gate_form.as_ref().unwrap().given[0],
        None,
        "Delete clears the focused answer"
    );
    press(&mut state, KeyCode::Esc);
    assert!(state.gate_form.is_none(), "Esc closes the form");
    assert!(
        sink.answered.lock().unwrap().iter().all(|(_, _, _, dry)| *dry),
        "nothing was answered"
    );
}

/// A gate whose questions rhei synthesized keeps the release it has always
/// offered, with the brief drawn above the choices. The choices are the
/// brief's destinations, a `from: "*"` one included, and its two synthesized
/// questions are those choices and the composer, not a list. §FS-rhei-viz.5.1.3
#[test]
fn a_plain_gate_keeps_its_release_under_the_brief() {
    let dir = tempfile::tempdir().expect("tmpdir");
    let mut state = state_at_gate(gate(vec![]), "synthesized", dir.path());
    let long =
        "back to the author, who reworks the proposal from the reviewer's notes and sends it back";
    let brief = state.plan.gates.get_mut("1").unwrap();
    brief.destinations[1].description = Some(long.into());
    // The machine's wildcard edge to `cancelled` is a destination too.
    brief.destinations.push(GateDestination {
        state: "cancelled".into(),
        description: Some("Dropped".into()),
        terminal: true,
        ..Default::default()
    });
    brief.questions = vec![
        GateQuestion {
            id: "destination".into(),
            kind: "choice".into(),
            prompt: "Where should 1 go?".into(),
            required: true,
            options: Some(
                ["approved", "rework", "cancelled"]
                    .iter()
                    .map(|to| GateOption {
                        value: (*to).into(),
                        label: Some((*to).into()),
                        ..Default::default()
                    })
                    .collect(),
            ),
            help: None,
        },
        GateQuestion {
            id: "reason".into(),
            kind: "text".into(),
            prompt: "Why?".into(),
            required: true,
            ..Default::default()
        },
    ];
    state.refresh_plan();
    press(&mut state, KeyCode::Enter);
    assert!(state.gate_active && state.gate_form.is_none());
    let shown = screen(&state);
    for expected in [
        "Vet the proposal before it ships",
        "1 · Vet the proposal — gate vetting · visit 2 · since unknown",
        "Read the draft, then decide.",
        "draft.md  readable",
        "│ First line of the draft",
        "Leads to approved (final) · rework — back to the author",
        // A long line wraps under its label rather than running off the block.
        "│         notes and sends it back",
        "1  vetting → approved",
        "2  vetting → rework",
        "3  vetting → cancelled",
    ] {
        assert!(shown.contains(expected), "missing {expected:?} in\n{shown}");
    }
    for absent in ["Questions", "Where should 1 go?", "Why?", "required"] {
        assert!(!shown.contains(absent), "a plain gate lists no questions: {absent:?} in\n{shown}");
    }

    // The wildcard destination is a choice like any other.
    press(&mut state, KeyCode::Char('3'));
    let composer = state.composer.as_ref().expect("the choice opens the result composer");
    match &composer.kind {
        super::state::ComposerKind::GateResult { to, .. } => assert_eq!(to, "cancelled"),
        _ => panic!("expected a gate composer"),
    }
}

/// An open form follows the plan: a new brief redraws it, keeping each answer
/// whose question is still asked, and a ticket that left its gate closes it.
/// §FS-rhei-viz.5.1.1
#[test]
fn the_form_follows_the_brief_the_plan_shows() {
    let dir = tempfile::tempdir().expect("tmpdir");
    let mut state = state_at_gate(gate(vec![]), "request", dir.path());
    press(&mut state, KeyCode::Enter);
    press(&mut state, KeyCode::Char('y'));

    let next = state.plan.gates.get_mut("1").unwrap();
    next.digest = "d2".into();
    next.questions.remove(1);
    state.refresh_plan();
    let form = state.gate_form.as_ref().expect("the form stays open");
    assert_eq!(form.brief.digest, "d2");
    assert_eq!(form.given, vec![Some(Given::Yes(true)), None, None]);

    state.plan.gates.clear();
    state.refresh_plan();
    assert!(state.gate_form.is_none(), "a ticket that left its gate closes the form");
}
