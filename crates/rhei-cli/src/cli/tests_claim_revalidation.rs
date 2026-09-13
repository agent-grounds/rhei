fn claim_revalidation_machine(source_inputs: bool) -> &'static str {
    if source_inputs {
        r#"name: claim-revalidation
version: 1
states:
  draft:
    initial: true
    description: Setup
    inputs:
      - name: brief
        path: runtime/brief.md
  pending:
    description: Work
  completed:
    final: true
    description: Done
transitions:
  - from: draft
    to: pending
  - from: pending
    to: completed
"#
    } else {
        r#"name: claim-revalidation
version: 1
states:
  pending:
    initial: true
    description: Work
  completed:
    final: true
    description: Done
transitions:
  - from: pending
    to: completed
"#
    }
}

/// Removing a source input after selection is observed by the auto-advancing
/// claim's locked re-read, before state, ownership, metadata, or ledger writes.
/// §FS-rhei-next.3.1 §FS-rhei-next.3.3
#[test]
fn auto_advance_claim_rechecks_source_inputs_after_selection() {
    let dir = tempfile::tempdir().expect("tempdir");
    let plan = dir.path().join("plan.rhei.md");
    let machine = dir.path().join("states.yaml");
    let brief = dir.path().join("runtime/brief.md");
    fs::create_dir_all(brief.parent().unwrap()).expect("runtime");
    fs::write(&brief, "ready\n").expect("brief");
    let original = "# Rhei: Revalidate input\n\n## Tasks\n\n### Task 1: Work\n**State:** draft\n";
    fs::write(&plan, original).expect("plan");
    fs::write(&machine, claim_revalidation_machine(true)).expect("machine");
    let removed = brief.clone();
    set_claim_before_lock_hook(move || fs::remove_file(removed).expect("remove selected input"));

    let error = next_command(&plan, Some(&machine), None, false, true, false, &[])
        .expect_err("the locked re-read must observe the removed source input");

    assert!(error.to_string().contains("Missing required input artifact: brief"));
    assert_eq!(fs::read_to_string(&plan).unwrap(), original);
    assert!(!brief.exists(), "the concurrent input removal must be preserved");
    assert!(!dir.path().join("runtime/state-transitions.log").exists());
}

/// Adding an open descendant after selection makes an already-runnable initial
/// task ineligible under the lock, without overwriting the concurrent edit.
/// §FS-rhei-next.3.1
#[test]
fn in_place_claim_rechecks_non_input_eligibility_after_selection() {
    let dir = tempfile::tempdir().expect("tempdir");
    let plan = dir.path().join("plan.rhei.md");
    let machine = dir.path().join("states.yaml");
    let original = "# Rhei: Revalidate descendants\n\n## Tasks\n\n### Task 1: Parent\n**State:** pending\n";
    let changed = "# Rhei: Revalidate descendants\n\n## Tasks\n\n### Task 1: Parent\n**State:** pending\n\n#### Task 1.1: Concurrent child\n**State:** pending\n";
    fs::write(&plan, original).expect("plan");
    fs::write(&machine, claim_revalidation_machine(false)).expect("machine");
    let changed_path = plan.clone();
    set_claim_before_lock_hook(move || fs::write(changed_path, changed).expect("add child"));

    let error = next_command(&plan, Some(&machine), Some("1"), false, true, false, &[])
        .expect_err("the locked re-read must observe the new open descendant");

    assert!(error.to_string().contains("is no longer claimable"));
    assert_eq!(fs::read_to_string(&plan).unwrap(), changed);
    assert!(!fs::read_to_string(&plan).unwrap().contains("**Assignee:**"));
    assert!(!dir.path().join("runtime/state-transitions.log").exists());
}
