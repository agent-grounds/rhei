//! The header rule itself, away from any reader that applies it.
//! §FS-rhei-plan-language.2.3

use super::*;

fn header_line(text: &str) -> Option<usize> {
    find_header_line(text.lines())
}

#[test]
fn any_whitespace_separates_the_parts_of_a_header() {
    for line in [
        "# Rhei: T",
        "#  Rhei: T",
        "#\tRhei: T",
        "#\u{a0}Rhei: T",
        "# Rhei:  T",
        "# Rhei:\tT",
        "# Panta: T",
        "#  Panta: T",
        "#\tPanta: T",
    ] {
        assert_eq!(header_line(line), Some(0), "{line:?} is a header");
    }
}

#[test]
fn an_indented_line_is_a_header_because_the_line_is_trimmed() {
    assert_eq!(header_line("   # Rhei: T"), Some(0));
    assert_eq!(header_line("\t#  Panta: T  "), Some(0));
}

#[test]
fn a_line_with_no_title_is_not_a_header() {
    // The trim takes the only whitespace after the colon, as it does for the parsers.
    for line in ["# Rhei:", "# Rhei:   ", "#  Rhei:\t", "# Panta:", "# Panta:  "] {
        assert_eq!(header_line(line), None, "{line:?} has no title");
    }
}

#[test]
fn a_line_shaped_differently_is_not_a_header() {
    for line in ["#Rhei: T", "# Rhei:T", "## Rhei: T", "# rhei: T", "# Other: T", "text # Rhei: T"]
    {
        assert_eq!(header_line(line), None, "{line:?} is not a header");
    }
}

#[test]
fn a_header_quoted_in_a_fence_is_passed_over() {
    assert_eq!(header_line("```markdown\n# Rhei: quoted\n```\n\n# Rhei: real\n"), Some(4));
    assert_eq!(header_line("~~~\n# Panta: quoted\n~~~\n#  Panta: real\n"), Some(3));
}

#[test]
fn a_fence_with_a_longer_run_is_not_closed_by_a_shorter_one() {
    let text = "````markdown\n```\n# Rhei: quoted\n```\n# Rhei: quoted\n````\n# Rhei: real\n";
    assert_eq!(header_line(text), Some(6));
}

#[test]
fn a_header_only_inside_a_fence_is_no_header() {
    assert_eq!(header_line("```\n# Rhei: quoted\n```\n"), None);
    assert_eq!(header_line("```\n# Rhei: quoted, and the fence never closes\n"), None);
}

#[test]
fn the_first_header_outside_a_fence_is_the_one_found() {
    assert_eq!(header_line("prose\n\n# Rhei: first\n# Rhei: second\n"), Some(2));
    assert_eq!(header_line("# Panta: first\n# Rhei: second\n"), Some(0));
}

#[test]
fn the_title_is_the_first_capture_under_the_names_asked_for() {
    let rhei = header_regex(&[RHEI_HEADER]);
    assert_eq!(&rhei.captures("#  Rhei:  A title").unwrap()[1], "A title");
    assert!(!rhei.is_match("# Panta: A title"));

    let any = header_regex(&[RHEI_HEADER, PANTA_HEADER]);
    assert_eq!(&any.captures("# Panta: B").unwrap()[1], "B");
}

fn block_of(text: &str) -> Option<(usize, Option<usize>)> {
    let place = find_frontmatter(text.lines()).expect("the document has a header");
    place.block.map(|block| (block.open, block.close))
}

#[test]
fn a_document_with_no_header_has_no_frontmatter_place() {
    assert_eq!(find_frontmatter("---\nowner: me\n---\n".lines()), None);
    assert_eq!(find_frontmatter("```\n# Rhei: quoted\n```\n".lines()), None);
}

#[test]
fn the_block_is_the_next_thing_under_the_header() {
    assert_eq!(block_of("# Rhei: T\n---\nowner: me\n---\n"), Some((1, Some(3))));
    assert_eq!(block_of("#  Panta: T\n\n  \n---\n---\n"), Some((3, Some(4))));
    assert_eq!(block_of("# Rhei: T\n  ---  \nowner: me\n---\t\n"), Some((1, Some(3))));
}

#[test]
fn a_fence_under_the_header_is_passed_over_on_the_way_to_the_block() {
    assert_eq!(block_of("# Rhei: T\n\n```\nx\n```\n\n---\nowner: me\n---\n"), Some((6, Some(8))));
}

#[test]
fn a_dashed_line_in_a_quotation_is_not_the_block() {
    assert_eq!(block_of("```\n---\n```\n\n# Rhei: T\n"), None);
    assert_eq!(block_of("```\n---\n```\n\n# Rhei: T\n---\n---\n"), Some((5, Some(6))));
    assert_eq!(block_of("# Rhei: T\n```\n---\n```\n"), None);
    assert_eq!(block_of("# Rhei: T\n```\n---\n"), None);
}

#[test]
fn a_dashed_line_below_anything_else_is_content() {
    assert_eq!(block_of("# Rhei: T\n\nprose\n\n---\nowner: me\n---\n"), None);
    assert_eq!(block_of("# Rhei: T\n\n## Notes\n\n---\n"), None);
    assert_eq!(block_of("# Rhei: T\n# Rhei: second\n---\nowner: me\n---\n"), None);
}

#[test]
fn a_block_left_open_says_so_and_a_fence_does_not_close_it() {
    assert_eq!(block_of("# Rhei: T\n---\nowner: me\n"), Some((1, None)));
    assert_eq!(block_of("# Rhei: T\n---\n```\n---\n```\n"), Some((1, Some(3))));
}

#[test]
fn the_place_names_the_header_above_the_block() {
    let place = find_frontmatter("prose\n\n# Rhei: T\n\n---\n---\n".lines()).unwrap();
    assert_eq!(place.header, 2);
    assert_eq!(place.block, Some(FrontmatterBlock { open: 4, close: Some(5) }));
}
