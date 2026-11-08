//! Successful live gate releases must explain the possibly held visits budget.
//! §FS-rhei-transition-cmd.3.4 §FS-rhei-run.3

use super::live_gate_budget_support::*;
use super::*;
use std::fs;

fn raised_budget_release(project: bool) {
    let fixture = BudgetFixture::new(project);
    let run_input = if project { fixture.dir.to_path_buf() } else { fixture.input.clone() };
    assert_success(&run_cli_without_machine("validate", &run_input, &[]));
    let mut live = fixture.start(&run_input, "live");
    let pid = live.child().id();
    wait_for_budget_run(&mut live, "gate after visit 2", || {
        fixture.gated() && fixture.visits() == 2
    });
    assert!(live.child().try_wait().unwrap().is_none(), "keepalive must hold the run open");
    let original = fixture.descriptor(&fixture.dir);
    assert_eq!(original["pid"], pid);

    let machine = fs::read_to_string(&fixture.machine).unwrap();
    fs::write(&fixture.machine, machine.replace("visits: 2", "visits: 5")).unwrap();
    assert_success(&run_cli_without_machine("validate", &run_input, &[]));
    // A project run is addressed through its member: there is no member run.json.
    let release = fixture.transition(&fixture.input, "human-review", "work");
    assert_release_success(&release, fixture.id, "human-review", "work");
    wait_for_budget_run(&mut live, "the same run re-gating at visit 3", || {
        fixture.gated() && fixture.visits() == 3
    });
    assert!(live.child().try_wait().unwrap().is_none());
    assert_eq!(live.child().id(), pid);
    let after = fixture.descriptor(&fixture.dir);
    assert_eq!(after["id"], original["id"]);
    assert_eq!(after["pid"], original["pid"]);
    assert_eq!(after["status"], "running");
    // No member was added. Preserve the historical control: a fresh run reads 5.
    write_fixture_file(&fixture.member, "finish", "finished\n");
    finish_budget_run(&mut live);
    let second_release = fixture.transition(&fixture.input, "human-review", "work");
    assert_release_success(&second_release, fixture.id, "human-review", "work");
    assert_no_advisory(&second_release);
    let mut fresh = fixture.start(&run_input, "fresh");
    finish_budget_run(&mut fresh);
    assert!(fixture.gated());
    assert_eq!(fixture.visits(), 5, "a fresh run must honour the raised budget");

    // Last, so today's red failure is the missing diagnostic, after both controls.
    assert_advisory(&release, fixture.id, &fixture.dir, original["id"].as_str());
}

#[test]
fn issue_653_live_gate_release_explains_held_visits_budget() {
    raised_budget_release(false);
}

#[test]
fn issue_653_member_release_explains_enclosing_live_run_budget() {
    raised_budget_release(true);
}

fn held_scope_release(project_lock: bool) {
    let fixture = BudgetFixture::new(true);
    fixture.set_source("human-review");
    let root = if project_lock { fixture.dir.to_path_buf() } else { fixture.member.clone() };
    let _held = run_lock(&root, true);
    // Probe project-only and member-only locks separately. No mismatch is known,
    // so the advisory must be conservative even with no machine edit.
    for input in [&fixture.input, &*fixture.dir] {
        let release = fixture.transition(input, "human-review", "work");
        assert_release_success(&release, fixture.id, "human-review", "work");
        fixture.set_source("human-review");
        assert_advisory(&release, fixture.id, &root, None);
    }
}

#[test]
fn issue_653_gate_release_detects_project_activity_without_member_lock() {
    held_scope_release(true);
}

#[test]
fn issue_653_gate_release_detects_member_activity_without_project_lock() {
    held_scope_release(false);
}

#[test]
fn issue_653_gate_release_without_live_run_has_no_advisory() {
    for stale in [false, true] {
        let fixture = BudgetFixture::new(true);
        fixture.set_source("human-review");
        if stale {
            // File existence and even a live pid in old ownership bytes are insufficient.
            for root in [&*fixture.dir, &fixture.member] {
                let _unlocked = run_lock(root, false);
                fs::write(
                    root.join(".rhei/run.lock"),
                    serde_json::json!({
                        "version": 1, "id": "stale-run", "pid": std::process::id(),
                        "workspace": root,
                    })
                    .to_string(),
                )
                .unwrap();
            }
        }
        let release = fixture.transition(&fixture.input, "human-review", "work");
        assert_release_success(&release, fixture.id, "human-review", "work");
        assert_no_advisory(&release);
    }
}

#[test]
fn issue_653_gate_release_ignores_unrelated_member_activity() {
    let fixture = BudgetFixture::new(true);
    fixture.set_source("human-review");
    let other = fixture.dir.join("other");
    fs::create_dir_all(&other).unwrap();
    write_fixture_file(&other, "index.rhei.md", "# Rhei: Unrelated\n");
    fs::copy(&fixture.machine, other.join("states.yaml")).unwrap();
    let _unrelated = run_lock(&other, true);
    let release = fixture.transition(&fixture.dir, "human-review", "work");
    assert_release_success(&release, fixture.id, "human-review", "work");
    assert_no_advisory(&release);
}

#[test]
fn issue_653_live_non_gate_and_non_budget_moves_have_no_advisory() {
    for (from, to) in [("plain", "work"), ("human-review", "plain")] {
        let fixture = BudgetFixture::new(false);
        fixture.set_source(from);
        let _held = run_lock(&fixture.dir, true);
        let release = fixture.transition(&fixture.input, from, to);
        assert_release_success(&release, fixture.id, from, to);
        assert_no_advisory(&release);
    }
}

#[test]
fn issue_653_refused_live_gate_release_has_no_advisory() {
    let fixture = BudgetFixture::new(false);
    let _held = run_lock(&fixture.dir, true);
    let before = fs::read(&fixture.task_file).unwrap();
    let release = fixture.transition(&fixture.input, "human-review", "work");
    assert!(!release.status.success());
    assert_eq!(fs::read(&fixture.task_file).unwrap(), before);
    assert_no_advisory(&release);
}
