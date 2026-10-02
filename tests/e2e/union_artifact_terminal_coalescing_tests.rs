//! Terminal coalescing retains the target definition even when outputs differ.
//! Only writers of the checked path can preserve an internal pair; an incoming
//! output the target definition discards cannot introduce a pair.
//! §FS-rhei-library.7.2.1 §FS-rhei-library.7.2.4

use std::path::Path;

use super::into_support::{read, run_into};
use super::union_artifact_paths_support::*;
use super::*;

const NOTE: &str = "runtime/notes/plan.md";
const OTHER: &str = "runtime/notes/other.md";
const WRITERS_HELP: &str = "help: give the two states distinct artifact paths, or keep one of \
                            them as the path's writer and have the other list it under `inputs:`.";

fn set_terminal_output(root: &Path, path: &str) {
    let machine = root.join("states.yaml");
    let original = "  completed:\n    final: true\n    description: Done\n";
    let replacement = format!("{original}    outputs:\n      - name: note\n        path: {path}\n");
    let text = read(&machine);
    assert!(text.contains(original), "fixture has its ordinary completed terminal");
    std::fs::write(machine, text.replace(original, &replacement)).expect("set terminal output");
}

fn machine_value(root: &Path) -> serde_yaml::Value {
    serde_yaml::from_str(&read(&root.join("states.yaml"))).expect("fixture machine parses")
}

fn assert_retained_single_writer(root: &Path, before: &serde_yaml::Value, writer: &str) {
    let machine = machine_value(root);
    assert_eq!(machine["states"]["completed"], before["states"]["completed"]);
    let writers: Vec<&str> = machine["states"]
        .as_mapping()
        .expect("states mapping")
        .iter()
        .filter(|(_, state)| {
            state["outputs"].as_sequence().is_some_and(|outputs| {
                outputs.iter().any(|output| output["path"].as_str() == Some(NOTE))
            })
        })
        .map(|(name, _)| name.as_str().expect("state name"))
        .collect();
    assert_eq!(writers, [writer], "only the retained nonterminal writes this path");
    assert!(!root.join(NOTE).exists(), "instantiation does not create the declared output");
    assert!(!root.join(OTHER).exists(), "instantiation does not create the other output");
}

/// The retained terminal writes NOTE but the incoming terminal writes nothing
/// or a different path. The new completed/polish pair is refused.
/// §FS-rhei-library.7.2.1
#[test]
fn placement_refuses_a_retained_terminal_writer_missing_from_the_incoming_path() {
    for incoming_output in [None, Some(OTHER)] {
        let (dir, root) = target_with("artifact-terminal-into-collision", &[("pending", &[])]);
        set_terminal_output(&root, NOTE);
        let template = placed(&dir, "drafts", &[("polish", &[Writes(NOTE)])]);
        if let Some(path) = incoming_output {
            set_terminal_output(&template, path);
        }
        let before = snapshot(&root);

        let result = run_into(&["instantiate", "drafts", "--into", "release"], &dir);
        assert!(!result.status.success(), "coalescing is not shared writer membership");
        assert_stderr_contains(
            &result,
            &format!(
                "states 'completed' (in the target) and 'polish' (in template 'drafts') both \
                 declare the rhei-scoped artifact path '{NOTE}' in `outputs:`"
            ),
        );
        assert_stderr_contains(&result, WRITERS_HELP);
        assert_eq!(before, snapshot(&root), "refusal precedes writes");
    }
}

/// Includes uses the same retained terminal and path-specific membership,
/// naming the entry and leaving no output directory. §FS-rhei-library.7.2.1
#[test]
fn includes_refuse_a_retained_terminal_writer_missing_from_the_incoming_path() {
    for incoming_output in [None, Some(OTHER)] {
        let dir = unique_temp_dir("artifact-terminal-includes-collision");
        let template = part(&dir, "drafts", &[("polish", &[Writes(NOTE)])]);
        if let Some(path) = incoming_output {
            set_terminal_output(&template, path);
        }
        let host = ticket_host(&dir, &[], &["polish"], &["../drafts"]);
        set_terminal_output(&host, NOTE);
        let before = snapshot(&host);

        let result = run_into(&["instantiate", "ticket-host", "--output", "out"], &dir);
        assert!(!result.status.success(), "coalescing is not shared writer membership");
        assert_stderr_contains(
            &result,
            &format!(
                "`includes:` entry '../drafts' of template 'ticket-host': states 'completed' \
                 (in 'ticket-host') and 'polish' (in '../drafts') both declare the rhei-scoped \
                 artifact path '{NOTE}' in `outputs:`"
            ),
        );
        assert_stderr_contains(&result, WRITERS_HELP);
        assert_eq!(before, snapshot(&host), "the template remains unchanged");
        assert!(!dir.join("out").exists(), "refusal leaves no output directory");
    }
}

/// In the reverse direction the incoming terminal's NOTE output is discarded
/// with its definition. A separate incoming writer still collides with pending.
/// §FS-rhei-library.7.2.1 §FS-rhei-library.7.2.4
#[test]
fn placement_checks_actual_writers_after_retaining_the_target_terminal() {
    for target_output in [None, Some(OTHER)] {
        for polish_writes in [false, true] {
            let (dir, root) =
                target_with("artifact-terminal-into-retention", &[("pending", &[Writes(NOTE)])]);
            if let Some(path) = target_output {
                set_terminal_output(&root, path);
            }
            let polish: &[Decl] = if polish_writes { &[Writes(NOTE)] } else { &[] };
            let template = placed(&dir, "drafts", &[("polish", polish)]);
            set_terminal_output(&template, NOTE);
            let before = snapshot(&root);
            let before_machine = machine_value(&root);

            let result = run_into(&["instantiate", "drafts", "--into", "release"], &dir);
            if polish_writes {
                assert!(!result.status.success(), "pending/polish is a new writer pair");
                assert_stderr_contains(
                    &result,
                    &format!(
                        "states 'pending' (in the target) and 'polish' (in template 'drafts') \
                         both declare the rhei-scoped artifact path '{NOTE}' in `outputs:`"
                    ),
                );
                assert_stderr_contains(&result, WRITERS_HELP);
                assert_eq!(before, snapshot(&root), "refusal precedes writes");
            } else {
                assert_success(&result);
                assert!(!result.stderr.contains(NOTE), "one retained writer is silent");
                assert_retained_single_writer(&root, &before_machine, "pending");
            }
        }
    }
}

/// An included terminal's discarded output neither creates a collision nor
/// excuses a separate writer against the including supervisor.
/// §FS-rhei-library.7.2.1 §FS-rhei-library.7.2.4
#[test]
fn includes_check_actual_writers_after_retaining_the_target_terminal() {
    for target_output in [None, Some(OTHER)] {
        for polish_writes in [false, true] {
            let dir = unique_temp_dir("artifact-terminal-includes-retention");
            let polish: &[Decl] = if polish_writes { &[Writes(NOTE)] } else { &[] };
            let template = part(&dir, "drafts", &[("polish", polish)]);
            set_terminal_output(&template, NOTE);
            let host = ticket_host(&dir, &[Writes(NOTE)], &["polish"], &["../drafts"]);
            if let Some(path) = target_output {
                set_terminal_output(&host, path);
            }
            let before = snapshot(&host);
            let before_machine = machine_value(&host);

            let result = run_into(&["instantiate", "ticket-host", "--output", "out"], &dir);
            assert_eq!(before, snapshot(&host), "the template remains unchanged");
            if polish_writes {
                assert!(!result.status.success(), "supervising/polish is a new writer pair");
                assert_stderr_contains(
                    &result,
                    &format!(
                        "`includes:` entry '../drafts' of template 'ticket-host': states \
                         'supervising' (in 'ticket-host') and 'polish' (in '../drafts') both \
                         declare the rhei-scoped artifact path '{NOTE}' in `outputs:`"
                    ),
                );
                assert_stderr_contains(&result, WRITERS_HELP);
                assert!(!dir.join("out").exists(), "refusal leaves no output directory");
            } else {
                assert_success(&result);
                assert!(!result.stderr.contains(NOTE), "one retained writer is silent");
                assert_retained_single_writer(&dir.join("out"), &before_machine, "supervising");
            }
        }
    }
}
