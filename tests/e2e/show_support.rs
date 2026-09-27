//! Fixtures and the one runner for `rhei show`: a plan whose tickets are the
//! three cases the verb has to answer — a terminal ticket nobody may claim, a
//! ticket carrying an appended record, and a ticket with nothing written under
//! it. §FS-rhei-show

use std::path::{Path, PathBuf};

use super::*;

/// A flat plan of three tickets, the shape the report was filed against: one
/// ticket is `completed`, so the ready-set gate refuses it and it is reachable
/// today only by rendering the whole plan. §FS-rhei-show.2
pub const SHOW_PLAN: &str = r#"# Rhei: Probe
**States:** integration-test

## Tasks

### Task 1: Read one ticket without paying for the plan
**State:** draft

- Context: one plan, three tickets, one of them terminal.
- Expected: a verb that prints exactly what was asked for, by id.

### Task 2: A finished ticket nobody may claim
**State:** completed

- What happened: the command refused with a message that named no alternative.
- Expected: a verb that prints one task's body by id, whatever its state.

<!-- record: appended by a hook, and part of the body as far as rhei knows -->
> **Record** · 2 transitions · now `completed`

### Task 3: A ticket with nothing written under it
**State:** draft
"#;

/// What `rhei show probe.2` must print below its heading, to the byte.
///
/// The appended record is in it. Rhei knows no boundary inside a body, so the
/// text a hook wrote under the prose is body too. §FS-rhei-show.3
pub const TICKET_2_BODY: &str = "\
- What happened: the command refused with a message that named no alternative.
- Expected: a verb that prints one task's body by id, whatever its state.

<!-- record: appended by a hook, and part of the body as far as rhei knows -->
> **Record** · 2 transitions · now `completed`";

/// The whole of `rhei show probe.2`'s stdout: one heading, one blank line, the
/// body, and nothing after it. §FS-rhei-show.3
pub fn expected_ticket_2_output() -> String {
    format!("## Task probe.2: A finished ticket nobody may claim\n\n{TICKET_2_BODY}\n")
}

/// The single-rhei fixture: `probe.rhei.md` beside the machine it names.
pub fn show_fixture(prefix: &str) -> (TestDir, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let plan = write_fixture_file(&dir, "probe.rhei.md", SHOW_PLAN);
    write_fixture_file(&dir, "states.yaml", STATE_MACHINE);
    (dir, plan)
}

/// A project holding two rheis that both number a ticket `7`, so a bare `7` has
/// to be refused with the qualified candidates named. §FS-rhei-show.5
pub fn ambiguous_fixture(prefix: &str) -> (TestDir, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let project = dir.join("proj");
    std::fs::create_dir_all(&project).expect("the project should be creatable");
    write_fixture_file(&project, "index.panta.md", "# Panta: Two Rheis\n");
    for (name, title) in [("alpha", "Alpha"), ("beta", "Beta")] {
        write_fixture_file(
            &project,
            &format!("{name}.rhei.md"),
            &format!(
                "# Rhei: {title}\n**States:** integration-test\n\n## Tasks\n\n\
                 ### Task 7: The {name} seven\n**State:** draft\n\nBody of {name} seven.\n"
            ),
        );
    }
    write_fixture_file(&dir, "states.yaml", STATE_MACHINE);
    (dir, project)
}

/// `rhei show`, run from a working directory, with whatever arguments the case
/// is about. No `--state-machine`: `show` resolves no machine, and a test that
/// handed it one would not be showing that.
pub fn run_show(home: &Path, cwd: &Path, args: &[&str]) -> CliRun {
    let output = rhei_command(home)
        .current_dir(cwd)
        .arg("show")
        .args(args)
        .output()
        .expect("rhei show should execute");
    CliRun::from(&output)
}
