//! Cancellation counterexamples must agree at identity and mounted boundaries.
//! §FS-rhei-library.4–5 §FS-rhei-states.1.4 §FS-rhei-transitions.4.5–6
use super::block_composition_support::{assert_failed_with, run_compose};
use super::*;
use std::fs;
use std::path::{Path, PathBuf};

fn instantiate(root: &Path, name: &str, states: &str, mounted: bool) -> (PathBuf, String) {
    let block = root.join(name);
    fs::create_dir_all(block.join("tasks")).unwrap();
    fs::write(block.join("template.yaml"), format!("name: {name}\nversion: 1\ndescription: Cancellation fixture\nports: {{entry: work, exits: {{done: done}}}}\n")).unwrap();
    fs::write(block.join("index.rhei.md"), format!("# Rhei: Cancellation\n**States:** {name}\n"))
        .unwrap();
    fs::write(block.join("tasks/job.md"), "### Task job: Work\n**State:** work\n").unwrap();
    fs::write(block.join("states.yaml"), format!("name: {name}\nversion: 1\n{states}\n")).unwrap();
    let output = root.join(format!("out-{name}"));
    let mount = format!("a={}", block.display());
    let args = if mounted {
        vec!["instantiate", "--mount", &mount, "--output", output.to_str().unwrap()]
    } else {
        vec!["instantiate", block.to_str().unwrap(), "--output", output.to_str().unwrap()]
    };
    assert_success(&run_compose(root, &args));
    (output, if mounted { "m1_a__".into() } else { String::new() })
}

#[test]
fn block_cancellation_waives_missing_outputs_but_success_still_requires_them() {
    let root = unique_temp_dir("block-cancel-artifact");
    for mounted in [false, true] {
        let (output, prefix) = instantiate(
            &root,
            if mounted { "mounted" } else { "identity" },
            r#"
states:
  work: {outputs: [{name: report, path: runtime/report.md}]}
  done: {final: true}
  cancelled: {final: true}
transitions: [{from: work, to: done}, {from: '*', to: cancelled}]
profiles: {primary: {initial: work, allowed: [work, done, cancelled]}}
node_policy: {root: primary, default: primary}
"#,
            mounted,
        );
        let task = format!("{prefix}job");
        let from = format!("{prefix}work");
        let done = format!("{prefix}done");
        let cancel = format!("{prefix}cancelled");
        let args = [
            "transition",
            output.to_str().unwrap(),
            "--task",
            &task,
            "--from",
            &from,
            "--to",
            &done,
            "--result",
            "Done",
            "--no-callbacks",
        ];
        assert_failed_with(
            &run_compose(&root, &args),
            &["Missing required output artifact", "report"],
        );
        let mut args = args;
        args[7] = &cancel;
        assert_success(&run_compose(&root, &args));
    }
}

#[test]
fn block_cancellation_is_not_a_completion_target_or_automatic_fallback() {
    let root = unique_temp_dir("block-cancel-progress");
    for mounted in [false, true] {
        let (output, prefix) = instantiate(
            &root,
            if mounted { "mounted" } else { "identity" },
            r#"
states: {work: {}, middle: {}, done: {final: true}, cancelled: {final: true}}
transitions:
  - {from: work, to: middle, condition: 'visitCount > 9'}
  - {from: middle, to: done}
  - {from: '*', to: cancelled}
profiles: {primary: {initial: work, allowed: [work, middle, done, cancelled]}}
node_policy: {root: primary, default: primary}
"#,
            mounted,
        );
        assert_failed_with(
            &run_compose(
                &root,
                &[
                    "complete",
                    output.to_str().unwrap(),
                    "--task",
                    &format!("{prefix}job"),
                    "--result",
                    "Done",
                    "--no-callbacks",
                ],
            ),
            &["no transition to a terminal state"],
        );
        let run =
            run_compose(&root, &["run", output.to_str().unwrap(), "--no-tui", "--no-callbacks"]);
        assert!(
            !run.status.success(),
            "conditional edge must leave work stalled: {} {}",
            run.stdout,
            run.stderr
        );
        let task_path =
            if mounted { output.join("tasks/m1_a__/job.md") } else { output.join("tasks/job.md") };
        assert!(fs::read_to_string(task_path)
            .unwrap()
            .contains(&format!("**State:** {prefix}work")));
    }
}
