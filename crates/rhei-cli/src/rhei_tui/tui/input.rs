//! Keyboard handling for the Flow surface: the two-level selection model, view
//! switching, filters, and the live intervene/gate composers.
//! §FS-rhei-run-tui.1.5.2 §FS-rhei-run-tui.1.5.5

use crossterm::event::{KeyCode, KeyModifiers};

use crate::rhei_tui::event::MessageLevel;

use super::derive::{inspector_sections, outline_fold, outline_order, ChipAction};
use super::state::{Composer, ComposerKind, FlowFocus, UiState, View};

/// What the render loop should do after a key event.
pub(super) enum InputAction {
    Continue,
    ForwardSigint,
    Quit,
}

pub(super) fn handle_key_event(
    state: &mut UiState,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> InputAction {
    // Ctrl+C restores the terminal and re-raises SIGINT (§1.8) — unless this
    // surface is only attached, in which case it disconnects and the run never
    // hears about it. §FS-rhei-run-headless.5.1
    if code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
        if state.attached {
            return InputAction::Quit;
        }
        state.push_journal(MessageLevel::Info, "(ctrl+c received — forwarding SIGINT)".to_string());
        return InputAction::ForwardSigint;
    }

    // Modal inputs intercept everything else.
    if state.composer.is_some() {
        return handle_composer(state, code);
    }
    if state.gate_active {
        return handle_gate(state, code);
    }
    if state.filter_editing {
        return handle_filter(state, code);
    }
    if state.help {
        if matches!(code, KeyCode::Char('?') | KeyCode::Esc) {
            state.help = false;
        }
        return InputAction::Continue;
    }

    match code {
        KeyCode::Char('?') => state.help = true,
        KeyCode::Char('q') => {
            // Quit only once the run has finished; during a live run the
            // operator stops with Ctrl+C (§1.5.2). An attached surface detaches
            // whenever it likes: it is not the thing the run is waiting on.

            // §FS-rhei-run-headless.5.1
            if state.finished || state.attached {
                return InputAction::Quit;
            }
        }
        KeyCode::Char('1') => switch_view(state, View::Flow),
        KeyCode::Char('2') => switch_view(state, View::Machine),
        KeyCode::Char('3') => switch_view(state, View::Cost),
        KeyCode::Char('4') => switch_view(state, View::Journal),
        KeyCode::Char('h') | KeyCode::Left => cycle_view(state, -1),
        KeyCode::Char('l') | KeyCode::Right => cycle_view(state, 1),
        KeyCode::Char('j') | KeyCode::Down => move_focus(state, 1),
        KeyCode::Char('k') | KeyCode::Up => move_focus(state, -1),
        KeyCode::PageDown => move_focus(state, 10),
        KeyCode::PageUp => move_focus(state, -10),
        KeyCode::Tab => {
            if state.view == View::Flow {
                state.flow_focus = match state.flow_focus {
                    FlowFocus::Outline => FlowFocus::Inspector,
                    FlowFocus::Inspector => {
                        state.inspector_item = None;
                        FlowFocus::Outline
                    }
                };
            }
        }
        KeyCode::Esc => {
            if state.view == View::Flow
                && state.flow_focus == FlowFocus::Inspector
                && state.inspector_item.is_some()
            {
                state.inspector_item = None;
            }
        }
        KeyCode::Enter => handle_enter(state),
        KeyCode::Char('/') => {
            state.filter_editing = true;
            state.filter = Some(state.filter.clone().unwrap_or_default());
        }
        KeyCode::Char('g') => {
            if state.view == View::Cost {
                state.cost_group = state.cost_group.next();
                state.cost_cursor = 0;
            }
        }
        KeyCode::Char('f') => {
            if state.view == View::Journal {
                state.journal_filter = state.journal_filter.next();
                state.journal_scroll = 0;
            }
        }
        KeyCode::Char('m') => open_composer(state),
        _ => {}
    }

    InputAction::Continue
}

fn switch_view(state: &mut UiState, view: View) {
    state.view = view;
    if view == View::Flow {
        state.flow_focus = FlowFocus::Outline;
    }
}

fn cycle_view(state: &mut UiState, delta: isize) {
    let idx = state.view.index() as isize;
    let len = View::ORDER.len() as isize;
    let next = ((idx + delta) % len + len) % len;
    switch_view(state, View::ORDER[next as usize]);
}

fn move_focus(state: &mut UiState, delta: isize) {
    match state.view {
        View::Flow => match state.flow_focus {
            FlowFocus::Outline => {
                let order = outline_order(state);
                state.move_selected_in(&order, delta);
            }
            FlowFocus::Inspector => {
                let sections = state
                    .selected
                    .clone()
                    .map(|id| inspector_sections(state, &id))
                    .unwrap_or_default();
                if sections.is_empty() {
                    return;
                }
                state.inspector_section =
                    state.inspector_section.min(sections.len().saturating_sub(1));
                if let Some(item) = state.inspector_item {
                    let item_count = sections[state.inspector_section].items.len();
                    if item_count == 0 {
                        state.inspector_item = None;
                    } else {
                        let next = (item as isize + delta).clamp(0, item_count as isize - 1);
                        state.inspector_item = Some(next as usize);
                    }
                } else {
                    let cur = state.inspector_section as isize;
                    state.inspector_section =
                        (cur + delta).clamp(0, sections.len() as isize - 1) as usize;
                }
            }
        },
        View::Machine => {
            let order = state.machine_view_order();
            if !order.is_empty() {
                let current = order.iter().position(|idx| *idx == state.machine_focus).unwrap_or(0);
                let next = (current as isize + delta).clamp(0, order.len() as isize - 1);
                state.machine_focus = order[next as usize];
            }
        }
        View::Cost => {
            if matches!(state.cost_group, super::state::CostGroup::Task) {
                let order = state.visible_task_indices();
                state.move_selected_in(&order, delta);
            } else {
                state.cost_cursor = (state.cost_cursor as isize + delta).max(0) as usize;
            }
        }
        View::Journal => {
            state.journal_scroll = clamp_scroll(state.journal_scroll, delta);
        }
    }
}

fn clamp_scroll(current: u16, delta: isize) -> u16 {
    (current as isize + delta).max(0) as u16
}

/// Expand the selected outline row in place when it is a folded parent, so a
/// finished child is never unreachable from the outline. §FS-rhei-run-tui.1.5.3
fn expand_folded(state: &mut UiState) -> bool {
    let Some(task) = state.selected_task() else { return false };
    if outline_fold(state, task).is_none() {
        return false;
    }
    let id = task.id.clone();
    state.expanded.insert(id)
}

fn handle_enter(state: &mut UiState) {
    match state.view {
        View::Flow if state.flow_focus == FlowFocus::Outline => {
            // Enter on a folded parent expands it in place (§1.5.3); on a gating
            // task it opens the human-gate chooser (§1.5.5). No gate is ever folded.
            if !expand_folded(state) {
                open_gate(state);
            }
        }
        View::Flow => {
            let Some(id) = state.selected.clone() else { return };
            let sections = inspector_sections(state, &id);
            if sections.is_empty() {
                return;
            }
            state.inspector_section = state.inspector_section.min(sections.len() - 1);
            let section = &sections[state.inspector_section];
            let Some(item) = state.inspector_item else {
                if !section.items.is_empty() {
                    state.inspector_item = Some(0);
                }
                return;
            };
            let Some(chip) = section.items.get(item) else {
                state.inspector_item = None;
                return;
            };
            match &chip.action {
                ChipAction::SelectTask(target) => {
                    state.select_task(target);
                }
                ChipAction::MarkState(target) => {
                    // Mark the target state in the Machine view, keeping the
                    // selected task in context.
                    if let Some(pos) =
                        state.plan.machine.states.iter().position(|s| &s.name == target)
                    {
                        state.machine_focus = pos;
                        switch_view(state, View::Machine);
                    }
                }
                ChipAction::None => {}
            }
        }
        _ => {}
    }
}

fn handle_filter(state: &mut UiState, code: KeyCode) -> InputAction {
    match code {
        KeyCode::Esc => {
            state.filter = None;
            state.filter_editing = false;
            state.reconcile_filter_focus();
        }
        KeyCode::Enter => {
            state.filter_editing = false;
            if state.filter.as_deref() == Some("") {
                state.filter = None;
            }
            state.reconcile_filter_focus();
        }
        KeyCode::Backspace => {
            if let Some(f) = state.filter.as_mut() {
                f.pop();
            }
        }
        KeyCode::Char(c) => {
            state.filter.get_or_insert_with(String::new).push(c);
        }
        _ => {}
    }
    InputAction::Continue
}

/// Open the intervene composer for the selected live task, when its agent is
/// reachable. Otherwise leave a journal note naming the remedy.
/// §FS-rhei-run-tui.1.5.5
fn open_composer(state: &mut UiState) {
    if state.finished {
        return;
    }
    let Some(id) = state.selected.clone() else { return };
    let Some((slot, _)) = state.running_slot(&id) else {
        return;
    };
    let reachable =
        state.intervene.as_ref().map(|sink| sink.reachable(&id, Some(slot))).unwrap_or(false);
    if !reachable {
        state.push_journal(
            MessageLevel::Warn,
            format!("{id}: agent is not reachable — set intervene_stdin and rerun"),
        );
        return;
    }
    state.composer = Some(Composer {
        task: id,
        slot: Some(slot),
        input: String::new(),
        kind: ComposerKind::Intervene,
    });
}

fn handle_composer(state: &mut UiState, code: KeyCode) -> InputAction {
    match code {
        KeyCode::Esc => {
            state.composer = None;
        }
        KeyCode::Backspace => {
            if let Some(c) = state.composer.as_mut() {
                c.input.pop();
            }
        }
        KeyCode::Char(ch) => {
            if let Some(c) = state.composer.as_mut() {
                c.input.push(ch);
            }
        }
        KeyCode::Enter => {
            let Some(composer) = state.composer.take() else {
                return InputAction::Continue;
            };
            let message = composer.input.trim().to_string();
            match &composer.kind {
                ComposerKind::Intervene => {
                    // Nothing to deliver: put the composer back rather than
                    // sending an empty line to the agent.
                    if message.is_empty() {
                        state.composer = Some(composer);
                        return InputAction::Continue;
                    }
                    let result = match &state.intervene {
                        Some(sink) => sink.deliver(Some(&composer.task), composer.slot, &message),
                        None => Err("intervene is not available".to_string()),
                    };
                    match result {
                        Ok(()) => state.push_journal(
                            MessageLevel::Info,
                            format!("⌨ intervene → {}: {message}", composer.task),
                        ),
                        Err(reason) => state.push_journal(
                            MessageLevel::Warn,
                            format!("intervene to {} failed: {reason}", composer.task),
                        ),
                    }
                }
                // An empty line submits with no message: the server decides
                // whether this edge needs one, and its refusal is echoed here.
                // §FS-rhei-run-tui.1.5.5 §FS-rhei-states.3.3
                ComposerKind::GateResult { from, to, .. } => {
                    submit_gate_transition(
                        state,
                        &composer.task,
                        from,
                        to,
                        (!message.is_empty()).then_some(message.as_str()),
                    );
                }
            }
        }
        _ => {}
    }
    InputAction::Continue
}

/// Send one gate decision and echo the outcome in the journal.
/// §FS-rhei-run-tui.1.5.5
fn submit_gate_transition(
    state: &mut UiState,
    task: &str,
    from: &str,
    to: &str,
    result: Option<&str>,
) {
    let outcome = match &state.gate {
        Some(sink) => sink.transition_gate(task, from, to, result),
        None => Err("gate transitions are not available".to_string()),
    };
    match outcome {
        Ok(effective) => {
            state.push_journal(MessageLevel::Info, format!("⮞ gate {task}: {from}→{effective}"))
        }
        Err(reason) => state.push_journal(
            MessageLevel::Warn,
            format!("gate {task} {from}→{to} rejected: {reason}"),
        ),
    }
}

/// Pick a human-gate transition: the digit keys select one of the gating
/// state's explicit outgoing transitions, then the composer collects the result
/// message that rides the move. §FS-rhei-run-tui.1.5.5
fn handle_gate(state: &mut UiState, code: KeyCode) -> InputAction {
    match code {
        KeyCode::Esc => {
            state.gate_active = false;
        }
        KeyCode::Char(ch) if ch.is_ascii_digit() => {
            let Some(choice) = ch.to_digit(10) else {
                return InputAction::Continue;
            };
            if choice == 0 {
                return InputAction::Continue;
            }
            let choices = gate_choices(state);
            if let Some((from, to)) = choices.get((choice - 1) as usize).cloned() {
                let Some(id) = state.selected.clone() else {
                    return InputAction::Continue;
                };
                // A human finishing a ticket by hand is the case where the
                // reason matters most, so the choice opens the composer instead
                // of firing straight away. §FS-rhei-states.3.3
                let terminal = state.machine_state(&to).map(|st| st.terminal).unwrap_or(false);
                state.composer = Some(Composer {
                    task: id,
                    slot: None,
                    input: String::new(),
                    kind: ComposerKind::GateResult { from, to, terminal },
                });
            }
            state.gate_active = false;
        }
        _ => {}
    }
    InputAction::Continue
}

/// The explicit outgoing `(from, to)` transitions for the selected task's
/// current gating state.
pub(super) fn gate_choices(state: &UiState) -> Vec<(String, String)> {
    let Some(task) = state.selected_task() else {
        return Vec::new();
    };
    let Some(st) = state.machine_state(&task.state) else {
        return Vec::new();
    };
    if !st.gating {
        return Vec::new();
    }
    st.transitions.iter().filter(|t| !t.wildcard).map(|t| (st.name.clone(), t.to.clone())).collect()
}

/// Whether the `m` intervene action currently applies (selected task is live).
pub(super) fn intervene_available(state: &UiState) -> bool {
    !state.finished && state.selected.as_ref().map(|id| state.is_live(id)).unwrap_or(false)
}

/// Open the gate chooser for the selected task when it sits in a live gating
/// state. Returns whether it opened (used by the action bar key).
pub(super) fn open_gate(state: &mut UiState) -> bool {
    if state.finished {
        return false;
    }
    if gate_choices(state).is_empty() || state.gate.is_none() {
        return false;
    }
    state.gate_active = true;
    true
}
