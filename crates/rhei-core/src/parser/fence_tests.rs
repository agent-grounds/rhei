//! A fenced code block is content, never structure.
//!
//! One definition of a fence governs which `##` lines are chapters and which
//! `### <kind> <id>:` lines are task nodes: a run of three or more backticks or
//! tildes, closed only by a bare run of the same character at least as long,
//! and running to the end of the file when nothing closes it.
//! §FS-rhei-plan-language.2.1

use super::*;
use crate::ast::Rhei;

/// The plan from agent-grounds/rhei#336: a tilde fence quoting the shape of a
/// plan, then the plan's own tasks chapter.
const TILDE_QUOTED_PLAN: &str = r#"# Rhei: A plan that quotes a plan

## Notes

~~~markdown
## Tasks

### Task demo-1: an illustration
~~~

## Tasks

### Task real-1: the only real task
**State:** pending

Body.
"#;

/// A four-backtick fence holding an unclosed three-backtick run — the shape a
/// forge comment takes when a paste is truncated inside a transcript.
const ODD_PARITY_PLAN: &str = r#"# Rhei: A plan that quotes a transcript

## The conversation

````markdown
# Proposal

```console
$ rhei validate plan.rhei.md
## Terms

Truncated mid-transcript.
````

## Tasks

### Task real-1: the only real task
**State:** pending

Body.
"#;

/// The same plan with the inner run closed. It parses today as well: the old
/// toggle landed back where it started, which is why the trigger is the inner
/// run's length and not how many of them there are.
const EVEN_PARITY_PLAN: &str = r#"# Rhei: A plan that quotes a transcript

## The conversation

````markdown
# Proposal

```console
$ rhei validate plan.rhei.md
## Terms
```

Closed mid-transcript.
````

## Tasks

### Task real-1: the only real task
**State:** pending

Body.
"#;

fn only_task_id(rhei: &Rhei) -> String {
    assert_eq!(rhei.tasks.len(), 1, "one authored task node, got {:?}", rhei.tasks);
    rhei.tasks[0].id.to_string()
}

/// §FS-rhei-plan-language.2.1: `~~~` is a fence, so the `## Tasks` it wraps is
/// a quotation and the plan's own chapter is still the final one.
#[test]
fn a_tilde_fence_hides_the_tasks_chapter_it_quotes() {
    let rhei = parse(TILDE_QUOTED_PLAN).expect("a plan may quote a plan in a tilde fence");

    assert_eq!(only_task_id(&rhei), "real-1");
    assert_eq!(rhei.content_sections.len(), 1, "one chapter: Notes");
    assert_eq!(rhei.content_sections[0].title, "Notes");
    assert!(
        rhei.content_sections[0].content.contains("### Task demo-1: an illustration"),
        "the quoted heading stays in the chapter text verbatim: {:?}",
        rhei.content_sections[0].content
    );
}

/// §FS-rhei-plan-language.2.1: a three-backtick run does not close a
/// four-backtick fence. Paired with the test below, which is the same plan with
/// the inner run closed: under a flag that flips on every fence-looking line
/// only this one fails, so the defect looks like a property of the paste rather
/// than of the reader.
#[test]
fn an_inner_run_too_short_to_close_leaves_the_fence_open() {
    let odd = parse(ODD_PARITY_PLAN).expect("an unclosed inner run is content");

    assert_eq!(only_task_id(&odd), "real-1");
    assert_eq!(odd.content_sections.len(), 1, "one chapter: The conversation");
    assert_eq!(odd.content_sections[0].title, "The conversation");
}

/// §FS-rhei-plan-language.2.1: the control, and the two facts it separates.
/// Closing the inner run lands a line-by-line toggle back where it started, so
/// this plan is *accepted* before the fix — which is why a fixture that closes
/// its inner block pins nothing about the refusal. What parity never fixed is
/// the reading: today the quoted `## Terms` still authors a chapter.
#[test]
fn an_inner_run_that_closes_itself_is_accepted_and_still_misread() {
    let even = parse(EVEN_PARITY_PLAN).expect("accepted before the fix as well as after");

    assert_eq!(only_task_id(&even), "real-1");
    assert_eq!(
        even.content_sections.iter().map(|section| section.title.as_str()).collect::<Vec<_>>(),
        vec!["The conversation"],
        "the quoted `## Terms` must not author a chapter"
    );
}

/// §FS-rhei-plan-language.2.1: a fence that is never closed runs to the end of
/// the text that opened it, so what follows is content rather than a node.
#[test]
fn an_unclosed_fence_runs_to_the_end_of_the_plan() {
    let input = r#"# Rhei: An unclosed quotation

## Tasks

### Task real-1: the only real task
**State:** pending

~~~markdown
### Task quoted-2: never authored
**State:** pending
"#;

    let rhei = parse(input).expect("parse ok");

    assert_eq!(only_task_id(&rhei), "real-1");
    assert!(
        rhei.tasks[0].content.contains("### Task quoted-2: never authored"),
        "everything after the open fence is the first task's content: {:?}",
        rhei.tasks[0].content
    );
}

/// §FS-rhei-plan-language.2.1: a closing run carrying an info string is not
/// bare, so it does not close the block it looks like.
#[test]
fn a_run_with_an_info_string_never_closes_a_fence() {
    let input = r#"# Rhei: Two quoted blocks

## Notes

```markdown
## Tasks
```console
## Overview
```

## Tasks

### Task real-1: the only real task
**State:** pending
"#;

    let rhei = parse(input).expect("parse ok");

    assert_eq!(only_task_id(&rhei), "real-1");
    assert_eq!(
        rhei.content_sections.iter().map(|section| section.title.as_str()).collect::<Vec<_>>(),
        vec!["Notes"],
        "the quoted `## Overview` must not open a chapter"
    );
}

/// §FS-rhei-plan-language.2.1: the workspace index reads a fence by the same
/// rule, so an index may quote the plan format it documents.
#[test]
fn a_workspace_index_may_quote_a_tasks_chapter() {
    let input = r#"# Rhei: A workspace that quotes a plan

## Notes

~~~markdown
## Tasks

### Task demo-1: an illustration
~~~

More notes.
"#;

    let index =
        parse_workspace_index(input).expect("a workspace index may quote a plan in a tilde fence");

    assert_eq!(
        index.content_sections.iter().map(|section| section.title.as_str()).collect::<Vec<_>>(),
        vec!["Notes"],
        "one chapter, and no quoted Tasks section"
    );
}

/// §FS-rhei-plan-language.2.1: and the same for an index whose quotation is a
/// four-backtick fence with an unclosed run inside it.
#[test]
fn a_workspace_index_may_quote_a_truncated_transcript() {
    let input = r#"# Rhei: A workspace that quotes a transcript

## The conversation

````markdown
```console
$ rhei validate plan.rhei.md
## Tasks

Truncated mid-transcript.
````
"#;

    let index = parse_workspace_index(input).expect("an unclosed inner run is content");

    assert_eq!(
        index.content_sections.iter().map(|section| section.title.as_str()).collect::<Vec<_>>(),
        vec!["The conversation"]
    );
}
