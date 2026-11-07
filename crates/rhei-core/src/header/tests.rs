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
