//! The language's one reading of a document's header.
//!
//! The parsers, the tokenizer and every command that writes a document's
//! frontmatter have to agree on which line is the header, because a line one
//! calls the header and another does not is a plan that validates and then
//! cannot be written. So the pattern, and the search for the line, live here
//! once and nowhere else. §FS-rhei-plan-language.2.3

use crate::fence::FenceTracker;
use regex::Regex;

/// The name a plan or a workspace index's header carries.
pub const RHEI_HEADER: &str = "Rhei";

/// The name a project manifest's header carries.
pub const PANTA_HEADER: &str = "Panta";

/// The header line under any of `names`, with the title as capture group 1.
///
/// It is applied to a line with its surrounding whitespace removed, so a line
/// that holds nothing after the name's colon has no space left to match and is
/// not a header. §FS-rhei-plan-language.2.3
pub fn header_regex(names: &[&str]) -> Regex {
    let names = names.iter().map(|name| regex::escape(name)).collect::<Vec<_>>().join("|");
    Regex::new(&format!(r#"^#\s+(?:{names}):\s+(.*)$"#)).unwrap()
}

/// The index of the first header among `lines`, reading a fence as the parsers
/// do, so a header quoted in a fenced block is passed over.
///
/// Both header names are searched for: a caller holding a document's text
/// writes a plan, an index and a manifest alike. §FS-rhei-plan-language.2.3
pub fn find_header_line<'a>(lines: impl IntoIterator<Item = &'a str>) -> Option<usize> {
    let header = header_regex(&[RHEI_HEADER, PANTA_HEADER]);
    let mut fence = FenceTracker::default();
    lines.into_iter().position(|line| {
        let is_fence = fence.read(line);
        !is_fence && !fence.is_open() && header.is_match(line.trim())
    })
}

#[cfg(test)]
mod tests;
