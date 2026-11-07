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
//!
//! A command that scans a document's raw lines reads them through
//! [`DocumentFence`], which knows that block, so the lines the parsers hand to
//! YAML are not the plan's text to it either. §FS-rhei-plan-language.2.1

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
    let block = content
        .filter(|(_, line)| line.trim() == "---")
        .map(|(open, _)| block_opened_at(open, &mut below));
    Some(FrontmatterPlace { header, block })
}

/// The block a bare task file leads with: past any blank lines, its first line is
/// `---`. A task file has no header above it, so [`find_frontmatter`] never finds
/// this one, and the node parser never sees it. §FS-rhei-plan-language.1.4
pub fn find_leading_frontmatter<'a>(
    lines: impl IntoIterator<Item = &'a str>,
) -> Option<FrontmatterBlock> {
    let mut lines = lines.into_iter().enumerate();
    let (open, first) = lines.find(|(_, line)| !line.trim().is_empty())?;
    (first.trim() == "---").then(|| block_opened_at(open, lines))
}

/// The block whose opening `---` is at `open`, closed by the first `---` below it.
fn block_opened_at<'a>(
    open: usize,
    mut below: impl Iterator<Item = (usize, &'a str)>,
) -> FrontmatterBlock {
    let close = below.find(|(_, line)| line.trim() == "---").map(|(close, _)| close);
    FrontmatterBlock { open, close }
}

/// Which entry point the parsers read a document through, and so where its
/// frontmatter block stands. Text alone cannot say: the same bytes can be a plan
/// holding a ticket and a task file holding none, so the caller, which knows
/// which file it opened, says which. §FS-rhei-plan-language.2.3
/// §FS-rhei-plan-language.1.4
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentRole {
    /// A document with a header: a single-file plan, a workspace index or a
    /// project manifest. Its block is the one under the header.
    Plan,
    /// A workspace task file or a basin ticket file. Its body is read under a
    /// synthetic header, so its block is only a leading one, and a `# Rhei:` line
    /// and a `---` below it are the body's text.
    TaskFile,
}

impl DocumentRole {
    /// The frontmatter block the parsers read in `text` for this role.
    /// §FS-rhei-plan-language.2.3 §FS-rhei-plan-language.1.4
    pub fn frontmatter_block(self, text: &str) -> Option<FrontmatterBlock> {
        match self {
            DocumentRole::Plan => find_frontmatter(text.lines())?.block,
            DocumentRole::TaskFile => find_leading_frontmatter(text.lines()),
        }
    }
}

/// The `## Tasks` line, spelled any way the parsers accept it: the line's
/// surrounding whitespace and the run of whitespace after `##` do not matter.
/// §FS-rhei-plan-language.2.3
pub fn is_tasks_chapter(line: &str) -> bool {
    line.trim()
        .strip_prefix("##")
        .is_some_and(|rest| rest.starts_with(char::is_whitespace) && rest.trim() == "Tasks")
}

/// The fence a line-by-line scan of a document's raw text is inside, counting the
/// frontmatter block as one.
///
/// The block's lines are YAML, so none of them is read as a fence, and
/// [`DocumentFence::is_open`] holds from its opening `---` through its closing one:
/// a scan that takes no production from a fenced line takes none from them. This is
/// [`FenceTracker`] for text that still carries its block, and it finds that block
/// where the parsers do rather than each scan skipping it for itself.
/// §FS-rhei-plan-language.2.1
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DocumentFence {
    fence: FenceTracker,
    block: Option<FrontmatterBlock>,
    next_line: usize,
    in_block: bool,
}

impl DocumentFence {
    /// The reader of a whole document in `role`, its frontmatter block found in
    /// `text` where the parser for that role reads it. §FS-rhei-plan-language.2.1
    pub fn of_role(role: DocumentRole, text: &str) -> Self {
        Self { block: role.frontmatter_block(text), ..Self::default() }
    }

    /// The reader of a plan, a workspace index or a manifest: its block is the one
    /// under the header. §FS-rhei-plan-language.2.3
    pub fn plan(text: &str) -> Self {
        Self::of_role(DocumentRole::Plan, text)
    }

    /// The reader of a task file: its block is only a leading one.
    /// §FS-rhei-plan-language.1.4
    pub fn task_file(text: &str) -> Self {
        Self::of_role(DocumentRole::TaskFile, text)
    }

    /// The reader of text whose block is already off, such as a template's body:
    /// a `---` at the top of that text is content. §FS-rhei-plan-language.2.1
    pub fn body() -> Self {
        Self::default()
    }

    /// Read the document's next line, and say whether it is a fence line. Call this
    /// exactly once per line, in order, from the document's first line: the block
    /// is known by line number. §FS-rhei-plan-language.2.1
    pub fn read(&mut self, line: &str) -> bool {
        let index = self.next_line;
        self.next_line += 1;
        self.in_block = self
            .block
            .is_some_and(|block| index >= block.open && block.close.is_none_or(|c| index <= c));
        !self.in_block && self.fence.read(line)
    }

    /// True while the line just read is in a fenced block or the frontmatter block,
    /// so no production of the grammar is recognized on it. §FS-rhei-plan-language.2.1
    pub fn is_open(&self) -> bool {
        self.in_block || self.fence.is_open()
    }
}

#[cfg(test)]
mod document_fence_tests;
#[cfg(test)]
mod tests;
