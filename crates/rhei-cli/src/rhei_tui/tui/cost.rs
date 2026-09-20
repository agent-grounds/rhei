//! Cost view and grouping, including the separate lifetime budget.
//! §FS-rhei-budgets.10 §FS-rhei-run-tui.1.5.2

use super::derive::{has_children, task_direct, task_subtree, CostRollup};
use super::render::{format_cost_micro, format_tokens, render_list};
use super::state::UiState;
use super::text::{sanitize_terminal_text, truncate_chars};
use super::views::dim_line;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders};
use ratatui::Frame;

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
    let mut lines: Vec<(bool, Line)> = state
        .budget
        .as_ref()
        .map(|budget| {
            budget
                .detail_lines()
                .into_iter()
                .map(|line| (false, Line::from(sanitize_terminal_text(&line))))
                .collect()
        })
        .unwrap_or_default();
    lines.push((false, col_header));
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
