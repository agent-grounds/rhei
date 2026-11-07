//! A restore finds the heading that followed a worker's region by its text alone, so a copy of
//! that line anywhere else in the file refuses the restore: a later task's quotation of it is
//! never taken for the boundary, and the tasks around it are kept. §FS-rhei-run.3.7.3

use std::fs;

use super::python_fixture::{fixture_command, write_python_agent};
use super::*;

/// Task 2's program, once: a heading in its own body, and Task 3's heading renamed.
const COVER_PROGRAM: &str = r#"here = pathlib.Path(__file__).parent
plan = here / 'plan.rhei.md'
marker = here / 'noted'
if not marker.exists():
    marker.write_text('x')
    text = plan.read_text(encoding='utf-8')
    text = text.replace('**State:** work\n', '**State:** work\n\n#### Visit 1 (cover)\n\nnote\n', 1)
    text = text.replace('\n### Task 3: After\n**State:**', '\n### Task 3: After, renamed\n**State:**', 1)
    write(plan, text)
result('Task 2 wrote its note.\n')
"#;

const MACHINE: &str = "name: following-copy
version: 1
states:
  work:
    description: Writes a note into its own body.
    program:
      command: COMMAND
    program_timeout: 30s
  completed:
    description: Done.
    final: true
  failed:
    description: Failed.
    final: true
transitions:
  - from: work
    to: completed
    exit_code: 0
  - from: \"*\"
    to: failed
";

/// Task 3's heading is copied in a frontmatter value above the chapter and quoted in a fence
/// under Task 4, as the review's reproducer lays it.
const PLAN: &str = "# Rhei: P

---
metadata:
  note: \"
### Task 3: After
\"
---

## Tasks

### Task 1: Earlier
**State:** completed

Earlier.

### Task 2: Cover
**State:** work

Body.

### Task 3: After
**State:** completed

Task three body, must survive.

### Task 4: Later
**State:** completed

Quoting:

```
### Task 3: After
```

Task four tail.
";

/// The worker's text, as its program leaves the plan.
fn as_the_worker_left_it() -> String {
    PLAN.replacen("**State:** work\n", "**State:** work\n\n#### Visit 1 (cover)\n\nnote\n", 1)
        .replacen("\n### Task 3: After\n**State:**", "\n### Task 3: After, renamed\n**State:**", 1)
}

#[test]
fn a_renamed_following_heading_copied_above_and_quoted_below_refuses_the_restore() {
    let dir = unique_temp_dir("worker-edit-following-copy");
    let program = write_python_agent(&dir, "cover.py", COVER_PROGRAM);
    let machine = write_fixture_file(
        &dir,
        "states.yaml",
        &MACHINE.replace("COMMAND", &fixture_command(&program)),
    );
    let plan = write_fixture_file(&dir, "plan.rhei.md", PLAN);
    assert_success(&run_cli("validate", &plan, &machine, &[]));

    let ran = run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]);
    let out = format!("{}{}", ran.stdout, ran.stderr);
    // The report wraps its message under a gutter; read it as one line.
    let flat = out.replace('│', " ").split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(!ran.status.success(), "the run stops\n{out}");
    assert!(
        flat.contains(
            "The run refused to restore Task plan.2: the heading that followed it, \
             `### Task 3: After`, is in the file 2 time(s)"
        ),
        "{out}"
    );
    assert!(!flat.contains("Reverted Task plan.2"), "{out}");

    // Nothing is written: the plan is the worker's text, Task 3 and Task 4 whole.
    let after = fs::read_to_string(&plan).expect("plan");
    assert_eq!(after, as_the_worker_left_it());
    assert!(after.contains("Task three body, must survive."), "{after}");
    assert!(after.contains("### Task 4: Later"), "{after}");
    // The worker's own note is all that breaks it.
    let without_note = after.replacen("\n#### Visit 1 (cover)\n\nnote\n", "", 1);
    fs::write(&plan, without_note).expect("write plan");
    assert_success(&run_cli("validate", &plan, &machine, &[]));
}
