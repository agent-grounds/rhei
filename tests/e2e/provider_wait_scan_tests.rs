//! Which tickets a provider wait holds, and which run modes leave a held ticket
//! out of the ready set (agent-grounds/rhei#725).
//!
//! A ticket is held only while it has at least one invocation still to be
//! spawned and every such invocation waits on a provider deadline, and only the
//! continuous run leaves a held ticket out of its ready set. The spec once read
//! as holding a ticket with nothing left to spawn, and as leaving a held ticket
//! out under `--until-idle` and `--dry-run` too. The code never did either;
//! these pin that it still does not.
// §FS-rhei-run.3 §FS-rhei-run.5.1

use super::provider_limit_support::*;
use super::provider_wait_quiet_support::*;
use super::until_idle_support::{assert_exit, bounded, EXIT_IDLE};
use super::*;

/// `finishing` fans out over `codex/openai` and `mock/mock` and owes one output
/// per arm, so an arm whose output is written is satisfied; it leads to the
/// `review` gate, so a satisfied arm owes no result fragment. `solo` uses the
/// `codex/openai` identity on one target alone. §FS-rhei-run.3.3
const MACHINE: &str = r#"name: provider-wait-scan
version: 1
states:
  finishing:
    initial: true
    all_targets:
      - codex:openai:alpha
      - mock:mock:beta
    attempts: 1
    outputs:
      - name: arm
        path: runtime/fanout/{model.name}.txt
  solo:
    target: codex:openai:alpha
    attempts: 1
  review:
    gating: true
  completed:
    final: true
transitions:
  - from: finishing
    to: review
  - from: review
    to: completed
  - from: solo
    to: completed
"#;

/// A stand-in that records every start, by arm, and refuses it: nothing in
/// this file may start.
const LOGS_EVERY_START: &str = r#"root = pathlib.Path(env('RHEI_ROOT'))
append(root / 'starts', env('RHEI_MODEL_NAME', '?') + '\n')
raise SystemExit(1)
"#;

/// Nothing started.
#[track_caller]
fn assert_nothing_started(fixture: &ProviderFixture) {
    let starts = fixture.root.join("starts");
    assert!(
        !starts.exists(),
        "no agent may start here; started: {}",
        fs::read_to_string(&starts).unwrap_or_default()
    );
}

/// The issue's own case. Task 1's fan-out has written both arms' outputs, so it
/// has no invocation left to spawn, while a `codex/openai` wait it recorded
/// still runs for an hour. Task 2 is held by that wait, so the continuous run
/// has a deadline to sleep until. Task 1 is not held: the run advances it to
/// its gate without a spawn, and then sleeps for Task 2. Read as holding a ticket with
/// nothing left to spawn, the rule would sleep the hour with Task 1 unmoved.
/// The same in the sequential loop and the `--parallel` worker pool.
/// §FS-rhei-run.3 §FS-rhei-run.5.1
#[test]
fn a_fan_out_with_no_arm_left_to_spawn_advances_during_its_wait() {
    for (name, extra) in [("sequential", &[][..]), ("pool", &["--parallel", "2"][..])] {
        let fixture = ProviderFixture::new(
            &format!("satisfied-fan-out-{name}"),
            &[
                ("01.md", "### Task 1: Finished fan-out\n**State:** finishing\n"),
                ("02.md", "### Task 2: Held\n**State:** solo\n"),
            ],
            MACHINE,
            LOGS_EVERY_START,
        );
        let deadline = epoch_now() + 3600;
        edit_metadata(&fixture.root, |m| {
            m["metadata"] = serde_json::json!({"tasks": {
                "1": {"providerLimits": {"finishing": limit_record(deadline)}},
                "2": {"providerLimits": {"solo": limit_record(deadline)}},
            }});
        });
        let outputs = fixture.root.join("runtime/fanout");
        fs::create_dir_all(&outputs).expect("create the fan-out's output directory");
        for arm in ["alpha", "beta"] {
            fs::write(outputs.join(format!("{arm}.txt")), "done").expect("write an arm's output");
        }

        let mut run = start_text_run(&fixture, name, extra);
        let task_one = fixture.root.join("tasks/01.md");
        let stdout = fixture.dir.join(format!("{name}.stdout"));
        wait_for("Task 1 to advance and the run to sleep for Task 2", &mut run, || {
            fs::read_to_string(&task_one).unwrap_or_default().contains("**State:** review")
                && stdout_from_file(&stdout).contains(SLEEP_LINE)
        });
        run.stop();
        assert_task_state(&fixture.root, &fixture.machine, "2", "solo");
        assert_nothing_started(&fixture);
    }
}

/// One ticket, held by a `codex/openai` wait for an hour. `--dry-run` and
/// `--until-idle` report the scan as read: each lists the held ticket as ready
/// in its first pass, and neither starts it. Only the continuous run leaves it
/// out. §FS-rhei-run.3 §FS-rhei-run.5.1
#[test]
fn dry_run_and_until_idle_list_a_held_ticket_as_ready() {
    let fixture = ProviderFixture::new(
        "held-ticket-scan",
        &[("01.md", "### Task 1: Held\n**State:** solo\n")],
        MACHINE,
        LOGS_EVERY_START,
    );
    edit_metadata(&fixture.root, |m| {
        m["metadata"] = serde_json::json!({"tasks": {
            "1": {"providerLimits": {"solo": limit_record(epoch_now() + 3600)}},
        }});
    });

    for (mode, code) in [("--dry-run", 0), ("--until-idle", EXIT_IDLE)] {
        let mut command = rhei_command(fixture.root.join(".home"));
        command.arg("--state-machine").arg(&fixture.machine).arg("run").arg(&fixture.root);
        command.args([mode, "--no-dashboard"]);
        let result = bounded(command, mode, &fixture.dir);
        assert_exit(&result, code, &format!("rhei run {mode} over a held ticket"));
        let pass = result.stdout.lines().any(|line| line.starts_with("Pass 1: 1 ready"));
        let ready =
            result.stdout.lines().any(|line| line.starts_with("Ready:") && line.contains("Held"));
        assert!(
            pass && ready,
            "rhei run {mode} reports the scan as read, so it lists the held ticket as ready in \
             its first pass; it printed:\n{}\nstderr:\n{}",
            result.stdout,
            result.stderr
        );
    }
    assert_task_state(&fixture.root, &fixture.machine, "1", "solo");
    assert_nothing_started(&fixture);
}
