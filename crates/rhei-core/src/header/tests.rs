//! The header rule itself, away from any reader that applies it.
//! §FS-rhei-plan-language.2.3

use super::*;
use crate::ast::Metadata;
use crate::parser::{parse, parse_panta_manifest, parse_workspace_index};

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

/// The title and block `find_frontmatter` locates in `text`, read the way a parser
/// reads them.
fn located(text: &str) -> (String, Option<Metadata>) {
    let lines: Vec<&str> = text.lines().collect();
    let place = find_frontmatter(lines.iter().copied()).expect("the document has a header");
    let header = header_regex(&[RHEI_HEADER, PANTA_HEADER]);
    let title = header.captures(lines[place.header].trim()).expect("a header line")[1].to_string();
    let metadata = place.block.map(|block| {
        let close = block.close.expect("a parser refuses a block left open");
        serde_yaml::from_str(&lines[block.open + 1..close].join("\n")).expect("a YAML mapping")
    });
    (title, metadata)
}

/// The parsers walk the lines themselves, so this one table holds them and the locator
/// to one reading of each shape. A plan needs its `## Tasks`, so only an index and a
/// manifest can really end on the block's `---`. §FS-rhei-plan-language.2.3
#[test]
fn the_parsers_read_the_header_and_block_the_locator_finds() {
    const TASKS: &str = "\n## Tasks\n\n### Task 1: One\n**State:** open\n";
    let shapes = [
        ("a plain block", "# Rhei: T\n---\nowner: me\n---\n"),
        ("trailing whitespace on ---", "# Rhei: T\n---  \nowner: me\n---\t\n"),
        ("an indented ---", "# Rhei: T\n  ---\nowner: me\n\t---\n"),
        ("whitespace-only blank lines", "# Rhei: T\n  \n\t\n---\nowner: me\n---\n"),
        ("a fence under the header", "# Rhei: T\n\n```\nx\n```\n\n---\nowner: me\n---\n"),
        ("a block in that fence", "# Rhei: T\n```\n---\nowner: x\n---\n```\n---\nowner: me\n---\n"),
        ("only a fence under the header", "# Rhei: T\n```\n---\nowner: x\n---\n```\n"),
        (
            "a fence above the header",
            "```\n# Rhei: x\n---\nowner: x\n---\n```\n#  Rhei: T\n---\nowner: me\n---\n",
        ),
        ("CRLF", "# Rhei: T\r\n\r\n---\r\nowner: me\r\n---\r\n"),
        ("no block", "# Rhei: T\n"),
        ("a --- below prose", "# Rhei: T\n\nprose\n\n---\nowner: x\n---\n"),
        ("a block under a second header", "# Rhei: T\n# Rhei: U\n---\nowner: x\n---\n"),
        ("a --- on the last line", "# Rhei: T\n---\nowner: me\n---"),
    ];
    for (shape, text) in shapes {
        let plan = format!("{text}{TASKS}");
        let manifest = text.replace("Rhei:", "Panta:");
        let plan_read = parse(&plan).unwrap_or_else(|err| panic!("plan, {shape}: {err:?}"));
        let index =
            parse_workspace_index(text).unwrap_or_else(|err| panic!("index, {shape}: {err:?}"));
        let project = parse_panta_manifest(&manifest)
            .unwrap_or_else(|err| panic!("manifest, {shape}: {err:?}"));
        let reads = [
            ("plan", plan.as_str(), plan_read.title, plan_read.metadata),
            ("index", text, index.title, index.metadata),
            ("manifest", manifest.as_str(), project.title, project.metadata),
        ];
        for (parser, document, title, metadata) in reads {
            assert_eq!((title, metadata), located(document), "{parser}, {shape}: {document:?}");
        }
    }
}
