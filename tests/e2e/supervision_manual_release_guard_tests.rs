// §FS-rhei-transition-cmd.2 §FS-rhei-transition-cmd.5 §FS-rhei-supervision.3.1
use super::supervision_retry_support::*;
use super::*;

const EDGE: &[&str] = &["--from", "supervising", "--to", "supervising", "--no-callbacks"];

#[test]
fn plain_manual_self_loop_refuses_instead_of_silently_dropping_unread_checkpoint() {
    let (dir, plan, _) = fixture();
    exhaust(&dir, &plan);
    let bytes = fs::read(&plan).unwrap();
    let held = supervision(&plan);
    let ledger = fs::read(dir.join("runtime/state-transitions.log")).unwrap();
    let output = cli(&dir, "transition", "plan.1", EDGE, false);
    assert!(
        !output.status.success(),
        "manual release must refuse; today it silently drops the checkpoint:\n{}\nsupervision before: {held:?}\nsupervision after: {:?}",
        text(&output), supervision(&plan)
    );
    assert_eq!(fs::read(&plan).unwrap(), bytes);
    assert_eq!(supervision(&plan), held);
    assert_eq!(fs::read(dir.join("runtime/state-transitions.log")).unwrap(), ledger);
    for expected in [
        "1 unread checkpoint",
        "plan.1.1",
        "work",
        "completed",
        "releases the subtree",
        "rhei retry plan.1",
        "--release-subtree",
    ] {
        assert!(
            text(&output).contains(expected),
            "refusal must say {expected:?}: {}",
            text(&output)
        );
    }
}

#[test]
fn explicit_manual_release_drops_checkpoint_and_reports_the_loss() {
    let (dir, plan, _) = fixture();
    exhaust(&dir, &plan);
    let mut args = EDGE.to_vec();
    args.push("--release-subtree");
    let output = cli(&dir, "transition", "plan.1", &args, false);
    assert!(output.status.success(), "{}", text(&output));
    let released = supervision(&plan);
    assert_eq!(released["phase"].as_str(), Some("released"));
    assert!(released["checkpoints"].as_sequence().is_none_or(|events| events.is_empty()));
    assert!(
        text(&output).contains("released the subtree of Task plan.1; dropped 1 unread checkpoint"),
        "{}",
        text(&output)
    );
}

#[test]
fn zero_pending_plain_self_loop_still_releases() {
    let (dir, plan, _) = fixture();
    let output = cli(&dir, "transition", "plan.1", EDGE, false);
    assert!(output.status.success(), "{}", text(&output));
    assert_eq!(supervision(&plan)["phase"].as_str(), Some("released"));
}

#[test]
fn zero_pending_explicit_release_is_accepted_as_a_no_op_flag() {
    let (dir, plan, _) = fixture();
    let mut args = EDGE.to_vec();
    args.push("--release-subtree");
    let output = cli(&dir, "transition", "plan.1", &args, false);
    assert!(output.status.success(), "{}", text(&output));
    assert_eq!(supervision(&plan)["phase"].as_str(), Some("released"));
}

// §FS-rhei-supervision.3.4
#[test]
fn manual_worker_release_command_explicitly_names_release_subtree() {
    let (dir, _, _) = fixture();
    let output = cli(&dir, "next", "plan.rhei.md", &["--task", "plan.1", "--peek"], false);
    assert!(output.status.success(), "{}", text(&output));
    let section =
        output.stdout.split("## Supervising This Subtree").nth(1).expect("supervisor notes");
    let command = section
        .lines()
        .find(|line| line.starts_with("rhei") && line.contains("transition"))
        .expect("release command");
    assert!(command.contains("--release-subtree"), "worker release command: {command}");
}
