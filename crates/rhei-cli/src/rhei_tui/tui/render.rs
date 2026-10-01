//! The Flow surface frame: shared chrome (header, tab bar, links strip, action
//! bar), responsive layout, the active view, and the modal overlays.
//! §FS-rhei-run-tui.1.5.1 §FS-rhei-run-tui.1.5.6

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use super::derive::run_rollup;
use super::input::{gate_choices, intervene_available};
use super::state::{ComposerKind, UiState, View};
use super::text::truncate_chars;
use super::theme::{category, category_glyph, Category, Theme};
use super::views;

/// Format a micro-dollar amount as `$d.cc`.
pub(super) fn format_cost_micro(value: u64) -> String {
    let cents = (value + 5_000) / 10_000;
    format!("${}.{:02}", cents / 100, cents % 100)
}

/// Format a token count compactly (`1.2k`, `3.4M`).
pub(super) fn format_tokens(value: u64) -> String {
    if value >= 1_000_000 {
        format!("{:.1}M", value as f64 / 1_000_000.0)
    } else if value >= 1_000 {
        format!("{:.1}k", value as f64 / 1_000.0)
    } else {
        value.to_string()
    }
}

/// The category glyph + colored state label, the shared "state pill" every view
/// reuses so a state reads identically across surfaces (§FS-rhei-viz-ux.3.2).
pub(super) fn state_pill(
    theme: &Theme,
    machine: &super::state::Machine,
    state: &str,
) -> Vec<Span<'static>> {
    let cat = category(machine, state);
    let color = theme.category_color(cat);
    vec![
        Span::styled(format!("{} ", category_glyph(cat)), Style::default().fg(color)),
        Span::styled(state.to_string(), Style::default().fg(color)),
    ]
}

pub(super) fn draw(f: &mut Frame, state: &UiState) {
    let area = f.size();
    if area.width < 24 || area.height < 6 {
        let msg = Paragraph::new("rhei run — terminal too small")
            .style(Style::default().fg(Color::Yellow));
        f.render_widget(msg, area);
        return;
    }

    // Minimal: below the room for two regions, collapse to a compact list +
    // links strip (§1.5.6).
    let minimal = area.width < 60 || area.height < 14;

    let header_height = header_height(state);
    let links_strip = if minimal && area.height < 18 { 0 } else { links_height(state) };
    let action_height = 1u16;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_height),
            Constraint::Length(1), // tab bar
            Constraint::Min(3),    // body
            Constraint::Length(links_strip),
            Constraint::Length(action_height),
        ])
        .split(area);

    render_header(f, chunks[0], state);
    render_tab_bar(f, chunks[1], state);
    if minimal {
        views::render_minimal(f, chunks[2], state);
    } else {
        render_body(f, chunks[2], state);
    }
    if links_strip > 0 {
        render_links_strip(f, chunks[3], state);
    }
    render_action_bar(f, chunks[4], state);

    // Modal overlays draw last so they sit on top.
    if state.help {
        render_help(f, area, state);
    }
    if let Some(composer) = &state.composer {
        render_composer(f, area, state, composer);
    }
    if state.gate_active {
        render_gate(f, area, state);
    }
}

fn header_height(state: &UiState) -> u16 {
    let mut h = 2; // title line + counts line
    if has_accounting(state) {
        h += 1;
    }
    h
}

fn has_accounting(state: &UiState) -> bool {
    !state.invocations.is_empty() || state.accounting.is_some()
}

fn render_header(f: &mut Frame, area: Rect, state: &UiState) {
    let theme = &state.theme;
    let mut lines: Vec<Line> = Vec::new();

    let title = state.plan.plan_title.clone().unwrap_or_else(|| "rhei run".to_string());
    let plan_state = derived_plan_state(state);
    let mut title_spans = vec![
        Span::styled(truncate_chars(&title, 48), Style::default().add_modifier(Modifier::BOLD)),
        Span::raw("  "),
    ];
    title_spans.extend(state_pill(theme, &state.plan.machine, &plan_state));
    // Say plainly which kind of surface this is: an attached one cannot stop
    // the run, and the operator must not have to discover that by pressing a
    // key. §FS-rhei-run-headless.5.1
    if state.attached {
        title_spans.push(Span::styled("  [attached]", Style::default().fg(theme.accent())));
    }
    if state.finished {
        let note = if state.attached {
            "  [run ended — q to leave]"
        } else {
            "  [finished — q to quit]"
        };
        title_spans.push(Span::styled(note, Style::default().fg(theme.dim())));
    }
    lines.push(Line::from(title_spans));

    lines.push(category_counts_line(state));

    if has_accounting(state) {
        lines.push(cost_strip_line(state));
    }

    let block = Block::default().borders(Borders::BOTTOM);
    f.render_widget(Paragraph::new(lines).block(block), area);
}

/// Strip line with category counts over the total plus the running count
/// (§FS-rhei-viz.1.2). Computed over top-level tasks.
fn category_counts_line(state: &UiState) -> Line<'static> {
    let theme = &state.theme;
    let roots: Vec<&super::state::TaskRow> =
        state.plan.tasks.iter().filter(|t| t.depth == 0).collect();
    let total = roots.len();
    let count = |cat: Category| {
        roots.iter().filter(|t| category(&state.plan.machine, &t.state) == cat).count()
    };
    // Running reflects actual slot activity, not just top-level tasks: a live
    // nested subtask occupies a slot and must be counted, or the indicator reads
    // `0 running` while work is visibly underway (§FS-rhei-viz.1.2).
    let running = state.slots.iter().filter(|s| s.active).count();

    let mut spans =
        vec![Span::styled(format!("{total} tasks  "), Style::default().fg(theme.dim()))];
    for (label, cat) in [
        ("active", Category::Active),
        ("blocked", Category::Blocked),
        ("gate", Category::Gate),
        ("done", Category::Done),
        ("failed", Category::Failed),
    ] {
        let color = theme.category_color(cat);
        spans.push(Span::styled(
            format!("{}{} ", category_glyph(cat), count(cat)),
            Style::default().fg(color),
        ));
        spans.push(Span::styled(format!("{label}  "), Style::default().fg(theme.dim())));
    }
    spans.push(Span::styled(
        format!("│ {running} running"),
        Style::default().fg(theme.live_color()),
    ));
    Line::from(spans)
}

fn cost_strip_line(state: &UiState) -> Line<'static> {
    let theme = &state.theme;
    let roll = run_rollup(&state.invocations);
    let mut spans = vec![Span::styled("cost ", Style::default().fg(theme.dim()))];
    let cost = match roll.cost_micro {
        Some(c) => format_cost_micro(c),
        None => "—".to_string(),
    };
    spans.push(Span::styled(cost, Style::default().fg(theme.accent())));
    spans.push(Span::styled(
        format!(
            "  total {}  in {}  in_cached {}  out {}",
            format_tokens(roll.total_tokens),
            format_tokens(roll.input_tokens),
            format_tokens(roll.input_cached_read_tokens),
            format_tokens(roll.output_tokens)
        ),
        Style::default().fg(theme.dim()),
    ));
    Line::from(spans)
}

/// The derived plan state, promoted to `active` when a top-level task is running
/// (the runtime signal the pure derivation cannot see). §FS-rhei-viz.9
fn derived_plan_state(state: &UiState) -> String {
    let base = state.plan.plan_state.clone().unwrap_or_else(|| "draft".to_string());
    let any_root_live = state.plan.tasks.iter().any(|t| t.depth == 0 && state.is_live(&t.id));
    if any_root_live {
        "active".to_string()
    } else {
        base
    }
}

fn render_tab_bar(f: &mut Frame, area: Rect, state: &UiState) {
    let theme = &state.theme;
    let mut spans = Vec::new();
    for (i, view) in View::ORDER.iter().enumerate() {
        let active = *view == state.view;
        let label = format!(" {} {} ", i + 1, view.label());
        let style = if active {
            Style::default().fg(theme.accent()).add_modifier(Modifier::BOLD | Modifier::REVERSED)
        } else {
            Style::default().fg(theme.dim())
        };
        spans.push(Span::styled(label, style));
        spans.push(Span::raw(" "));
    }
    if let Some(filter) = &state.filter {
        spans.push(Span::styled(format!("  /{filter}"), Style::default().fg(theme.accent())));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_body(f: &mut Frame, area: Rect, state: &UiState) {
    match state.view {
        View::Flow => views::render_flow(f, area, state),
        View::Machine => views::render_machine(f, area, state),
        View::Cost => views::render_cost(f, area, state),
        View::Journal => views::render_journal(f, area, state),
    }
}

fn render_links_strip(f: &mut Frame, area: Rect, state: &UiState) {
    let block = Block::default().title(" links ").borders(Borders::ALL);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 {
        return;
    }
    let mut lines: Vec<Line> = Vec::new();
    if let Some(url) = &state.dashboard_url {
        lines.push(Line::from(format!("Dashboard  {url}")));
    }
    lines.push(Line::from(format!("Workspace  {}", state.workspace.display())));
    for link in &state.links {
        if Some(&link.url) == state.dashboard_url.as_ref() {
            continue;
        }
        lines.push(Line::from(format!("{}  {}", link.label, link.url)));
    }
    lines.truncate(inner.height as usize);
    f.render_widget(Paragraph::new(lines), inner);
}

fn links_height(state: &UiState) -> u16 {
    let mut rows = 1; // workspace
    if state.dashboard_url.is_some() {
        rows += 1;
    }
    rows +=
        state.links.iter().filter(|link| Some(&link.url) != state.dashboard_url.as_ref()).count();
    (rows as u16 + 2).min(6)
}

fn render_action_bar(f: &mut Frame, area: Rect, state: &UiState) {
    let theme = &state.theme;
    if state.filter_editing {
        let filter = state.filter.clone().unwrap_or_default();
        let line = Line::from(vec![
            Span::styled("filter: ", Style::default().fg(theme.accent())),
            Span::raw(filter),
            Span::styled("▏", Style::default().fg(theme.accent())),
            Span::styled("  (Enter apply · Esc clear)", Style::default().fg(theme.dim())),
        ]);
        f.render_widget(Paragraph::new(line), area);
        return;
    }

    let mut hints: Vec<String> = Vec::new();
    if intervene_available(state) {
        hints.push("m message agent".to_string());
    }
    if !gate_choices(state).is_empty() && !state.finished && state.gate.is_some() {
        hints.push("⏎ gate".to_string());
    }
    match state.view {
        View::Cost => hints.push(format!("g group:{}", state.cost_group.label())),
        View::Journal => hints.push(format!("f {}", state.journal_filter.label())),
        View::Flow => {
            hints.push("Tab focus".to_string());
            if state.inspector_item.is_some() {
                hints.push("Esc back to sections".to_string());
            }
        }
        View::Machine => {}
    }
    hints.push("/ filter".to_string());
    hints.push("? help".to_string());
    // Make the stop/quit affordance visible at all times: during a live run the
    // operator stops with Ctrl+C; once finished, `q` exits. An attached surface
    // offers only what it can actually do: detaching — the run is stopped by
    // name, from a shell.

    // §FS-rhei-run-tui.1.5.2 §FS-rhei-run-headless.5.1
    hints.push(if state.attached {
        "q/^C detach".to_string()
    } else if state.finished {
        "q quit".to_string()
    } else {
        "^C stop".to_string()
    });

    let line = Line::from(vec![Span::styled(hints.join("   "), Style::default().fg(theme.dim()))]);
    f.render_widget(Paragraph::new(line), area);
}

fn render_help(f: &mut Frame, area: Rect, state: &UiState) {
    let (quit_key, stop_key) = if state.attached {
        ("  q           detach from this run (it keeps running)", "  Ctrl+C      detach")
    } else {
        ("  q           quit (after the run finishes)", "  Ctrl+C      stop the run")
    };
    let mut lines = vec![
        Line::from("Keys"),
        Line::from("  j/k ↓/↑     move focus in the active view"),
        Line::from("  1–4         Flow · Machine · Cost · Journal"),
        Line::from("  h/l ←/→     previous / next view"),
        Line::from("  Tab         (Flow) outline ⇄ inspector"),
        Line::from("  Enter       open inspector section / activate item / gate / expand fold"),
        Line::from("  Esc         return from inspector item view to section headers"),
        Line::from("  PgUp/PgDn   scroll the focused pane"),
        Line::from("  /           filter the active view (Esc clears)"),
        Line::from("  g           (Cost) cycle grouping"),
        Line::from("  f           (Journal) cycle severity filter"),
        Line::from("  m           send a message to the selected running agent"),
        Line::from("  ?           toggle this help"),
        Line::from(quit_key),
        Line::from(stop_key),
    ];
    if state.attached {
        lines.push(Line::from("  stop it with: rhei stop <id>"));
    }
    let popup = centered_rect(area, 56, lines.len() as u16 + 2);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .title(" help ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(state.theme.accent()));
    f.render_widget(Paragraph::new(lines).block(block), popup);
}

fn render_composer(f: &mut Frame, area: Rect, state: &UiState, composer: &super::state::Composer) {
    let theme = &state.theme;
    let popup = bottom_rect(area, 3);
    f.render_widget(Clear, popup);
    // The gate composer says what it is collecting and, on a terminal edge,
    // that an empty line will be refused — before Enter, not after.
    // §FS-rhei-run-tui.1.5.5 §FS-rhei-states.3.3
    let (title, hint) = match &composer.kind {
        ComposerKind::Intervene => {
            (format!(" intervene → {} ", composer.task), "   (Enter send · Esc cancel)".to_string())
        }
        ComposerKind::GateResult { from, to, terminal } => (
            format!(" gate result · {} {from}→{to} ", composer.task),
            if *terminal {
                format!(
                    "   (Enter release · '{to}' is final, so a blank result is refused unless \
                     the ticket already has one · Esc cancel)"
                )
            } else {
                "   (Enter release · empty sends no result · Esc cancel)".to_string()
            },
        ),
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent()));
    let line = Line::from(vec![
        Span::raw(composer.input.clone()),
        Span::styled("▏", Style::default().fg(theme.accent())),
        Span::styled(hint, Style::default().fg(theme.dim())),
    ]);
    f.render_widget(Paragraph::new(line).block(block), popup);
}

fn render_gate(f: &mut Frame, area: Rect, state: &UiState) {
    let theme = &state.theme;
    let choices = gate_choices(state);
    let task = state.selected.clone().unwrap_or_default();
    let mut lines = vec![Line::from(Span::styled(
        format!("human gate — {task}"),
        Style::default().add_modifier(Modifier::BOLD),
    ))];
    for (i, (from, to)) in choices.iter().enumerate() {
        lines.push(Line::from(format!("  {}  {from} → {to}", i + 1)));
    }
    lines.push(Line::from(Span::styled(
        "  digit to choose · Esc cancel",
        Style::default().fg(theme.dim()),
    )));
    let popup = centered_rect(area, 48, lines.len() as u16 + 2);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .title(" gate ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent()));
    f.render_widget(Paragraph::new(lines).block(block), popup);
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect { x, y, width, height }
}

fn bottom_rect(area: Rect, height: u16) -> Rect {
    let height = height.min(area.height);
    let width = area.width.min(area.width.saturating_sub(2)).max(1);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + area.height.saturating_sub(height);
    Rect { x, y, width, height }
}

/// Shared helper: render a list of (selected, spans) rows into a paragraph,
/// keeping the selected row visible by windowing.
pub(super) fn render_list(
    f: &mut Frame,
    area: Rect,
    rows: Vec<(bool, Line<'static>)>,
    focused: bool,
    theme: &Theme,
) {
    if area.height == 0 {
        return;
    }
    let height = area.height as usize;
    let selected_idx = rows.iter().position(|(sel, _)| *sel).unwrap_or(0);
    let offset = selected_idx.saturating_sub(height.saturating_sub(1));
    let mut lines: Vec<Line> = Vec::new();
    for (selected, line) in rows.iter().skip(offset).take(height) {
        let marker = if *selected {
            Span::styled(
                if focused { "▌" } else { "│" },
                Style::default().fg(theme.accent()).add_modifier(Modifier::BOLD),
            )
        } else {
            Span::raw(" ")
        };
        let mut spans = vec![marker];
        spans.extend(line.spans.iter().cloned());
        lines.push(Line::from(spans));
    }
    f.render_widget(Paragraph::new(lines).alignment(Alignment::Left), area);
}
