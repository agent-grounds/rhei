//! A task heading above `## Tasks`: task metadata under a node-shaped heading
//! that no `## Tasks` section owns. The plan is refused as before, and the
//! refusal names the heading and the section rather than the field.
//! §FS-rhei-errors.1.7

use regex::Regex;

use super::{ParseError, ParseErrorKind};
use crate::fence::FenceTracker;

/// Whether a trimmed line is an ATX heading: one to six `#`, then whitespace or
/// the end of the line. The nearest heading above a field outside `## Tasks` is the heading
/// the field sits under. §FS-rhei-errors.1.7
pub(super) fn is_heading(line: &str) -> bool {
    let hashes = line.bytes().take_while(|byte| *byte == b'#').count();
    let rest = &line[hashes..];
    (1..=6).contains(&hashes) && (rest.is_empty() || rest.starts_with(char::is_whitespace))
}

/// The refusal of task metadata under the task heading at `heading_line`, raised
/// at the heading's line. `rest` is the plan after the field, read for a
/// `## Tasks` line outside a fence: one there means the heading sits in a
/// content section above it, none means the plan has no `## Tasks` at all.
/// §FS-rhei-errors.1.7
pub(super) fn refusal<'a>(
    heading_line: usize,
    mut rest: impl Iterator<Item = &'a str>,
    re_tasks: &Regex,
) -> ParseError {
    let mut fence = FenceTracker::default();
    let tasks_section_follows =
        rest.any(|raw| !fence.read(raw) && !fence.is_open() && re_tasks.is_match(raw.trim()));
    let message = if tasks_section_follows {
        "Task heading outside the `## Tasks` section: it sits in a content section above \
         `## Tasks`, so the task metadata under it belongs to no task"
    } else {
        "Task heading outside a `## Tasks` section: this plan has no `## Tasks` section, so \
         the task metadata under the heading belongs to no task"
    };
    ParseError {
        kind: ParseErrorKind::TaskHeadingAboveTasks { tasks_section_follows },
        ..ParseError::new(message, Some(heading_line))
    }
}

/// The end of the block the heading at index `start` opens: the index of the
/// next heading outside a fence, or the end of `lines`. Recovery drops the
/// whole block: every field in it sits under that heading, and dropping the
/// heading alone would leave them to be refused again, each as a field outside
/// a task. §FS-rhei-errors.1.7
pub(super) fn block_end(lines: &[&str], start: usize) -> usize {
    let mut fence = FenceTracker::default();
    let next = lines[start + 1..]
        .iter()
        .position(|raw| !fence.read(raw) && !fence.is_open() && is_heading(raw.trim()));
    next.map_or(lines.len(), |offset| start + 1 + offset)
}
