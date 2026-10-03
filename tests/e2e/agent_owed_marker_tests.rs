//! The `unresolved template` marker on a missing result, as the run log's
//! missing-output warning prints it and as the retry prompt's owed clause
//! repeats it. The two are readings of one list, so they give one answer, and
//! that answer is about the path as authored rather than the root it sits
//! under (agent-grounds/rhei#399).

// §FS-rhei-agents.3.2.1 §FS-rhei-memory.4.4

use std::fs;
use std::path::{Path, PathBuf};

use super::agent_reentry_support::write_settings;
use super::*;

const MARKER: &str = ", unresolved template)";

/// A one-task plan in `state`, two attempts, a forward edge into a
/// `final: true` state and nothing declared, so the result is the whole
/// completion condition.
fn machine(state: &str, targets: &str) -> String {
    format!(
        r#"name: owed-marker
version: 1
states:
  "{state}":
    initial: true
    description: Act on a validated ticket
    attempts: 2
    agent_timeout: 20s
{targets}    instructions: |
      Publish Task {{task_id}}.
  tracked:
    description: An upstream issue carries it
    final: true
transitions:
  - {{ from: "{state}", to: tracked, description: The issue is recorded. }}
"#
    )
}

/// The fixture under `root`, with an agent that keeps every prompt it is
/// handed and writes nothing.
fn setup(root: &Path, state: &str, targets: &str) -> (PathBuf, PathBuf) {
    fs::create_dir_all(root).expect("create the fixture root");
    let plan = write_fixture_file(
        root,
        "plan.rhei.md",
        &format!("# Rhei: Owed marker\n\n## Tasks\n\n### Task 1: Publish the ticket\n**State:** {state}\n"),
    );
    let machine = write_fixture_file(root, "states.yaml", &machine(state, targets));
    let agent = write_python_agent(
        root,
        "mock-agent.py",
        r#"root = pathlib.Path(env('RHEI_ROOT'))
name = 'attempt-' + env('RHEI_ATTEMPT', 'unknown') + '-' + env('RHEI_MODEL', 'none') + '.md'
write(root / 'prompts' / name, agent_prompt())
"#,
    );
    write_settings(root, &agent, false);
    (plan, machine)
}

/// The result entries of every missing-output warning both passes printed,
/// and of the owed clause of every attempt-2 prompt.
fn both_surfaces(root: &Path, plan: &Path, machine: &Path) -> (Vec<String>, Vec<String>) {
    let args = ["--no-tui", "--no-callbacks"];
    let mut log = String::new();
    for _ in 0..2 {
        let run = run_cli("run", plan, machine, &args);
        log.push_str(&run.stdout);
        log.push_str(&run.stderr);
    }
    let warning: Vec<String> = log
        .lines()
        .filter_map(|line| {
            line.split_once("required outputs are missing for task ")?.1.split_once("': ")
        })
        .flat_map(|(_, list)| result_entries(list))
        .collect();
    let mut clause = Vec::new();
    for entry in fs::read_dir(root.join("prompts"))
        .unwrap_or_else(|_| panic!("the agent was never spawned:\n{log}"))
    {
        let path = entry.expect("read prompts").path();
        if !path.file_name().is_some_and(|name| name.to_string_lossy().starts_with("attempt-2-")) {
            continue;
        }
        let prompt = fs::read_to_string(&path).expect("read prompt");
        let notice = prompt
            .lines()
            .find(|line| line.starts_with("Retrying this visit:"))
            .unwrap_or_else(|| panic!("{} carries no retry paragraph:\n{prompt}", path.display()));
        let owed = notice.split_once("still owes: ").map(|(_, rest)| rest).unwrap_or("");
        clause.extend(result_entries(owed.split_once(". Its transcript").map_or(owed, |(l, _)| l)));
    }
    assert!(!warning.is_empty(), "no missing-output warning named the result:\n{log}");
    assert!(!clause.is_empty(), "no attempt-2 owed clause named the result:\n{log}");
    (warning, clause)
}

fn result_entries(list: &str) -> Vec<String> {
    list.split("result (")
        .skip(1)
        .map(|rest| format!("result ({}", rest.trim_end_matches(", ")))
        .collect()
}

/// A checked result path under a directory whose name holds a brace: the
/// authored `runtime/results/plan.1.md` has none, so neither surface marks it.
// §FS-rhei-agents.3.2.1
#[test]
fn a_braced_root_marks_the_result_on_neither_surface() {
    let dir = unique_temp_dir("owed-marker-braced-root");
    let root = dir.join("co{br}");
    let (plan, machine) = setup(&root, "publishing", "");

    let (warning, clause) = both_surfaces(&root, &plan, &machine);

    assert!(
        warning.iter().all(|entry| !entry.contains(MARKER)),
        "the warning marks a path it checked as a template: {warning:#?}"
    );
    assert!(clause.iter().all(|entry| !entry.contains(MARKER)), "got {clause:#?}");
}

/// A braced state name with fan-out: the fragment path carries the state name
/// verbatim, so the authored path holds a brace and both surfaces say so.
// §FS-rhei-agents.3.2.1 §FS-rhei-memory.4.4 §FS-rhei-states.3.3
#[test]
fn a_braced_state_name_marks_the_fragment_on_both_surfaces() {
    let dir = unique_temp_dir("owed-marker-braced-state");
    let targets = "    all_targets: [\"mock:local:m1\", \"mock:local:m2\"]\n";
    let (plan, machine) = setup(&dir, "pub{lish}", targets);

    let (warning, clause) = both_surfaces(&dir, &plan, &machine);

    assert!(warning.iter().all(|entry| entry.contains(MARKER)), "got {warning:#?}");
    assert!(
        clause.iter().all(|entry| entry.contains(MARKER)),
        "the owed clause drops the marker the warning prints for the same fragment: {clause:#?}"
    );
}
