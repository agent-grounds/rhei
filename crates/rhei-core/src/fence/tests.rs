//! The fence rule itself, away from any reader that applies it.
//! §FS-rhei-plan-language.2.1

use super::*;

/// The lines a tracker calls fence lines, in order.
fn fence_lines(text: &str) -> Vec<usize> {
    let mut fence = FenceTracker::default();
    text.lines()
        .enumerate()
        .filter_map(|(index, line)| fence.read(line).then_some(index + 1))
        .collect()
}

/// Lines that are neither a fence line nor inside a fence: what a structural
/// scan is still allowed to read as a production of the grammar.
fn readable_lines(text: &str) -> Vec<usize> {
    let mut fence = FenceTracker::default();
    text.lines()
        .enumerate()
        .filter_map(|(index, line)| (!fence.read(line) && !fence.is_open()).then_some(index + 1))
        .collect()
}

/// §FS-rhei-plan-language.2.1: three or more of either character opens a
/// fence; two of them, or a run of something else, is an ordinary line.
#[test]
fn a_run_of_three_or_more_backticks_or_tildes_is_a_fence() {
    assert_eq!(code_fence_run("```"), Some(('`', 3, true)));
    assert_eq!(code_fence_run("````markdown"), Some(('`', 4, false)));
    assert_eq!(code_fence_run("  ~~~~~"), Some(('~', 5, true)));
    assert_eq!(code_fence_run("``"), None);
    assert_eq!(code_fence_run("~~"), None);
    assert_eq!(code_fence_run("## Tasks"), None);
    assert_eq!(code_fence_run(""), None);
}

/// §FS-rhei-plan-language.2.1: a bare `~~~` opens a block exactly as a
/// backtick run does, and what it wraps is content.
#[test]
fn a_tilde_run_opens_and_closes_a_block() {
    let text = "before\n~~~\ninside\n~~~\nafter\n";

    assert_eq!(fence_lines(text), vec![2, 4]);
    assert_eq!(readable_lines(text), vec![1, 5]);
}

/// §FS-rhei-plan-language.2.1: nesting is the rule, not parity — neither a
/// shorter run nor the other character closes the block it sits in.
#[test]
fn only_a_matching_run_at_least_as_long_closes_a_fence() {
    let shorter = "````\n```\nstill inside\n````\nafter\n";
    assert_eq!(fence_lines(shorter), vec![1, 4]);
    assert_eq!(readable_lines(shorter), vec![5]);

    let other_character = "```\n~~~\nstill inside\n```\nafter\n";
    assert_eq!(fence_lines(other_character), vec![1, 4]);
    assert_eq!(readable_lines(other_character), vec![5]);
}

/// §FS-rhei-plan-language.2.1: a run carrying an info string is not bare, so
/// it never closes a block — it is a line of the one it sits in.
#[test]
fn a_run_with_an_info_string_does_not_close_a_block() {
    let text = "```\n```console\nstill inside\n```\nafter\n";

    assert_eq!(fence_lines(text), vec![1, 4]);
    assert_eq!(readable_lines(text), vec![5]);
}

/// §FS-rhei-plan-language.2.1: a fence that is never closed runs to the end of
/// the text that opened it.
#[test]
fn an_unclosed_fence_runs_to_the_end_of_the_text() {
    let text = "before\n~~~markdown\n## Tasks\n\n### Task 1: quoted\n";

    let mut fence = FenceTracker::default();
    for line in text.lines() {
        fence.read(line);
    }
    assert!(fence.is_open(), "nothing closed the fence opened on line 2");
    assert_eq!(readable_lines(text), vec![1]);
}
