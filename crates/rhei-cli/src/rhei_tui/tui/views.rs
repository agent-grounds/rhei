//! The five terminal views (§FS-rhei-run-tui.1.5.3, §1.5.4) and the surroundings
//! inspector. Each renders the one shared run model under the console language;
//! view *content* is defined in §FS-rhei-viz and realized here for the terminal.

#[cfg(test)]
pub(super) use super::cost::cost_rows;
pub(super) use super::cost::render_cost;

use crate::rhei_viz_model::{MachineProcessKind, MachineState};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

#[cfg(test)]
pub(super) use super::inspector::inspector_lines;
pub(super) use super::inspector::machine_state_flags;
use super::inspector::render_inspector;
#[cfg(test)]
pub(super) use super::inspector::task_flags;

use super::derive::{machine_groups, subtree_progress};
use super::render::{render_list, state_pill};
use super::state::{FlowFocus, JournalEntry, ProcessKind, TaskRow, UiState};
use super::text::truncate_chars;
use super::theme::{category, category_glyph};
use crate::rhei_tui::event::MessageLevel;

/// One journal line, colored by severity (color rides a prefix glyph too).
pub(super) fn journal_line(state: &UiState, entry: &JournalEntry) -> Line<'static> {
    let theme = &state.theme;
    let (prefix, color) = match entry.level {
        MessageLevel::Info => ("·", theme.dim()),
        MessageLevel::Warn => ("!", theme.category_color(super::theme::Category::Blocked)),
        MessageLevel::Error => ("✗", theme.category_color(super::theme::Category::Failed)),
    };
    Line::from(vec![
        Span::styled(format!("{prefix} "), Style::default().fg(color)),
        Span::raw(entry.text.clone()),
    ])
}

/// The Flow outline row for one task: glyph (or live marker), id, title, state
/// pill, and `done/total ✓` for parents. §FS-rhei-viz.2
fn outline_row(state: &UiState, task: &TaskRow) -> Line<'static> {
    let theme = &state.theme;
    let indent = "  ".repeat(task.depth as usize);
    let mut spans = vec![Span::raw(indent)];
    if let Some(marker) = live_process_marker(state, &task.id) {
        spans.push(marker);
    } else {
        let cat = category(&state.plan.machine, &task.state);
        spans.push(Span::styled(
            format!("{} ", category_glyph(cat)),
            Style::default().fg(theme.category_color(cat)),
        ));
    }
    spans
        .push(Span::styled(format!("{} ", task.id), Style::default().add_modifier(Modifier::BOLD)));
    spans.push(Span::raw(truncate_chars(&task.title, 32)));
    spans.push(Span::raw("  "));
    spans.extend(state_pill(theme, &state.plan.machine, &task.state));
    if let Some((done, total)) = subtree_progress(&state.plan, task) {
        spans.push(Span::styled(format!("  {done}/{total} ✓"), Style::default().fg(theme.dim())));
    }
    Line::from(spans)
}

fn live_process_marker(state: &UiState, task_id: &str) -> Option<Span<'static>> {
    let theme = &state.theme;
    match state.running_process_kind(task_id)? {
        ProcessKind::Agent => Some(Span::styled(
            format!("{} ", state.spinner_glyph()),
            Style::default().fg(theme.live_color()).add_modifier(Modifier::BOLD),
        )),
        ProcessKind::Program => Some(Span::styled(
            "● ".to_string(),
            Style::default().fg(theme.program_color()).add_modifier(Modifier::BOLD),
        )),
    }
}

pub(super) fn render_flow(f: &mut Frame, area: Rect, state: &UiState) {
    // Wide: panes side by side. Narrow: stack the inspector below the outline.
    let narrow = area.width < 96;
    let (outline_area, inspector_area) = if narrow {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
            .split(area);
        (chunks[0], chunks[1])
    } else {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
            .split(area);
        (chunks[0], chunks[1])
    };

    render_outline(f, outline_area, state);
    render_inspector(f, inspector_area, state);
}

fn render_outline(f: &mut Frame, area: Rect, state: &UiState) {
    let focused = matches!(state.flow_focus, FlowFocus::Outline);
    let block = Block::default()
        .title(" plan ")
        .borders(Borders::ALL)
        .border_style(border_style(state, focused));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let order = state.visible_task_indices();
    if order.is_empty() {
        render_placeholder(f, inner, state, "no tasks match");
        return;
    }
    let rows: Vec<(bool, Line)> = order
        .iter()
        .map(|i| {
            let task = &state.plan.tasks[*i];
            let selected = state.selected.as_deref() == Some(task.id.as_str());
            (selected, outline_row(state, task))
        })
        .collect();
    render_list(f, inner, rows, focused, &state.theme);
}

pub(super) fn dim_line(theme: &super::theme::Theme, text: &str) -> Line<'static> {
    Line::from(Span::styled(text.to_string(), Style::default().fg(theme.dim())))
}

pub(super) fn section_header(theme: &super::theme::Theme, label: &str) -> Line<'static> {
    Line::from(Span::styled(
        label.to_string(),
        Style::default().fg(theme.accent()).add_modifier(Modifier::BOLD),
    ))
}

pub(super) fn border_style(state: &UiState, focused: bool) -> Style {
    if focused {
        Style::default().fg(state.theme.accent())
    } else {
        Style::default().fg(state.theme.dim())
    }
}

pub(super) fn render_placeholder(f: &mut Frame, area: Rect, state: &UiState, text: &str) {
    let p = Paragraph::new(Line::from(Span::styled(
        text.to_string(),
        Style::default().fg(state.theme.dim()),
    )));
    f.render_widget(p, area);
}

fn machine_state_pill(
    state: &UiState,
    machine_state: &MachineState,
    machine: &super::state::Machine,
) -> Vec<Span<'static>> {
    match machine_state.process {
        Some(MachineProcessKind::Agent) => process_state_pill(
            "◆",
            &machine_state.name,
            Style::default().fg(state.theme.live_color()).add_modifier(Modifier::BOLD),
        ),
        Some(MachineProcessKind::Program) => process_state_pill(
            "●",
            &machine_state.name,
            Style::default().fg(state.theme.program_color()).add_modifier(Modifier::BOLD),
        ),
        None => state_pill(&state.theme, machine, &machine_state.name),
    }
}

fn process_state_pill(glyph: &str, label: &str, style: Style) -> Vec<Span<'static>> {
    vec![Span::styled(format!("{glyph} "), style), Span::styled(label.to_string(), style)]
}

fn machine_legend_line(state: &UiState) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            "▌ focused state · ◀ selected task state · ",
            Style::default().fg(state.theme.dim()),
        ),
        Span::styled("◆ agent state", Style::default().fg(state.theme.live_color())),
        Span::styled(" · ", Style::default().fg(state.theme.dim())),
        Span::styled("● program state", Style::default().fg(state.theme.program_color())),
        Span::styled(
            " · · idle · ● active · ⏸ gate · ✓ done",
            Style::default().fg(state.theme.dim()),
        ),
    ])
}

fn render_machine_legend(f: &mut Frame, area: Rect, state: &UiState) {
    let block = Block::default().title(" legend ").borders(Borders::ALL);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height > 0 {
        f.render_widget(Paragraph::new(machine_legend_line(state)), inner);
    }
}

fn machine_task_line(
    state: &UiState,
    machine_state: &MachineState,
    task: &TaskRow,
) -> Line<'static> {
    let mut spans = vec![Span::raw("   ")];
    match machine_state.process {
        Some(MachineProcessKind::Agent) => {
            let style = Style::default().fg(state.theme.live_color());
            spans.push(Span::styled("◆ ".to_string(), style.add_modifier(Modifier::BOLD)));
            spans.push(Span::styled(format!("{} ", task.id), style.add_modifier(Modifier::BOLD)));
            spans.push(Span::styled(truncate_chars(&task.title, 48), style));
        }
        Some(MachineProcessKind::Program) => {
            let style = Style::default().fg(state.theme.program_color());
            spans.push(Span::styled("● ".to_string(), style.add_modifier(Modifier::BOLD)));
            spans.push(Span::styled(format!("{} ", task.id), style.add_modifier(Modifier::BOLD)));
            spans.push(Span::styled(truncate_chars(&task.title, 48), style));
        }
        None => {
            spans.push(Span::raw("  "));
            spans.push(Span::styled(
                format!("{} ", task.id),
                Style::default().add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::raw(truncate_chars(&task.title, 48)));
        }
    }
    Line::from(spans)
}

pub(super) fn render_machine(f: &mut Frame, area: Rect, state: &UiState) {
    let (main_area, legend_area) = if area.height >= 8 {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(3)])
            .split(area);
        (chunks[0], Some(chunks[1]))
    } else {
        (area, None)
    };

    if let Some(legend_area) = legend_area {
        render_machine_legend(f, legend_area, state);
    }

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(main_area);

    // Left: grouped state list.
    let block = Block::default().title(" machine ").borders(Borders::ALL);
    let inner = block.inner(chunks[0]);
    f.render_widget(block, chunks[0]);

    let machine = &state.plan.machine;
    let groups = machine_groups(machine);
    let selected_state = state.selected_task().map(|task| task.state.clone());

    let visible_states = state.machine_view_order();
    let mut rows: Vec<(bool, Line)> = Vec::new();
    for (gi, group) in groups.iter().enumerate() {
        let visible_group: Vec<&usize> =
            group.iter().filter(|state_idx| visible_states.contains(state_idx)).collect();
        if visible_group.is_empty() {
            continue;
        }
        rows.push((false, dim_line(&state.theme, &format!("workflow {}", gi + 1))));
        for state_idx in visible_group {
            let st = &machine.states[*state_idx];
            let focused_row = *state_idx == state.machine_focus;
            let mut spans = vec![Span::raw(" ")];
            spans.extend(machine_state_pill(state, st, machine));
            if selected_state.as_deref() == Some(st.name.as_str()) {
                spans.push(Span::styled(
                    "  ◀ selected task",
                    Style::default().fg(state.theme.live_color()),
                ));
            }
            rows.push((focused_row, Line::from(spans)));
        }
    }
    if rows.is_empty() {
        render_placeholder(f, inner, state, "no states match");
    } else {
        render_list(f, inner, rows, true, &state.theme);
    }

    // Right: state detail panel for the focused state.
    let detail_block = Block::default().title(" state ").borders(Borders::ALL);
    let detail_inner = detail_block.inner(chunks[1]);
    f.render_widget(detail_block, chunks[1]);
    if let Some(st) = machine.states.get(state.machine_focus) {
        let mut lines: Vec<Line> = Vec::new();
        let mut head = vec![Span::raw("")];
        head.extend(machine_state_pill(state, st, machine));
        lines.push(Line::from(head));
        // §FS-rhei-viz.6: the counted budget is this panel's, the rest shared.
        let flags = machine_state_flags(st, true);
        if !flags.is_empty() {
            lines.push(dim_line(&state.theme, &flags.join(" · ")));
        }
        if let Some(desc) = &st.description {
            lines.push(Line::from(desc.clone()));
        }
        lines.push(Line::from(""));
        lines.push(section_header(&state.theme, "incoming / outgoing"));
        for other in &machine.states {
            if other.transitions.iter().any(|t| t.to == st.name) {
                lines.push(dim_line(&state.theme, &format!("   ⮜ {}", other.name)));
            }
        }
        for tr in &st.transitions {
            let marker = if tr.wildcard { " (from *)" } else { "" };
            lines.push(dim_line(&state.theme, &format!("   ⮞ {}{marker}", tr.to)));
        }
        // Occupying tasks.
        let occupants: Vec<&TaskRow> =
            state.plan.tasks.iter().filter(|task| task.state == st.name).collect();
        if !occupants.is_empty() {
            lines.push(Line::from(""));
            lines.push(section_header(&state.theme, "tasks here"));
            for task in occupants {
                lines.push(machine_task_line(state, st, task));
            }
        }
        if let Some(prompt) = &st.instructions {
            lines.push(Line::from(""));
            lines.push(section_header(&state.theme, "prompt template"));
            for raw in prompt.lines().take(10) {
                lines.push(dim_line(&state.theme, &format!("   {raw}")));
            }
        }
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), detail_inner);
    }
}

pub(super) fn render_journal(f: &mut Frame, area: Rect, state: &UiState) {
    let block = Block::default()
        .title(format!(" journal — {} ", state.journal_filter.label()))
        .borders(Borders::ALL);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let entries = state.filtered_journal();
    let height = inner.height as usize;
    let total = entries.len();
    // Scroll from the bottom; journal_scroll counts lines up from the tail.
    let bottom = total.saturating_sub(state.journal_scroll as usize);
    let start = bottom.saturating_sub(height);
    let lines: Vec<Line> =
        entries[start..bottom.min(total)].iter().map(|e| journal_line(state, e)).collect();
    f.render_widget(Paragraph::new(lines), inner);
}

/// Minimal layout: a compact one-line-per-task list with the shared links strip
/// (§FS-rhei-run-tui.1.5.6).
pub(super) fn render_minimal(f: &mut Frame, area: Rect, state: &UiState) {
    let order = state.visible_task_indices();
    if order.is_empty() {
        render_placeholder(f, area, state, "no tasks");
        return;
    }
    let rows: Vec<(bool, Line)> = order
        .iter()
        .map(|i| {
            let task = &state.plan.tasks[*i];
            let selected = state.selected.as_deref() == Some(task.id.as_str());
            let mut spans = Vec::new();
            if let Some(marker) = live_process_marker(state, &task.id) {
                spans.push(marker);
            } else {
                let cat = category(&state.plan.machine, &task.state);
                spans.push(Span::styled(
                    format!("{} ", category_glyph(cat)),
                    Style::default().fg(state.theme.category_color(cat)),
                ));
            }
            spans.push(Span::raw(format!("{} ", task.id)));
            spans.push(Span::raw(truncate_chars(&task.title, 24)));
            (selected, Line::from(spans))
        })
        .collect();
    render_list(f, area, rows, true, &state.theme);
}
