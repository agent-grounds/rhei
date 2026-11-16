//! A ticket halted on its spent attempts opens no pass while a provider wait
//! beside it keeps the run asleep (agent-grounds/rhei#726).
//!
//! Ticket 1 is held by a session limit two hours out; ticket 2's agent exits 0
//! without its result, so its two attempts are spent and its spawn site halts
//! it. The deadline sleep used to drop that stall on every 500 ms slice, so the
//! ticket came back, was halted again, and a `Pass N:` header and the same halt
//! line printed twice a second until the wait ended. §FS-rhei-run.3.6.1
//! §FS-rhei-run.5.1

use super::provider_limit_support::*;
use super::provider_wait_quiet_support::*;
use super::*;
use std::thread;

/// The halt a spent visit prints, once per halt. §FS-rhei-run.3.6.1
const SPENT: &str = "attempts spent on this visit";

/// Ticket 1 in `working` (held), ticket 2 in `other` with two attempts.
const MACHINE: &str = r#"name: provider-wait-spent-bound
version: 1
states:
  working:
    initial: true
    target: codex:openai:alpha
    attempts: 1
  other:
    target: mock:mock:beta
    attempts: 2
  completed:
    final: true
transitions:
  - { from: working, to: completed }
  - { from: working, to: other }
  - { from: other, to: completed }
"#;

const TASKS: &[(&str, &str)] = &[
    ("01.md", "### Task 1: Held\n**State:** working\n"),
    ("02.md", "### Task 2: Spent\n**State:** other\n"),
];

/// Ticket 1 is refused with a far session limit; ticket 2 says it did nothing
/// and exits 0 until `finish-2` exists, then writes its result.
fn fixture(name: &str, machine: &str) -> ProviderFixture {
    ProviderFixture::new(
        name,
        TASKS,
        machine,
        &format!(
            r#"root = pathlib.Path(env('RHEI_ROOT'))
local = env('RHEI_TASK_ID_LOCAL')
append(root / (local + '-starts'), env('RHEI_ATTEMPT') + '\n')
if local == '1':
    print({signal:?}, file=sys.stderr, flush=True)
    raise SystemExit(1)
if (root / 'finish-2').exists():
    result('## Result\n\nDone after the retry.\n')
else:
    print('did nothing')
"#,
            signal = far_limit_signal()
        ),
    )
}

/// What the run printed on both of its streams so far.
fn printed(fixture: &ProviderFixture, name: &str) -> String {
    let dir = &fixture.dir;
    format!(
        "{}\n{}",
        stdout_from_file(dir.join(format!("{name}.stdout"))),
        stderr_from_file(dir.join(format!("{name}.stderr")))
    )
}

/// Wait for ticket 2's spent halt, then watch the run for [`QUIET_WINDOW`]:
/// it stays asleep, halts the ticket no second time and opens no pass after
/// the halt. §FS-rhei-run.3.6.1
fn assert_halted_once_and_quiet(fixture: &ProviderFixture, run: &mut RunningChild, what: &str) {
    wait_for("ticket 2 to halt on its spent attempts", run, || {
        printed(fixture, "run").contains(SPENT)
    });
    thread::sleep(QUIET_WINDOW);
    if let Some(status) = run.child().try_wait().expect("inspect run status") {
        panic!("{what}: the run exited {status} instead of sleeping; {}", run.output());
    }
    let text = printed(fixture, "run");
    let stdout = stdout_from_file(fixture.dir.join("run.stdout"));
    let halts = text.matches(SPENT).count();
    let total = pass_headers(&stdout);
    assert!(
        halts == 1 && total <= 3,
        "{what}: in the {QUIET_WINDOW:?} after ticket 2 spent its attempts the run halted it \
         {halts} time(s) and opened {total} pass(es) in all; one halt and at most 3 passes \
         expected (the held ticket's, then ticket 2's two attempts) while the provider wait \
         kept the run asleep; it printed:\n{text}"
    );
    assert_eq!(line_count(&fixture.root.join("2-starts")), 2, "{what}: two attempts, no more");
    assert_eq!(line_count(&fixture.root.join("1-starts")), 1, "{what}: one refused start");
}

/// The ticket's own case, in the sequential loop. §FS-rhei-run.3.6.1
#[test]
#[ignore = "red until #726: the deadline sleep re-admits a spent ticket every slice; implement removes this"]
fn a_spent_ticket_beside_a_provider_wait_opens_no_pass_while_the_run_sleeps() {
    let fixture = fixture("spent-beside-wait", MACHINE);
    let mut run = start_text_run(&fixture, "run", &[]);
    assert_halted_once_and_quiet(&fixture, &mut run, "the sequential loop");
}

/// The same in the `--parallel 2` worker pool. §FS-rhei-run.3.6.1 §FS-rhei-run.5
#[test]
#[ignore = "red until #726: the worker pool's sleep re-admits a spent ticket every slice; implement removes this"]
fn a_spent_ticket_beside_a_provider_wait_opens_no_pass_in_the_worker_pool() {
    let machine = MACHINE
        .replace("    initial: true", "    initial: true\n    concurrent: true")
        .replace("    attempts: 2", "    attempts: 2\n    concurrent: true");
    let fixture = fixture("spent-beside-wait-pool", &machine);
    let mut run = start_text_run(&fixture, "run", &["--parallel", "2"]);
    assert_halted_once_and_quiet(&fixture, &mut run, "the worker pool");
}

/// A `rhei retry` from another process while the run sleeps releases the
/// spent ticket: its next pass spawns it with the new attempts, and it
/// completes, while the held ticket still waits. §FS-rhei-run.3.6.1
#[test]
fn a_retry_during_the_sleep_lets_the_spent_ticket_back_in() {
    let fixture = fixture("spent-retry", MACHINE);
    let mut run = start_text_run(&fixture, "run", &[]);
    wait_for("ticket 2 to halt on its spent attempts", &mut run, || {
        printed(&fixture, "run").contains(SPENT)
    });
    fs::write(fixture.root.join("finish-2"), "go").unwrap();
    let retry = rhei_command(fixture.root.join(".home"))
        .arg("--state-machine")
        .arg(&fixture.machine)
        .arg("retry")
        .arg(&fixture.root)
        .args(["--task", "2"])
        .output()
        .expect("run rhei retry");
    assert!(
        retry.status.success(),
        "rhei retry during the sleep: {}{}",
        stdout(&retry),
        stderr(&retry)
    );
    let starts = fixture.root.join("2-starts");
    wait_for("the retried ticket to start again", &mut run, || line_count(&starts) >= 3);
    wait_for("the retried ticket to complete", &mut run, || {
        markdown_text(&fixture.root).contains("**State:** completed")
    });
    assert!(run.child().try_wait().unwrap().is_none(), "the held ticket still waits");
    assert_eq!(line_count(&fixture.root.join("1-starts")), 1, "the held ticket did not start");
}
