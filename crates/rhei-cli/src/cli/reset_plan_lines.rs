/// Which reading of a file's text applies: the parser reads frontmatter under a plan's
/// header, and only at the very top of a task file, which has no header. It is the
/// one role every reader of a plan's raw text takes. §FS-rhei-plan-language.2.1
type PlanFileKind = rhei_core::header::DocumentRole;

/// Every line of a plan file's `raw` with whether a reset may read it as structure:
/// a task heading, a `**State:**`, `**Assignee:**` or `> **Result:**` line. A fenced
/// line is content, and so is frontmatter, which [`DocumentFence`] holds open where the
/// parser for `kind` reads it: a fence-shaped line in a YAML value opens nothing.
///
/// The one walk reset's text passes share, so they cannot read a line differently
/// from each other or from `rhei validate`. §FS-rhei-reset.2.4 §FS-rhei-plan-language.2.1
///
/// [`DocumentFence`]: rhei_core::header::DocumentFence
fn plan_file_lines(raw: &str, kind: PlanFileKind) -> Vec<(&str, bool)> {
    let mut fence = rhei_core::header::DocumentFence::of_role(kind, raw);
    raw.lines().map(|line| (line, outside_fence(&mut fence, line))).collect()
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
    let mut fence = rhei_core::header::DocumentFence::of_role(kind, &lines.join("\n"));
    let mut heading = None;
    let mut opening = lines.len();
    for (at, line) in lines.iter().enumerate() {
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
/// lines that open and close one are not, nor a frontmatter block's. Call it once per
/// line, in order. §FS-rhei-plan-language.2.1
fn outside_fence(fence: &mut rhei_core::header::DocumentFence, line: &str) -> bool {
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
    let mut fence = rhei_core::header::DocumentFence::body();
    content.lines().any(|line| outside_fence(&mut fence, line) && is_result_link(line))
}
