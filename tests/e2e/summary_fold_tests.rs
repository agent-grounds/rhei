//! `rhei summary` folds the consecutive attempts at one visit into one step,
//! so a visit a gate held for hundreds of attempts stays one line of a pull
//! request body. §FS-rhei-summary.2.2.1

use super::summary_tests::{write_invocation, Invocation, SUMMARY_PLAN};
use super::*;

/// `2026-05-20T<hh>:<mm>:<ss>Z` for a second of that day, so a fixture can lay
/// out hundreds of attempts without a date library.
fn may_20_at(second_of_day: u64) -> String {
    format!(
        "2026-05-20T{:02}:{:02}:{:02}Z",
        second_of_day / 3600,
        second_of_day / 60 % 60,
        second_of_day % 60
    )
}

/// A visit rhei started again and again, the way a routing gate holds one: an
/// ordinary step, four attempts in a row at `plan.1` supervising visit 2 on one
/// model, a fifth on another model, an unrelated step, and a sixth attempt on
/// the first model after it. Written to disk out of order, so only `started_at`
/// can put them in this order. §FS-rhei-summary.2.2.1
fn write_a_held_visit(root: &Path) {
    let attempts = [
        // (id, task, state, visit, model, started, ended)
        ("h-implement", "plan.2", "pending", 1, "claude-sonnet-5", "10:00:00", "10:18:04"),
        ("c-held-1", "plan.1", "supervising", 2, "claude-fable-5", "10:20:00", "10:21:00"),
        ("f-held-2", "plan.1", "supervising", 2, "claude-fable-5", "10:22:00", "10:22:30"),
        ("a-held-3", "plan.1", "supervising", 2, "claude-fable-5", "10:23:00", "10:25:15"),
        ("g-held-4", "plan.1", "supervising", 2, "claude-fable-5", "10:26:00", "10:26:45"),
        ("b-other-model", "plan.1", "supervising", 2, "claude-opus-5", "10:27:00", "10:29:00"),
        ("e-review", "plan.3", "review", 1, "claude-sonnet-5", "10:30:00", "10:31:30"),
        ("d-held-again", "plan.1", "supervising", 2, "claude-fable-5", "10:32:00", "10:32:40"),
    ];
    for (id, task_id, state, visit, model, started, ended) in attempts {
        let (started_at, ended_at) =
            (format!("2026-05-20T{started}Z"), format!("2026-05-20T{ended}Z"));
        write_invocation(
            root,
            &Invocation {
                id,
                task_id,
                state,
                visit,
                model,
                started_at: &started_at,
                ended_at: &ended_at,
                measured: true,
            },
        );
    }
}

#[test]
#[ignore = "red until #655: every retry of one visit prints as its own step; implement removes this"]
fn summary_folds_consecutive_attempts_at_one_visit_into_one_entry() {
    // §FS-rhei-summary.2.2.1: consecutive attempts sharing task, state, visit,
    // agent and model are one `×N` entry with their durations and tokens summed.
    let (dir, plan_path, machine_path) = setup_single_file("summary-fold", SUMMARY_PLAN);
    write_a_held_visit(&dir);

    let result = run_cli("summary", &plan_path, &machine_path, &[]);
    assert_success(&result);

    // Folding shortens the steps and nothing else: the lead still counts all
    // eight records (§FS-rhei-summary.2.1) and the table still sums all eight
    // (§FS-rhei-summary.2.3), at 75.0k total, 71.2k in, 30.0k cached, 3.8k out each.
    assert_eq!(
        result.stdout.lines().next(),
        Some(
            "`integration-test` workflow: 8 agent invocations across 3 models; \
             2 tasks completed, 1 cancelled, 1 in progress."
        ),
        "got:\n{}",
        result.stdout
    );
    let expected_rows = [
        "| total tokens | 600.0k |",
        "| input tokens (incl. cache) | 569.6k |",
        "| input cache read | 240.0k |",
        "| input cache write | - |",
        "| output tokens (incl. cache) | 30.4k |",
    ]
    .join("\n");
    assert!(
        result.stdout.contains(&expected_rows),
        "the accounting sums every record:\n{expected_rows}\ngot:\n{}",
        result.stdout
    );

    // The four attempts in a row are one entry: 60s + 30s + 135s + 45s, and
    // four times 71.2k in / 3.8k out. The attempt on another model keeps its
    // own line, and the attempt after `plan.3` starts a run of its own rather
    // than joining the four. Entries are numbered 1..5 after folding.
    let steps: Vec<&str> =
        result.stdout.lines().filter(|line| line.starts_with(char::is_numeric)).collect();
    assert_eq!(
        steps,
        vec![
            "1. `plan.2` pending — claude-code, anthropic/claude-sonnet-5 — 18m04s \
             — 71.2k in / 3.8k out",
            "2. `plan.1` supervising (visit 2) ×4 — claude-code, anthropic/claude-fable-5 \
             — 4m30s — 284.8k in / 15.2k out",
            "3. `plan.1` supervising (visit 2) — claude-code, anthropic/claude-opus-5 \
             — 2m00s — 71.2k in / 3.8k out",
            "4. `plan.3` review — claude-code, anthropic/claude-sonnet-5 — 1m30s \
             — 71.2k in / 3.8k out",
            "5. `plan.1` supervising (visit 2) — claude-code, anthropic/claude-fable-5 \
             — 40.0s — 71.2k in / 3.8k out",
        ],
        "got:\n{}",
        result.stdout
    );
    for (index, step) in steps.iter().enumerate() {
        assert!(step.starts_with(&format!("{}. ", index + 1)), "numbered 1..M; got:\n{step}");
    }
}

#[test]
#[ignore = "red until #655: a visit held 400 retries prints 400 steps; implement removes this"]
fn summary_details_of_a_visit_held_for_hundreds_of_attempts_stays_a_fraction_of_a_pr_body() {
    // §FS-rhei-summary.2.2.1 §FS-rhei-summary: however long a gate held a visit,
    // it is one entry, so the `--details` block stays compact enough to paste
    // into a pull request body (GitHub refuses one over 65,536 characters).
    const ATTEMPTS: u64 = 400;
    const BOUND: usize = 16_384;
    let (dir, plan_path, machine_path) = setup_single_file("summary-fold-size", SUMMARY_PLAN);
    let step = |id: &str, task_id: &str, state: &str, started: u64, seconds: u64| {
        let (started_at, ended_at) = (may_20_at(started), may_20_at(started + seconds));
        write_invocation(
            &dir,
            &Invocation {
                id,
                task_id,
                state,
                visit: if task_id == "plan.1" { 2 } else { 1 },
                model: "claude-fable-5",
                started_at: &started_at,
                ended_at: &ended_at,
                measured: true,
            },
        );
    };
    step("before", "plan.2", "pending", 9 * 3600, 1084);
    // One attempt a minute from 10:00, each 20s-49s long, all before 17:00.
    let held_seconds: Vec<u64> = (1..=ATTEMPTS).map(|n| 20 + n * 7 % 30).collect();
    for (n, seconds) in (1..=ATTEMPTS).zip(&held_seconds) {
        step(&format!("held-{n:03}"), "plan.1", "supervising", 10 * 3600 + (n - 1) * 60, *seconds);
    }
    step("after", "plan.3", "review", 17 * 3600, 90);

    let result = run_cli("summary", &plan_path, &machine_path, &["--details"]);
    assert_success(&result);

    assert!(
        result.stdout.lines().nth(1).is_some_and(|summary| summary
            .contains(&format!("{} agent invocations across 1 model;", ATTEMPTS + 2))),
        "the lead still counts every record; got:\n{}",
        result.stdout
    );
    let characters = result.stdout.chars().count();
    let held: Vec<&str> = result
        .stdout
        .lines()
        .filter(|line| line.starts_with(char::is_numeric))
        .filter(|line| line.contains("`plan.1` supervising (visit 2)"))
        .collect();
    assert_eq!(
        held.len(),
        1,
        "the held visit is one entry; it printed {} entries and the block {characters} characters",
        held.len()
    );
    let total: u64 = held_seconds.iter().sum();
    assert_eq!(
        held[0],
        format!(
            "2. `plan.1` supervising (visit 2) ×{ATTEMPTS} — claude-code, \
             anthropic/claude-fable-5 — {}m{:02}s — 28.5M in / 1.5M out",
            total / 60,
            total % 60
        ),
        "got:\n{}",
        result.stdout
    );
    assert!(
        characters < BOUND,
        "a held visit keeps the block under {BOUND} characters; it is {characters}"
    );
}
