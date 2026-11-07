//! The language's one reading of a document's header.
//!
//! The parsers, the tokenizer and every command that writes a document's
//! frontmatter have to agree on which line is the header and which block is the
//! frontmatter, because a line one calls the header and another does not is a
//! plan that validates and then cannot be written. So the pattern, and the
//! searches a command runs over a document's text, live here once. The parsers
//! walk the same lines themselves, taking the first header by this pattern and
//! the fence rule, and passing the same lines over on the way to its frontmatter.
//! §FS-rhei-plan-language.2.3

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

/// Where a document's header and frontmatter block are, as line indices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrontmatterPlace {
    /// The header's line.
    pub header: usize,
    /// The block the parsers read under that header, if the document has one.
    pub block: Option<FrontmatterBlock>,
}

/// The two `---` lines of a frontmatter block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrontmatterBlock {
    /// The opening `---`.
    pub open: usize,
    /// The closing `---`, or `None` when the block is left unterminated.
    pub close: Option<usize>,
}

/// The header among `lines` and the frontmatter block the parsers read under it:
/// past the header, blank lines and fenced blocks are passed over, and the block
/// is there only if the next line is `---`. A `---` anywhere else is content.
///
/// `None` is a document with no header. §FS-rhei-plan-language.2.3
pub fn find_frontmatter<'a>(lines: impl IntoIterator<Item = &'a str>) -> Option<FrontmatterPlace> {
    let mut lines = lines.into_iter();
    let header = find_header_line(lines.by_ref())?;
    let mut fence = FenceTracker::default();
    let mut below = (header + 1..).zip(lines);
    let content =
        below.find(|(_, line)| !(fence.read(line) || fence.is_open() || line.trim().is_empty()));
    let block =
        content.filter(|(_, line)| line.trim() == "---").map(|(open, _)| FrontmatterBlock {
            open,
            close: below.find(|(_, line)| line.trim() == "---").map(|(close, _)| close),
        });
    Some(FrontmatterPlace { header, block })
}

#[cfg(test)]
mod tests;
