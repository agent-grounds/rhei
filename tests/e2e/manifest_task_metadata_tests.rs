//! A task left in a workspace index must fail the common CLI load, whether it
//! would otherwise disappear or become agent context (#696).
//! §FS-rhei-plan-language.1.2

use std::path::Path;

use super::new_tests::flattened_output;
use super::*;

const PACK: &str = "### Task 2: Pack\n**State:** pending\n";

fn readers(ws: &Path, machine: &Path) -> Vec<(&'static str, CliRun)> {
    vec![
        ("validate", run_cli("validate", ws, machine, &[])),
        ("list", run_cli("list", ws, machine, &[])),
        ("render", run_cli("render", ws, machine, &["--format", "json"])),
    ]
}

fn assert_index_refused(index: &str, metadata_line: usize) {
    let (_dir, ws, machine) = create_workspace("rhei-696-manifest", index, &[("02.md", PACK)]);
    // Run all three readers before asserting so the red result demonstrates
    // the common loader, rather than stopping after validate's acceptance.
    let runs = readers(&ws, &machine);
    let accepted: Vec<_> = runs
        .iter()
        .filter(|(_, run)| run.status.success())
        .map(|(command, run)| format!("{command} accepted index:\n{}", flattened_output(run)))
        .collect();
    assert!(accepted.is_empty(), "{}", accepted.join("\n"));

    for (command, run) in runs {
        let said = flattened_output(&run);
        assert!(
            said.contains("workspace/index.rhei.md") || said.contains("workspace\\index.rhei.md"),
            "{command} must name the offending source path: {said}"
        );
        let line = metadata_line.to_string();
        let gutter = format!("{line}|");
        assert!(
            said.contains(&format!("line {line}:"))
                || said.contains(&format!("line {line} when"))
                || said.contains(&format!(":{line}:"))
                || said.contains(&format!(" {line} │"))
                || said.contains(&format!(" {line} |"))
                || said.split_whitespace().any(|word| word == gutter),
            "{command} must identify metadata at original line {line}: {said}"
        );
        assert!(said.contains("tasks/"), "{command} must direct the author to task files: {said}");
    }
}

#[test]
fn readers_refuse_task_metadata_before_index_header() {
    assert_index_refused("### Task 1: Book\n**State:** pending\n\n# Rhei: WS\n", 2);
}

#[test]
fn readers_refuse_task_metadata_before_index_sections() {
    assert_index_refused("# Rhei: WS\n\n### Task 1: Book\n**State:** pending\n", 4);
}

#[test]
fn readers_refuse_task_metadata_inside_index_notes() {
    assert_index_refused(
        "# Rhei: WS\n\n## Notes\n\nSome notes.\n\n### Task 1: Book\n**State:** pending\n",
        8,
    );
}

/// The workspace stays loadable; quoted examples, YAML literal values, prose
/// subheadings and arbitrary bold labels never create or reject a task.
/// §FS-rhei-plan-language.1.2 §FS-rhei-plan-language.2.1
#[test]
fn valid_workspace_and_quoted_manifest_metadata_load() {
    for index in [
        "# Rhei: WS\n",
        "# Rhei: WS\n\n---\nmetadata:\n  example: |\n    ### Task 1: Book\n    **State:** pending\n    ```\n---\n\n\
         ## Notes\n\n### Step 1: Explain\n\n### Task 1: Only a prose heading\n\n\
         **Audience:** authors\n\n\
         ```markdown\n### Task 1: Book\n**State:** pending\n**Prior:** Task 2\n```\n\n\
         ~~~markdown\n### Task 1: Book\n**Assignee:** alice\n~~~\n",
    ] {
        let (_dir, ws, machine) =
            create_workspace("rhei-696-manifest-control", index, &[("02.md", PACK)]);
        for (_, run) in readers(&ws, &machine) {
            assert_success(&run);
        }
        let rendered = render_json(&ws, &machine);
        let tasks = rendered["tasks"].as_array().expect("render tasks");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0]["title"], "Pack");
        if index.contains("## Notes") {
            let notes = rendered["content_sections"][0]["content"].as_str().expect("Notes content");
            assert!(notes.contains("### Step 1: Explain"));
            assert!(notes.contains("**Audience:** authors"));
            assert!(notes.contains("**State:** pending"));
            assert!(notes.contains("**Assignee:** alice"));
        }
    }
}
