//! A reverted edit in a task file the culprit shares
//! with the sibling whose transition lands during the visit, and what an agent
//! is told about the edit before and after it (agent-grounds/rhei#310).
//!
//! §FS-rhei-run.3.7

use std::fs;
use std::path::Path;

use super::agent_reentry_support::write_settings;
use super::worker_edit_one_file_support::OneFileScenario;
use super::worker_edit_revert_support::{Mode, MODES, NOTE};
use super::*;

/// Tasks 1 and 2 share one task file; Task 3, in a file of its own, keeps the
/// workspace from being one file, which `--parallel` would run sequentially.
pub(super) const SHARED_FILE: &str = "### Task 1: Raise line coverage
**State:** cover

Raise coverage of `src/report.rs` above 80%.

### Task 2: Unrelated work
**State:** other

Does something else.
";

pub(super) const LATER_FILE: &str = "### Task 3: Later work
**State:** cover
**Prior:** Task 2

Runs after Task 2.
";

/// Task 1 waits, under `--parallel`, until Task 2's transition is visible, then
/// takes the engine's stable sidecar before reading the current image. The
/// sibling holds that lock through result-link finalization, so the staged
/// replacement preserves its completed state and result. §FS-rhei-run.3.7.3
/// §AR-agent-orchestrator-workflow.3.3.1 §AR-agent-orchestrator-workflow.3.3.1.1
/// Task 1 still puts a malformed note at the end of its own region, requiring
/// a real restore and charged retry with retained text. §FS-rhei-run.3.7.4 §FS-rhei-run.3.7.7
/// Task 3 runs the same state and writes nothing.
pub(super) const ONE_FILE_COVER: &str = concat!(
    include_str!("fixtures/worker_edit_shared_writer.py"),
    r#"concurrent = sys.argv[1] == 'concurrent'
here = pathlib.Path(__file__).parent
plan = here / 'ws' / 'tasks' / '01-shared.md'
marker = here / 'noted'
if env('RHEI_TASK_ID_LOCAL') == '1' and not marker.exists():
    marker.write_text('x')
    if concurrent:
        deadline = time.monotonic() + 15
        while True:
            sibling = plan.read_text(encoding='utf-8').split('### Task 2:', 1)[1]
            if '**State:** completed' in sibling:
                break
            if time.monotonic() >= deadline:
                raise RuntimeError(f'Task 2 transition timed out in {plan}:\n{sibling}')
            time.sleep(0.05)
    # Optional test timing seam, before the authoritative read/edit operation.
    if '_before_shared_edit' in globals():
        _before_shared_edit()
    with _shared_writer(plan):
        text = plan.read_text(encoding='utf-8')
        at = text.index('### Task 2:')
        note = '#### Visit 1 (cover)\n\nLatest measurement was report-1.json.\n\n'
        _publish_shared_image(plan, text[:at] + note + text[at:])
result('Task 1 wrote its note.\n')
"#
);

pub(super) const ONE_FILE_OTHER: &str = r#"here = pathlib.Path(__file__).parent
append(here / 'other-runs.txt', 'ran\n')
result('Task 2 did its unrelated work.\n')
"#;

pub(super) fn one_file_machine(cover: &str, other: &str) -> String {
    format!(
        r#"name: worker-edit-one-file
version: 1
states:
  cover:
    description: Writes a note into its own region.
    program:
      command: {cover}
    program_timeout: 30s
  other:
    description: Unrelated work.
    program:
      command: {other}
    program_timeout: 30s
  completed:
    description: Done.
    final: true
transitions:
  - from: cover
    to: completed
    exit_code: 0
  - from: other
    to: completed
    exit_code: 0
"#
    )
}

/// The block of one task in a shared file, heading to next heading.
pub(super) fn block<'a>(plan: &'a str, heading: &str) -> &'a str {
    let start = plan.find(heading).unwrap_or_else(|| panic!("no `{heading}` in:\n{plan}"));
    let rest = &plan[start + heading.len()..];
    let end = rest.find("\n### ").map_or(plan.len(), |at| start + heading.len() + at + 1);
    &plan[start..end]
}

/// The restore replaces Task 1's region only, so Task 2's transition — written
/// to the same file while Task 1 ran — is still there, and Task 2 is not run
/// again because a whole-file snapshot put its old state back.
// §FS-rhei-run.3.7.3 §FS-rhei-run.3.7.4
#[test]
fn a_siblings_transition_in_the_same_file_survives_the_restore() {
    for mode in MODES {
        let scenario = OneFileScenario::new(&format!("worker-edit-one-file-{mode:?}"), mode, "");
        let mut args = vec!["--no-tui", "--no-callbacks"];
        args.extend_from_slice(mode.flags());
        let ran = run_cli("run", &scenario.ws, &scenario.machine, &args);
        scenario.assert_completed(&ran);
    }
}

const AGENT_PLAN: &str = "# Rhei: Agent note

## Tasks

### Task 1: Raise line coverage
**State:** cover

Raise coverage of `src/report.rs` above 80%.
";

/// The line the note's heading landed on in the plan on disk: where the task's
/// heading sits there, plus the heading's offset in the text the run reverted.
/// Rhei writes its own front matter into the plan before the agent spawns, so
/// the authored [`AGENT_PLAN`] does not say where that is.
fn agent_note_line(plan: &Path, reverted: &str) -> usize {
    let text = fs::read_to_string(plan).expect("plan");
    let heading = reverted.lines().next().expect("the reverted region opens with its heading");
    let task_line = text.lines().position(|line| line == heading).expect("task heading on disk");
    let offset = reverted.lines().position(|line| line.starts_with("#### Visit")).expect("note");
    task_line + offset + 1
}

const AGENT_MACHINE: &str = r#"name: worker-edit-agent
version: 1
states:
  cover:
    initial: true
    description: Leaves a note in its own task body
    attempts: 2
    agent: mock
    agent_timeout: 20s
    instructions: |
      Cover Task {task_id}: {task_title}.
  completed:
    description: Done
    final: true
transitions:
  - { from: cover, to: completed, description: Covered. }
"#;

/// The mock agent keeps every prompt, and on its first attempt writes the note
/// the prompt invites under a heading the prompt forbids.
fn agent_body() -> String {
    format!(
        r#"root = pathlib.Path(env('RHEI_ROOT'))
attempt = env('RHEI_ATTEMPT', 'unknown')
write(root / 'prompts' / ('attempt-' + attempt + '.md'), agent_prompt())
if attempt == '1':
    append(root / 'plan.rhei.md', {NOTE:?})
result('covered.\n')
"#
    )
}

/// The sentence every agent reads says what a heading costs it: its own task
/// body reverted and its attempt spent, not the whole run.
// §FS-rhei-memory.3.4
const TRAIL_SENTENCE: &str = "- Write progress as plain paragraphs or lists, never Markdown headings: a heading inside a task body declares a child task, so one such as `#### Notes` breaks the plan; the run reverts your task body to what it was before this attempt and spends the attempt.";

/// The retry after a reverted edit is told where, why, and where its text went.
// §FS-rhei-memory.3.4 §FS-rhei-memory.4.4 §FS-rhei-agents.3.2
#[test]
fn an_agent_is_told_the_cost_of_a_heading_and_its_retry_where_the_text_went() {
    for mode in [Mode::Sequential, Mode::Parallel] {
        let dir = unique_temp_dir(&format!("worker-edit-agent-{mode:?}"));
        let plan = write_fixture_file(&dir, "plan.rhei.md", AGENT_PLAN);
        let machine = write_fixture_file(&dir, "states.yaml", AGENT_MACHINE);
        let agent = write_python_agent(&dir, "mock-agent.py", &agent_body());
        write_settings(&dir, &agent, false);
        let mut args = vec!["--no-tui", "--no-callbacks"];
        args.extend_from_slice(mode.flags());
        // One ticket alone makes no progress on a reverted attempt, so the
        // retry is the next run's, as for any stalled visit. §FS-rhei-run.3.6
        let first = run_cli("run", &plan, &machine, &args);
        let second = run_cli("run", &plan, &machine, &args);
        let output = format!(
            "mode {mode:?}\nfirst:\n{}{}\nsecond:\n{}{}",
            first.stdout, first.stderr, second.stdout, second.stderr
        );

        let attempt_one = read(&dir.join("prompts/attempt-1.md"), &output);
        assert!(
            attempt_one.contains(TRAIL_SENTENCE),
            "the prompt's trail sentence:\n{attempt_one}"
        );

        let attempt_two = read(&dir.join("prompts/attempt-2.md"), &output);
        let retry = attempt_two
            .lines()
            .find(|line| line.starts_with("Retrying this visit:"))
            .unwrap_or_else(|| panic!("no retry paragraph:\n{attempt_two}"));
        let reverted = read(&dir.join("runtime/logs/task-plan.1-cover.reverted.md"), &output);
        let line = agent_note_line(&plan, &reverted);
        let location = format!("Its edit broke the plan at `plan.rhei.md:{line}`");
        assert!(retry.contains(&location), "got:\n{retry}");
        assert!(retry.contains("(Malformed node heading"), "got:\n{retry}");
        assert!(retry.contains("task-plan.1-cover.reverted.md`."), "got:\n{retry}");
        assert!(second.status.success(), "{output}");
        assert!(!fs::read_to_string(&plan).expect("plan").contains("#### Visit"));
    }
}

fn read(path: &Path, output: &str) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|_| panic!("{} was not written\n{output}", path.display()))
}
