/// Every line of a plan file's `raw` with whether a reset may read it as structure:
/// a task heading, a `**State:**`, `**Assignee:**` or `> **Result:**` line. A fenced
/// line is content, and so is frontmatter, which the tracker never reads because the
/// parser does not: a fence-shaped line in a YAML value opens nothing.
///
/// The one walk reset's text passes share, so they cannot read a line differently
/// from each other or from `rhei validate`. §FS-rhei-reset.2.4 §FS-rhei-plan-language.2.1
fn plan_file_lines(raw: &str) -> Vec<(&str, bool)> {
    let lines: Vec<&str> = raw.lines().collect();
    let frontmatter = plan_file_frontmatter(&lines).map_or(0..0, |block| {
        block.open..block.close.map_or(lines.len(), |close| close + 1)
    });
    let mut fence = rhei_core::fence::FenceTracker::default();
    lines
        .iter()
        .enumerate()
        .map(|(at, line)| (*line, !frontmatter.contains(&at) && outside_fence(&mut fence, line)))
        .collect()
}

/// The block the parser reads as `lines`' frontmatter: a task file's leading one, or the
/// one under a plan's header. A plan's header comes before its tasks, so a `# Rhei:` line
/// below a task heading is a task file's body text, and a `---` under it is too.
/// §FS-rhei-reset.2.4 §FS-rhei-plan-language.2.3
fn plan_file_frontmatter(lines: &[&str]) -> Option<rhei_core::header::FrontmatterBlock> {
    if let Some(block) = rhei_core::header::find_leading_frontmatter(lines.iter().copied()) {
        return Some(block);
    }
    let place = rhei_core::header::find_frontmatter(lines.iter().copied())?;
    let mut fence = rhei_core::fence::FenceTracker::default();
    let below_a_task = lines[..place.header]
        .iter()
        .any(|line| outside_fence(&mut fence, line) && task_heading_id(line).is_some());
    place.block.filter(|_| !below_a_task)
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
