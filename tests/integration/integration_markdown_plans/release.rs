// §FS-rhei-release: `rhei release` drops a claim without touching state,
// artifacts, or the ledger.

const RELEASE_MACHINE: &str = r#"name: release-test
version: 1
states:
  pending:
    description: Ready
    initial: true
  in-progress:
    description: Active
  completed:
    description: Done
    final: true
transitions:
  - from: pending
    to: in-progress
  - from: in-progress
    to: completed
"#;

fn run_release(args: &[&std::ffi::OsStr]) -> std::process::Output {
    rhei_command()
        .arg("release")
        .args(args)
        .output()
        .expect("release command should run")
}

/// §FS-rhei-release.3: an abandoned claim is dropped and the ticket becomes
/// claimable again, with its state and body untouched.
#[test]
fn release_drops_a_claim_and_unwedges_the_queue() {
    let dir = unique_temp_dir("release-single");
    let plan_path = write_fixture_file(
        &dir,
        "plan.rhei.md",
        "# Rhei: Work\n\n## Tasks\n\n### Task 1: Alpha\n**State:** pending\n\
         **Assignee:** codex\n\nBody text.\n\n### Task 2: Beta\n**State:** pending\n",
    );
    let machine_path = write_fixture_file(&dir, "states.yaml", RELEASE_MACHINE);

    let output = run_release(&[
        "--state-machine".as_ref(),
        machine_path.as_os_str(),
        plan_path.as_os_str(),
        "--task".as_ref(),
        "1".as_ref(),
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "release should succeed: {stdout}");
    assert!(
        stdout.contains("Released Task plan.1 (was assigned to codex)"),
        "release names the ticket and the claim it dropped: {stdout}"
    );

    let updated = fs::read_to_string(&plan_path).expect("read plan");
    assert!(!updated.contains("**Assignee:**"), "the claim is gone:\n{updated}");
    assert!(updated.contains("**State:** pending"), "the state is untouched:\n{updated}");
    assert!(updated.contains("Body text."), "the body is untouched:\n{updated}");
}

/// §FS-rhei-release.3: releasing an unclaimed ticket is an error — it almost
/// always means the wrong id was typed — and `--task` with `--all` is refused.
#[test]
fn release_refuses_an_unclaimed_ticket_and_ambiguous_flags() {
    let dir = unique_temp_dir("release-guards");
    let plan_path = write_fixture_file(
        &dir,
        "plan.rhei.md",
        "# Rhei: Work\n\n## Tasks\n\n### Task 1: Alpha\n**State:** pending\n",
    );
    let machine_path = write_fixture_file(&dir, "states.yaml", RELEASE_MACHINE);

    let unclaimed = run_release(&[
        "--state-machine".as_ref(),
        machine_path.as_os_str(),
        plan_path.as_os_str(),
        "--task".as_ref(),
        "1".as_ref(),
    ]);
    assert!(!unclaimed.status.success(), "an unclaimed ticket is an error");
    assert!(
        String::from_utf8_lossy(&unclaimed.stderr).contains("holds no claim"),
        "the error says the ticket holds no claim"
    );

    let both = run_release(&[
        plan_path.as_os_str(),
        "--task".as_ref(),
        "1".as_ref(),
        "--all".as_ref(),
    ]);
    assert!(!both.status.success(), "--task with --all is ambiguous");

    let neither = run_release(&[plan_path.as_os_str()]);
    assert!(!neither.status.success(), "neither --task nor --all is ambiguous");
}

/// §FS-rhei-release.3: `--all` sweeps claimed non-terminal tickets, leaves a
/// finished ticket's assignee as the record of who finished it, and `--dry-run`
/// changes nothing.
#[test]
fn release_all_spares_terminal_tickets_and_honours_dry_run() {
    let dir = unique_temp_dir("release-all");
    let plan_path = write_fixture_file(
        &dir,
        "plan.rhei.md",
        "# Rhei: Work\n\n## Tasks\n\n### Task 1: Alpha\n**State:** pending\n\
         **Assignee:** codex\n\n### Task 2: Beta\n**State:** completed\n**Assignee:** claude\n\
         \n### Task 3: Gamma\n**State:** pending\n",
    );
    let machine_path = write_fixture_file(&dir, "states.yaml", RELEASE_MACHINE);
    let before = fs::read_to_string(&plan_path).expect("read plan");

    let dry = run_release(&[
        "--state-machine".as_ref(),
        machine_path.as_os_str(),
        plan_path.as_os_str(),
        "--all".as_ref(),
        "--dry-run".as_ref(),
    ]);
    let dry_stdout = String::from_utf8_lossy(&dry.stdout);
    assert!(dry.status.success(), "dry run should succeed: {dry_stdout}");
    assert!(dry_stdout.contains("Would release Task plan.1"), "dry run previews: {dry_stdout}");
    assert!(
        !dry_stdout.contains("plan.2"),
        "a terminal ticket's assignee is a record, not a claim: {dry_stdout}"
    );
    assert_eq!(
        fs::read_to_string(&plan_path).expect("read plan"),
        before,
        "dry run must not rewrite the plan"
    );

    let output = run_release(&[
        "--state-machine".as_ref(),
        machine_path.as_os_str(),
        plan_path.as_os_str(),
        "--all".as_ref(),
    ]);
    assert!(output.status.success(), "release --all should succeed");

    let updated = fs::read_to_string(&plan_path).expect("read plan");
    let rhei = parse(&updated).expect("parse released plan");
    assert_eq!(rhei.tasks[0].assignee, None, "the claimed pending ticket is released");
    assert_eq!(
        rhei.tasks[1].assignee.as_deref(),
        Some("claude"),
        "the completed ticket keeps its record"
    );
}

/// §FS-rhei-release.3.1: released from a non-initial state, the ticket is
/// unclaimed but not yet claimable — say so and name the transition, rather
/// than silently discarding a transition that already ran.
#[test]
fn release_explains_a_ticket_left_past_the_initial_state() {
    let dir = unique_temp_dir("release-mid-state");
    let plan_path = write_fixture_file(
        &dir,
        "plan.rhei.md",
        "# Rhei: Work\n\n## Tasks\n\n### Task 1: Alpha\n**State:** in-progress\n\
         **Assignee:** codex\n",
    );
    let machine_path = write_fixture_file(&dir, "states.yaml", RELEASE_MACHINE);

    let output = run_release(&[
        "--state-machine".as_ref(),
        machine_path.as_os_str(),
        plan_path.as_os_str(),
        "--task".as_ref(),
        "1".as_ref(),
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "release should succeed: {stdout}");
    // The undo names the target and machine release was given. §FS-rhei-errors.1.2.1
    let undo = format!(
        "rhei transition {} --task plan.1 --from in-progress --to pending --state-machine {}",
        rhei_core::platform::shell_quote(&plan_path.display().to_string()),
        rhei_core::platform::shell_quote(&machine_path.display().to_string())
    );
    assert!(
        stdout.contains("still in 'in-progress'") && stdout.contains(&undo),
        "the note names the exact transition back: {stdout}"
    );

    let updated = fs::read_to_string(&plan_path).expect("read plan");
    assert!(!updated.contains("**Assignee:**"), "the claim is still dropped");
    assert!(updated.contains("**State:** in-progress"), "the state is left alone");
}
