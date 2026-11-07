/// Which reading of a file's text applies: the parser reads frontmatter under a plan's
/// header, and only at the very top of a task file, which has no header. It is the
/// one role every reader of a plan's raw text takes. §FS-rhei-plan-language.2.1
type PlanFileKind = rhei_core::header::DocumentRole;

/// What `path` is to the loader. A plan document is named `*.rhei.md`; a task file may
/// be named anything, that included, once it sits in a directory the loader reads task
/// files from. §FS-rhei-plan-language.1.4 §FS-rhei-panta.2
fn plan_file_kind(path: &Path) -> PlanFileKind {
    let named_as_a_plan = path.file_name().and_then(|name| name.to_str()).is_some_and(|name| name.ends_with(".rhei.md"));
    let in_task_directory = path.ancestors().skip(1).any(|dir| {
        let Some(parent) = dir.parent() else { return false };
        match dir.file_name().and_then(|name| name.to_str()) {
            Some("tasks") => rhei_core::workspace::is_workspace(parent),
            Some(rhei_core::workspace::BASIN_RHEI_ID) => rhei_core::workspace::is_panta_project(parent),
            _ => false,
        }
    });
    if named_as_a_plan && !in_task_directory {
        PlanFileKind::Plan
    } else {
        PlanFileKind::TaskFile
    }
}

/// The line range the parser reads as frontmatter in a file of this `kind`, empty when
/// it has none. An unclosed block runs to the end of the text.
/// §FS-rhei-reset.2.4 §FS-rhei-plan-language.2.3
fn plan_file_frontmatter(lines: &[&str], kind: PlanFileKind) -> std::ops::Range<usize> {
    let block = match kind {
        PlanFileKind::TaskFile => rhei_core::header::find_leading_frontmatter(lines.iter().copied()),
        PlanFileKind::Plan => {
            rhei_core::header::find_frontmatter(lines.iter().copied()).and_then(|place| place.block)
        }
    };
    block.map_or(0..0, |block| block.open..block.close.map_or(lines.len(), |close| close + 1))
}

/// Every line of a plan file's `raw` with whether a reset may read it as structure:
/// a task heading, a `**State:**`, `**Assignee:**` or `> **Result:**` line. A fenced
/// line is content, and so is frontmatter, which the tracker never reads because the
/// parser does not: a fence-shaped line in a YAML value opens nothing.
///
/// The one walk reset's text passes share, so they cannot read a line differently
/// from each other or from `rhei validate`. §FS-rhei-reset.2.4 §FS-rhei-plan-language.2.1
fn plan_file_lines(raw: &str, kind: PlanFileKind) -> Vec<(&str, bool)> {
    let lines: Vec<&str> = raw.lines().collect();
    let frontmatter = plan_file_frontmatter(&lines, kind);
    let mut fence = rhei_core::fence::FenceTracker::default();
    lines
        .iter()
        .enumerate()
        .map(|(at, line)| (*line, !frontmatter.contains(&at) && outside_fence(&mut fence, line)))
        .collect()
}

/// `lines` cut at the opening of the fence task `task_id`'s text ends inside: the lines
/// a completion may read and write, and the fence that stays below them as written. The
/// second part is empty for a task whose text ends outside any fence.
/// §FS-rhei-complete.3.3 §FS-rhei-plan-language.2.1
fn split_at_a_tasks_final_fence<'a, 'b>(
    lines: &'b [&'a str],
    kind: PlanFileKind,
    task_id: &str,
) -> (&'b [&'a str], &'b [&'a str]) {
    let frontmatter = plan_file_frontmatter(lines, kind);
    let mut fence = rhei_core::fence::FenceTracker::default();
    let mut heading = None;
    let mut opening = lines.len();
    for (at, line) in lines.iter().enumerate().filter(|(at, _)| !frontmatter.contains(at)) {
        if fence.read(line) {
            opening = at;
        } else if !fence.is_open() {
            heading = task_heading_id(line).or(heading);
        }
    }
    if fence.is_open() && heading == Some(task_id) {
        lines.split_at(opening)
    } else {
        (lines, &[])
    }
}

/// Add a completion's result link below `lines`, one blank line above it. Above a fence
/// it keeps the blank line that stood there, so a reset gives the text back.
/// §FS-rhei-complete.3.3
fn append_result_link(lines: &mut Vec<String>, link: String, above_a_fence: bool) {
    let blank_above = lines.last().is_some_and(|line| line.trim().is_empty());
    trim_trailing_blank_lines(lines);
    lines.push(String::new());
    lines.push(link);
    if above_a_fence && blank_above {
        lines.push(String::new());
    }
}

/// Read one line by the fence rule, and say whether it is outside any fenced block: the
/// lines that open and close one are not. Call it once per line, in order.
/// §FS-rhei-plan-language.2.1
fn outside_fence(fence: &mut rhei_core::fence::FenceTracker, line: &str) -> bool {
    !fence.read(line) && !fence.is_open()
}

/// Whether `line`, read as structure, is a result link. §FS-rhei-reset.2.4
fn is_result_link(line: &str) -> bool {
    line.trim_start().starts_with("> **Result:**")
}

/// Whether a task's parsed body holds a result link, as opposed to quoting one. A body
/// has no frontmatter, and a fence cannot span two tasks, so it is read from its first
/// line. §FS-rhei-reset.2.4
fn has_result_link(content: &str) -> bool {
    let mut fence = rhei_core::fence::FenceTracker::default();
    content.lines().any(|line| outside_fence(&mut fence, line) && is_result_link(line))
}
