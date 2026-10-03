//! What a retry is told about the attempt before it, when that attempt did not
//! exit but was ended by a signal — the OOM killer, an external `kill`.
//!
//! Its spawn record carries no exit code. The `Re-spawning` line and the retry
//! paragraph both read that absence as `exited 0`, and the paragraph went on to
//! say the completion condition was unmet beside an owed clause that named
//! nothing unmet (agent-grounds/rhei#400).

// §FS-rhei-agents.3.2.1 §FS-rhei-memory.4.4

use std::fs;
use std::path::{Path, PathBuf};

use super::agent_reentry_support::write_settings;
use super::*;

const PLAN: &str = r#"# Rhei: Retry ending

## Tasks

### Task 1: Publish the ticket
**State:** publishing
"#;

/// The shape of `publishing` in the `grounded-ticket` machine: one declared
/// output and a forward edge into a `final: true` state.
const MACHINE: &str = r#"name: retry-ending
version: 1
states:
  publishing:
    initial: true
    description: Act on a validated ticket
    attempts: 2
    agent: mock
    agent_timeout: 20s
    outputs:
      - name: issue
        path: runtime/triage/{task_id}.issue.md
        description: The issue this state created or adopted.
        format: markdown
    instructions: |
      Publish Task {task_id}: {task_title}.
  tracked:
    description: An upstream issue carries it
    final: true
transitions:
  - { from: publishing, to: tracked, description: The issue is recorded. }
"#;

const SIGNAL_ENDING: &str = "ended without an exit code (a signal ended it)";
const UNMET: &str = "without meeting this state's completion condition";
const OWES: &str = " It did not write what this visit still owes: ";

/// Attempt 1 optionally writes everything it owes, then sends itself `SIGKILL`;
/// any later attempt writes everything and exits 0.
fn setup(name: &str, write_before_kill: bool) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(name);
    let plan = write_fixture_file(&dir, "plan.rhei.md", PLAN);
    let machine = write_fixture_file(&dir, "states.yaml", MACHINE);
    let agent = write_python_agent(
        &dir,
        "mock-agent.py",
        &format!(
            r#"import signal
root = pathlib.Path(env('RHEI_ROOT'))
attempt = env('RHEI_ATTEMPT', 'unknown')
write(root / 'prompts' / ('attempt-' + attempt + '.md'), agent_prompt())
def write_all():
    write(root / 'runtime' / 'triage' / 'plan.1.issue.md', 'attempt ' + attempt + ' wrote the issue.\n')
    result('attempt ' + attempt + ' wrote the result.\n')
if attempt == '1':
    if {write_first}:
        write_all()
    sys.stdout.flush()
    os.kill(os.getpid(), signal.SIGKILL)
write_all()
"#,
            write_first = if write_before_kill { "True" } else { "False" },
        ),
    );
    write_settings(&dir, &agent, false);
    (dir, plan, machine)
}

/// The two passes: the first is killed, the second re-spawns it as attempt 2.
/// Returns the second pass's output, where the `Re-spawning` line is printed,
/// and the prompt attempt 2 was handed.
fn second_pass(dir: &Path, plan: &Path, machine: &Path) -> (String, String) {
    let args = ["--no-tui", "--no-callbacks"];
    let first = run_cli("run", plan, machine, &args);
    let record = fs::read_to_string(dir.join("runtime/spawns/task-plan.1-publishing.json"))
        .unwrap_or_else(|_| {
            panic!("attempt 1 left no spawn record:\n{}{}", first.stdout, first.stderr)
        });
    assert!(
        record.contains("\"code\": null"),
        "the fixture only reproduces the defect while the record carries no exit code; got:\n{record}"
    );
    let second = run_cli("run", plan, machine, &args);
    let output = format!("{}{}", second.stdout, second.stderr);
    let prompt = fs::read_to_string(dir.join("prompts/attempt-2.md")).unwrap_or_else(|_| {
        panic!(
            "no attempt 2 was spawned\nfirst pass:\n{}{}\nsecond pass:\n{output}",
            first.stdout, first.stderr
        )
    });
    (output, prompt)
}

fn respawn_line(output: &str) -> String {
    output
        .lines()
        .find(|line| line.contains("Re-spawning Task plan.1"))
        .unwrap_or_else(|| panic!("the run printed no Re-spawning line:\n{output}"))
        .trim()
        .to_string()
}

fn retry_paragraph(prompt: &str) -> String {
    prompt
        .lines()
        .find(|line| line.starts_with("Retrying this visit:"))
        .unwrap_or_else(|| panic!("the prompt carries no retry paragraph:\n{prompt}"))
        .to_string()
}

/// Case A of the report: attempt 1 wrote nothing and was killed. Both surfaces
/// say it ended without an exit code, and neither says it exited 0.
// §FS-rhei-agents.3.2.1 §FS-rhei-memory.4.4
#[test]
fn a_killed_attempt_that_wrote_nothing_is_not_said_to_have_exited_0() {
    let (dir, plan, machine) = setup("retry-ending-killed-empty", false);

    let (output, prompt) = second_pass(&dir, &plan, &machine);
    let line = respawn_line(&output);
    let notice = retry_paragraph(&prompt);

    assert!(
        line.contains(&format!(
            "attempt 2 of 2; the previous attempt {SIGNAL_ENDING} (previous log: "
        )),
        "the Re-spawning line reads the ending from the record; got:\n{line}"
    );
    assert!(!line.contains("exited 0"), "the record has no exit code; got:\n{line}");
    assert!(
        notice.starts_with(&format!(
            "Retrying this visit: attempt 2. The previous attempt {SIGNAL_ENDING}.{OWES}issue (`"
        )),
        "the retry is told how the attempt ended, then what it still owes; got:\n{notice}"
    );
    assert!(!notice.contains("exited 0"), "the record has no exit code; got:\n{notice}");
}

/// Case B of the report: attempt 1 wrote everything it owed and was then
/// killed. The kill is not successful work, so it is retried, but nothing is
/// unmet — the paragraph may not say the condition went unmet.
// §FS-rhei-agents.3.2.1 §FS-rhei-memory.4.4
#[test]
fn a_killed_attempt_that_wrote_everything_is_not_said_to_have_missed_its_condition() {
    let (dir, plan, machine) = setup("retry-ending-killed-written", true);

    let (output, prompt) = second_pass(&dir, &plan, &machine);
    let line = respawn_line(&output);
    let notice = retry_paragraph(&prompt);

    assert!(
        line.contains(&format!("the previous attempt {SIGNAL_ENDING} (previous log: ")),
        "the Re-spawning line reads the ending from the record; got:\n{line}"
    );
    assert!(!line.contains(UNMET), "the line cannot back that claim; got:\n{line}");
    assert!(
        notice.starts_with(&format!(
            "Retrying this visit: attempt 2. The previous attempt {SIGNAL_ENDING}. \
             Its transcript is `"
        )),
        "nothing is owed, so the retry is told the ending and the transcript only; \
         got:\n{notice}"
    );
    assert!(
        !notice.contains(UNMET),
        "the paragraph says the condition was unmet only where it names what is; got:\n{notice}"
    );
}
