//! The five terminal views (§FS-rhei-run-tui.1.5.3, §1.5.4) and the surroundings
//! inspector. Each renders the one shared run model under the console language;
//! view *content* is defined in §FS-rhei-viz and realized here for the terminal.

use std::io::Read;
use std::path::Path;

use crate::rhei_viz_model::{MachineProcessKind, MachineState};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use super::derive::{
    artifact_rows, has_children, inspector_sections, machine_groups, outline_fold, outline_order,
    subtree_progress, task_direct, task_subtree, CostRollup, InspectorSectionKind,
};
use super::render::{format_cost_micro, format_tokens, render_list, state_pill};
use super::state::{FlowFocus, JournalEntry, ProcessKind, TaskRow, UiState};
use super::text::truncate_chars;
use super::theme::{category, category_glyph};
use crate::rhei_tui::event::{AgentStream, MessageLevel};

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
    // A folded parent says what its subtree came to in place of its progress. §FS-rhei-run-tui.1.5.3
    if let Some(clause) = outline_fold(state, task) {
        spans.push(Span::styled(format!(" {clause}"), Style::default().fg(theme.dim())));
    } else if let Some((done, total)) = subtree_progress(&state.plan, task) {
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

    let order = outline_order(state);
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

fn render_inspector(f: &mut Frame, area: Rect, state: &UiState) {
    let focused = matches!(state.flow_focus, FlowFocus::Inspector);
    let block = Block::default()
        .title(" surroundings ")
        .borders(Borders::ALL)
        .border_style(border_style(state, focused));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(task) = state.selected_task().cloned() else {
        render_placeholder(f, inner, state, "select a task");
        return;
    };

    let (lines, focus_row) = inspector_lines(state, &task, focused);
    let scroll = focus_row
        .map(|row| row.saturating_sub((inner.height as usize).saturating_sub(1)) as u16)
        .unwrap_or(state.inspector_scroll);
    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false }).scroll((scroll, 0));
    f.render_widget(paragraph, inner);
}

/// The surroundings inspector content, in §FS-rhei-viz.4 order. Sections and
/// items are numbered against `inspector_sections` so focus navigates exactly
/// where it is drawn.
pub(super) fn inspector_lines(
    state: &UiState,
    task: &TaskRow,
    focused: bool,
) -> (Vec<Line<'static>>, Option<usize>) {
    let theme = &state.theme;
    let machine = &state.plan.machine;
    let mut lines: Vec<Line> = Vec::new();
    let mut focus_row = None;

    // 1. Head + description.
    let mut head = vec![Span::raw("")];
    head.extend(state_pill(theme, machine, &task.state));
    head.push(Span::styled(
        format!("  {} ", task.id),
        Style::default().add_modifier(Modifier::BOLD),
    ));
    head.push(Span::raw(task.title.clone()));
    lines.push(Line::from(head));
    let flags = task_flags(state, task);
    if !flags.is_empty() {
        lines.push(Line::from(Span::styled(flags, Style::default().fg(theme.dim()))));
    }
    lines.push(readiness_line(state, task));
    if let Some(st) = state.machine_state(&task.state) {
        if let Some(desc) = &st.description {
            lines.push(Line::from(Span::styled(desc.clone(), Style::default().fg(theme.dim()))));
        }
    }
    lines.push(Line::from(""));

    let sections = inspector_sections(state, &task.id);
    if let Some((section_idx, section)) = sections.iter().enumerate().find(|(idx, section)| {
        *idx == state.inspector_section
            && state.inspector_item.is_some()
            && section.kind == InspectorSectionKind::Prompt
    }) {
        lines.clear();
        lines.push(section_header_focus(theme, &section.title, section.items.len(), false, true));
        let item_context = ItemRenderContext { state, focused, section_idx };
        let prompt = state
            .machine_state(&task.state)
            .and_then(|st| st.instructions.as_deref())
            .map(|prompt| instantiate(prompt, task))
            .unwrap_or_default();
        for (item_idx, raw) in prompt.lines().enumerate() {
            push_item_line(&mut lines, &mut focus_row, &item_context, item_idx, raw, false);
        }
        return (lines, focus_row);
    }

    for (section_idx, section) in sections.iter().enumerate() {
        let section_selected =
            focused && state.inspector_section == section_idx && state.inspector_item.is_none();
        if section_selected {
            focus_row = Some(lines.len());
        }
        let section_open = state.inspector_section == section_idx && state.inspector_item.is_some();
        lines.push(section_header_focus(
            theme,
            &section.title,
            section.items.len(),
            section_selected,
            section_open,
        ));
        let item_context = ItemRenderContext { state, focused, section_idx };

        match section.kind {
            InspectorSectionKind::Dependencies => {
                if section.items.is_empty() {
                    lines.push(dim_line(theme, "   (none)"));
                } else {
                    for (item_idx, chip) in section.items.iter().enumerate() {
                        push_item_line(
                            &mut lines,
                            &mut focus_row,
                            &item_context,
                            item_idx,
                            &chip.label,
                            false,
                        );
                    }
                }
                let waiting = state.unresolved_priors(task);
                if !waiting.is_empty() {
                    lines.push(dim_line(theme, &format!("   waiting on: {}", waiting.join(", "))));
                }
            }
            InspectorSectionKind::PreviousStates => {
                if section.items.is_empty() {
                    lines.push(dim_line(theme, "   (none recorded)"));
                } else {
                    for (item_idx, chip) in section.items.iter().enumerate() {
                        let selected =
                            item_selected(state, focused, item_context.section_idx, item_idx);
                        if selected {
                            focus_row = Some(lines.len());
                        }
                        let mut spans = vec![Span::raw("   ")];
                        spans.extend(state_pill(theme, machine, &chip.label));
                        if selected {
                            for span in &mut spans {
                                span.style = span
                                    .style
                                    .fg(theme.accent())
                                    .add_modifier(Modifier::REVERSED | Modifier::BOLD);
                            }
                        }
                        lines.push(Line::from(spans));
                    }
                }
            }
            InspectorSectionKind::NextState => {
                if section.items.is_empty() {
                    lines.push(dim_line(theme, "   (terminal)"));
                } else {
                    for (item_idx, chip) in section.items.iter().enumerate() {
                        push_item_line(
                            &mut lines,
                            &mut focus_row,
                            &item_context,
                            item_idx,
                            &chip.label,
                            true,
                        );
                    }
                }
            }
            InspectorSectionKind::Prompt => {
                let prompt_lines = state
                    .machine_state(&task.state)
                    .and_then(|st| st.instructions.as_deref())
                    .map(|prompt| instantiate(prompt, task))
                    .unwrap_or_default();
                let visible_lines: Vec<&str> = if section_open {
                    prompt_lines.lines().collect()
                } else {
                    prompt_lines.lines().take(8).collect()
                };
                for (item_idx, raw) in visible_lines.into_iter().enumerate() {
                    push_item_line(&mut lines, &mut focus_row, &item_context, item_idx, raw, false);
                }
                if !section_open && section.items.len() > 8 {
                    lines.push(dim_line(theme, "   … Enter to open full prompt"));
                }
            }
            InspectorSectionKind::LiveAgent => {
                if let Some((slot, slot_state)) = state.running_slot(&task.id) {
                    let elapsed = slot_state.started_at.map(|t| t.elapsed().as_secs()).unwrap_or(0);
                    let (process, color) = if let Some(agent) = &slot_state.agent {
                        (format!("agent {agent}"), theme.live_color())
                    } else {
                        ("program".to_string(), theme.program_color())
                    };
                    let mut meta = vec![Span::styled(
                        format!("   {process} · slot {slot} · {elapsed}s"),
                        Style::default().fg(color),
                    )];
                    if let Some(usage) = &slot_state.usage {
                        let cost = usage
                            .cost_micro
                            .or(usage.priced_cost_micro)
                            .map(format_cost_micro)
                            .unwrap_or_else(|| "—".to_string());
                        meta.push(Span::styled(
                            format!(
                                "  {cost}  in {} out {}",
                                format_tokens(usage.input_total.value.unwrap_or(0)),
                                format_tokens(usage.output_total.value.unwrap_or(0))
                            ),
                            Style::default().fg(theme.dim()),
                        ));
                    }
                    lines.push(Line::from(meta));
                    for traffic in slot_state.traffic.iter().rev().take(12).rev() {
                        let (label, color) = match traffic.stream {
                            AgentStream::Stdout => ("out", theme.dim()),
                            AgentStream::Stderr => {
                                ("err", theme.category_color(super::theme::Category::Blocked))
                            }
                        };
                        lines.push(Line::from(vec![
                            Span::styled(format!("   {label}▏ "), Style::default().fg(color)),
                            Span::raw(truncate_chars(&traffic.text, 200)),
                        ]));
                    }
                }
            }
            InspectorSectionKind::Artifacts => {
                for (item_idx, row) in artifact_rows(state, task).iter().enumerate() {
                    let opt = if row.optional { " (optional)" } else { "" };
                    let from = match &row.from_state {
                        Some(previous) => format!(" (from {previous})"),
                        None => String::new(),
                    };
                    let relative = instantiate(&row.path, task);
                    push_item_line(
                        &mut lines,
                        &mut focus_row,
                        &item_context,
                        item_idx,
                        &format!("{} {} {relative}{opt}{from}", row.marker(), row.name),
                        false,
                    );
                    let (excerpt, truncated) = artifact_excerpt(&state.workspace, &relative);
                    for text in excerpt {
                        lines.push(Line::from(vec![
                            Span::styled("   ▏ ", Style::default().fg(theme.dim())),
                            Span::raw(text),
                        ]));
                    }
                    if truncated {
                        lines.push(dim_line(theme, "   ▏ …"));
                    }
                }
            }
            InspectorSectionKind::Children => {
                let children: Vec<&TaskRow> = state
                    .plan
                    .tasks
                    .iter()
                    .filter(|t| t.parent.as_deref() == Some(task.id.as_str()))
                    .collect();
                for (item_idx, child) in children.iter().enumerate() {
                    let selected = item_selected(state, focused, section_idx, item_idx);
                    if selected {
                        focus_row = Some(lines.len());
                    }
                    let mut spans = vec![Span::raw("   ")];
                    spans.extend(state_pill(theme, machine, &child.state));
                    spans.push(Span::styled(
                        format!("  {} ", child.id),
                        Style::default().add_modifier(Modifier::BOLD),
                    ));
                    spans.push(Span::raw(truncate_chars(&child.title, 36)));
                    if selected {
                        for span in &mut spans {
                            span.style = span
                                .style
                                .fg(theme.accent())
                                .add_modifier(Modifier::REVERSED | Modifier::BOLD);
                        }
                    }
                    lines.push(Line::from(spans));
                }
            }
        }
        lines.push(Line::from(""));
    }

    (lines, focus_row)
}

/// What a state contributes to a detail panel's flags line, in the order both
/// panels print them. The task inspector (§FS-rhei-viz.4) and the state detail
/// (§FS-rhei-viz.6) differ in one flag — only the state detail names the
/// counted-loop budget — and agree on the rest, including the person a poll
/// waits on: the pause color says the node is somebody's turn, and only the
/// label says whose. Built here so neither panel can lose it alone.
pub(super) fn machine_state_flags(st: &MachineState, counted: bool) -> Vec<String> {
    let mut flags = Vec::new();
    if st.initial {
        flags.push("initial".to_string());
    }
    if st.terminal {
        flags.push("terminal".to_string());
    }
    if st.gating {
        flags.push("gating".to_string());
    }
    if counted {
        if let Some(visits) = st.visits {
            flags.push(format!("counted ×{visits}"));
        }
    }
    if let Some(label) = &st.waiting_on {
        flags.push(format!("waiting on {label}"));
    }
    flags
}

pub(super) fn task_flags(state: &UiState, task: &TaskRow) -> String {
    let mut flags = Vec::new();
    if task.depth == 0 {
        flags.push("root task".to_string());
    } else {
        flags.push(format!("depth {}", task.depth));
    }
    if let Some(st) = state.machine_state(&task.state) {
        flags.extend(machine_state_flags(st, false));
    }
    if state.deferred.contains(&task.id) {
        flags.push("deferred".to_string());
    }
    flags.join(" · ")
}

fn readiness_line(state: &UiState, task: &TaskRow) -> Line<'static> {
    let readiness = state.task_ready(task);
    let waiting = state.unresolved_priors(task);
    let mut text = format!("ready: {readiness}");
    if !waiting.is_empty() {
        text.push_str(&format!(" · waiting on {}", waiting.join(", ")));
    }
    Line::from(Span::styled(text, Style::default().fg(state.theme.dim())))
}

/// Resolve the scalar template variables a node can render without guessing.
fn instantiate(template: &str, task: &TaskRow) -> String {
    let mut out = template
        .replace("{task_id}", &task.id)
        .replace("{task_title}", &task.title)
        .replace("{state}", &task.state);
    if let Some(visit) = task.visit_count {
        out = out.replace("{visit_count}", &visit.to_string());
    }
    out
}

/// How many head lines of an artifact the inspector shows in place.
const ARTIFACT_EXCERPT_LINES: usize = 8;
/// Bounded read: the excerpt needs a screenful, never the file.
const ARTIFACT_EXCERPT_BYTES: u64 = 16 * 1024;

/// The head excerpt of a resolved workspace-relative artifact on disk, plus
/// whether the file continues past it. Unresolved, escaping, rooted, or
/// unreadable paths yield nothing — the row stays bare.
///
/// Rooted rather than merely absolute, and asked through the guard the
/// validator and the prompt renderer share: on Windows `is_absolute()` is
/// false for both `/etc/passwd` and `C:out.md`, and an inspector that reads
/// either one is reading outside the run it is inspecting.
// §FS-rhei-run-tui.1.5.3 §FS-rhei-states.1.3
fn artifact_excerpt(workspace: &Path, relative: &str) -> (Vec<String>, bool) {
    if relative.contains('{') || relative.contains("..") || crate::path_is_rooted(relative) {
        return (Vec::new(), false);
    }
    let path = workspace.join(relative);
    let Ok(file) = std::fs::File::open(&path) else {
        return (Vec::new(), false);
    };
    let mut head = String::new();
    if file.take(ARTIFACT_EXCERPT_BYTES).read_to_string(&mut head).is_err() {
        return (Vec::new(), false);
    }
    let excerpt: Vec<String> = head
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(ARTIFACT_EXCERPT_LINES)
        .map(|line| truncate_chars(line, 200))
        .collect();
    let shown = head.lines().filter(|line| !line.trim().is_empty()).count();
    let truncated = shown > ARTIFACT_EXCERPT_LINES || head.len() as u64 == ARTIFACT_EXCERPT_BYTES;
    (excerpt, truncated)
}

fn item_selected(state: &UiState, focused: bool, section_idx: usize, item_idx: usize) -> bool {
    focused && state.inspector_section == section_idx && state.inspector_item == Some(item_idx)
}

fn section_header_focus(
    theme: &super::theme::Theme,
    label: &str,
    item_count: usize,
    selected: bool,
    open: bool,
) -> Line<'static> {
    let marker = if open { "▾" } else { "▸" };
    let count = if item_count > 0 { format!("  {item_count}") } else { String::new() };
    let style = if selected {
        Style::default().fg(theme.accent()).add_modifier(Modifier::BOLD | Modifier::REVERSED)
    } else {
        Style::default().fg(theme.accent()).add_modifier(Modifier::BOLD)
    };
    Line::from(Span::styled(format!("{marker} {label}{count}"), style))
}

struct ItemRenderContext<'a> {
    state: &'a UiState,
    focused: bool,
    section_idx: usize,
}

fn push_item_line(
    lines: &mut Vec<Line<'static>>,
    focus_row: &mut Option<usize>,
    context: &ItemRenderContext,
    item_idx: usize,
    text: &str,
    state_link: bool,
) {
    let selected = item_selected(context.state, context.focused, context.section_idx, item_idx);
    if selected {
        *focus_row = Some(lines.len());
    }
    let mut style = if state_link {
        Style::default().fg(context.state.theme.accent())
    } else {
        Style::default().fg(context.state.theme.dim())
    };
    if selected {
        style = style.add_modifier(Modifier::BOLD | Modifier::REVERSED);
    }
    lines.push(Line::from(vec![Span::raw("   "), Span::styled(text.to_string(), style)]));
}

fn section_header(theme: &super::theme::Theme, label: &str) -> Line<'static> {
    Line::from(Span::styled(
        label.to_string(),
        Style::default().fg(theme.accent()).add_modifier(Modifier::BOLD),
    ))
}

fn dim_line(theme: &super::theme::Theme, text: &str) -> Line<'static> {
    Line::from(Span::styled(text.to_string(), Style::default().fg(theme.dim())))
}

fn border_style(state: &UiState, focused: bool) -> Style {
    if focused {
        Style::default().fg(state.theme.accent())
    } else {
        Style::default().fg(state.theme.dim())
    }
}

fn render_placeholder(f: &mut Frame, area: Rect, state: &UiState, text: &str) {
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

pub(super) fn render_cost(f: &mut Frame, area: Rect, state: &UiState) {
    let block = Block::default()
        .title(format!(" cost — group by {} ", state.cost_group.label()))
        .borders(Borders::ALL);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height < 2 {
        return;
    }

    let theme = &state.theme;
    let col_header = Line::from(Span::styled(
        format!(
            "{:<20} {:>10} {:>9} {:>9} {:>9} {:>9}",
            "key", "cost", "total", "in", "in cache", "out"
        ),
        Style::default().fg(theme.dim()),
    ));

    let rows = cost_rows(state);
    let mut lines: Vec<(bool, Line)> = vec![(false, col_header)];
    for (key, roll, selected) in rows {
        lines.push((selected, cost_row_line(theme, &key, &roll)));
    }
    if lines.len() == 1 {
        // Empty cost table reads like a bug unless it says why. After the run
        // finishes with nothing recorded, the agents simply reported no usage
        // (e.g. mock or non-metered agents); before then, data streams in live.
        let msg = if state.finished {
            "no cost data — agents reported no token usage for this run"
        } else {
            "no accounting yet — usage appears here as agents report it"
        };
        lines.push((false, dim_line(theme, msg)));
    }
    render_list(f, inner, lines, true, theme);
}

fn cost_row_line(theme: &super::theme::Theme, key: &str, roll: &CostRollup) -> Line<'static> {
    let cost = roll.cost_micro.map(format_cost_micro).unwrap_or_else(|| "—".to_string());
    Line::from(vec![
        Span::raw(format!("{:<20} ", truncate_chars(key, 20))),
        Span::raw(format!("{cost:>10} ")),
        Span::styled(
            format!("{:>9} ", format_tokens(roll.total_tokens)),
            Style::default().fg(theme.dim()),
        ),
        Span::styled(
            format!("{:>9} ", format_tokens(roll.input_tokens)),
            Style::default().fg(theme.dim()),
        ),
        Span::styled(
            format!("{:>9} ", format_tokens(roll.input_cached_read_tokens)),
            Style::default().fg(theme.dim()),
        ),
        Span::styled(
            format!("{:>9} ", format_tokens(roll.output_tokens)),
            Style::default().fg(theme.dim()),
        ),
    ])
}

/// Build the cost rows for the active grouping. Per-task rows carry subtree cost
/// and mark the selected task; other groupings aggregate by key.
pub(super) fn cost_rows(state: &UiState) -> Vec<(String, CostRollup, bool)> {
    use super::state::CostGroup;
    match state.cost_group {
        CostGroup::Task => state
            .visible_task_indices()
            .iter()
            .filter_map(|i| {
                let task = &state.plan.tasks[*i];
                let direct = task_direct(&state.invocations, &task.id);
                let subtree = task_subtree(&state.plan, &state.invocations, &task.id);
                if direct.invocations == 0 && subtree.invocations == 0 {
                    return None;
                }
                let roll = if has_children(&state.plan, task) { subtree } else { direct };
                let selected = state.selected.as_deref() == Some(task.id.as_str());
                Some((task.id.clone(), roll, selected))
            })
            .collect(),
        CostGroup::Agent => group_rollup(state, |u| u.agent.clone()),
        CostGroup::Model => {
            group_rollup(state, |u| u.model.clone().unwrap_or_else(|| "—".to_string()))
        }
        CostGroup::State => group_rollup(state, |u| u.state.clone()),
    }
}

fn group_rollup(
    state: &UiState,
    key_of: impl Fn(&crate::rhei_tui::event::UsageSummary) -> String,
) -> Vec<(String, CostRollup, bool)> {
    use std::collections::BTreeMap;
    let mut groups: BTreeMap<String, CostRollup> = BTreeMap::new();
    for rec in &state.invocations {
        groups.entry(key_of(&rec.usage)).or_default().add(&rec.usage);
    }
    groups.into_iter().enumerate().map(|(i, (k, v))| (k, v, i == state.cost_cursor)).collect()
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
