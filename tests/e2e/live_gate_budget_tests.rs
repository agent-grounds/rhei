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

    fixture.raise_budget();
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

fn selected_scope_release(project_run: bool) {
    for project_address in [false, true] {
        let fixture = BudgetFixture::new(true);
        let other = fixture.add_sibling();
        let run_input = if project_run { &*fixture.dir } else { &fixture.input };
        let mut live = fixture.start(run_input, "selected-scope");
        wait_for_budget_run(&mut live, "selected target gating", || {
            fixture.gated() && fixture.visits() == 2
        });
        let descriptor = fixture.descriptor(&fixture.dir);
        assert_eq!(descriptor["pid"], live.child().id());
        assert!(live.child().try_wait().unwrap().is_none());
        if !project_run {
            assert!(live.output().contains("Scope: narrowed to ticket"));
            assert!(!other.join("visits.txt").exists());
        }
        let input = if project_address { &*fixture.dir } else { &fixture.input };
        fixture.raise_budget();
        let release = fixture.transition(input, "human-review", "work");
        assert_release_success(&release, fixture.id, "human-review", "work");
        assert_advisory(&release, fixture.id, &fixture.dir, descriptor["id"].as_str());
        if !project_run {
            assert!(release.stderr.contains("--rhei ticket"), "{}", release.stderr);
        }
    }
}

#[test]
fn issue_653_gate_release_detects_selected_project_activity() {
    selected_scope_release(true);
}

#[test]
fn issue_653_gate_release_detects_selected_member_activity() {
    selected_scope_release(false);
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
fn issue_653_gate_release_ignores_real_narrowed_sibling_run() {
    for explicit_scope in [false, true] {
        let fixture = BudgetFixture::new(true);
        let other = fixture.add_sibling();
        fixture.set_source("human-review");
        let (input, args): (_, &[&str]) = if explicit_scope {
            (&*fixture.dir, &["--rhei", "other"])
        } else {
            (other.as_path(), &[])
        };
        let mut live = fixture.start_with_args(input, "unrelated-scope", args);
        wait_for_budget_run(&mut live, "sibling gating outside the ticket scope", || {
            fs::read_to_string(other.join("visits.txt")).unwrap_or_default().lines().count() == 2
                && fs::read_to_string(other.join("tasks/01-ticket.md"))
                    .unwrap()
                    .contains("**State:** human-review")
        });
        let descriptor = fixture.descriptor(&fixture.dir);
        assert_eq!(descriptor["pid"], live.child().id());
        assert!(live.output().contains("Scope: narrowed to other"));
        for input in [&fixture.input, &*fixture.dir] {
            let release = fixture.transition(input, "human-review", "work");
            assert_release_success(&release, fixture.id, "human-review", "work");
            assert_no_advisory(&release);
            assert!(live.child().try_wait().unwrap().is_none(), "sibling must remain alive");
            assert_eq!(fixture.visits(), 0, "the live run must not select the target");
            fixture.set_source("human-review");
        }
    }
}

#[test]
fn issue_653_gate_release_without_descriptor_gives_foreground_recovery() {
    for project in [false, true] {
        let fixture = BudgetFixture::new(project);
        let input = if project { &*fixture.dir } else { &fixture.input };
        // Publication is best-effort: the run must still work with this obstruction.
        fs::create_dir_all(fixture.dir.join("runtime/run.json")).unwrap();
        let mut live = fixture.start(input, "unpublished");
        wait_for_budget_run(&mut live, "unpublished run gating", || {
            fixture.gated() && fixture.visits() == 2
        });
        assert!(live.output().contains("could not publish the run descriptor"));
        fixture.raise_budget();
        let release = fixture.transition(&fixture.input, "human-review", "work");
        assert_release_success(&release, fixture.id, "human-review", "work");
        assert_advisory(&release, fixture.id, &fixture.dir, None);
        assert!(release.stderr.contains("interrupt the foreground run"), "{}", release.stderr);
        assert!(release.stderr.contains("Ctrl-C"), "{}", release.stderr);
        assert!(release.stderr.contains("no resolvable live run descriptor"));
        assert!(!release.stderr.contains("rhei stop"), "must not offer an unusable command");
        assert!(!release.stderr.contains(&format!("kill {}", live.child().id())));
        assert!(live.child().try_wait().unwrap().is_none());
        // On Unix exercise the same SIGINT as Ctrl-C, using the Child handle's
        // known pid rather than trusting any suggested lock-only process id.
        #[cfg(unix)]
        {
            nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(live.child().id() as i32),
                nix::sys::signal::Signal::SIGINT,
            )
            .unwrap();
            assert_eq!(live.wait_for_exit("foreground interruption").code(), Some(130));
        }
        #[cfg(not(unix))]
        {
            write_fixture_file(&fixture.member, "finish", "finished\n");
            finish_budget_run(&mut live);
        }
    }
}

#[test]
fn issue_653_empty_lock_owner_does_not_invent_a_stop_target() {
    let fixture = BudgetFixture::new(false);
    fixture.set_source("human-review");
    let _held = run_lock(&fixture.dir, true);
    let release = fixture.transition(&fixture.input, "human-review", "work");
    assert_release_success(&release, fixture.id, "human-review", "work");
    assert_advisory(&release, fixture.id, &fixture.dir, None);
    assert!(release.stderr.contains("Ctrl-C"));
    assert!(!release.stderr.contains("rhei stop"));
}

#[test]
fn issue_653_gate_release_offers_usable_descriptor_path_without_registry() {
    let fixture = BudgetFixture::new(false);
    let mut live = fixture.start(&fixture.input, "usable-stop");
    wait_for_budget_run(&mut live, "recorded run gating", || {
        fixture.gated() && fixture.visits() == 2
    });
    let descriptor = fixture.descriptor(&fixture.dir);
    fixture.raise_budget();
    let release = fixture.transition(&fixture.input, "human-review", "work");
    assert_release_success(&release, fixture.id, "human-review", "work");
    assert_advisory(&release, fixture.id, &fixture.dir, descriptor["id"].as_str());
    #[cfg(unix)]
    {
        let target =
            rhei_core::platform::canonical_path(&fixture.dir).unwrap().display().to_string();
        let suggested = format!("`rhei stop {}`", rhei_core::platform::shell_quote(&target));
        assert!(release.stderr.contains(&suggested), "{}", release.stderr);
        // An absolute descriptor path must resolve even when the registry is unavailable.
        let output = fixture
            .command()
            .env("XDG_STATE_HOME", fixture.dir.join("no-registry"))
            .args(["stop", &target])
            .output()
            .unwrap();
        assert_success(&CliRun::from(&output));
        let status = live.wait_for_exit("the suggested stop command");
        assert!(!status.success(), "an interrupted run must record interruption");
    }
    // `rhei stop` needs POSIX signals, so it must not be offered here. §FS-rhei-transition-cmd.3.4
    #[cfg(not(unix))]
    {
        let reason = "`rhei stop` is not supported on this platform";
        assert!(release.stderr.contains(reason), "{}", release.stderr);
        // The stated reason is the only mention: no invocation of it is offered.
        assert!(!release.stderr.replace(reason, "").contains("rhei stop"), "{}", release.stderr);
        assert!(release.stderr.contains("Ctrl-C"), "{}", release.stderr);
        write_fixture_file(&fixture.member, "finish", "finished\n");
        finish_budget_run(&mut live);
    }
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
