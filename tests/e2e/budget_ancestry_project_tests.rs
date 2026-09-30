//! An ancestry descriptor is only an ancestor in the journal that minted it.
//!
//! The scope this suite adds over the unit cases beside `authenticate_ancestor`
//! is that it is a real `rhei run`, with a real environment, against a project
//! of its own. That matters because the defect these tests pin was not a test
//! harness defect: a `rhei run` of any plan outside the exporting agent's
//! project was refused before it spawned, and so was every one of rhei's own
//! tests, each of which resolves a state root of its own. A run whose account
//! is not the one holding the ancestor is admitted against its own account and
//! says so once; inside one account, forging a name still buys nothing.
//!
//! Every test here decides **both** variables, setting the one it wants and
//! removing the one it does not. A test that merely omits one inherits whatever
//! the surrounding process has, which is exactly how this defect hid: run from
//! inside a `rhei run` agent, the suite carried a live ancestry token nobody
//! wrote down and three targets failed on an unrelated assertion.

// §FS-rhei-budgets.7 §FS-rhei-budgets.7.1 §FS-rhei-budgets.7.2

use std::path::{Path, PathBuf};
use std::process::Command;

use super::budget_support::*;
use super::*;

/// The two halves of the descriptor. Spelled here rather than imported so that
/// a rename in the product is a failure this suite reports rather than one it
/// silently follows: these are names an operator's environment carries.
/// §FS-rhei-budgets.7.1
const RESERVATION_ENV: &str = "RHEI_BUDGET_PARENT_RESERVATION";
const ACCOUNT_ENV: &str = "RHEI_BUDGET_PARENT_ACCOUNT";

/// A reservation no journal these tests make can hold, and an account uuid no
/// project these tests make can be minted with. Together they are a descriptor
/// from somewhere else, which is what an agent of another project exports.
const FOREIGN_RESERVATION: &str = "reservation:11111111-2222-3333-4444-555555555555";
const FOREIGN_ACCOUNT: &str = "99999999-9999-4999-8999-999999999999";

/// What the refusal reads like when a name is looked up where it was never
/// recorded. Asserted on by name so that a test failing for some *other* reason
/// cannot be mistaken for this one. §FS-rhei-budgets.7.1
const UNRESOLVABLE: &str = "no such reservation";
const REFUSAL: &str = "nested admission cannot use ancestor";

/// A fake agent that records the spawn **with the descriptor it was handed**,
/// and then satisfies its state so the run ends rather than retrying.
///
/// The descriptor is read from `os.environ` rather than through the fixture's
/// `env`, because the two budget variables are not among the five identity
/// names the prelude recovers from the prompt: an absent one must read as
/// absent here, not as something to reconstruct. §FS-rhei-agents.4
const DESCRIPTOR_AGENT: &str = r#"import os as _os
root = pathlib.Path(env('RHEI_ROOT'))
state = env('RHEI_STATE')
append(root / 'runtime' / 'spawn-count.log', '{} {} {}\n'.format(
    state,
    _os.environ.get('RHEI_BUDGET_PARENT_RESERVATION', '-'),
    _os.environ.get('RHEI_BUDGET_PARENT_ACCOUNT', '-')))
write(root / 'runtime' / '{}.md'.format(state), '{}\n'.format(state))
result('done\n')
"#;

/// A machine whose one agent state is named after the plan that runs it, so
/// each plan in a project owns an artifact of its own.
///
/// One artifact per plan is what keeps a project's later runs honest: were two
/// plans to share an output path, the first run's file would satisfy the
/// second's completion condition and the second would never spawn, so a test
/// counting spawns would be counting one. §FS-rhei-states.3.3
fn machine(states: &[&str]) -> String {
    let mut text = String::from("name: budget-ancestry\nversion: 1\nstates:\n");
    for (index, state) in states.iter().enumerate() {
        text.push_str(&format!(
            "  {state}:\n{}    description: Do the work once\n    agent: mock\n    \
             agent_timeout: 30s\n    outputs:\n      - name: {state}\n        path: \
             runtime/{state}.md\n",
            if index == 0 { "    initial: true\n" } else { "" }
        ));
    }
    text.push_str("  completed:\n    description: Done\n    final: true\ntransitions:\n");
    for state in states {
        text.push_str(&format!("  - {{ from: {state}, to: completed, description: Done }}\n"));
    }
    text
}

/// A plan of `tasks` tasks, each pinned to `state`.
fn plan(state: &str, tasks: usize) -> String {
    let mut text = String::from("# Rhei: Bounded work\n\n## Tasks\n");
    for index in 1..=tasks {
        text.push_str(&format!("\n### Task {index}: Work {index}\n**State:** {state}\n"));
    }
    text
}

/// A project of its own, holding one single-file plan per name in `plans`.
///
/// One directory, several plans: a bare rhei's project root is its execution
/// root, so plans that sit side by side share one account — which is what lets a
/// later run of this project be placed under an ancestor an earlier one minted.
/// A plan of its own per run is what a second run needs in order to have any
/// work left to admit at all. §FS-rhei-budgets.5.1
fn project(prefix: &str, plans: &[&str]) -> (TestDir, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let agent = write_python_agent(&dir, "mock-agent.py", DESCRIPTOR_AGENT);
    write_machine_settings(&dir, &agent_defaults(&agent, ""));
    for name in plans {
        write_fixture_file(&dir, &format!("{name}.rhei.md"), &plan(name, 1));
        write_fixture_file(&dir, &format!("{name}-states.yaml"), &machine(&[name]));
    }
    let root = dir.to_path_buf();
    (dir, root)
}

/// `rhei run` of one plan in `root`, with **both** halves of the descriptor
/// decided: each is either set to a value this test chose or removed outright,
/// never inherited. §FS-rhei-budgets.7.1
fn run_with_descriptor(
    root: &Path,
    name: &str,
    reservation: Option<&str>,
    account: Option<&str>,
) -> CliRun {
    let mut cmd: Command = rhei_command(home_for(root));
    cmd.arg("--state-machine")
        .arg(root.join(format!("{name}-states.yaml")))
        .arg("run")
        .arg(root.join(format!("{name}.rhei.md")))
        .arg("--no-tui")
        .arg("--no-callbacks");
    for (key, value) in [(RESERVATION_ENV, reservation), (ACCOUNT_ENV, account)] {
        match value {
            Some(value) => cmd.env(key, value),
            None => cmd.env_remove(key),
        };
    }
    CliRun::from(&cmd.output().expect("rhei run should run"))
}

/// The account uuid this project was minted with: the one directory under its
/// account home. §FS-rhei-budgets.5.1
fn account_uuid(root: &Path) -> String {
    let home = root.join(".agent-grounds/rhei/budgets");
    let mut found: Vec<String> = std::fs::read_dir(&home)
        .unwrap_or_else(|why| panic!("no account at {}: {why}", home.display()))
        .map(|entry| entry.expect("account entry").file_name().to_string_lossy().into_owned())
        .collect();
    found.sort();
    assert_eq!(found.len(), 1, "a project has exactly one account: {found:?}");
    found.remove(0)
}

/// Every reservation this project's journal holds, newest last, as the `reserve`
/// receipts recorded them. §FS-rhei-budgets.5.2
fn reservations(root: &Path) -> Vec<serde_json::Value> {
    let path =
        root.join(".agent-grounds/rhei/budgets").join(account_uuid(root)).join("journal.jsonl");
    let text = std::fs::read_to_string(&path).expect("the journal should be readable");
    let mut rows = Vec::new();
    for line in text.lines() {
        let receipt: serde_json::Value = serde_json::from_str(line).expect("a receipt is JSON");
        if receipt["kind"] != "reserve" {
            continue;
        }
        let Some(reserved) = receipt["payload"]["reservations"].as_array() else { continue };
        rows.extend(reserved.iter().cloned());
    }
    rows
}

/// The line on which the run refused an ancestry claim, where it did.
fn ancestry_refusal(result: &CliRun) -> Option<String> {
    format!("{}{}", result.stdout, result.stderr)
        .lines()
        .find(|line| line.contains(REFUSAL))
        .map(str::to_owned)
}

/// Assert the run did not refuse the descriptor, quoting the refusal when it
/// did — so a failing leg says `no such reservation` in so many words rather
/// than only reporting a spawn count of zero.
fn assert_admitted(result: &CliRun, why: &str) {
    if let Some(refusal) = ancestry_refusal(result) {
        panic!("{why}, but the run refused it before any spawn:\n{refusal}");
    }
}

/// Every `note:` line about a descriptor minted elsewhere. §FS-rhei-budgets.7.2
fn cross_project_notes(result: &CliRun) -> Vec<String> {
    format!("{}{}", result.stdout, result.stderr)
        .lines()
        .filter(|line| line.contains("note:") && line.contains("another project"))
        .map(str::to_owned)
        .collect()
}

/// The descriptor each spawn was handed, as the agent read it out of its own
/// environment: `(state, reservation, account)`, with `-` for an absent value.
fn handed_down(root: &Path) -> Vec<(String, String, String)> {
    spawns(root)
        .iter()
        .map(|line| {
            let mut parts = line.split_whitespace();
            let state = parts.next().unwrap_or_default().to_owned();
            let reservation = parts.next().unwrap_or_default().to_owned();
            let account = parts.next().unwrap_or_default().to_owned();
            (state, reservation, account)
        })
        .collect()
}

/// A run whose own account is not the one holding the ancestor is no descendant
/// at all: it opens a balance in its own ledger, and its receipt names no
/// parent — which is what keeps replay from later reading the chain as corrupt.
/// §FS-rhei-budgets.7.2
///
/// **Fails before the fix**, and this is the defect: today the descriptor's name
/// is looked up in a journal that never recorded it, and the run is refused
/// `no such reservation` before a single agent starts.
#[test]
fn a_run_charging_another_account_is_admitted_against_its_own() {
    let (_dir, root) = project("ancestry-cross-project", &["alpha"]);

    let result =
        run_with_descriptor(&root, "alpha", Some(FOREIGN_RESERVATION), Some(FOREIGN_ACCOUNT));

    assert_admitted(&result, "a descriptor minted for another project names no ancestor here");
    assert_spawn_count(&root, 1, "the run should have admitted and spawned its one task");
    let reserved = reservations(&root);
    assert_eq!(reserved.len(), 1, "one admission, one reserve receipt: {reserved:?}");
    assert_eq!(
        reserved[0]["parent_reservation"],
        serde_json::Value::Null,
        "an unparented admission must record no parent: {}",
        reserved[0]
    );
}

/// And it says so once per `rhei run` — not once per admission, which is what
/// made the refusal expensive to read in the first place. §FS-rhei-budgets.7.2
///
/// **Fails before the fix**: the run is refused before any note could exist.
#[test]
fn a_cross_project_descriptor_is_reported_once_per_run() {
    let (_dir, root) = project("ancestry-note", &["alpha"]);
    let single =
        run_with_descriptor(&root, "alpha", Some(FOREIGN_RESERVATION), Some(FOREIGN_ACCOUNT));
    assert_admitted(&single, "a one-task plan should be admitted against its own account");
    let notes = cross_project_notes(&single);
    assert_eq!(notes.len(), 1, "exactly one note for a one-task plan: {notes:?}");
    assert!(notes[0].contains(FOREIGN_RESERVATION), "the note names the reservation: {}", notes[0]);
    let own = root.file_name().expect("project name").to_string_lossy().into_owned();
    assert!(
        notes[0].contains(&own),
        "the note names the account this run charges instead ({own}): {}",
        notes[0]
    );

    // A second ticket is a second admission and not a second fact: the
    // descriptor is a property of the run's environment.
    let (_pair_dir, pair_root) = project("ancestry-note-pair", &["duo"]);
    write_fixture_file(&pair_root, "duo.rhei.md", &plan("duo", 2));
    let pair =
        run_with_descriptor(&pair_root, "duo", Some(FOREIGN_RESERVATION), Some(FOREIGN_ACCOUNT));
    assert_admitted(&pair, "a two-task plan should be admitted against its own account");
    assert_spawn_count(&pair_root, 2, "both tickets should have spawned");
    let notes = cross_project_notes(&pair);
    assert_eq!(notes.len(), 1, "a second ticket does not repeat the note: {notes:?}");
}

/// The refusal that survives says **where the value came from**. Naming only the
/// value reads as damaged budget state, and sends the reader to the account
/// directory rather than to the environment. §FS-rhei-budgets.7.1
///
/// **Fails before the fix**: the message names the reservation and nothing else.
#[test]
fn a_refused_descriptor_names_the_variable_it_came_from() {
    let (_dir, root) = project("ancestry-provenance", &["alpha", "beta"]);
    // One clean admission, only so that the project has an account to name.
    let first = run_with_descriptor(&root, "alpha", None, None);
    assert_admitted(&first, "a run with no descriptor at all has no ancestry to authenticate");
    let own = account_uuid(&root);

    let refused = run_with_descriptor(&root, "beta", Some(FOREIGN_RESERVATION), Some(&own));

    let refusal = ancestry_refusal(&refused).unwrap_or_else(|| {
        panic!(
            "forging a name in one's own ledger must still be refused;\nstdout:\n{}\nstderr:\n{}",
            refused.stdout, refused.stderr
        )
    });
    assert!(refusal.contains(UNRESOLVABLE), "the refusal keeps its force: {refusal}");
    assert!(
        refusal.contains(RESERVATION_ENV),
        "the refusal should name where the value came from: {refusal}"
    );
}

/// **Guard — passes before the fix and must keep passing after it.** The half of
/// the contract the fix must not cost: a nested run of the *same* project is
/// still placed under its ancestor, its receipt names that parent, and a second
/// child under an envelope of one is still refused by that envelope.
/// §FS-rhei-budgets.7 §FS-rhei-budgets.7.1
#[test]
fn a_same_project_descriptor_still_places_the_child_under_its_ancestor() {
    let (_dir, root) = project("ancestry-same-project", &["alpha", "beta", "gamma"]);

    // The first run mints a live ancestor: one started reservation whose
    // descendant envelope is one. The agent reads its own token out for us,
    // which is the same value a nested `rhei run` inside it would inherit.
    let first = run_with_descriptor(&root, "alpha", None, None);
    assert_admitted(&first, "the run that mints the ancestor claims none of its own");
    let own = account_uuid(&root);
    let handed = handed_down(&root);
    let ancestor = handed
        .iter()
        .find(|(state, _, _)| state == "alpha")
        .map(|(_, reservation, _)| reservation.clone())
        .expect("the first spawn should have been handed a reservation of its own");
    assert!(
        ancestor.starts_with("reservation:"),
        "an ancestry token names a reservation: {handed:?}"
    );

    // A nested run naming it, with the accounts matching, is its descendant.
    let child = run_with_descriptor(&root, "beta", Some(&ancestor), Some(&own));
    assert_admitted(&child, "a descriptor of this very project names a live ancestor");
    assert_spawn_count(&root, 2, "the child should have spawned under its ancestor");
    let placed = reservations(&root)
        .into_iter()
        .filter(|row| row["parent_reservation"] == serde_json::Value::String(ancestor.clone()))
        .count();
    assert_eq!(placed, 1, "the child's receipt should carry that ancestor as its parent");

    // And the envelope of one is now spent, so a second child is refused by the
    // envelope rather than by the ancestor being unavailable.
    let second = run_with_descriptor(&root, "gamma", Some(&ancestor), Some(&own));
    assert_spawn_count(&root, 2, "a second child under an envelope of one spawns nothing");
    assert_halt_mentions(&second, "permits 1 descendant invocations");
    assert_eq!(
        ancestry_refusal(&second),
        None,
        "an exhausted envelope is not an unavailable ancestor;\nstdout:\n{}\nstderr:\n{}",
        second.stdout,
        second.stderr
    );
}

/// **Guard — passes before the fix and must keep passing after it.** With
/// `RHEI_BUDGET_PARENT_ACCOUNT` absent, the reservation is taken as an ancestor
/// exactly as it was before that variable existed, refusal included. This is the
/// whole of the compatibility argument: a parent too old to name its account
/// loses nothing. §FS-rhei-budgets.7.1
#[test]
fn an_absent_account_variable_leaves_the_reservation_an_ancestor() {
    let (_dir, root) = project("ancestry-absent-account", &["alpha"]);

    let result = run_with_descriptor(&root, "alpha", Some(FOREIGN_RESERVATION), None);

    let refusal = ancestry_refusal(&result).unwrap_or_else(|| {
        panic!(
            "a reservation with no account beside it is still an ancestry claim;\nstdout:\n{}\nstderr:\n{}",
            result.stdout, result.stderr
        )
    });
    assert!(
        refusal.contains(UNRESOLVABLE),
        "and an unresolvable one still buys nothing: {refusal}"
    );
    assert_spawn_count(&root, 0, "the refusal lands before any spawn");
}
