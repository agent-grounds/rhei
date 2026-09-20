//! Surroundings inspector content and focus rendering.
//! §FS-rhei-run-tui.1.5.2 §FS-rhei-viz.4

use super::derive::{artifact_rows, inspector_sections, InspectorSectionKind};
use super::render::{format_cost_micro, format_tokens, state_pill};
use super::state::{FlowFocus, TaskRow, UiState};
use super::text::truncate_chars;
use super::views::{border_style, dim_line, render_placeholder};
use crate::rhei_tui::event::AgentStream;
use crate::rhei_viz_model::MachineState;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;
use std::io::Read;
use std::path::Path;

pub(super) fn render_inspector(f: &mut Frame, area: Rect, state: &UiState) {
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
