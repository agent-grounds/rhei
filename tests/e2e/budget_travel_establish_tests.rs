//! The first hand-applied edge in a project that has no budget account: it
//! establishes the account, spends a travel unit, and says nothing about it.
//!
//! This is the bypass `agent-grounds/rhei#370` reported. `rhei run` and `rhei
//! budget init` both establish an absent account; the manual path did not, so
//! `defaults.transition_limit` bound nothing at all in a project nobody had
//! run, and a ticket driven by hand could ping-pong without limit.

// §FS-rhei-budgets.4.1 §FS-rhei-budgets.5.4 §FS-rhei-budgets.11

use std::fs;
use std::path::{Path, PathBuf};

use super::budget_edge_support::*;
use super::budget_support::*;
use super::*;

/// Two travel units, which is what makes the third hand edge the refused one.
const TWO_MOVES: &str = r#", "transition_limit": 2"#;

/// The three edges of the report's arm A, applied one command at a time against
/// a project with no account and a machine that declares two units.
///
/// Every assertion is about one of the two halves §FS-rhei-budgets.5.4 and
/// §FS-rhei-budgets.4.1 hold together: the first charge of either kind
/// establishes the account, and an applied edge is a charge whoever selected
/// it. The silence is asserted as hard as the count, because the natural way to
/// write establishment is the way that announces it, and absence that becomes
/// an error rather than an account is what §FS-rhei-budgets.11 forbids.
// §FS-rhei-budgets.4.1 §FS-rhei-budgets.5.4
#[test]
fn the_first_hand_applied_edge_establishes_the_absent_account_and_spends_a_unit() {
    let (dir, plan, machine) = setup_with_agent(
        "budget-travel-establish",
        BARE_PING_PONG_MACHINE,
        RECORDING_AGENT,
        TWO_MOVES,
    );
    assert!(
        !dir.join(".agent-grounds/rhei/budgets").exists(),
        "the scenario starts from an absent account, which is lawful and silent"
    );

    let first = hand_edge(&plan, &machine, "work", "review");

    assert_success(&first);
    // Byte-for-byte, on both streams: establishment prints nothing, prompts for
    // nothing, and warns about nothing. A test that merely looked for the
    // absence of the word "budget" would pass on a build that announced it.
    assert_eq!(
        first.stdout, "Task plan.1 transitioned: 'work' \u{2192} 'review'\n",
        "the establishing edge's stdout is the move's own line and nothing else"
    );
    assert_eq!(first.stderr, "", "and its stderr is empty: nothing is printed, nothing warned");

    let account = account_directory(&dir);
    assert!(
        !dir.join("runtime").join(".agent-grounds").exists(),
        "the account is beside `runtime/` rather than inside it, so a reset cannot reach it"
    );
    assert_eq!(
        receipt_kinds(&dir),
        ["initialize", "identity", "transition"],
        "one transaction wrote all three: the account, the ticket's identity, and the edge"
    );
    let witness = home_for(&dir)
        .join("state/rhei/budget-authority")
        .join(account.file_name().expect("the account directory is named by its uuid"))
        .join("history.jsonl");
    assert!(
        witness.is_file(),
        "establishment writes the machine-wide witness too, at {}",
        witness.display()
    );
    assert!(
        budget_identity(&plan, "1").is_some(),
        "the ticket that moved was given the travel identity the unit was charged against"
    );

    assert_success(&hand_edge(&plan, &machine, "review", "work"));
    let refused = hand_edge(&plan, &machine, "work", "review");

    assert!(
        !refused.status.success(),
        "the third hand edge meets the bound the machine declared\nstdout:\n{}\nstderr:\n{}",
        refused.stdout,
        refused.stderr
    );
    // The same words `rhei run` and `rhei budget init` already produce: the
    // asymmetry the report is about was that this path said nothing at all.
    assert_halt_mentions(&refused, "ticket travel");
    assert_halt_mentions(&refused, "2 (machine)");
    assert_halt_mentions(&refused, "consumed:    2  outstanding: 0  remaining: 0");
    assert_halt_mentions(&refused, "per ticket identity");
    assert_halt_mentions(&refused, "set `defaults.transition_limit` in the machine settings file");
    assert_task_state(&plan, &machine, "1", "work");
}

/// One hand-applied edge, named the way a person runs it: both states spelled
/// out on the command line rather than read back from the document.
///
/// The halt tests read the state they move from, because they edit the plan
/// between invocations. Nothing edits it here, so the sequence of edges is part
/// of what this scenario asserts and is written down.
fn hand_edge(plan: &Path, machine: &Path, from: &str, to: &str) -> CliRun {
    run_at(
        "transition",
        plan,
        machine,
        None,
        &["--task", "1", "--from", from, "--to", to, "--no-callbacks"],
    )
}

/// The one account directory the project holds, named by the uuid establishment
/// minted for it.
///
/// This is where the scenario fails on a build that does not establish: the
/// directory the uuid would sit in does not exist to be read.
fn account_directory(root: &Path) -> PathBuf {
    let accounts = root.join(".agent-grounds/rhei/budgets");
    let mut found: Vec<PathBuf> = fs::read_dir(&accounts)
        .unwrap_or_else(|error| {
            panic!(
                "the first hand edge should have established the account at {}: {error}",
                accounts.display()
            )
        })
        .map(|entry| entry.expect("an account directory").path())
        .collect();
    assert_eq!(found.len(), 1, "one project, one account: {found:?}");
    found.pop().expect("the account directory")
}

/// Every receipt kind in the account, in the order it was appended.
fn receipt_kinds(root: &Path) -> Vec<String> {
    account_journal(root)
        .iter()
        .map(|receipt| receipt["kind"].as_str().expect("every receipt names its kind").to_owned())
        .collect()
}
