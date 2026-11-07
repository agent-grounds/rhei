//! `rhei instantiate --into` writes a host's frontmatter where the parser reads it, so
//! neither a quotation above the header nor prose under it refuses a plan `rhei validate`
//! accepts (agent-grounds/rhei#581).

// §FS-rhei-plan-language.2.3 §FS-rhei-library.4

use std::path::{Path, PathBuf};

use super::into_support::*;
use super::*;

/// The `review-loop` template placed into a host with this index, validated before and
/// after. Returns the index as it was left.
#[track_caller]
fn placed_into(name: &str, index: &str) -> String {
    let (dir, root) = host_workspace(name);
    write_review_template(&dir);
    write_fixture_file(&root, "index.rhei.md", index);
    assert_host_validates(&dir, &root);
    let args = ["instantiate", "review-loop", "change_ref=HEAD~1", "--into", "release"];
    assert_success(&run_into(&args, &dir));
    assert_host_validates(&dir, &root);
    read(&root.join("index.rhei.md"))
}

#[track_caller]
fn assert_host_validates(dir: &Path, root: &Path) {
    let root: PathBuf = root.to_path_buf();
    assert_success(&run_into(&["validate", root.to_str().expect("a utf-8 path")], dir));
}

/// The text from the header on, with the node kinds the placement brought under it.
#[track_caller]
fn assert_kinds_under(text: &str, header_and_above: &str) -> String {
    let below = text
        .strip_prefix(header_and_above)
        .unwrap_or_else(|| panic!("the header and what is above it stay:\n{text}"));
    let block = below.trim_start_matches('\n').strip_prefix("---\n");
    let block = block.unwrap_or_else(|| panic!("the block comes next under the header:\n{text}"));
    let (block, rest) = block.split_once("---\n").expect("the block closes");
    assert!(block.contains("nodeKinds:") && block.contains("- step"), "the kinds:\n{text}");
    rest.to_string()
}

#[test]
fn a_quoted_block_above_the_header_is_not_the_frontmatter() {
    let quoted = "```\n---\n```\n\n# Rhei: Release\n";
    let text = placed_into("header-into-quoted", &format!("{quoted}---\nowner: keep\n---\n"));
    assert_kinds_under(&text, quoted);
    assert!(text.contains("owner: keep"), "the author's frontmatter stays:\n{text}");
}

#[test]
fn a_quoted_block_above_a_header_with_no_frontmatter_is_not_the_frontmatter() {
    let quoted = "```\n---\n```\n\n# Rhei: Release\n";
    assert_kinds_under(&placed_into("header-into-quoted-none", quoted), quoted);
}

/// Today's output for a document that holds only a header, spacing included.
#[test]
fn a_header_alone_keeps_the_block_it_always_got() {
    let header = "#  Rhei: Release\n";
    let text = placed_into("header-into-alone", header);
    assert!(text.starts_with("#  Rhei: Release\n\n---\nstructure:\n"), "got:\n{text}");
}

#[test]
fn prose_under_the_header_stays_below_the_new_block() {
    let text = placed_into("header-into-prose", "# Rhei: Release\n\nprose\n");
    let rest = assert_kinds_under(&text, "# Rhei: Release\n");
    assert_eq!(rest.trim(), "prose", "the prose stays:\n{text}");
}

/// A `---` in prose is a thematic break, not a block the placement may write into.
#[test]
fn a_dashed_line_under_prose_is_not_the_frontmatter() {
    let text = placed_into("header-into-break", "# Rhei: Release\n\nprose\n\n---\n\nmore\n");
    let rest = assert_kinds_under(&text, "# Rhei: Release\n");
    assert_eq!(rest.trim(), "prose\n\n---\n\nmore", "the text stays:\n{text}");
}

/// A block below a fence is the one the parser reads, so the placement writes it and
/// opens no second one above the fence.
#[test]
fn a_block_below_a_fence_is_the_frontmatter() {
    let fenced = "# Rhei: Release\n\n```\nx\n```\n\n";
    let text = placed_into("header-into-fence", &format!("{fenced}---\nowner: keep\n---\n"));
    assert_kinds_under(&text, fenced);
    assert_eq!(text.matches("structure:").count(), 1, "one block, not two:\n{text}");
    assert!(text.contains("owner: keep"), "the author's frontmatter stays:\n{text}");
}

/// A placement's write leaves LF as `rhei complete`'s does. §FS-rhei-plan-language.2.3
#[test]
fn a_placement_keeps_the_text_above_it_and_leaves_no_carriage_return() {
    let index = "<!-- a -->\r\n#  Rhei: Release\r\n---\r\nowner: keep\r\n---\r\n";
    let text = placed_into("header-into-crlf", index);
    assert!(!text.contains('\r'), "no carriage return is left:\n{text:?}");
    assert_kinds_under(&text, "<!-- a -->\n#  Rhei: Release\n");
    assert!(text.contains("owner: keep"), "the author's frontmatter stays:\n{text}");
}
