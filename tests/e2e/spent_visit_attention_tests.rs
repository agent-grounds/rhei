//! A spent current visit has one cause on every human-facing halt surface.
//! §FS-rhei-run-report.3.1 §FS-rhei-run.5.1.1 §FS-rhei-retry.3

use super::spent_visit_attention_support::*;
use super::*;

fn assert_spent(fixture: &Fixture, out: &CliRun, spent: u64) {
    let report = fixture.report();
    let attention = row(&report, "Attention");
    // Preserve worker history if the ledger appends a separate halt row.
    let ledger = section(&report, "Transition Ledger")
        .lines()
        .find(|line| line.contains(TASK) && line.contains("attempts spent on this visit"))
        .unwrap_or_else(|| row(&report, "Transition Ledger"));
    let final_state = row(&report, "Task Final States");
    // Check the shared stderr explanation where emitted; the spawn warning
    // is separate, and this change does not require a new output surface.
    let explanation = out.stderr.lines().find(|line| line.contains(&format!("Task {TASK} (win):")));
    let mut failures = Vec::new();
    let mut surfaces = vec![
        ("Attention", attention),
        ("Transition Ledger", ledger),
        ("Task Final States", final_state),
    ];
    if let Some(line) = explanation {
        surfaces.push(("stderr halt explanation", line));
    }
    for (surface, line) in surfaces {
        for expected in [
            format!("{spent} attempts spent on this visit"),
            "delivery".to_string(),
            fixture
                .member
                .join("runtime")
                .join("exports")
                .join(TASK)
                .join("delivery.md")
                .display()
                .to_string(),
            fixture
                .member
                .join("runtime")
                .join("results")
                .join(format!("{TASK}.md"))
                .display()
                .to_string(),
        ] {
            if !line.contains(&expected) {
                failures.push(format!("{surface} lacks {expected:?}: {line}"));
            }
        }
        for wrong in [
            "not scheduled before",
            "rerun to pick it up",
            "rhei next",
            "write the file(s)",
            "rhei transition",
            "inspect logs",
            "cancelled",
            "stalled in non-terminal state",
        ] {
            if line.contains(wrong) {
                failures.push(format!("{surface} gives contradictory {wrong:?}: {line}"));
            }
        }
    }
    let remedies = [("Attention", attention)];
    for (surface, line) in
        remedies.into_iter().chain(explanation.map(|line| ("stderr halt explanation", line)))
    {
        let retry = line.find(&format!("rhei retry {TASK}"));
        let rerun = line.find("rerun").or_else(|| line.find("re-run"));
        if !matches!((retry, rerun), (Some(retry), Some(rerun)) if retry < rerun) {
            failures
                .push(format!("{surface} must prescribe rhei retry {TASK} before rerun: {line}"));
        }
    }
    assert!(!section(&report, "Waiting").contains(TASK), "spent visit is actionable:\n{report}");
    assert!(
        failures.is_empty(),
        "spent-visit classification is missing:\n{}\nstderr:\n{}",
        failures.join("\n"),
        out.stderr
    );
}

fn regression(parallel: bool, exit: i32, later: bool) {
    let fixture = Fixture::new(parallel, exit);
    fixture.consume(1);
    let last = fixture.consume(2);
    assert_eq!(fixture.attempts(), [format!("{TASK}:1:1"), format!("{TASK}:1:2")]);
    let out = if later { fixture.later(2) } else { last };
    assert_spent(&fixture, &out, 2);
}

#[test]
fn issue749_sequential_exit1_last_attempt() {
    regression(false, 1, false);
}

#[test]
fn issue749_sequential_exit1_later_run() {
    regression(false, 1, true);
}

#[test]
fn issue749_sequential_exit0_last_attempt() {
    regression(false, 0, false);
}

#[test]
fn issue749_sequential_exit0_later_run() {
    regression(false, 0, true);
}

#[test]
fn issue749_parallel_exit1_last_attempt() {
    regression(true, 1, false);
}

#[test]
fn issue749_parallel_exit1_later_run() {
    regression(true, 1, true);
}

#[test]
fn issue749_parallel_exit0_last_attempt() {
    regression(true, 0, false);
}

#[test]
fn issue749_parallel_exit0_later_run() {
    regression(true, 0, true);
}

#[test]
fn issue749_exhausted_grant_reports_four_spent_attempts() {
    let fixture = Fixture::new(false, 0);
    fixture.consume(1);
    fixture.consume(2);
    assert_success(&fixture.cli(&["retry", TASK]));
    fixture.consume(3);
    fixture.consume(4);
    let later = fixture.later(4);
    assert_spent(&fixture, &later, 4);
}

fn assert_unspent(fixture: &Fixture, exit: i32) {
    let report = fixture.report();
    let attention = row(&report, "Attention");
    for wrong in ["attempts spent on this visit", "rhei retry"] {
        assert!(!attention.contains(wrong), "unspent allowance: {attention}");
    }
    if exit == 0 {
        assert!(attention.contains("worker exited 0 without delivery"), "{attention}");
        assert!(attention.contains("write the file(s) above and rerun"), "{attention}");
    } else {
        assert!(attention.contains("stalled in non-terminal state win"), "{attention}");
        assert!(attention.contains("inspect logs or mark the task cancelled"), "{attention}");
    }
}

#[test]
fn issue749_unspent_failures_keep_existing_advice() {
    for exit in [0, 1] {
        let fixture = Fixture::new(false, exit);
        fixture.consume(1);
        assert_unspent(&fixture, exit);
    }
}

#[test]
fn issue749_grant_and_new_visit_do_not_inherit_spent_classification() {
    let fixture = Fixture::new(false, 0);
    fixture.consume(1);
    fixture.consume(2);
    assert_success(&fixture.cli(&["retry", TASK]));
    fixture.consume(3);
    assert_unspent(&fixture, 0);
    // A self-loop enforces the old visit's output contract too. Fulfil it for
    // the transition, then remove it so the new visit still owes its own work.
    let output = fixture.member.join(format!("runtime/exports/{TASK}/delivery.md"));
    fs::create_dir_all(output.parent().unwrap()).unwrap();
    fs::write(&output, "old visit delivered\n").unwrap();
    assert_success(&fixture.cli(&[
        "transition",
        "csa-constants",
        "--task",
        "deliver",
        "--from",
        "win",
        "--to",
        "win",
        "--no-callbacks",
    ]));
    fs::remove_file(output).unwrap();
    let new_visit = fixture.run();
    assert_eq!(new_visit.status.code(), Some(1), "{}", text(&new_visit));
    assert_eq!(fixture.attempts().last(), Some(&format!("{TASK}:2:1")), "new visit starts fresh");
    assert_unspent(&fixture, 0);
}
