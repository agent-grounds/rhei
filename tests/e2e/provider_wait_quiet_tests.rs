//! A ticket held by a provider wait opens no pass while the run sleeps
//! (agent-grounds/rhei#681).
//!
//! The run used to keep a held ticket in its ready set, so every 500 ms slice
//! of the deadline sleep opened a pass that printed `Pass N: 1 ready` and
//! spawned nothing, until the deadline: about 13,000 passes for a two-hour
//! session limit. Two controls already pin what must not change:
//! `provider_limit_restart_runs_unrelated_fanout_identity` (a fan-out's free arm
//! spawns during its sibling's wait), and
//! `until_idle_reports_a_provider_limit_only_wait_as_idle_rather_than_failure`
//! with `a_pi_session_that_loses_its_provider_waits_without_spending_an_attempt`
//! (`--until-idle` exits 3 at the wait and reports the earliest next attempt).
// §FS-rhei-run.3 §FS-rhei-run.3.3 §FS-rhei-run.5.1

use super::provider_limit_scheduling_tests::fanout_machine;
use super::provider_limit_support::*;
use super::provider_unavailable_pi_support::{gave_up, starts, Outage, PROVIDER_BACK};
use super::provider_wait_quiet_support::*;
use super::*;

/// A stand-in that refuses its first start with `signal` and finishes the
/// ticket on every start after it, counting its starts in `<local>-starts`.
/// With `await_both`, the refusal waits until both tickets have started, so
/// the two are in flight together in the worker pool.
fn refuses_once(signal: &str, await_both: bool) -> String {
    let both = if await_both {
        "    await_marker(root / '1-starts')\n    await_marker(root / '2-starts')\n"
    } else {
        ""
    };
    format!(
        r#"root = pathlib.Path(env('RHEI_ROOT'))
starts = root / (env('RHEI_TASK_ID_LOCAL') + '-starts')
append(starts, env('RHEI_ATTEMPT') + '\n')
if len(starts.read_text().splitlines()) == 1:
{both}    print({signal:?}, file=sys.stderr, flush=True)
    raise SystemExit(1)
result('## Result\n\nDone after the provider wait.\n')
"#
    )
}

/// The ticket's own case: a session-limit refusal two hours out. The run says
/// once that it is sleeping, then prints nothing until the wait ends, and the
/// ticket resumes in the same run once it does. §FS-rhei-run.5.1 §FS-rhei-run.3.3
#[test]
fn a_session_limit_wait_opens_no_pass_while_the_run_sleeps() {
    let fixture = ProviderFixture::new(
        "quiet-session-limit",
        SINGLE_TASK,
        SIMPLE_MACHINE,
        &refuses_once(&far_limit_signal(), false),
    );
    let started = fixture.root.join("1-starts");
    let mut run = start_text_run(&fixture, "run", &[]);
    let watched = watch_the_sleep(&fixture, &mut run, "run", || line_count(&started));
    watched.assert_quiet("a session-limit wait");
    assert_eq!(line_count(&started), 1, "one refused start before the wait");

    expire_provider_deadlines(&fixture.root);
    let status = run.wait_for_exit("the held ticket to resume once its wait ended");
    assert!(status.success(), "the resumed run exited {status}; {}", run.output());
    assert_all_tasks_in_state(&fixture.root, &fixture.machine, "completed");
    assert_eq!(line_count(&started), 2, "the wait ended and the ticket started again");
}

/// The same for a pi outage: the first `provider_unavailable` ending waits a
/// minute, and the run sleeps through it without a pass. §FS-rhei-run.3.3
/// §FS-rhei-agents.2.5 §FS-rhei-run.5.1
#[test]
fn an_outage_wait_opens_no_pass_while_the_run_sleeps() {
    let fixture = Outage::pi(gave_up()).fixture("quiet-outage");
    let mut run = start_text_run(&fixture, "run", &[]);
    let watched = watch_the_sleep(&fixture, &mut run, "run", || starts(&fixture));
    watched.assert_quiet("an outage wait");
    assert_eq!(starts(&fixture), 1, "one unavailable start before the wait");

    fs::write(fixture.root.join(PROVIDER_BACK), "").unwrap();
    expire_provider_deadlines(&fixture.root);
    let status = run.wait_for_exit("the held ticket to resume once the provider is back");
    assert!(status.success(), "the resumed run exited {status}; {}", run.output());
    assert_all_tasks_in_state(&fixture.root, &fixture.machine, "completed");
}

/// Two tickets refused together in the `--parallel 2` worker pool: once both
/// are held, the pool's run sleeps as quietly as the sequential one.
/// §FS-rhei-run.5.1 §FS-rhei-run.3.3
#[test]
fn held_tickets_in_the_worker_pool_open_no_pass_while_the_run_sleeps() {
    let fixture = ProviderFixture::new(
        "quiet-worker-pool",
        &[
            ("01.md", "### Task 1: First held\n**State:** working\n"),
            ("02.md", "### Task 2: Second held\n**State:** working\n"),
        ],
        &SIMPLE_MACHINE.replace("    initial: true", "    initial: true\n    concurrent: true"),
        &refuses_once(&far_limit_signal(), true),
    );
    let (first, second) = (fixture.root.join("1-starts"), fixture.root.join("2-starts"));
    let mut run = start_text_run(&fixture, "run", &["--parallel", "2"]);
    wait_for_provider_waits(&fixture.root, &mut run, 2);
    let watched =
        watch_the_sleep(&fixture, &mut run, "run", || line_count(&first) + line_count(&second));
    watched.assert_quiet("two held tickets in the worker pool");

    expire_provider_deadlines(&fixture.root);
    let status = run.wait_for_exit("both held tickets to resume once their waits ended");
    assert!(status.success(), "the resumed run exited {status}; {}", run.output());
    assert_all_tasks_in_state(&fixture.root, &fixture.machine, "completed");
    assert_eq!((line_count(&first), line_count(&second)), (2, 2));
}

/// A fan-out whose free arm finished during its sibling's wait is held like any
/// other ticket: the free arm spawns, and then the run sleeps without a pass.
/// §FS-rhei-run.3 §FS-rhei-run.5.1
#[test]
fn a_fan_out_left_with_only_its_held_arm_opens_no_pass() {
    let fixture = ProviderFixture::new(
        "quiet-fan-out",
        SINGLE_TASK,
        &fanout_machine("mock:mock:beta"),
        r#"root = pathlib.Path(env('RHEI_ROOT'))
append(root/(env('RHEI_MODEL_NAME') + '-starts'), env('RHEI_ATTEMPT') + '\n')
write(root/'runtime/fanout'/(env('RHEI_MODEL_NAME') + '.txt'), 'done')
result('## Result\n\nDone.\n')
"#,
    );
    edit_metadata(&fixture.root, |m| {
        m["metadata"] = serde_json::json!({"tasks": {"1": {
            "providerLimits": {"working": limit_record(epoch_now() + 3600)}
        }}});
    });
    let (alpha, beta) = (fixture.root.join("alpha-starts"), fixture.root.join("beta-starts"));
    let mut run = start_text_run(&fixture, "run", &[]);
    let watched =
        watch_the_sleep(&fixture, &mut run, "run", || line_count(&alpha) + line_count(&beta));
    watched.assert_quiet("a fan-out whose only arm left is held");
    assert_eq!(line_count(&beta), 1, "the free arm spawned during the wait");
    assert_eq!(line_count(&alpha), 0, "the held arm did not");
}

/// A run started onto a wait an earlier run persisted opens no pass at all and
/// prints no halt report: it sleeps, stays alive, and ends cleanly on Ctrl+C
/// with the wait still recorded. §FS-rhei-run.5.1 §FS-rhei-run.3.3
/// §FS-rhei-run.3.2
#[cfg(unix)]
#[test]
fn a_run_started_onto_a_persisted_wait_sleeps_without_a_pass() {
    let fixture = ProviderFixture::new(
        "quiet-restart",
        SINGLE_TASK,
        SIMPLE_MACHINE,
        &refuses_once(&far_limit_signal(), false),
    );
    edit_metadata(&fixture.root, |m| {
        m["metadata"] = serde_json::json!({"tasks": {"1": {
            "providerLimits": {"working": limit_record(epoch_now() + 3600)}
        }}});
    });
    let started = fixture.root.join("1-starts");
    let mut run = start_text_run(&fixture, "restarted", &[]);
    let watched = watch_the_sleep(&fixture, &mut run, "restarted", || line_count(&started));
    let opened = pass_headers(&watched.stdout);
    assert_eq!(
        opened, 0,
        "a run started onto a persisted wait opened {opened} pass(es) for a ticket it could \
         not spawn; it printed:\n{}",
        watched.stdout
    );
    watched.assert_quiet("a run started onto a persisted wait");
    let said = run.output();
    assert!(!said.contains("Nothing to schedule"), "a held ticket is waiting, not stuck; {said}");
    assert_eq!(line_count(&started), 0, "nothing spawns before the deadline");

    let status = interrupt(&mut run);
    assert_eq!(status.code(), Some(130), "Ctrl+C ends the sleep; {}", run.output());
    assert!(markdown_text(&fixture.root).contains("nextAttemptAt:"), "the wait survives");
}
