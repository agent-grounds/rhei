//! The gate block: the brief the dashboard renders, drawn over the run TUI,
//! followed by what the gate takes — the form at a briefed gate, the numbered
//! release choices at a plain one. §FS-rhei-run-tui.1.5.5 §FS-rhei-viz.5.1.1

use std::path::Path;

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::rhei_viz_model::{GateAnswerError, GateBrief, GateFile, GateQuestion};

use super::gate_form::{GateForm, Given, Stage};
use super::input::gate_choices;
use super::render::centered_rect;
use super::state::UiState;
use super::text::{sanitize_terminal_text, truncate_chars};
use super::theme::{Category, Theme};

/// Widest the block grows; wider terminals keep it readable rather than long.
const BLOCK_WIDTH: u16 = 104;

/// Text from a brief, which agents and request files wrote, made safe to print.
fn clean(text: &str) -> String {
    sanitize_terminal_text(text)
}

/// Break `text` into lines of at most `width` characters at spaces.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(8);
    let mut out = Vec::new();
    for paragraph in clean(text).lines() {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
                out.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(&truncate_chars(word, width));
        }
        out.push(line);
    }
    out
}

fn dim(theme: &Theme) -> Style {
    Style::default().fg(theme.dim())
}

fn error_line(theme: &Theme, indent: &str, message: &str) -> Line<'static> {
    let style = Style::default().fg(theme.category_color(Category::Failed));
    Line::from(Span::styled(format!("{indent}✗ {}", clean(message)), style))
}

fn file_line(theme: &Theme, label: &str, file: &GateFile) -> Line<'static> {
    let status = match &file.problem {
        None => Span::styled("  readable".to_string(), dim(theme)),
        Some(problem) => Span::styled(
            format!("  unreadable: {}", clean(problem)),
            Style::default().fg(theme.category_color(Category::Failed)),
        ),
    };
    Line::from(vec![
        Span::styled(format!("{label:<9}"), dim(theme)),
        Span::raw(clean(&file.path)),
        status,
    ])
}

/// The brief down to its questions: title, ticket and visit, summary, the
/// display file with a bounded head excerpt, and the attachments. The excerpt
/// follows the inspector's artifact rule, read against the ticket's root.
/// §FS-rhei-viz.5.1.1 §FS-rhei-run-tui.1.5.3
fn brief_head(theme: &Theme, brief: &GateBrief, width: usize) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(Span::styled(
        clean(&brief.title),
        Style::default().add_modifier(Modifier::BOLD),
    ))];
    let since = brief.since.as_deref().unwrap_or("unknown");
    lines.push(Line::from(Span::styled(
        clean(&format!(
            "{} · {} — gate {} · visit {} · since {since}",
            brief.ticket, brief.ticket_title, brief.state, brief.visit
        )),
        dim(theme),
    )));
    if let Some(summary) = brief.summary.as_deref().filter(|text| !text.trim().is_empty()) {
        lines.push(Line::default());
        lines.extend(wrap(summary, width).into_iter().map(Line::from));
    }
    if brief.display.is_some() || !brief.attachments.is_empty() {
        lines.push(Line::default());
    }
    if let Some(display) = &brief.display {
        lines.push(file_line(theme, "Read", display));
        if display.readable {
            let (excerpt, truncated) =
                super::views::artifact_excerpt(Path::new(&brief.root), &display.path);
            for line in excerpt {
                lines.push(Line::from(Span::styled(format!("  │ {}", clean(&line)), dim(theme))));
            }
            if truncated {
                lines.push(Line::from(Span::styled("  │ …".to_string(), dim(theme))));
            }
        }
    }
    for attachment in &brief.attachments {
        lines.push(file_line(theme, "Attached", attachment));
    }
    lines
}

/// Where the gate leads and how answers pick it, and a degraded brief's
/// problems. §FS-rhei-viz.5.1.1
fn brief_tail(theme: &Theme, brief: &GateBrief, width: usize) -> Vec<Line<'static>> {
    let mut lines = vec![Line::default()];
    let destinations: Vec<String> = brief
        .destinations
        .iter()
        .map(|destination| {
            let mut text = destination.state.clone();
            if destination.terminal {
                text.push_str(" (final)");
            }
            if let Some(condition) = &destination.condition {
                text.push_str(&format!(" when {condition}"));
            }
            if let Some(description) = &destination.description {
                text.push_str(&format!(" — {description}"));
            }
            text
        })
        .collect();
    lines.extend(labelled(theme, "Leads to ", &destinations.join(" · "), width));
    if !brief.routes.is_empty() {
        let routes: Vec<String> = brief
            .routes
            .iter()
            .map(|route| {
                let when: Vec<String> =
                    route.when.iter().map(|(id, value)| format!("{id} = {value}")).collect();
                let when = if when.is_empty() { "always".to_string() } else { when.join(", ") };
                format!("{} when {when}", route.to)
            })
            .collect();
        lines.extend(labelled(theme, "Routes   ", &routes.join(" · "), width));
    }
    for problem in &brief.problems {
        lines.push(error_line(theme, "", &format!("degraded: {problem}")));
    }
    lines
}

/// `text` wrapped under a dim `label`, its later lines indented to match.
fn labelled(theme: &Theme, label: &'static str, text: &str, width: usize) -> Vec<Line<'static>> {
    let indent = label.chars().count();
    wrap(text, width.saturating_sub(indent))
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            let head = if index == 0 {
                Span::styled(label, dim(theme))
            } else {
                Span::raw(" ".repeat(indent))
            };
            Line::from(vec![head, Span::raw(line)])
        })
        .collect()
}

/// What the operator has picked for one option: a radio or a checkbox mark.
fn mark(kind: &str, picked: bool) -> &'static str {
    match (kind, picked) {
        ("multi", true) => "[x]",
        ("multi", false) => "[ ]",
        (_, true) => "(•)",
        (_, false) => "( )",
    }
}

/// How the form draws one question.
struct QuestionView<'a> {
    question: &'a GateQuestion,
    number: usize,
    /// `None` on a plain gate, which lists its questions without controls.
    given: Option<&'a Option<Given>>,
    focused: bool,
    cursor: usize,
    errors: Vec<&'a GateAnswerError>,
}

/// One question: its kind, required mark, prompt and help, then its control.
/// §FS-rhei-viz.5.1.1 §FS-rhei-run-tui.1.5.5
fn question_lines(theme: &Theme, view: &QuestionView<'_>, width: usize) -> Vec<Line<'static>> {
    let question = view.question;
    let required = if question.required { "required" } else { "optional" };
    let marker = if view.focused { "▸" } else { " " };
    let head_style = if view.focused {
        Style::default().fg(theme.accent()).add_modifier(Modifier::BOLD)
    } else {
        Style::default().add_modifier(Modifier::BOLD)
    };
    let mut lines = vec![Line::from(vec![
        Span::styled(format!("{marker} {}. {}", view.number, clean(&question.id)), head_style),
        Span::styled(format!("  {} · {required}", clean(&question.kind)), dim(theme)),
    ])];
    lines.extend(
        wrap(&question.prompt, width.saturating_sub(5))
            .into_iter()
            .map(|line| Line::from(format!("     {line}"))),
    );
    if let Some(help) = &question.help {
        for line in wrap(help, width.saturating_sub(5)) {
            lines.push(Line::from(Span::styled(format!("     {line}"), dim(theme))));
        }
    }
    let picked_style = Style::default().fg(theme.accent()).add_modifier(Modifier::BOLD);
    match (question.kind.as_str(), view.given) {
        ("boolean", Some(given)) => {
            let yes = matches!(given, Some(Given::Yes(true)));
            let no = matches!(given, Some(Given::Yes(false)));
            let pill = |label: &str, on: bool| {
                if on {
                    Span::styled(format!("[{label}]"), picked_style)
                } else {
                    Span::styled(format!(" {label} "), dim(theme))
                }
            };
            lines.push(Line::from(vec![
                Span::raw("     "),
                pill("yes", yes),
                Span::raw("  "),
                pill("no", no),
            ]));
        }
        ("choice" | "multi", given) => {
            for (index, option) in question.options().iter().enumerate() {
                let picked = match given {
                    Some(Some(Given::One(value))) => *value == option.value,
                    Some(Some(Given::Many(values))) => values.contains(&option.value),
                    _ => false,
                };
                let at = view.focused && given.is_some() && view.cursor == index;
                let lead = if given.is_some() {
                    format!(
                        "   {} {} {} ",
                        if at { "›" } else { " " },
                        mark(&question.kind, picked),
                        index + 1
                    )
                } else {
                    "     - ".to_string()
                };
                let mut text = clean(option.shown());
                if option.shown() != option.value {
                    text.push_str(&format!(" ({})", clean(&option.value)));
                }
                let style = if picked { picked_style } else { Style::default() };
                let mut spans = vec![Span::raw(lead), Span::styled(text, style)];
                if let Some(description) = &option.description {
                    spans.push(Span::styled(format!(" — {}", clean(description)), dim(theme)));
                }
                lines.push(Line::from(spans));
            }
        }
        (_, Some(given)) => {
            let text = match given {
                Some(Given::Text(text)) => clean(text),
                _ => String::new(),
            };
            let mut spans = vec![Span::styled("     > ".to_string(), dim(theme)), Span::raw(text)];
            if view.focused {
                spans.push(Span::styled("▏", Style::default().fg(theme.accent())));
            }
            lines.push(Line::from(spans));
        }
        _ => {}
    }
    for error in &view.errors {
        lines.push(error_line(theme, "     ", &error.message));
    }
    lines
}

/// The hint line under the form: what the focused question takes, or the
/// destination the dry run reported. §FS-rhei-run-tui.1.5.5
fn form_footer(theme: &Theme, form: &GateForm) -> Line<'static> {
    if let Stage::Confirm { to } = &form.stage {
        return Line::from(vec![
            Span::styled(
                clean(&format!("this moves {} to {to}", form.brief.ticket)),
                Style::default().fg(theme.accent()).add_modifier(Modifier::BOLD),
            ),
            Span::styled("   Enter answer · Esc back to the form", dim(theme)),
        ]);
    }
    let keys = match form.brief.questions.get(form.focus).map(|question| question.kind.as_str()) {
        Some("boolean") => "y/n or Space",
        Some("choice") => "←/→ or digit picks",
        Some("multi") => "←/→ moves · Space or digit toggles",
        _ => "type · Backspace",
    };
    Line::from(Span::styled(
        format!("Enter dry run · ↑/↓ Tab question · {keys} · Del clear · Esc close"),
        dim(theme),
    ))
}

/// Draw `lines` in a centered block, keeping line `focus` in view, with the
/// `footer` lines pinned under them.
fn draw_block(
    f: &mut Frame,
    area: Rect,
    theme: &Theme,
    title: &str,
    lines: Vec<Line<'static>>,
    focus: usize,
    footer: Vec<Line<'static>>,
) {
    let wanted = (lines.len() + footer.len() + 2).min(u16::MAX as usize) as u16;
    let popup = centered_rect(
        area,
        BLOCK_WIDTH.min(area.width.saturating_sub(4)),
        wanted.min(area.height.saturating_sub(2)),
    );
    f.render_widget(Clear, popup);
    let block = Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent()));
    let inner = block.inner(popup);
    f.render_widget(block, popup);
    let footer_height = (footer.len() as u16).min(inner.height);
    let body = Rect { height: inner.height - footer_height, ..inner };
    let height = body.height as usize;
    // Keep the focused question and a few lines of its control on screen.
    let offset = (focus + 4).saturating_sub(height).min(lines.len().saturating_sub(height));
    f.render_widget(Paragraph::new(lines).scroll((offset as u16, 0)), body);
    let foot = Rect { y: body.y + body.height, height: footer_height, ..inner };
    f.render_widget(Paragraph::new(footer), foot);
}

fn inner_width(area: Rect) -> usize {
    BLOCK_WIDTH.min(area.width.saturating_sub(4)).saturating_sub(2) as usize
}

/// The block at a briefed gate: the brief, then one control per question.
/// §FS-rhei-viz.5.1.2 §FS-rhei-run-tui.1.5.5
pub(super) fn render_form(f: &mut Frame, area: Rect, state: &UiState, form: &GateForm) {
    let theme = &state.theme;
    let width = inner_width(area);
    let mut lines = brief_head(theme, &form.brief, width);
    lines.push(Line::default());
    lines.push(Line::from(Span::styled("Questions".to_string(), dim(theme))));
    let mut focus = 0;
    for (index, question) in form.brief.questions.iter().enumerate() {
        if index == form.focus {
            focus = lines.len();
        }
        let view = QuestionView {
            question,
            number: index + 1,
            given: Some(&form.given[index]),
            focused: index == form.focus && form.stage == Stage::Editing,
            cursor: form.cursor[index],
            errors: form
                .errors
                .iter()
                .filter(|error| error.question.as_deref() == Some(&question.id))
                .collect(),
        };
        lines.extend(question_lines(theme, &view, width));
    }
    lines.extend(brief_tail(theme, &form.brief, width));
    let asked: Vec<&str> =
        form.brief.questions.iter().map(|question| question.id.as_str()).collect();
    let mut footer: Vec<Line<'static>> = form
        .errors
        .iter()
        .filter(|error| error.question.as_deref().is_none_or(|id| !asked.contains(&id)))
        .map(|error| error_line(theme, "", &format!("{}: {}", error.kind, error.message)))
        .collect();
    footer.push(form_footer(theme, form));
    draw_block(
        f,
        area,
        theme,
        &format!("human gate — {}", clean(&form.task)),
        lines,
        focus,
        footer,
    );
}

/// The block at a plain gate: the brief, its synthesized questions listed,
/// and the numbered release choices. Without a brief it is the choices alone.
/// §FS-rhei-viz.5.1.3 §FS-rhei-run-tui.1.5.5
pub(super) fn render_release(f: &mut Frame, area: Rect, state: &UiState) {
    let theme = &state.theme;
    let task = state.selected.clone().unwrap_or_default();
    let width = inner_width(area);
    let mut lines = Vec::new();
    if let Some(brief) = state.plan.gates.get(&task) {
        lines = brief_head(theme, brief, width);
        lines.push(Line::default());
        lines.push(Line::from(Span::styled("Questions".to_string(), dim(theme))));
        for (index, question) in brief.questions.iter().enumerate() {
            let view = QuestionView {
                question,
                number: index + 1,
                given: None,
                focused: false,
                cursor: 0,
                errors: Vec::new(),
            };
            lines.extend(question_lines(theme, &view, width));
        }
        lines.extend(brief_tail(theme, brief, width));
        lines.push(Line::default());
    }
    lines.push(Line::from(Span::styled("Release".to_string(), dim(theme))));
    for (index, (from, to)) in gate_choices(state).iter().enumerate() {
        lines.push(Line::from(format!("  {}  {from} → {to}", index + 1)));
    }
    let focus = lines.len().saturating_sub(1);
    let footer =
        vec![Line::from(Span::styled("digit to choose · Esc cancel".to_string(), dim(theme)))];
    draw_block(f, area, theme, &format!("human gate — {}", clean(&task)), lines, focus, footer);
}
