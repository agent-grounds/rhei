//! The bounded synthetic cycle: a ticket that would ping-pong forever stops at
//! its travel bound, where it stands, and says what stopped it.
//!
//! This is the failure `agent-grounds/rhei#232` paid $66 for — a review/fix
//! loop that nothing outside the loop bounded.

// §FS-rhei-budgets.4.1 §FS-rhei-budgets.8 §FS-rhei-run.3.4

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::budget_support::*;
use super::*;

const FOUR_MOVES: &str = r#", "transition_limit": 4"#;

/// Four travel units buys four moves. The fifth spawn is the one that must not
/// happen: admission is checked *before* the work it bounds, so a ticket at its
/// bound never starts a process it cannot pay for.
// §FS-rhei-budgets.6.1
#[test]
fn a_ping_pong_ticket_halts_before_the_spawn_that_would_exceed_its_travel_bound() {
    let (dir, plan, machine) = setup("budget-travel-halt", PING_PONG_MACHINE, FOUR_MOVES);

    let result = run_plan(&plan, &machine, None);

    assert_spawn_count(&dir, 4, "four travel units buys four moves and no fifth spawn");
    assert_eq!(ledger(&dir).len(), 4, "one ledger entry per applied edge: {:?}", ledger(&dir));
    assert!(!result.status.success(), "a run that ends with a halted ticket exits non-zero");
}

/// The ticket keeps the state it reached and the artifacts it earned. An engine
/// that advanced it, cancelled it, or wrote a result in its place would be
/// recording a verdict on work it never watched.
// §FS-rhei-budgets.8
#[test]
fn the_halted_ticket_keeps_its_state_its_artifacts_and_its_silence() {
    let (dir, plan, machine) = setup("budget-travel-preserved", PING_PONG_MACHINE, FOUR_MOVES);

    run_plan(&plan, &machine, None);

    // Four moves from `work`: work -> review -> work -> review -> work. The
    // count is asserted first on purpose: an unbounded loop ends on the same
    // state name every even number of moves, so the state alone would let this
    // pass on a build with no bound at all.
    assert_spawn_count(&dir, 4, "the ticket was stopped by its travel bound");
    assert_task_state(&plan, &machine, "1", "work");
    assert_eq!(
        fs::read_to_string(dir.join("runtime/work.md")).expect("the earned artifact survives"),
        "work\n"
    );
    assert!(
        !dir.join("runtime/results/plan.1.md").exists(),
        "no terminal result is invented for a ticket that was stopped rather than finished"
    );
}

/// A halt that does not say what to do is a halt someone has to
/// reverse-engineer. Every field of §FS-rhei-budgets.8 is pinned, including the
/// one that is easiest to get wrong: the remedy names the key that actually
/// limits, never an inner value the ceiling would clamp.
// §FS-rhei-budgets.8
#[test]
fn the_halt_names_the_dimension_the_numbers_the_mode_and_one_remedy() {
    let (_dir, plan, machine) = setup("budget-travel-message", PING_PONG_MACHINE, FOUR_MOVES);

    let result = run_plan(&plan, &machine, None);

    assert_halt_mentions(&result, "ticket travel");
    assert_halt_mentions(&result, "4 (machine)");
    assert_halt_mentions(&result, "consumed:    4  outstanding: 0  remaining: 0");
    assert_halt_mentions(&result, "per ticket identity");
    assert_halt_mentions(&result, "set `defaults.transition_limit` in the machine settings file");
}

/// Travel outlives the run, and it outlives `rhei reset`. "Once per run,
/// forever" is exactly the unbounded case the bound exists for, and reset is
/// the other door into it: a count that reset returned would make reset the way
/// to buy more.
// §FS-rhei-budgets.4.1 §FS-rhei-reset
#[test]
fn reset_and_rerun_converges_on_the_travel_bound_instead_of_escaping_it() {
    let (dir, plan, machine) = setup("budget-travel-reset", PING_PONG_MACHINE, FOUR_MOVES);

    run_plan(&plan, &machine, None);
    assert_spawn_count(&dir, 4, "the first run spends the ticket's whole travel bound");

    // Reset deletes `runtime/`, and with it the spawn log: the rerun's own
    // count starts from nothing, which is why zero is the assertion below.
    assert_success(&run_at("reset", &plan, &machine, None, &["--yes"]));
    let after = run_plan(&plan, &machine, None);

    assert_spawn_count(
        &dir,
        0,
        "the ticket's identity kept its spent travel, so the rerun buys no move at all",
    );
    assert!(
        !after.status.success(),
        "a ticket whose identity has spent its travel is halted again on the next run"
    );
    assert_halt_mentions(&after, "ticket travel");
}

/// A ticket whose identity write was lost adopts the binding the ledger already
/// holds instead of being minted a second one.
///
/// The two writes of an admission are not one transaction: the reservation is
/// appended to the journal and the uuid is written to the plan afterwards, so
/// ENOSPC, a read-only mount, an OOM kill or power loss between them leaves
/// travel charged against a uuid the plan does not carry. Minting on the next
/// admission would hand the ticket a whole second `transition_limit` — the same
/// "every fresh run starts the count again" this bound exists to close.
// §FS-rhei-budgets.5.2 §FS-rhei-budgets.6.1
#[test]
fn a_lost_identity_write_converges_on_the_travel_bound_instead_of_doubling_it() {
    let (dir, plan, machine) = setup("budget-travel-orphan", PING_PONG_MACHINE, FOUR_MOVES);

    run_plan(&plan, &machine, None);
    assert_spawn_count(&dir, 4, "the first run spends the ticket's whole travel bound");

    strip_budget_identity(&plan);
    let after = run_plan(&plan, &machine, None);

    // `runtime/` survives, so the log still holds the first run's four spawns:
    // eight is the doubling this pins against, four is convergence.
    assert_spawn_count(&dir, 4, "the ledger's own binding is adopted, so nothing is bought back");
    assert!(
        !after.status.success(),
        "a ticket that has spent its travel is halted again, whatever its plan still names"
    );
    assert_halt_mentions(&after, "ticket travel");
}

/// The same at the other writer. `budget_charge_applied_edge` appends its
/// `identity` and `transition` receipts and only then hands the metadata back
/// for the caller to write, so a lost write there leaves a travel unit
/// *consumed* — nothing releases one — for an edge that was never applied. A
/// manual move after that must meet the bound it already spent.
// §FS-rhei-budgets.4.1 §FS-rhei-budgets.5.2
#[test]
fn a_manual_edge_after_a_lost_identity_write_meets_the_bound_it_already_spent() {
    let (dir, plan, machine) = setup("budget-travel-orphan-edge", PING_PONG_MACHINE, FOUR_MOVES);

    run_plan(&plan, &machine, None);
    assert_spawn_count(&dir, 4, "the first run spends the ticket's whole travel bound");

    strip_budget_identity(&plan);
    let moved = run_at(
        "transition",
        &plan,
        &machine,
        None,
        &["--task", "1", "--from", "work", "--to", "review", "--no-callbacks"],
    );

    assert!(!moved.status.success(), "a manual edge cannot be the door back to a fresh counter");
    assert_halt_mentions(&moved, "ticket travel");
    assert_task_state(&plan, &machine, "1", "work");
}

// ---------------------------------------------------------------------------
// One identity, one live ticket
// ---------------------------------------------------------------------------

const SIX_MOVES: &str = r#", "transition_limit": 6"#;

/// A bound small enough that the edge a move arrives on is the edge that meets
/// it, which is the case a move's warning used to be lost in.
const TWO_MOVES: &str = r#", "transition_limit": 2"#;

/// A ping-pong that declares no output artifacts.
///
/// These two scenarios take every edge by hand, and `rhei transition` holds a
/// state's declared outputs against the move — no agent runs here, so nothing
/// would ever write one. [`PING_PONG_MACHINE`] declares them because the
/// scenarios above let `rhei run` drive the loop. §FS-rhei-states.3.3
const BARE_PING_PONG_MACHINE: &str = r#"name: budget-identity-ping-pong
version: 1
states:
  work:
    initial: true
    description: Do a round of work
    agent: mock
    agent_timeout: 30s
  review:
    description: Send it back for another round
    agent: mock
    agent_timeout: 30s
  cancelled:
    description: Stop
    final: true
transitions:
  - { from: work, to: review, description: Round done }
  - { from: review, to: work, description: Another round }
  - { from: work, to: cancelled, description: Stop }
  - { from: review, to: cancelled, description: Stop }
"#;

/// Two tasks in one plan, which is one metadata file. That is the shape the
/// binding key cannot read: a definition copied onto a sibling and a definition
/// renumbered in place are the same bytes at the same path.
/// §FS-rhei-budgets.5.2.1
const TWO_TASK_PLAN: &str = r#"# Rhei: Bounded work

## Tasks

### Task 1: The original
**State:** work

### Task 2: The copy
**State:** work
"#;

/// A ticket's travel history cannot be taken from it by a second ticket naming
/// its identity, and a travel bound cannot be doubled by copying a task
/// definition.
///
/// `plan.1` earns a binding and spends two of its six units. A person copies its
/// definition onto `plan.2`, `budgetTicketId` and all — which no command does and
/// `cp` does for free — and moves the copy. Today the binding's `display_id`
/// silently becomes `plan.2`: the two tickets draw one counter, and once `plan.1`
/// no longer matches the binding it is minted a whole second bound. The ticket
/// that earned the history is the one that loses it.
// §FS-rhei-budgets.5.2.1 §REQ-bounded-neural-work.4
#[test]
fn a_copy_claiming_a_live_ticket_s_identity_is_refused_rather_than_rebound() {
    let (dir, plan, machine) =
        setup_identity_plan("budget-identity-claimed", TWO_TASK_PLAN, SIX_MOVES);

    assert_success(&edge(&plan, &machine, "1"));
    assert_success(&edge(&plan, &machine, "1"));
    let uuid = budget_identity(&plan, "1").expect("plan.1 earned a budget identity");
    let bindings = receipts(&dir, "identity");

    copy_budget_identity(&plan, "1", "2");
    let refused = edge(&plan, &machine, "2");

    assert!(
        !refused.status.success(),
        "a second live ticket claiming a bound identity is refused\nstdout:\n{}\nstderr:\n{}",
        refused.stdout,
        refused.stderr
    );
    // Both display ids and the uuid, because those are what a person needs to
    // find the key they duplicated.
    assert_stderr_names(&refused, "plan.1");
    assert_stderr_names(&refused, "plan.2");
    assert_stderr_names(&refused, &uuid);
    assert_task_state(&plan, &machine, "2", "work");
    assert_eq!(
        receipts(&dir, "identity"),
        bindings,
        "a refused admission moves no binding, so it appends no identity receipt"
    );

    // And the original still spends against its own counter: two of six gone,
    // four left, and no fifth.
    for spent in 2..6 {
        assert_success(&edge(&plan, &machine, "1"));
        assert_eq!(
            receipts(&dir, "transition"),
            vec![(uuid.clone(), "plan.1".to_owned()); spent + 1],
            "every unit charged against this identity was drawn by the ticket that owns it"
        );
    }
    let halted = edge(&plan, &machine, "1");

    assert!(!halted.status.success(), "the bound plan.1 earned is the bound plan.1 meets");
    assert_halt_mentions(&halted, "ticket travel");
}

/// The lawful half, pinned as lawful. A renumbered ticket is the same ticket:
/// the key follows it, the travel comes with it, and the move is reported rather
/// than silent.
///
/// This is the case the refusal cannot reach — one live ticket claims the
/// identity, so the document cannot be told from a relocation — which is exactly
/// why it owes the warning. Travel following a heading edit in silence is the
/// surprise this point exists to remove.
// §FS-rhei-budgets.5.2.1
#[test]
fn a_renumbered_ticket_keeps_its_travel_and_says_where_it_went() {
    let (dir, plan, machine) = setup_identity_plan("budget-identity-renumbered", PLAN, SIX_MOVES);

    assert_success(&edge(&plan, &machine, "1"));
    assert_success(&edge(&plan, &machine, "1"));
    let uuid = budget_identity(&plan, "1").expect("plan.1 earned a budget identity");

    renumber_task(&plan, "1", "3");
    let moved = edge(&plan, &machine, "3");

    assert_success(&moved);
    assert_eq!(
        budget_identity(&plan, "3").as_deref(),
        Some(uuid.as_str()),
        "a renumbered ticket is the same ticket: nothing is minted for it"
    );
    assert_eq!(
        receipts(&dir, "transition"),
        vec![
            (uuid.clone(), "plan.1".to_owned()),
            (uuid.clone(), "plan.1".to_owned()),
            (uuid.clone(), "plan.3".to_owned()),
        ],
        "the travel plan.1 spent is the travel plan.3 carries on spending"
    );
    // Asserted on stderr alone: the move's own line on stdout already names
    // plan.3, so a test that read both would pass on a build that says nothing.
    for named in ["warning:", "plan.1", "plan.3", uuid.as_str()] {
        assert_stderr_names(&moved, named);
    }
}

/// A lawful move is reported on what the journal did, never on what the edge
/// did.
///
/// `bind_ticket` appends the `identity` receipt before anything is charged, so
/// the binding's display id has moved durably whatever the charge then does.
/// Printing the line only where the whole edge succeeded dropped it exactly
/// where it mattered most — an exhausted bound — and dropped it for good: the
/// next edge matches the recorded display id and has no move to report.
// §FS-rhei-budgets.5.2.1
#[test]
fn a_move_whose_edge_is_then_refused_still_says_the_binding_moved() {
    let (dir, plan, machine) = setup_identity_plan("budget-identity-move-refused", PLAN, TWO_MOVES);

    assert_success(&edge(&plan, &machine, "1"));
    assert_success(&edge(&plan, &machine, "1"));
    let uuid = budget_identity(&plan, "1").expect("plan.1 earned a budget identity");

    renumber_task(&plan, "1", "3");
    let refused = edge(&plan, &machine, "3");

    assert!(!refused.status.success(), "plan.1 spent both units, so plan.3 meets the bound");
    assert_halt_mentions(&refused, "ticket travel");
    for named in ["warning: travel for", "plan.1", "plan.3", uuid.as_str()] {
        assert_stderr_names(&refused, named);
    }
    assert_eq!(
        receipts(&dir, "identity").last().map(|(_, display)| display.clone()),
        Some("plan.3".to_owned()),
        "the binding moved durably, which is why the line may not be dropped"
    );
}

// ---------------------------------------------------------------------------
// The fixture for the two scenarios above
// ---------------------------------------------------------------------------

/// A ping-pong workspace over `plan_text`, with the project's budget account
/// already established.
///
/// `rhei transition` charges an applied edge against an account that exists; it
/// does not establish one, which is why `budget init` is part of the fixture
/// rather than of the scenario. §FS-rhei-budgets.5.4
fn setup_identity_plan(prefix: &str, plan_text: &str, moves: &str) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let plan = write_fixture_file(&dir, "plan.rhei.md", plan_text);
    let machine = write_fixture_file(&dir, "states.yaml", BARE_PING_PONG_MACHINE);
    let agent = write_python_agent(&dir, "mock-agent.py", RECORDING_AGENT);
    write_machine_settings(&dir, &agent_defaults(&agent, moves));
    assert_success(&budget_init(&plan));
    (dir, plan, machine)
}

/// Assert a run named something **on stderr**, which is where a refusal and a
/// warning both belong.
///
/// [`assert_halt_mentions`] reads stdout as well, and would be satisfied here by
/// the move's own `Task plan.2 transitioned` line — so a build that refused
/// nothing and said nothing would pass.
fn assert_stderr_names(result: &CliRun, expected: &str) {
    assert!(
        result.stderr.contains(expected),
        "stderr must name {expected:?}\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
}

/// `rhei budget init`, which cannot go through [`run_at`]: `budget` takes its own
/// subcommand where that helper puts the plan.
fn budget_init(plan: &Path) -> CliRun {
    let root = plan.parent().expect("plan has a parent");
    let mut cmd: Command = rhei_command(home_for(root));
    cmd.arg("budget").arg("init").arg(plan);
    cmd.arg("--invocations").arg("100").arg("--reason").arg("pin the identity rule");
    CliRun::from(&cmd.output().expect("rhei budget init should run"))
}

/// One ping-pong edge for a task, whichever of the two states it stands in.
///
/// The state is read from the document rather than tracked, because these
/// scenarios edit the document between invocations and a counted move would
/// stop meaning what it says the moment one of those edits lands.
fn edge(plan: &Path, machine: &Path, task: &str) -> CliRun {
    let (from, to) = match task_state(plan, task).as_str() {
        "work" => ("work", "review"),
        "review" => ("review", "work"),
        other => panic!("plan.{task} stands in '{other}', which this fixture does not drive"),
    };
    run_at(
        "transition",
        plan,
        machine,
        None,
        &["--task", task, "--from", from, "--to", to, "--no-callbacks"],
    )
}

/// The state the document records for a task.
fn task_state(plan: &Path, task: &str) -> String {
    let text = fs::read_to_string(plan).expect("read the plan");
    let heading = format!("### Task {task}:");
    let mut lines = text.lines().skip_while(|line| !line.starts_with(&heading)).skip(1);
    lines
        .find_map(|line| {
            line.trim().strip_prefix("**State:**").map(|state| state.trim().to_owned())
        })
        .unwrap_or_else(|| panic!("plan.{task} has no state in {}", plan.display()))
}

/// The `budgetTicketId` the plan's frontmatter carries for a task, if any.
///
/// Read line by line rather than through a YAML parse because the metadata keys
/// are task *numbers*, which a parser hands back as integers while every caller
/// here has a display id in hand. §FS-rhei-budgets.5.2
fn budget_identity(plan: &Path, task: &str) -> Option<String> {
    let text = fs::read_to_string(plan).expect("read the plan");
    let mut under: Option<&str> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(id) = trimmed.strip_suffix(':') {
            if !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()) {
                under = Some(id);
            }
        }
        if under == Some(task) {
            if let Some(uuid) = trimmed.strip_prefix("budgetTicketId:") {
                return Some(uuid.trim().to_owned());
            }
        }
    }
    None
}

/// Give `to` the identity the plan carries for `from`: the hand copy of a task
/// definition that no command performs and `cp` performs for free.
/// §FS-rhei-budgets.5.2.1
fn copy_budget_identity(plan: &Path, from: &str, to: &str) {
    let uuid = budget_identity(plan, from)
        .unwrap_or_else(|| panic!("plan.{from} carries no identity to copy"));
    let text = fs::read_to_string(plan).expect("read the plan");
    assert_eq!(text.matches("  tasks:\n").count(), 1, "the plan has one tasks block:\n{text}");
    let copied =
        text.replace("  tasks:\n", &format!("  tasks:\n    {to}:\n      budgetTicketId: {uuid}\n"));
    fs::write(plan, copied).expect("write the plan with the copied identity");
}

/// Renumber a task, heading and metadata key together, which is how a person
/// relocates one: the identity keeps exactly one live claimant and the display
/// id the account counts against is gone. §FS-rhei-budgets.5.2.1
fn renumber_task(plan: &Path, from: &str, to: &str) {
    let text = fs::read_to_string(plan).expect("read the plan");
    let (heading, metadata) = (format!("### Task {from}:"), format!("{from}:"));
    let mut renamed = String::with_capacity(text.len());
    let (mut headings, mut keys) = (0, 0);
    for line in text.lines() {
        if line.starts_with(&heading) {
            headings += 1;
            renamed.push_str(&line.replacen(&heading, &format!("### Task {to}:"), 1));
        } else if line.trim() == metadata {
            keys += 1;
            renamed.push_str(&line.replacen(&metadata, &format!("{to}:"), 1));
        } else {
            renamed.push_str(line);
        }
        renamed.push('\n');
    }
    assert_eq!((headings, keys), (1, 1), "plan.{from} has one heading and one key:\n{text}");
    fs::write(plan, renamed).expect("write the renumbered plan");
}

/// Every receipt of `kind` in the project's account, as (ticket uuid, display
/// id) in the order they were appended. §FS-rhei-budgets.5.2
fn receipts(root: &Path, kind: &str) -> Vec<(String, String)> {
    let accounts = root.join(".agent-grounds/rhei/budgets");
    let mut journal = String::new();
    for account in fs::read_dir(&accounts).expect("the project has a budget account") {
        let path = account.expect("an account directory").path().join("journal.jsonl");
        journal.push_str(&fs::read_to_string(&path).unwrap_or_default());
    }
    journal
        .lines()
        .filter_map(|line| {
            let receipt: serde_json::Value =
                serde_json::from_str(line).expect("every journal line is one JSON receipt");
            if receipt["kind"] != kind {
                return None;
            }
            let identity = receipt["payload"]["ticket_identity"].as_str()?;
            let display = receipt["payload"]["display_id"].as_str()?;
            Some((identity.rsplit(':').next()?.to_owned(), display.to_owned()))
        })
        .collect()
}
