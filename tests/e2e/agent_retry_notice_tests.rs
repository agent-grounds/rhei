//! What a retried attempt is told it still owes: the artifacts of its own
//! completion condition that are not on disk when its prompt is composed, and
//! nothing at all when every one of them is.
//!
//! The run log has always named them correctly. The retry prompt — the one
//! surface a paid attempt reads — named the result path whenever any edge out
//! of the state reached a terminal one, whatever was actually missing, and
//! pasted that same file's contents four lines above the sentence claiming it
//! was never written (agent-grounds/rhei#376).

// §FS-rhei-memory.3.3 §FS-rhei-memory.4.4 §FS-rhei-agents.3.2.1

use std::fs;
use std::path::{Path, PathBuf};

use super::agent_reentry_support::write_settings;
use super::*;

const PLAN: &str = r#"# Rhei: Retry notice

## Tasks

### Task 1: Publish the ticket
**State:** publishing
"#;

/// The shape of `publishing` in the `grounded-ticket` machine this repository
/// is maintained with: one declared output, and a forward edge into a
/// `final: true` state, so the completion condition is that output plus the
/// ticket's result.
const DECLARED_OUTPUT_MACHINE: &str = r#"name: retry-notice
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

/// The same state with nothing declared, so the ticket's result is the whole
/// completion condition — the case the old wording was written for.
const RESULT_ONLY_MACHINE: &str = r#"name: retry-notice-result-only
version: 1
states:
  publishing:
    initial: true
    description: Act on a validated ticket
    attempts: 2
    agent: mock
    agent_timeout: 20s
    instructions: |
      Publish Task {task_id}: {task_title}.
  tracked:
    description: An upstream issue carries it
    final: true
transitions:
  - { from: publishing, to: tracked, description: The issue is recorded. }
"#;

const OWES: &str = " It did not write what this visit still owes: ";

fn setup(name: &str, machine: &str, agent_body: &str) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(name);
    let plan = write_fixture_file(&dir, "plan.rhei.md", PLAN);
    let machine = write_fixture_file(&dir, "states.yaml", machine);
    // Keep the prompt of every attempt: the notice is composed for the attempt
    // rather than logged by the run, so the prompt is the only place to read it.
    let agent = write_python_agent(
        &dir,
        "mock-agent.py",
        &format!(
            r#"root = pathlib.Path(env('RHEI_ROOT'))
attempt = env('RHEI_ATTEMPT', 'unknown')
write(root / 'prompts' / ('attempt-' + attempt + '.md'), agent_prompt())
{agent_body}"#
        ),
    );
    write_settings(&dir, &agent, false);
    (dir, plan, machine)
}

/// The two passes the notice needs: the first stalls the visit, the second
/// re-enters it as attempt 2, which is where the paragraph is composed.
fn prompt_handed_to_attempt_two(dir: &Path, plan: &Path, machine: &Path) -> String {
    let args = ["--no-tui", "--no-callbacks"];
    let first = run_cli("run", plan, machine, &args);
    let second = run_cli("run", plan, machine, &args);
    fs::read_to_string(dir.join("prompts/attempt-2.md")).unwrap_or_else(|_| {
        panic!(
            "no attempt 2 was spawned, so no retry notice was composed\n\
             first pass:\n{}{}\nsecond pass:\n{}{}",
            first.stdout, first.stderr, second.stdout, second.stderr
        )
    })
}

/// The retry paragraph alone. It is one line, and every assertion here is about
/// what that line claims — not about the rest of `## Previous Visits`, which
/// pastes the result file this sentence used to contradict.
fn retry_paragraph(prompt: &str) -> String {
    prompt
        .lines()
        .find(|line| line.starts_with("Retrying this visit:"))
        .unwrap_or_else(|| panic!("the prompt carries no retry paragraph:\n{prompt}"))
        .to_string()
}

/// The owed clause's entries, in the order the paragraph lists them, as
/// `(name, path as the prompt spelled it)`. Empty where the clause is absent.
fn owed_entries(notice: &str) -> Vec<(String, String)> {
    let Some(rest) = notice.split_once(OWES).map(|(_, rest)| rest) else { return Vec::new() };
    let list = rest.split_once(". Its transcript is").map(|(list, _)| list).unwrap_or(rest);
    list.split(", ")
        .map(|entry| {
            let (name, path) = entry
                .split_once(" (`")
                .unwrap_or_else(|| panic!("entry '{entry}' is not `<name> (`<path>`)`"));
            (name.to_string(), path.trim_end_matches("`)").to_string())
        })
        .collect()
}

/// The path a prompt-spelled path points at, whichever of the two spellings of
/// §FS-rhei-agents.4.1 this layout calls for. The test never writes a
/// separator of its own. §REQ-cross-platform
fn resolved(root: &Path, shown: &str) -> PathBuf {
    let path = Path::new(shown);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn joined(root: &Path, parts: &[&str]) -> PathBuf {
    let mut path = root.to_path_buf();
    for part in parts {
        path.push(part);
    }
    path
}

/// The path `## Result` names this invocation — the standing obligation, from
/// which the owed clause's `result` entry must not differ by one character.
fn result_path_shown(prompt: &str) -> String {
    let section = prompt
        .split_once("\n## Result\n")
        .map(|(_, rest)| rest)
        .unwrap_or_else(|| panic!("the prompt has no `## Result` section:\n{prompt}"));
    section
        .lines()
        .find_map(|line| line.strip_prefix("- `")?.strip_suffix('`'))
        .unwrap_or_else(|| panic!("`## Result` names no path:\n{section}"))
        .to_string()
}

/// Every path in the paragraph is spelled by one rule, so the obligation and
/// the transcript beside it cannot read as two different bases.
// §FS-rhei-agents.4.1 §FS-rhei-memory.4.4
fn assert_one_spelling(notice: &str) {
    let transcript = notice
        .split_once("Its transcript is `")
        .and_then(|(_, rest)| rest.split_once('`'))
        .map(|(path, _)| path.to_string())
        .unwrap_or_else(|| panic!("the paragraph names no transcript:\n{notice}"));
    let base = Path::new(&transcript).is_absolute();
    for (name, shown) in owed_entries(notice) {
        assert_eq!(
            Path::new(&shown).is_absolute(),
            base,
            "'{name}' is spelled against a different base than the transcript beside it; \
             got:\n{notice}"
        );
    }
}

/// The ticket verbatim: the declared output is absent, the result is on disk,
/// and the notice named the result.
// §FS-rhei-memory.3.3 §FS-rhei-memory.4.4
#[test]
fn the_notice_names_the_declared_output_that_is_missing_not_the_result_on_disk() {
    let (dir, plan, machine) = setup(
        "retry-notice-declared-output",
        DECLARED_OUTPUT_MACHINE,
        "result('attempt ' + attempt + ' wrote the result and left the output absent.\\n')\n",
    );

    let prompt = prompt_handed_to_attempt_two(&dir, &plan, &machine);
    let notice = retry_paragraph(&prompt);

    assert!(
        dir.join("runtime/results/plan.1.md").exists(),
        "the fixture only reproduces the defect while the result is the file on disk"
    );
    assert!(
        !dir.join("runtime/triage/plan.1.issue.md").exists(),
        "the declared output is what must be missing"
    );
    let owed = owed_entries(&notice);
    assert_eq!(
        owed.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>(),
        ["issue"],
        "the notice must name the artifact that is unmet, and only it, as the run log does; \
         got:\n{notice}"
    );
    assert_eq!(
        resolved(&dir, &owed[0].1),
        joined(&dir, &["runtime", "triage", "plan.1.issue.md"]),
        "and must name the path it checked; got:\n{notice}"
    );
    assert!(
        !notice.contains("plan.1.md`"),
        "the result is on disk and its contents are pasted four lines above this sentence; \
         naming it is the defect; got:\n{notice}"
    );
    assert_one_spelling(&notice);
}

/// The case the old wording was written for, and the one the narrowed defence
/// of §FS-rhei-memory.3.3 protects: nothing is declared, so the result is the
/// whole completion condition and the result is what the notice must name.
///
/// A recomputed owed list reaches it only if the terminal-edge question is
/// asked the way `## Result` asks it. No transition has been selected when a
/// prompt is composed, so a collector handed that absence verbatim drops the
/// result and tells this retry nothing at all.
// §FS-rhei-memory.3.3 §FS-rhei-memory.4.4
#[test]
fn the_notice_still_names_the_result_when_the_result_is_what_went_missing() {
    let (dir, plan, machine) = setup("retry-notice-result-only", RESULT_ONLY_MACHINE, "pass\n");

    let prompt = prompt_handed_to_attempt_two(&dir, &plan, &machine);
    let notice = retry_paragraph(&prompt);

    assert!(
        !dir.join("runtime/results/plan.1.md").exists(),
        "the result is what must be missing here"
    );
    let owed = owed_entries(&notice);
    assert_eq!(
        owed.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>(),
        ["result"],
        "the terminal result is still named, under the name the warning gives it; got:\n{notice}"
    );
    // One obligation, two surfaces, one visit count: `## Result` says where a
    // finished task's result is read from and this clause says it is unwritten,
    // and they may not name two files. §FS-rhei-memory.4.4
    assert_eq!(
        owed[0].1,
        result_path_shown(&prompt),
        "the owed result and the result `## Result` names must be one path, spelled once; \
         got:\n{notice}"
    );
    assert_one_spelling(&notice);
}

/// Both unmet, in declaration order with the result last, comma-separated —
/// the order the missing-output warning already prints.
// §FS-rhei-memory.4.4 §FS-rhei-agents.3.2.1
#[test]
fn the_notice_lists_every_unmet_artifact_in_declaration_order() {
    let (dir, plan, machine) =
        setup("retry-notice-both-missing", DECLARED_OUTPUT_MACHINE, "pass\n");

    let prompt = prompt_handed_to_attempt_two(&dir, &plan, &machine);
    let notice = retry_paragraph(&prompt);

    let owed = owed_entries(&notice);
    assert_eq!(
        owed.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>(),
        ["issue", "result"],
        "declared outputs come first, in declaration order, and the result last; got:\n{notice}"
    );
    assert_eq!(resolved(&dir, &owed[0].1), joined(&dir, &["runtime", "triage", "plan.1.issue.md"]));
    assert_eq!(owed[1].1, result_path_shown(&prompt));
    assert!(
        notice.contains(&format!("{OWES}{} (`", owed[0].0)),
        "the clause opens the list once; got:\n{notice}"
    );
    assert_one_spelling(&notice);
}

/// Nothing is owed, so the paragraph claims nothing about any file: the retry
/// is told that it is a retry and where the last transcript is, and no more.
///
/// The stall here is an attempt that wrote everything and then exited
/// non-zero — the reachable shape of an empty owed list, since an attempt that
/// exits 0 having written everything is work the next pass reuses rather than
/// spawns again (§FS-rhei-agents.3.2). What the clause does on an exit-0 stall
/// with nothing owed is pinned next door, in `tests_prompt_memory_visits.rs`.
// §FS-rhei-memory.3.3 §FS-rhei-memory.4.4
#[test]
fn the_notice_names_no_file_when_nothing_is_unmet() {
    let (dir, plan, machine) = setup(
        "retry-notice-nothing-owed",
        DECLARED_OUTPUT_MACHINE,
        r#"write(root / 'runtime' / 'triage' / 'plan.1.issue.md', 'the issue\n')
result('attempt ' + attempt + ' wrote everything and then fell over.\n')
sys.exit(3)
"#,
    );

    let prompt = prompt_handed_to_attempt_two(&dir, &plan, &machine);
    let notice = retry_paragraph(&prompt);

    assert!(dir.join("runtime/triage/plan.1.issue.md").exists(), "the output is on disk");
    assert!(dir.join("runtime/results/plan.1.md").exists(), "the result is on disk");
    assert!(
        notice.starts_with("Retrying this visit: attempt 2."),
        "the retry is still told that it is one; got:\n{notice}"
    );
    assert!(
        notice.contains("Its transcript is `"),
        "and where the last attempt's transcript is; got:\n{notice}"
    );
    assert!(
        !notice.contains("It did not write"),
        "naming no file beats naming a file that is there; got:\n{notice}"
    );
}
