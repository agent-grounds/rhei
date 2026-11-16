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

/// Ticket 1 is refused with a far session limit until `expired-1` exists, then
/// writes its result; ticket 2 says it did nothing and exits 0 until `finish-2`
/// exists, then writes its result.
fn fixture(name: &str, machine: &str) -> ProviderFixture {
    ProviderFixture::new(
        name,
        TASKS,
        machine,
        &format!(
            r#"root = pathlib.Path(env('RHEI_ROOT'))
local = env('RHEI_TASK_ID_LOCAL')
append(root / (local + '-starts'), env('RHEI_ATTEMPT') + '\n')
if local == '1' and (root / 'expired-1').exists():
    result('## Result\n\nDone once the wait ended.\n')
    raise SystemExit(0)
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
fn a_spent_ticket_beside_a_provider_wait_opens_no_pass_while_the_run_sleeps() {
    let fixture = fixture("spent-beside-wait", MACHINE);
    let mut run = start_text_run(&fixture, "run", &[]);
    assert_halted_once_and_quiet(&fixture, &mut run, "the sequential loop");
}

/// The same in the `--parallel 2` worker pool. §FS-rhei-run.3.6.1 §FS-rhei-run.5
#[test]
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

/// Run `rhei <args>` against the fixture from a second process, as an operator
/// would while the run sleeps.
fn operator(fixture: &ProviderFixture, args: &[&str]) {
    let out = rhei_command(fixture.root.join(".home"))
        .arg("--state-machine")
        .arg(&fixture.machine)
        .args(args)
        .output()
        .expect("run the operator's rhei command");
    assert!(
        out.status.success(),
        "rhei {args:?} during the sleep: {}{}",
        stdout(&out),
        stderr(&out)
    );
}

/// An advance into a new visit while the run sleeps releases the spent ticket:
/// the new visit gets its own two attempts, halts once on them, and the run
/// goes quiet again. §FS-rhei-run.3.6.1
#[test]
fn an_advance_during_the_sleep_lets_the_spent_ticket_back_in() {
    let machine = MACHINE.replace(
        "  - { from: other, to: completed }",
        "  - { from: other, to: completed }\n  - { from: other, to: other }",
    );
    let fixture = fixture("spent-advance", &machine);
    let mut run = start_text_run(&fixture, "run", &[]);
    wait_for("ticket 2 to halt on its spent attempts", &mut run, || {
        printed(&fixture, "run").contains(SPENT)
    });
    let root = fixture.root.to_str().unwrap();
    operator(&fixture, &["transition", root, "--task", "2", "--from", "other", "--to", "other"]);
    let starts = fixture.root.join("2-starts");
    wait_for("the new visit to spend its own attempts", &mut run, || {
        line_count(&starts) >= 4 && printed(&fixture, "run").matches(SPENT).count() >= 2
    });
    let passes = pass_headers(&stdout_from_file(fixture.dir.join("run.stdout")));
    thread::sleep(QUIET_WINDOW);
    assert!(run.child().try_wait().unwrap().is_none(), "the held ticket still waits");
    let text = printed(&fixture, "run");
    let after = pass_headers(&stdout_from_file(fixture.dir.join("run.stdout")));
    assert!(
        line_count(&starts) == 4 && text.matches(SPENT).count() == 2 && after == passes,
        "the new visit should spawn twice, halt once and leave the run quiet; it printed:\n{text}"
    );
}

/// The end of the wait releases the spent ticket as the generic rule always
/// has: the held ticket completes, the run halts with exit 1 and its halt
/// report still names the spent ticket. §FS-rhei-run.3.6.1 §FS-rhei-run.3 step 9
#[test]
fn the_end_of_the_wait_halts_the_run_naming_the_spent_ticket() {
    let fixture = fixture("spent-end-of-wait", MACHINE);
    let mut run = start_text_run(&fixture, "run", &[]);
    wait_for("ticket 2 to halt on its spent attempts", &mut run, || {
        printed(&fixture, "run").contains(SPENT)
    });
    fs::write(fixture.root.join("expired-1"), "go").unwrap();
    expire_provider_deadlines(&fixture.root);
    let status = run.wait_for_exit("the run to halt once the wait ends");
    let text = printed(&fixture, "run");
    assert_eq!(
        status.code(),
        Some(1),
        "a run ending with a spent ticket halts; it printed:\n{text}"
    );
    assert_eq!(line_count(&fixture.root.join("1-starts")), 2, "the held ticket started once more");
    assert_eq!(line_count(&fixture.root.join("2-starts")), 2, "no new attempts for ticket 2");
    let report = markdown_text(&fixture.root);
    assert!(report.contains("**State:** completed"), "the held ticket completed:\n{report}");
    assert!(
        text.contains("Task workspace.2: Spent [other]") && text.matches(SPENT).count() == 2,
        "the final halt names the spent ticket once more and its final state:\n{text}"
    );
}

/// A ticket whose first log name is taken by a file no spawn record accounts
/// for is refused (§FS-rhei-agents.8.1). Beside a provider wait the refusal
/// prints once and opens no further pass; moving the file away during the
/// sleep lets the ticket start. §FS-rhei-run.3.6.1
#[test]
fn a_refused_log_name_beside_a_provider_wait_opens_no_pass_until_it_is_freed() {
    let fixture = fixture("refused-log-beside-wait", MACHINE);
    let logs = fixture.root.join("runtime/logs");
    fs::create_dir_all(&logs).unwrap();
    let stray = logs.join("task-workspace.2-other-mock-mock-beta.log");
    fs::write(&stray, "stray").unwrap();
    let mut run = start_text_run(&fixture, "run", &[]);
    const REFUSED: &str = "exists and no spawn record accounts for it";
    wait_for("ticket 2's spawn to be refused", &mut run, || {
        printed(&fixture, "run").contains(REFUSED)
    });
    thread::sleep(QUIET_WINDOW);
    if let Some(status) = run.child().try_wait().expect("inspect run status") {
        panic!("the run exited {status} instead of sleeping; {}", run.output());
    }
    let text = printed(&fixture, "run");
    let refusals = text.matches(REFUSED).count();
    let total = pass_headers(&stdout_from_file(fixture.dir.join("run.stdout")));
    assert!(
        refusals == 1 && total <= 3,
        "in the {QUIET_WINDOW:?} after ticket 2's log name was refused the run refused it \
         {refusals} time(s) and opened {total} pass(es) in all; one refusal and at most 3 \
         passes expected while the provider wait kept the run asleep; it printed:\n{text}"
    );
    let starts = fixture.root.join("2-starts");
    assert_eq!(line_count(&starts), 0, "the refused ticket did not start");
    fs::rename(&stray, fixture.dir.join("moved-away.log")).unwrap();
    wait_for("the freed ticket to start", &mut run, || line_count(&starts) >= 1);
    assert_eq!(line_count(&fixture.root.join("1-starts")), 1, "the held ticket did not start");
}
