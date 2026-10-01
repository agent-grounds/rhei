use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyModifiers};

use super::derive::{outline_fold, outline_order};
use super::input::handle_key_event;
use super::state::UiState;
use crate::rhei_viz_model::{Machine, MachineState, TaskRow, VizModel};

// §FS-rhei-run-tui.1.5.3 §FS-rhei-run-report.3.2

fn state(name: &str, terminal: bool, gating: bool) -> MachineState {
    MachineState {
        name: name.to_string(),
        description: None,
        instructions: None,
        visits: None,
        initial: name == "work",
        terminal,
        gating,
        waiting_on: None,
        process: None,
        transitions: vec![],
        inputs: vec![],
        outputs: vec![],
        template_context: Default::default(),
        template_contexts: vec![],
    }
}

fn task(id: &str, state: &str) -> TaskRow {
    let parent = id.rsplit_once('.').map(|(parent, _)| parent.to_string());
    TaskRow {
        id: id.to_string(),
        title: format!("Task {id}"),
        depth: id.matches('.').count() as u8,
        parent,
        state: state.to_string(),
        visit_count: None,
        prior: vec![],
        history: vec![],
    }
}

/// A plan under a machine declaring `work`, a gate, and three terminals, with
/// the selection on the first root so nothing inside a subtree holds it open.
fn outline(tasks: &[(&str, &str)]) -> UiState {
    let mut ui = UiState::with_context(PathBuf::from("/ws"), 1, 0, None, None, None, false);
    ui.plan = VizModel {
        tasks: tasks.iter().map(|(id, state)| task(id, state)).collect(),
        machine: Machine {
            name: "fold".into(),
            states: vec![
                state("work", false, false),
                state("human-gate", false, true),
                state("completed", true, false),
                state("shipped", true, false),
                state("cancelled", true, false),
            ],
        },
        ..VizModel::default()
    };
    ui.refresh_plan();
    assert!(ui.select_task(tasks[0].0));
    ui
}

fn outline_ids(ui: &UiState) -> Vec<String> {
    outline_order(ui).into_iter().map(|i| ui.plan.tasks[i].id.clone()).collect()
}

/// A finished parent is one outline row carrying the console's clause, every
/// descendant bucketed in the machine's order. §FS-rhei-run-tui.1.5.3
#[test]
fn a_finished_parent_is_one_outline_row_with_the_console_clause() {
    let ui = outline(&[
        ("1", "completed"),
        ("1.1", "shipped"),
        ("1.1.1", "completed"),
        ("1.2", "cancelled"),
        ("2", "work"),
    ]);

    assert_eq!(outline_ids(&ui), ["1", "2"]);
    let parent = ui.task("1").expect("task 1").clone();
    assert_eq!(
        outline_fold(&ui, &parent).as_deref(),
        Some(" \u{2014} 3 subtasks: 1 completed, 1 shipped, 1 cancelled")
    );
}

/// The rule keeps a subtree open above anything a person has to see: a gate,
/// open work, or a parent that has not finished itself. §FS-rhei-run-tui.1.5.3
#[test]
fn a_gate_open_work_or_an_open_parent_keeps_the_subtree_in_the_outline() {
    for tasks in [
        [("1", "completed"), ("1.1", "completed"), ("1.2", "human-gate")],
        [("1", "completed"), ("1.1", "completed"), ("1.2", "work")],
        [("1", "work"), ("1.1", "completed"), ("1.2", "completed")],
    ] {
        let ui = outline(&tasks);
        assert_eq!(outline_ids(&ui), ["1", "1.1", "1.2"], "{tasks:?}");
    }
}

/// `Enter` on a folded parent expands it in place, so a finished child is never
/// unreachable from the outline, and the nested finished parent it reveals
/// stays folded until it is expanded in turn. §FS-rhei-run-tui.1.5.3
#[test]
fn enter_on_a_folded_parent_expands_it_in_place() {
    let mut ui = outline(&[
        ("1", "completed"),
        ("1.1", "completed"),
        ("1.1.1", "completed"),
        ("1.2", "completed"),
        ("2", "work"),
    ]);
    assert_eq!(outline_ids(&ui), ["1", "2"]);

    handle_key_event(&mut ui, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(outline_ids(&ui), ["1", "1.1", "1.2", "2"]);
    assert_eq!(ui.selected.as_deref(), Some("1"), "the selection stays on the row it expanded");
    let parent = ui.task("1").expect("task 1").clone();
    assert_eq!(outline_fold(&ui, &parent), None, "an expanded parent carries no clause");

    handle_key_event(&mut ui, KeyCode::Down, KeyModifiers::NONE);
    handle_key_event(&mut ui, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(outline_ids(&ui), ["1", "1.1", "1.1.1", "1.2", "2"]);
}

/// The selection standing inside a folded subtree holds it open, so selecting
/// a finished child, through the inspector's `children` chip say, never leaves
/// the selected row off the outline. §FS-rhei-run-tui.1.5.3
#[test]
fn a_selection_inside_a_finished_subtree_holds_it_open() {
    let mut ui = outline(&[("1", "completed"), ("1.1", "completed"), ("1.2", "completed")]);
    assert_eq!(outline_ids(&ui), ["1"]);

    assert!(ui.select_task("1.2"));
    assert_eq!(outline_ids(&ui), ["1", "1.1", "1.2"]);
}

/// An active `/` filter holds every subtree open, so a row the filter found is
/// never folded away; a filter of only whitespace filters nothing, so it holds
/// nothing either. §FS-rhei-run-tui.1.5.3
#[test]
fn an_active_filter_holds_a_finished_subtree_open_and_whitespace_does_not() {
    let mut ui = outline(&[("1", "completed"), ("1.1", "completed"), ("1.2", "completed")]);

    ui.filter = Some("task".to_string());
    assert_eq!(outline_ids(&ui), ["1", "1.1", "1.2"]);

    ui.filter = Some("   ".to_string());
    assert_eq!(outline_ids(&ui), ["1"]);
}
