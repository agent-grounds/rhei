//! Fixtures for the per-entry log names of a state that keeps no visit counter.

use std::fs;
use std::path::{Path, PathBuf};

use super::*;

pub(super) const ONE_TICKET: &str = r#"# Rhei: Entry logs

## Tasks

### Task 1: Loop
**State:** measure
"#;

/// The coverage loop the ticket was found in: `measure` is a program that
/// writes `report-{n}.json` and stops the loop after its third measurement,
/// `cover` an agent with no `visits:` that is entered twice.
pub(super) fn loop_machine(measure: &Path) -> String {
    format!(
        r#"name: entry-logs
version: 1
states:
  measure:
    initial: true
    description: Measure coverage
    program:
      command: {command}
    program_timeout: 20s
  cover:
    description: Raise coverage
    agent: mock
    agent_timeout: 20s
  done:
    description: Done
    final: true
transitions:
  - {{ from: measure, to: cover, exit_code: 0, description: Not covered yet }}
  - {{ from: measure, to: done, exit_code: 3, description: Covered }}
  - {{ from: cover, to: measure, description: Measure again }}
metrics:
  coverage:
    label: Coverage
    measured_by: [measure]
    drivers: [cover]
    artifact: "runtime/report-{{iteration}}.json"
    pointer: /percent
    unit: "%"
    goal: increase
"#,
        command = fixture_command(measure)
    )
}

/// Writes the next `report-{n}.json` and exits 3 once three are on disk.
pub(super) const MEASURE: &str = r#"root = pathlib.Path(env('RHEI_ROOT'))
n = len(list(root.joinpath('runtime').glob('report-*.json')))
write(root / 'runtime' / 'report-{}.json'.format(n), '{"percent": %d}' % [64, 71, 80][n])
sys.stdout.write('MEASURE-{}\n'.format(n))
if n >= 2:
    result('covered\n')
    raise SystemExit(3)
"#;

/// Says which spawn of `cover` it is, so two sessions never share a body.
pub(super) const COVER: &str = r#"root = pathlib.Path(env('RHEI_ROOT'))
counter = root / 'runtime' / 'cover-spawns.txt'
n = int(counter.read_text().strip()) + 1 if counter.exists() else 1
write(counter, str(n))
write(root / 'runtime' / 'prompts' / 'cover-{}.md'.format(n), agent_prompt())
sys.stdout.write('SESSION-{}-content\n'.format(n))
"#;

pub(super) fn settings(root: &Path, agent: &Path) {
    let dir = root.join(".agent-grounds/rhei");
    fs::create_dir_all(&dir).expect("create settings directory");
    fs::write(
        dir.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "mock", "agent_timeout": "20s" }},
  "agents": {{
    "mock": {{ "command": {}, "stdin_prompt": true, "timeout": "20s" }}
  }}
}}"#,
            fixture_command(agent)
        ),
    )
    .expect("write settings");
}

/// One plan, one machine, a `measure` program and a `cover` agent.
pub(super) fn loop_fixture(name: &str, cover: &str) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(name);
    let plan = write_fixture_file(&dir, "plan.rhei.md", ONE_TICKET);
    let measure = write_python_agent(&dir, "measure.py", MEASURE);
    let machine = write_fixture_file(&dir, "states.yaml", &loop_machine(&measure));
    let agent = write_python_agent(&dir, "cover.py", cover);
    settings(&dir, &agent);
    (dir, plan, machine)
}

pub(super) fn names_in(dir: &Path, sub: &str) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir.join(sub))
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

pub(super) fn read(dir: &Path, relative: &str) -> String {
    fs::read_to_string(dir.join(relative)).unwrap_or_else(|err| {
        panic!(
            "{relative} should exist ({err}); logs={:?} reports={:?}",
            names_in(dir, "runtime/logs"),
            names_in(dir, "runtime/reports")
        )
    })
}

/// A plain agent state left at a gate after each entry, so a test can act
/// between two entries of the same uncounted state.
pub(super) const GATED_MACHINE: &str = r#"name: entry-gated
version: 1
states:
  work:
    initial: true
    description: Do the work
    agent: mock
    agent_timeout: 20s
  verifying:
    description: Look at it
    gating: true
  cancelled:
    description: Stop
    final: true
transitions:
  - { from: work, to: verifying, description: Ready }
  - { from: verifying, to: work, description: Again }
  - { from: "*", to: cancelled, description: Stop }
"#;

/// Says which spawn of `work` it is, per ticket.
pub(super) const WORK: &str = r#"root = pathlib.Path(env('RHEI_ROOT'))
counter = root / 'runtime' / ('work-spawns-' + env('RHEI_TASK_ID') + '.txt')
n = int(counter.read_text().strip()) + 1 if counter.exists() else 1
write(counter, str(n))
sys.stdout.write('WORK-{}-{}\n'.format(env('RHEI_TASK_ID'), n))
"#;

pub(super) fn gated_fixture(name: &str) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(name);
    let plan = write_fixture_file(
        &dir,
        "plan.rhei.md",
        &ONE_TICKET.replace("**State:** measure", "**State:** work"),
    );
    let machine = write_fixture_file(&dir, "states.yaml", GATED_MACHINE);
    let agent = write_python_agent(&dir, "work.py", WORK);
    settings(&dir, &agent);
    (dir, plan, machine)
}

pub(super) const RUN: [&str; 2] = ["--no-tui", "--no-callbacks"];
