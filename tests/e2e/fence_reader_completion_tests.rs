//! Completing a valid quoted plan must use the parser's fence boundaries.
//! §FS-rhei-complete.4.2 §FS-rhei-complete.4.3 §FS-rhei-plan-language.2.1

use std::fs;

use super::fence_reader_support::*;
use super::*;

fn complete_below(quotation: &str, workspace: bool) {
    // The issue's two-task transcript: the target is below a quote in its neighbour.
    let neighbour = format!("### Task 1: Quote a transcript\n**State:** open\n\n{quotation}\n");
    let tasks = format!("{neighbour}### Task 2: Next\n**State:** open\n\nKeep target prose.\n");
    let (dir, path) = fixture("rhei-614-complete", workspace, &tasks);
    let index_before = workspace.then(|| fs::read(dir.join("index.rhei.md")).expect("index"));
    assert_success(&run(&dir, &["validate", target(workspace)]));
    let completed = run(
        &dir,
        &[
            "complete",
            target(workspace),
            "--task",
            "2",
            "--result",
            "fence contract recorded",
            "--no-callbacks",
        ],
    );
    assert!(completed.status.success(), "completion below {quotation:?} (workspace={workspace}) must succeed after validation; stderr:\n{}", completed.stderr);
    let after = fs::read_to_string(&path).expect("rewritten task document");
    assert!(after.contains(&neighbour), "quotation and neighbour changed:\n{after}");
    let target_body = after.split_once("### Task 2: Next\n").expect("target heading").1;
    assert!(target_body.contains("**State:** done"), "{after}");
    assert!(target_body.contains("Keep target prose."), "{after}");
    assert_eq!(target_body.matches("> **Result:**").count(), 1, "{after}");
    let listed = run(&dir, &["list", target(workspace)]);
    assert_success(&listed);
    // Read the authored link, since a Directory Workspace derives its id from its directory.
    let link =
        target_body.lines().find(|line| line.starts_with("> **Result:**")).expect("result link");
    let result_path = link.split_once("](").expect("link target").1.trim_end_matches(')');
    let result =
        fs::read_to_string(dir.join(result_path)).expect("result artifact under owning root");
    assert!(result.contains("fence contract recorded"), "{result}");
    assert_success(&run(&dir, &["validate", target(workspace)]));
    if let Some(before) = index_before {
        // Completion may update index frontmatter; its authored title must survive.
        assert!(fs::read(dir.join("index.rhei.md")).expect("index").starts_with(&before));
    }
}

macro_rules! red_completion {
    ($name:ident, $quote:expr, $workspace:expr) => {
        #[test]
        #[ignore = "red until #614: completion must use matching fences; implement removes this"]
        fn $name() {
            complete_below($quote, $workspace);
        }
    };
}

red_completion!(fence_reader_614_complete_transcript_single, TRANSCRIPT, false);
red_completion!(fence_reader_614_complete_transcript_workspace, TRANSCRIPT, true);
red_completion!(fence_reader_614_complete_info_single, INFO_STRING, false);
red_completion!(fence_reader_614_complete_info_workspace, INFO_STRING, true);
red_completion!(fence_reader_614_complete_tilde_single, TILDE, false);
red_completion!(fence_reader_614_complete_tilde_workspace, TILDE, true);

/// §FS-rhei-complete.4.2 §FS-rhei-complete.4.3: ordinary fences are controls.
#[test]
fn fence_reader_614_complete_ordinary_control() {
    for workspace in [false, true] {
        complete_below(CONTROL, workspace);
    }
}
