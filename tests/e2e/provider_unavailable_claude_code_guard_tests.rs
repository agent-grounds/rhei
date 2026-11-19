//! The endings a Claude Code session is not giving up on its provider, each
//! charged and routed as it is today. These guards pass before the fix and
//! must keep passing after it.
// §FS-rhei-provider-unavailability.2

use super::provider_limit_support::*;
use super::provider_unavailable_claude_code_support::*;

/// A named stream and the outage that plays it.
type Case = (&'static str, fn() -> Outage);

/// Claude Code's own verdict decides, narrowed by status class: a count that
/// stopped short, a `401` retried to the count, a count with no give-up event,
/// an error it never retried, and the same stream from a `codex` profile all
/// stay charged, through the single worker and the pool. §FS-rhei-agents.2.5
// §FS-rhei-agents.5.2.2
#[test]
fn claude_code_endings_that_are_not_an_outage_stay_charged() {
    let cases: [Case; 5] = [
        ("under-the-count", || Outage::claude(&under_the_count(REFUSED))),
        ("401", || Outage::claude(STATUS_401)),
        ("no-give-up", || Outage::claude(&without(REFUSED, is_give_up))),
        ("400", || Outage::claude(STATUS_400)),
        ("codex", || Outage { family: "codex", ..Outage::claude(REFUSED) }),
    ];
    for (name, outage) in cases {
        for mode in [MODES[0], MODES[2]] {
            let fixture = mode.outage(outage()).fixture(&mode.name(name));
            let pass = run_in(&fixture, name, mode);
            assert_not_recognized(&fixture, &pass, mode, "exited", true);
        }
    }
}

/// Without `--continue-on-error` an ending that is not recognized aborts the
/// run as any failed worker does. §FS-rhei-agents.5.2.1 §FS-rhei-run.3
#[test]
fn a_401_without_continue_on_error_aborts_the_run_as_today() {
    let fixture = Outage::claude(STATUS_401).fixture("claude-401-aborts");
    let pass = run_in(&fixture, "strict", MODES[1]);
    assert_not_recognized(&fixture, &pass, MODES[1], "exited", true);
    assert_eq!(pass.code, Some(1), "{}", pass.said());
    pass.assert_says(&format!("agent exited with code 1 for task {TASK}"));
}

/// A session that exits `0` is one Claude Code considered finished: the output
/// check deals with it, whatever its stream says. §FS-rhei-provider-unavailability.2
#[test]
fn a_give_up_stream_that_exits_zero_stays_charged() {
    for mode in [MODES[0], MODES[2]] {
        let fixture = mode.outage(Outage { code: 0, ..Outage::claude(REFUSED) });
        let fixture = fixture.fixture(&mode.name("exit-zero"));
        let pass = run_in(&fixture, "exit-zero", mode);
        assert_not_recognized(&fixture, &pass, mode, "exited", true);
        pass.assert_says("required outputs are missing");
    }
}

/// A target with no provider segment has nothing to key the wait on.
/// §FS-rhei-provider-unavailability.2
#[test]
fn a_claude_code_target_with_no_provider_stays_charged() {
    for mode in [MODES[0], MODES[2]] {
        let outage = Outage { target: TARGET_WITH_NO_PROVIDER, ..Outage::claude(REFUSED) };
        let fixture = mode.outage(outage).fixture(&mode.name("no-provider"));
        let pass = run_in(&fixture, "no-provider", mode);
        assert_not_recognized(&fixture, &pass, mode, "exited", true);
    }
}

/// A session the engine timed out ended by the engine's hand, whatever its
/// stream said before that. §FS-rhei-agents.2.5
#[test]
fn a_timed_out_claude_code_session_stays_charged() {
    for mode in [MODES[0], MODES[2]] {
        let outage = Outage { agent_timeout: Some("2s"), linger: 20, ..Outage::claude(REFUSED) };
        let fixture = mode.outage(outage).fixture(&mode.name("timed-out"));
        let pass = run_in(&fixture, "timed-out", mode);
        assert_not_recognized(&fixture, &pass, mode, "timed out", true);
    }
}

/// A run interrupted while the session lingers after its give-up stream is an
/// interruption, never a recognized ending. §FS-rhei-agents.2.5 §FS-rhei-run.3.2
#[cfg(unix)]
#[test]
fn an_interrupted_claude_code_session_is_not_recognized() {
    for mode in [MODES[0], MODES[2]] {
        let outage = Outage { linger: 20, ..Outage::claude(REFUSED) };
        let fixture = mode.outage(outage).fixture(&mode.name("interrupted"));
        let mut run = start(&fixture, "interrupted", &mode.args());
        let started = mode.tasks().len();
        wait_for("the give-up stream before the interrupt", &mut run, || {
            fixture.root.join(STREAM_OUT).exists() && starts(&fixture) == started
        });
        let pid = run.child().id().to_string();
        assert!(std::process::Command::new("kill")
            .args(["-INT", &pid])
            .status()
            .unwrap()
            .success());
        let pass = finish(&fixture, "interrupted", run);
        assert_eq!(pass.code, Some(130), "{mode:?} {}", pass.said());
        assert_not_recognized(&fixture, &pass, mode, "interrupted", false);
    }
}

/// A stream that also carries a recognized session limit parks as a refusal:
/// it is checked first, and its reset instant beats a backoff. §FS-rhei-agents.2.5
// §FS-rhei-agents.2.3
#[test]
fn a_refusal_in_the_same_stream_wins() {
    let fixture = Outage::claude(&with_refusal(REFUSED)).fixture("claude-refusal-wins");
    let pass = run_once(&fixture, "refusal");
    let record = record_of(&fixture, &pass, TASK);
    assert_eq!(record["ending"], "provider_limited", "{record:#}; {}", pass.said());
    assert_eq!(record["attempt_charged"], false, "{record:#}");
    let wait = task_wait(&fixture, "1");
    assert!(wait.get("kind").is_none(), "a limit, not a backoff: {wait:#}");
    assert_eq!(wait["signal"], LIMIT_SIGNAL, "{wait:#}");
    pass.assert_never_says("the provider was unavailable");
}
