//! What a selected run *decides*: which stop it reports, and what defeats it.
//!
//! Idle is a property of the whole in-scope plan. One ticket whose classified
//! blocker is actionable defeats it however many tickets wait beside it, which
//! is what keeps a real problem from hiding under a waiting result.
// §FS-rhei-run.3 §FS-rhei-run-report.3.1 §FS-rhei-run-json.5

use super::until_idle_support::*;
use super::*;

/// (1) Gate-only. The one open ticket is waiting on a person, so the run has
/// nothing to do and nothing to complain about: it returns `3`, names the
/// gate, reports no timed retry at all, leaves `## Attention` empty, and does
/// not move the ticket.
// §FS-rhei-run.3 §FS-rhei-run-report.3.1
#[test]
fn until_idle_returns_three_on_a_gate_only_plan() {
    let plan = IdlePlan::new(
        "until-idle-gate-only",
        "## Tasks\n\n### Task 1: Waiting on a reviewer\n**State:** gate\n",
    );

    let result = plan.run(&["--until-idle", "--no-dashboard"]);

    assert_exit(&result, EXIT_IDLE, "a gate-only plan is idle, not complete and not halted");
    assert_idle_line(&result, "gate", "none");
    let report = plan.report();
    assert!(
        report.contains("idle — waiting on a person"),
        "the report's result gains the idle phrase; got:\n{report}"
    );
    assert!(
        !report.contains("## Attention"),
        "a non-empty Attention means the run was not idle; got:\n{report}"
    );
    assert!(report.contains("## Waiting"), "the gated ticket belongs in Waiting; got:\n{report}");
    assert_task_state(&plan.plan, &plan.machine, "1", "gate");
}

/// (3) Provider-limit only. This is the case that catches the divergence: the
/// plan-wide deliberate-wait judgment never tested provider limits while the
/// deadline scan did, so the one wait the contract calls calm would have been
/// reported as attention and exited in the failure category. It exits `3`, not
/// `1`, and spends no attempt doing it.
// §FS-rhei-run.3.3 §FS-rhei-run-report.3.1
#[test]
fn until_idle_reports_a_provider_limit_only_wait_as_idle_rather_than_failure() {
    let plan = IdlePlan::new(
        "until-idle-provider-only",
        &format!(
            r#"---
metadata:
  tasks:
    1:
      {}
---

## Tasks

### Task 1: Parked on a provider limit
**State:** parked
"#,
            provider_limit("parked", FUTURE_POLL)
        ),
    );

    let result = plan.run(&["--until-idle", "--no-dashboard"]);

    assert_exit(
        &result,
        EXIT_IDLE,
        "a recognized provider limit is a deliberate wait, not a failure",
    );
    assert_idle_line(&result, "provider_limit", &instant(FUTURE_POLL));
    assert!(
        plan.records().is_empty(),
        "the attempt budget is preserved: nothing was spawned while the limit holds"
    );
    assert!(
        !plan.plan_text().contains("stateVisits"),
        "no visit was spent; got:\n{}",
        plan.plan_text()
    );
    assert_task_state(&plan.plan, &plan.machine, "1", "parked");
}

/// (7) A failure beside a wait is a failure. The gated ticket is deliberately
/// waiting and the other ticket's worker exited `0` without the artifact it
/// owed — an actionable blocker, so the run exits `1` and never `3`.
/// `--continue-on-error` governs how far independent work drains inside a
/// pass, never this precedence.
// §FS-rhei-run.3 §FS-rhei-run-report.3.1
#[test]
fn an_actionable_blocker_beside_a_gate_defeats_idle() {
    for extra in [vec![], vec!["--continue-on-error"]] {
        let plan = IdlePlan::new(
            "until-idle-failure-beside-wait",
            "## Tasks\n\n### Task 1: Waiting on a reviewer\n**State:** gate\n\n\
             ### Task 2: Owes an artifact\n**State:** silent\n",
        );

        let mut args = vec!["--until-idle", "--no-dashboard"];
        args.extend(extra.iter().copied());
        let result = plan.run(&args);

        assert_exit(&result, 1, &format!("attention outranks idle (args {args:?})"));
        let combined = format!("{}{}", result.stdout, result.stderr);
        assert!(
            !combined.contains("Run idle:"),
            "a run with work that needs a person is not idle; got:\n{combined}"
        );
    }
}

/// (8) A live claim beside a wait is the same answer for a different reason: a
/// claim is something an operator repairs, so the ticket is actionable and the
/// run says whose it is and what clears it.
// §FS-rhei-run-report.3.1
#[test]
fn a_live_claim_beside_a_gate_defeats_idle() {
    let plan = IdlePlan::new(
        "until-idle-claim-beside-wait",
        "## Tasks\n\n### Task 1: Waiting on a reviewer\n**State:** gate\n\n\
         ### Task 2: Held by someone\n**State:** work\n**Assignee:** alice\n",
    );

    let result = plan.run(&["--until-idle", "--no-dashboard"]);

    assert_exit(&result, 1, "a claim an operator must release is not a deliberate wait");
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(combined.contains("rhei release"), "the remedy for a claim is named; got:\n{combined}");
    assert!(!combined.contains("Run idle:"), "got:\n{combined}");
}

/// (9) The control that matters most to everyone who never types the flag: a
/// finished plan exits `0` with the option and without it, and the option adds
/// nothing to what it prints.
// §FS-rhei-run-json.5
#[test]
fn a_finished_plan_exits_zero_with_the_option_and_without_it() {
    let selected = IdlePlan::new(
        "until-idle-terminal-selected",
        "## Tasks\n\n### Task 1: Already done\n**State:** done\n",
    );
    let unselected = IdlePlan::new(
        "until-idle-terminal-unselected",
        "## Tasks\n\n### Task 1: Already done\n**State:** done\n",
    );

    let with = selected.run(&["--until-idle", "--no-dashboard"]);
    let without = unselected.run(&["--no-tui", "--no-dashboard"]);

    assert_exit(&with, 0, "a finished plan is complete, not idle");
    assert_exit(&without, 0, "and is complete without the option too");
    assert!(
        !with.stdout.contains("Run idle:"),
        "nothing is waiting, so there is no idle stop to report; got:\n{}",
        with.stdout
    );
    // The byte-compatibility promise itself, which is what this case is for:
    // the two halves run in workspaces of their own, so the one thing that may
    // legitimately differ is each workspace's unique directory name — and
    // normalising *that* normalises every spelling of every path under it,
    // including macOS's `/private/var` reading of the same directory.
    let scrub = |plan: &IdlePlan, text: &str| {
        let workspace =
            plan.root().file_name().and_then(|name| name.to_str()).expect("a workspace name");
        text.replace(workspace, "<workspace>")
    };
    assert_eq!(
        scrub(&selected, &with.stdout),
        scrub(&unselected, &without.stdout),
        "the option changes nothing about a plan it has nothing to wait for"
    );
}

/// (21) A claim on the *polled* ticket, which is the hole case (8) leaves: put
/// the assignee on work with no clock of its own and the answer is right by
/// accident, because the claim is the only reading there is. Move it onto the
/// poll and the two readings of one ticket diverge — the run's own ledger row
/// says `blocked … claimed by alice` while the stop would say `waiting on
/// poll; next attempt none`, a line that contradicts itself.
///
/// §FS-rhei-run.3 step 9 settles it in as many words: "a task that is both
/// polled and claimed classifies as claimed and is not [idle-compatible],
/// because a claim really does stop a poll from reaching its next attempt".
/// So the run exits `1`, and the remedy it names is the claim's.
// §FS-rhei-run.3 §FS-rhei-run-report.3.1
#[test]
fn a_claim_on_the_polled_ticket_defeats_idle() {
    let plan = IdlePlan::new(
        "until-idle-claim-on-the-poll",
        &format!(
            "---\nmetadata:\n  tasks:\n    1:\n      pollNextAttemptAt:\n        \
             poll: {FUTURE_POLL}\n---\n\n## Tasks\n\n\
             ### Task 1: Retries later, and claimed\n**State:** poll\n**Assignee:** alice\n"
        ),
    );

    let result = plan.run(&["--until-idle", "--no-dashboard"]);

    assert_exit(&result, 1, "a claim stops a poll from reaching its next attempt");
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        !combined.contains("Run idle:"),
        "a claimed poll is not a deliberate wait; got:\n{combined}"
    );
    assert!(
        combined.contains("rhei release"),
        "and the remedy is the claim's, not the clock's; got:\n{combined}"
    );
}

/// (22) A claim on a *supervising* parent, which is the hole case (21) leaves
/// one level up: the ticket carries a live `**Assignee:**` and an open subtree
/// at the same time, and every other rung of the order reads the subtree first.
/// A supervisor is ready *while* that subtree is open
/// (§FS-rhei-supervision.3.1 rule 1), so its claim is the only thing stopping
/// it and `rhei release` the only thing that starts it again — and the two
/// readings of one run diverged on exactly that: the ledger row said
/// `blocked … claimed by alice` while the stop said `waiting on gate`.
///
/// `classify_halt` never made the mistake — it skips its descendant rung for a
/// supervising ticket and selects `Claimed` — so §FS-rhei-run.3 step 9, which
/// decides idle-compatibility by the blocker the classification *selects*, was
/// already settled against the walks. The run exits `1`, and the remedy it
/// names is the claim's. This is the shape every grounded-ticket plan has while
/// a worker holds a supervisor's visit.
// §FS-rhei-run.3 §FS-rhei-run-report.3.1 §FS-rhei-supervision.3.1
#[test]
fn a_claim_on_a_supervising_parent_defeats_idle() {
    let plan = IdlePlan::new(
        "until-idle-claim-on-the-supervisor",
        "## Tasks\n\n### Task 1: A supervisor a worker still holds\n\
         **State:** supervising\n**Assignee:** alice\n\n\
         #### Task 1.1: Waiting on a reviewer\n**State:** gate\n",
    );

    let result = plan.run(&["--until-idle", "--no-dashboard"]);

    assert_exit(&result, 1, "a supervisor's claim is the only thing stopping it");
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        !combined.contains("Run idle:"),
        "a claimed supervisor is not a deliberate wait, whatever waits beneath it; got:\n{combined}"
    );
    assert!(
        combined.contains("rhei release"),
        "and the remedy is the claim's, not the gate's; got:\n{combined}"
    );
    assert_task_state(&plan.plan, &plan.machine, "1", "supervising");
    assert_task_state(&plan.plan, &plan.machine, "1.1", "gate");
}

/// (23) The control for the reading that must **not** move, and it passes
/// before case (22)'s fix as well as after: a *claimless* supervisor that has
/// released a subtree waiting on a person is still idle. Do not read a passing
/// control as a pin — its job is to refuse the wrong fix, not to prove the
/// right one.
///
/// The wrong fix is the tempting one: give the walks `classify_halt`'s
/// `!task_is_supervising` exclusion wholesale, so a supervising ticket is never
/// classified by its subtree at all. Follow that past the descendant rung here
/// and there is nothing left to answer — no gate, no claim, no prior, no
/// clock — so the plan stops being idle and the run exits `1` where it exits
/// `3` today. That is this whole ticket defeated in the opposite direction, on
/// the plan shape it exists for. The narrow reading is the one that holds: read
/// a supervising ticket's own gate and claim first, and keep the subtree as the
/// fallback.
// §FS-rhei-run.3 §FS-rhei-run-report.3.1 §FS-rhei-supervision.3.1
#[test]
fn a_claimless_supervisor_over_a_gated_child_is_still_idle() {
    let body = "---\nmetadata:\n  tasks:\n    1:\n      supervision:\n        \
                phase: released\n---\n\n## Tasks\n\n\
                ### Task 1: A supervisor that released its subtree\n**State:** supervising\n\n\
                #### Task 1.1: Waiting on a reviewer\n**State:** gate\n";
    let selected = IdlePlan::new("until-idle-claimless-supervisor-selected", body);
    let unselected = IdlePlan::new("until-idle-claimless-supervisor-unselected", body);

    let with = selected.run(&["--until-idle", "--no-dashboard"]);
    let without = unselected.run(&["--no-tui", "--no-dashboard"]);

    assert_exit(&with, EXIT_IDLE, "a supervisor with no claim is stopped by nothing of its own");
    assert_idle_line(&with, "gate", "none");
    assert_exit(&without, 0, "and the unselected run's exit is unchanged");
}

/// (24) The rung `fix-1` added, pinned in the direction it changed: a plan held
/// entirely behind a gate-parked supervisor is a deliberate wait. The supervisor
/// left its supervising state for a human gate, which keeps the block
/// (§FS-rhei-supervision.3.1 rule 4), so there is no next visit to release the
/// subtree and "the supervisor releases it on its next visit" stops being true
/// — what holds the child is the gate at the other end of the hold.
///
/// This is the case that bites: delete the `awaiting_human` short-circuit from
/// either walk and the held child falls through to its own rungs, where `work`
/// names no wait at all, so the selected run exits `1` instead of `3` and the
/// unselected one exits `1` instead of `0`. Round 2 established that nothing in
/// the suite moved when both short-circuits were removed; this is that gap.
// §FS-rhei-run.3 §FS-rhei-run-report.3.1 §FS-rhei-supervision.3.1
#[test]
fn a_plan_held_behind_a_gate_parked_supervisor_is_idle() {
    let body = "---\nmetadata:\n  tasks:\n    1:\n      supervision:\n        \
                phase: held\n---\n\n## Tasks\n\n\
                ### Task 1: A supervisor parked at a gate, still holding its subtree\n\
                **State:** gate\n\n\
                #### Task 1.1: Held, with work it could otherwise do\n**State:** work\n";
    let selected = IdlePlan::new("until-idle-gate-parked-supervisor-selected", body);
    let unselected = IdlePlan::new("until-idle-gate-parked-supervisor-unselected", body);

    let with = selected.run(&["--until-idle", "--no-dashboard"]);
    let without = unselected.run(&["--no-tui", "--no-dashboard"]);

    assert_exit(
        &with,
        EXIT_IDLE,
        "the gate at the other end of the hold is what the plan waits on",
    );
    assert_idle_line(&with, "gate", "none");
    assert_exit(&without, 0, "and an unselected run ends quietly rather than halting");
    assert!(
        selected.records().is_empty(),
        "nothing beneath a held supervisor is dispatched, so the child never ran"
    );
    assert_task_state(&selected.plan, &selected.machine, "1.1", "work");
}

/// (25) The other half of the same rung: a hold whose supervisor is still owed
/// a visit is *not* a deliberate wait, because the visit it ends at is one the
/// run itself still owes. `fix-1` left that reading alone deliberately, and
/// this keeps it that way — make the hold rung unconditional and the plan
/// becomes idle.
///
/// Read what it pins honestly. The exit is over-determined: the supervisor here
/// is claimed, which is also what keeps it from being dispatched, and case (22)
/// already makes a claimed supervisor non-idle on its own. So this case pins the
/// child's reading through the report rather than through the exit code — it is
/// classified as held with a visit still coming, not as waiting on a person —
/// and the exit is the outer invariant that must hold with it.
// §FS-rhei-run-report.3.1 §FS-rhei-supervision.3.4
#[test]
fn a_hold_whose_supervisor_is_still_owed_a_visit_keeps_its_exit() {
    let body = "## Tasks\n\n### Task 1: A supervisor a worker still holds\n\
                **State:** supervising\n**Assignee:** alice\n\n\
                #### Task 1.1: Held, with work it could otherwise do\n**State:** work\n";
    let selected = IdlePlan::new("until-idle-visit-owed-hold-selected", body);
    let unselected = IdlePlan::new("until-idle-visit-owed-hold-unselected", body);

    let with = selected.run(&["--until-idle", "--no-dashboard"]);
    let without = unselected.run(&["--no-tui", "--no-dashboard"]);

    assert_exit(&with, 1, "a hold that ends at a visit the run still owes is not a wait");
    assert_exit(&without, 1, "and the unselected run's exit is unchanged");
    let combined = format!("{}{}", with.stdout, with.stderr);
    assert!(
        !combined.contains("Run idle:"),
        "nothing here is waiting on a person; got:\n{combined}"
    );
    assert!(
        combined.contains("the supervisor releases it on its next visit"),
        "the child is held with a visit still coming, not parked at a gate; got:\n{combined}"
    );
    assert!(
        !combined.contains("still holds this subtree"),
        "which is the gate-parked reading, and this supervisor is not at a gate; got:\n{combined}"
    );
}
