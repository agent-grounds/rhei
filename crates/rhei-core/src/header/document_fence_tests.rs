//! The frontmatter block as a raw-text scan reads it, away from any command that scans.
//! Every heading scan in the CLI takes a `DocumentFence`, so what holds here holds there.
//! §FS-rhei-plan-language.2.1 §FS-rhei-plan-language.2.3

use super::*;

/// The lines a scan may take a production from, in order.
fn structure(text: &str) -> Vec<&str> {
    structure_with(DocumentFence::new(text), text)
}

fn structure_with(mut fence: DocumentFence, text: &str) -> Vec<&str> {
    text.lines()
        .filter(|line| {
            let is_fence = fence.read(line);
            !is_fence && !fence.is_open() && !line.trim().is_empty()
        })
        .collect()
}

const TASKS: [&str; 3] = ["## Tasks", "### Task 1: A", "**State:** open"];
const TASKS_TEXT: &str = "## Tasks\n\n### Task 1: A\n**State:** open\n";

#[test]
fn a_fence_shaped_value_opens_nothing_under_the_header() {
    let text = format!("# Rhei: T\n\n---\nnote: |\n  ```\n  x\n---\n\n{TASKS_TEXT}");
    let expected = [&["# Rhei: T"], &TASKS[..]].concat();
    assert_eq!(structure(&text), expected);
}

#[test]
fn a_fence_shaped_value_opens_nothing_in_a_leading_block() {
    let text = "---\nnote: |\n  ```\n---\n\n### Task 1: A\n**State:** open\n";
    assert_eq!(structure(text), TASKS[1..]);
}

#[test]
fn a_value_at_column_0_is_no_production() {
    let plan =
        format!("# Rhei: T\n\n---\nnote: \"\n### Task 1: x\n\n**State:** x\n\"\n---\n{TASKS_TEXT}");
    assert_eq!(structure(&plan), [&["# Rhei: T"], &TASKS[..]].concat());
    let file =
        "---\nnote: \"\n### Task 1: x\n\n**State:** x\n\"\n---\n### Task 1: A\n**State:** open\n";
    assert_eq!(structure(file), TASKS[1..]);
}

#[test]
fn the_block_is_read_as_a_fenced_line_is_while_it_lasts() {
    let text = format!("# Rhei: T\n---\nk: v\n---\n{TASKS_TEXT}");
    let mut fence = DocumentFence::new(&text);
    let open_on_each: Vec<bool> = text
        .lines()
        .map(|line| {
            assert!(!fence.read(line), "no line here is a fence: {line}");
            fence.is_open()
        })
        .collect();
    assert_eq!(&open_on_each[..5], &[false, true, true, true, false]);
}

#[test]
fn the_block_is_found_where_the_parsers_find_it() {
    for above in [
        "# Rhei: T\n\n```\nquoted\n```\n\n",
        "```\n# Rhei: quoted\n---\n```\n# Rhei: T\n",
        "### Task 9: prose above the header\n# Rhei: T\n",
    ] {
        let text = format!("{above}---\nnote: |\n  ```\n---\n{TASKS_TEXT}");
        assert!(structure(&text).ends_with(&TASKS), "{text}");
    }
    assert_eq!(structure("\n\n---\nk: ```\n```\n---\n### Task 1: A\n"), ["### Task 1: A"]);
}

#[test]
fn crlf_line_endings_change_nothing() {
    let text =
        format!("# Rhei: T\n\n---\nnote: |\n  ```\n---\n\n{TASKS_TEXT}").replace('\n', "\r\n");
    assert!(structure(&text).ends_with(&TASKS));
}

/// A `---` pair anywhere else is the text around it, so a fence between them is a fence.
#[test]
fn a_rule_elsewhere_is_content() {
    let in_body = format!("# Rhei: T\n\n{TASKS_TEXT}\n---\n```\n---\n\n### Task 2: B\n");
    assert!(!structure(&in_body).contains(&"### Task 2: B"));
    let below_prose = format!("# Rhei: T\n\nprose\n\n---\n```\n---\n\n{TASKS_TEXT}");
    assert!(!structure(&below_prose).contains(&"### Task 1: A"));
    let second_pair = "---\nk: v\n---\n---\n```\n---\n### Task 1: A\n";
    assert!(!structure(second_pair).contains(&"### Task 1: A"));
}

/// The parser reads a task file's body under a header of its own, so one written in it is text.
#[test]
fn a_header_in_a_task_file_opens_no_block() {
    for above in ["### Task 1: A\n**State:** open\n", "prose\n"] {
        let text = format!("{above}\n# Rhei: quoted\n\n---\n```\n---\n\n### Task 2: B\n");
        assert!(!structure(&text).contains(&"### Task 2: B"), "{text}");
    }
}

#[test]
fn an_unterminated_leading_block_runs_to_the_end() {
    assert!(structure("---\nk: v\n### Task 1: A\n").is_empty());
}

/// A template's body has had its block taken off, and may open with a rule of its own.
#[test]
fn text_past_its_block_has_no_block() {
    let body = "---\n```\n---\n### Task 1: A\n";
    assert_eq!(structure_with(DocumentFence::body(), body), ["---"]);
    assert_eq!(
        structure_with(DocumentFence::body(), "---\nprose\n---\n### Task 1: A\n"),
        ["---", "prose", "---", "### Task 1: A"]
    );
}
