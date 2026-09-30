//! A project laid down at a path another project used before it.
//!
//! The witness index is keyed by absolute path, so the new project resolves to
//! the old one's account, finds no journal of its own, and reads as a
//! rolled-back one. It stays refused — path reuse and a rolled-back journal
//! leave identical state, and the check exists for the second
//! ([§FS-rhei-budgets.5.4](../../docs/functional-spec/rhei-budgets.spec.md)) —
//! so nothing here asserts that the second run succeeds. What these cases pin
//! is that the refusal says which two things it cannot tell apart, that the one
//! diagnostic it names can actually be run, and that there is a command which
//! answers the reading the operator knows to be true.
//!
//! The remedy is the part with teeth. Copying the witness over the missing
//! journal, which the refusal instructs today, *works* — and hands a brand-new
//! project the entire spend of the one that held the path before it. So the
//! instruction must not be offered where the journal is wholly absent, and the
//! command that replaces it must refuse an account that verifies, or it becomes
//! the faucet the bound exists to close.

// §FS-rhei-budgets.5.3 §FS-rhei-budgets.5.4 §FS-rhei-budgets.10 §FS-rhei-errors.5

use std::fs;
use std::path::Path;

use super::budget_path_reuse_support::*;
use super::budget_support::assert_spawn_count;
use super::*;

/// A project established, run, and laid down again at the same path.
///
/// Nothing here asserts the second run succeeds — it must not, and
/// §FS-rhei-budgets.5.4 says why. What it asserts is that the refusal names
/// what it found instead of instructing the copy that adopts a stranger's
/// spend, that it ends in a command that can be run, and that once the operator
/// has said which reading is true the path is usable again.
// §FS-rhei-budgets.5.4
#[test]
fn the_documented_sequence_is_refused_with_both_readings_and_released_by_forget() {
    let world = world("budget-path-reuse-sequence");
    let (uuid, receipts) = reuse_the_path(&world);

    let second = world.run();

    assert!(!second.status.success(), "a project at a reused path is refused:\n{}", said(&second));
    let refusal = said(&second);
    assert!(
        refusal.contains("this project has no budget journal"),
        "the refusal says what it found — no journal at all, rather than one it \
         mistrusts; got:\n{refusal}"
    );
    assert!(
        refusal.contains(&world.witness(&uuid).display().to_string()),
        "the refusal names the witness that claims this path; got:\n{refusal}"
    );
    assert!(
        refusal.contains(&format!("records {receipts} receipt")),
        "the refusal says how much history is charged to this path, so an operator \
         can tell a lost tail from a stranger's account; got:\n{refusal}"
    );
    assert!(
        !refusal.contains("copying the witness back over it"),
        "where the journal is wholly absent the copy is not the remedy: it succeeds, \
         and charges this project the previous occupant's whole spend; got:\n{refusal}"
    );
    assert!(
        refusal.contains(&format!("rhei budget show {}", world.canonical_project())),
        "the refusal ends in the one diagnostic it names, spelled with the target \
         that was typed; got:\n{refusal}"
    );
    assert_spawn_count(&world.project, 0, "nothing ran: the refusal is before the first spawn");

    let forget = world.forget("scratch directory reused");
    assert_success(&forget);

    let third = world.run();

    assert_success(&third);
    assert_spawn_count(&world.project, 1, "the retired path admits the new project's first spawn");
}

/// `rhei budget show` is the command the refusal names, so it has to work in
/// the state it is offered for. Today it fails before it can classify the
/// account: the journal lock is opened without `create`, so a project with no
/// account directory never reaches the code that would call it damaged.
// §FS-rhei-budgets.10
#[test]
fn show_reports_the_damaged_account_rather_than_failing_to_open_it() {
    let world = world("budget-path-reuse-show");
    let (uuid, receipts) = reuse_the_path(&world);

    let result = world.show();

    assert!(
        !result.status.success(),
        "a damaged account is reported and still exits non-zero:\n{}",
        said(&result)
    );
    let report = said(&result);
    assert!(
        report.contains("damaged") && report.contains("journal_absent"),
        "the report names the health and which of the three sub-cases it is; got:\n{report}"
    );
    assert!(
        report.contains(&world.witness(&uuid).display().to_string()),
        "the report names the witness path; got:\n{report}"
    );
    assert!(
        report.contains(&world.account_dir(&uuid).join("journal.jsonl").display().to_string()),
        "the report names the journal path it did not find; got:\n{report}"
    );
    assert!(
        report.contains(&format!("{receipts} receipt")),
        "the report says what the recorded history holds; got:\n{report}"
    );
    assert!(
        report.contains(&format!("rhei budget forget {}", world.canonical_project())),
        "the report gives the reused-path reading a runnable command; got:\n{report}"
    );
    assert!(
        report.contains(&restore_copy(&world, &uuid)),
        "the report gives the lost-journal reading a runnable command too, and does \
         not choose between them; got:\n{report}"
    );
    assert!(
        !result.status.success() && !world.account_dir(&uuid).exists(),
        "the report is read-only: it created no account directory"
    );
    assert!(
        !report.contains("inspect the project's account with: rhei budget show"),
        "the one command the operator already ran is not the next thing to try; \
         got:\n{report}"
    );

    assert_the_restore_actually_restores(&world, &report, &uuid);
}

/// The copy the report offers for this world's account, in the shell the
/// platform gives the operator: `cp` under a POSIX shell, `copy` under `cmd`,
/// each with its own quoting. A case that matched `"cp "` was asserting Unix.
/// §FS-rhei-budgets.10
fn restore_copy(world: &World, uuid: &str) -> String {
    rhei_core::platform::copy_command(
        &world.witness(uuid),
        &world.account_dir(uuid).join("journal.jsonl"),
    )
}

/// The offered restore, run through the platform's own shell as the operator
/// would run it.
///
/// Matching a copy command proves only that a line was printed. What has to hold
/// is that it *works* in the state it was offered for — where the account
/// directory does not exist at all, a bare copy fails with
/// `No such file or directory`, which is the ticket's own complaint about
/// `rhei budget show` in miniature. Every platform, not Unix only: the claim
/// §FS-rhei-budgets.10 makes is that the line is runnable on the platform it was
/// printed on, and a Windows arm that only inspected the string is what let a
/// `cp` / `mkdir -p` line — which `cmd` can run neither half of — go unnoticed.
///
/// The offered line is found as the one line naming **both** paths, which is
/// what no other line of the report does, rather than by a leading word that
/// differs per shell. §FS-rhei-budgets.10
fn assert_the_restore_actually_restores(world: &World, report: &str, uuid: &str) {
    let witness = world.witness(uuid).display().to_string();
    let journal = world.account_dir(uuid).join("journal.jsonl").display().to_string();
    let offered = report
        .lines()
        .map(str::trim)
        .find(|line| line.contains(&witness) && line.contains(&journal))
        .unwrap_or_else(|| panic!("the report offers a restore command; got:\n{report}"));

    let restored = world.shell(offered);

    assert!(
        restored.status.success(),
        "the command the report offers runs in the state it is offered for; `{offered}` said:\n{}",
        stderr(&restored)
    );
    let after = world.show();
    assert_success(&after);
    assert!(
        after.stdout.contains("[verified]"),
        "the restore the report offers is the whole remedy: the account verifies \
         after it; got:\n{}",
        after.stdout
    );
}

/// The ticket names a harness that reuses a temporary path, and a harness reads
/// JSON. Today `--format json` prints the rendered miette prose, so a machine
/// consumer gets neither a report nor a refusal it can parse.
// §FS-rhei-errors.5 §FS-rhei-budgets.10
#[test]
fn show_json_reports_the_damaged_account_as_a_machine_readable_error() {
    let world = world("budget-path-reuse-json");
    let (uuid, _) = reuse_the_path(&world);

    let result = world.show_json();

    assert!(!result.status.success(), "the damaged account still exits non-zero");
    assert!(
        stdout(&result).trim().is_empty(),
        "a refusal writes nothing to stdout, so a reader never parses two shapes; got:\n{}",
        stdout(&result)
    );
    let line = raw_stderr(&result);
    let lines: Vec<&str> = line.lines().filter(|line| !line.trim().is_empty()).collect();
    assert_eq!(lines.len(), 1, "the error object is one line on stderr; got:\n{line}");
    let parsed: serde_json::Value = serde_json::from_str(lines[0])
        .unwrap_or_else(|error| panic!("stderr should parse as JSON ({error}):\n{line}"));
    let error = &parsed["error"];
    assert!(error.is_object(), "the envelope is {{\"error\":{{…}}}}; got:\n{line}");
    assert!(error["message"].is_string(), "the object carries a message; got:\n{line}");
    assert_eq!(error["health"], "damaged", "health is the closed vocabulary's value:\n{line}");
    assert_eq!(error["damage"], "journal_absent", "the sub-case is named:\n{line}");
    assert_eq!(
        error["witness"],
        serde_json::json!(world.witness(&uuid)),
        "the witness path is a member rather than only prose:\n{line}"
    );
    // One key, one kind of value, across both states of one command: a harness
    // that read `account` here and on a verified report got a bare uuid from one
    // and a directory from the other. §FS-rhei-budgets.10
    assert_eq!(
        error["project_id"],
        format!("panta:{uuid}"),
        "the identity is spelled as a verified report spells it:\n{line}"
    );
    assert_eq!(
        error["account"],
        serde_json::json!(world.account_dir(&uuid)),
        "`account` is the account directory, absent or not, as it is when verified:\n{line}"
    );
}

/// Retirement keeps the receipts and retracts the identity.
///
/// Both halves matter. Unlinking the history would destroy the audit trail the
/// account was bounded by, and leaving the `roots.json` entry would hand the
/// next project at this path the retired project's uuid — which is the residual
/// the bare `rm -rf` of the witness directory leaves today.
// §FS-rhei-budgets.5.3
#[test]
fn forget_retires_the_root_keeping_the_history_and_dropping_the_identity() {
    let world = world("budget-path-reuse-forget");
    let (uuid, receipts) = reuse_the_path(&world);
    let history = fs::read(world.witness(&uuid)).expect("the witness history is readable");

    let result = world.forget("scratch directory reused");

    assert_success(&result);
    assert!(
        !world.authority().join(&uuid).exists(),
        "the live witness no longer claims this uuid, so nothing resolves the path to it"
    );
    let retired: Vec<PathBuf> = fs::read_dir(world.authority().join("retired"))
        .expect("the retired directory exists")
        .map(|entry| entry.expect("retired entry").path())
        .collect();
    assert_eq!(retired.len(), 1, "exactly one root was retired; got {retired:?}");
    let kept = retired[0].join("history.jsonl");
    assert_eq!(
        fs::read(&kept).expect("the retired history is readable"),
        history,
        "the receipts are kept byte for byte: a retirement is not a deletion"
    );
    assert!(
        retired[0]
            .file_name()
            .expect("the retired directory is named")
            .to_string_lossy()
            .starts_with(&uuid),
        "the retired directory names the uuid it holds; got {retired:?}"
    );

    let receipt: serde_json::Value = serde_json::from_slice(
        &fs::read(retired[0].join("retirement.json")).expect("the retirement receipt exists"),
    )
    .expect("the retirement receipt is JSON");
    for field in ["actor", "written_at", "reason", "argv"] {
        assert!(
            !receipt[field].is_null(),
            "the receipt carries the audit shape `adjust` already carries, missing \
             {field}; got:\n{receipt:#}"
        );
    }
    assert_eq!(receipt["reason"], "scratch directory reused", "the reason is recorded as given");
    assert_eq!(receipt["uuid"], uuid, "the receipt names the uuid it retired");
    assert_eq!(
        receipt["root"],
        serde_json::json!(world.canonical_project()),
        "the receipt names the root it was retired from"
    );
    assert!(
        !world.roots_index().contains(&uuid),
        "the witness index no longer maps this path to the retired uuid; got:\n{}",
        world.roots_index()
    );

    let after = world.run();

    assert_success(&after);
    let fresh = world.sole_witness_uuid();
    assert_ne!(fresh, uuid, "the next admission mints a new identity rather than reviving one");
    let report = said(&world.show());
    assert!(
        report.contains(&fresh) && report.contains("1 consumed"),
        "the new account starts at zero and carries only its own run; got:\n{report}"
    );
    assert!(
        !report.contains(&format!("{receipts} consumed")),
        "nothing of the retired project's spend carries over; got:\n{report}"
    );
}

/// The guarantee of §FS-rhei-budgets.5.3, and the reason `forget` is a recovery
/// command rather than a reset. Without this refusal the bound would hold only
/// until someone ran it.
// §FS-rhei-budgets.5.3
#[test]
fn forget_refuses_an_account_whose_journal_verifies_and_changes_nothing() {
    let world = world("budget-path-reuse-sound");
    world.lay();
    assert_success(&world.run());
    let uuid = world.sole_witness_uuid();
    let history = fs::read(world.witness(&uuid)).expect("the witness history is readable");
    let roots = world.roots_index();
    let before = world.show();
    assert_success(&before);

    let result = world.forget("freeing the path");

    assert!(
        !result.status.success(),
        "retiring a sound account would recreate capacity:\n{}",
        said(&result)
    );
    let refusal = said(&result);
    assert!(
        refusal.contains("verifies"),
        "the refusal says the account is sound, which is the whole reason; got:\n{refusal}"
    );
    assert!(
        refusal.contains("rhei budget adjust"),
        "it names the command that does change an allowance, so the two do not \
         compete for the case; got:\n{refusal}"
    );
    assert_eq!(
        fs::read(world.witness(&uuid)).expect("the witness history is readable"),
        history,
        "a refused retirement wrote nothing to the witness"
    );
    assert_eq!(world.roots_index(), roots, "a refused retirement left the witness index alone");
    assert!(!world.authority().join("retired").exists(), "a refused retirement retired nothing");
    let after = world.show();
    assert_success(&after);
    assert_eq!(
        after.stdout, before.stdout,
        "the reported identity and counts are exactly what they were"
    );
}

/// Where a journal is present, it **is** this project's journal: its tail was
/// lost, and copying the witness back restores this project's own history. So
/// the two sub-cases below keep today's remedy, must not be offered a
/// retirement that would discard a real account — and must not perform one when
/// `forget` is asked for it directly, which would leave the journal exactly as
/// unverifiable as it is now with nothing left to retire.
///
/// They are two cases rather than one loop so that a build can be half right
/// and be told which half: a loop stops at the first sub-case and says nothing
/// about the second.
fn assert_restore_is_the_only_remedy(prefix: &str, damage: &str, break_it: fn(&Path)) {
    let world = world(prefix);
    world.lay();
    assert_success(&world.run());
    let uuid = world.sole_witness_uuid();
    break_it(&world.account_dir(&uuid).join("journal.jsonl"));
    let history = fs::read(world.witness(&uuid)).expect("the witness history is readable");
    let roots = world.roots_index();

    let result = world.show();

    assert!(!result.status.success(), "a damaged account exits non-zero:\n{}", said(&result));
    let report = said(&result);
    assert!(
        report.contains("damaged") && report.contains(damage),
        "the report names this sub-case rather than damage in general; got:\n{report}"
    );
    assert!(
        report.contains(&restore_copy(&world, &uuid)),
        "restore from the witness is the remedy here, because the journal is this \
         project's; got:\n{report}"
    );
    assert!(
        !report.contains("rhei budget forget"),
        "retirement is not offered for a journal that exists: it would discard an \
         account that is genuinely this project's; got:\n{report}"
    );

    let refused = world.forget("asking for it anyway");

    assert!(
        !refused.status.success(),
        "asked directly, `forget` refuses a journal that is there:\n{}",
        said(&refused)
    );
    let refusal = said(&refused);
    assert!(
        refusal.contains(&restore_copy(&world, &uuid)),
        "the refusal names the restore, which is the remedy the sub-case has; got:\n{refusal}"
    );
    assert_eq!(
        fs::read(world.witness(&uuid)).expect("the witness history is readable"),
        history,
        "a refused retirement wrote nothing to the witness"
    );
    assert_eq!(world.roots_index(), roots, "a refused retirement left the witness index alone");
    assert!(!world.authority().join("retired").exists(), "a refused retirement retired nothing");
    assert_eq!(
        said(&world.show()),
        report,
        "the account is reported exactly as it was: the refusal changed nothing"
    );
}

/// A journal with no receipts at all, which is what an interrupted copy or a
/// truncating editor leaves.
// §FS-rhei-budgets.5.4
#[test]
fn a_truncated_journal_keeps_the_restore_remedy() {
    assert_restore_is_the_only_remedy(
        "budget-path-reuse-truncated",
        "journal_truncated",
        |journal| fs::write(journal, "").expect("truncate the journal"),
    );
}

/// A journal whose last `previous_hash` no longer matches the bytes before it —
/// the hand-edit the chain exists to catch, and the case that must keep its
/// refusal exactly as sharp as it is today.
// §FS-rhei-budgets.5.4
#[test]
fn a_chain_broken_journal_keeps_the_restore_remedy() {
    assert_restore_is_the_only_remedy("budget-path-reuse-broken", "chain_broken", |journal| {
        let text = fs::read_to_string(journal).expect("read the journal");
        let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
        let last = lines.pop().expect("the journal holds at least one receipt");
        let mut receipt: serde_json::Value =
            serde_json::from_str(&last).expect("the receipt is JSON");
        receipt["previous_hash"] = serde_json::json!("0".repeat(64));
        lines.push(receipt.to_string());
        fs::write(journal, format!("{}\n", lines.join("\n"))).expect("write the broken journal");
    });
}

/// The report change is confined to the damaged state.
///
/// This case is green today and has to stay green: it is the guard that says
/// the damaged-state report was added beside the sound one rather than in place
/// of it. A build that reshaped a verified account's lines or its JSON keys
/// would break every reader that already parses them, which is nobody's fix for
/// this ticket.
// §FS-rhei-budgets.10
#[test]
fn show_on_a_sound_account_reports_exactly_what_it_reports_today() {
    let world = world("budget-path-reuse-verified");
    world.lay();
    assert_success(&world.run());
    let uuid = world.sole_witness_uuid();

    let text = world.show();
    let json = CliRun::from(&world.show_json());

    assert_success(&text);
    // The target as typed, which is what this line has always printed; asserting
    // the resolved spelling asserted that the two agree, and on a runner whose
    // temporary directory is a link they do not. §FS-rhei-budgets.10
    assert!(
        text.stdout.contains(&format!("Project: {} (panta:{uuid})", world.project.display())),
        "the identity line is unchanged; got:\n{}",
        text.stdout
    );
    assert!(
        text.stdout
            .contains(&format!("Account: {} [verified]", world.account_dir(&uuid).display())),
        "a sound account still reports `verified` in the same line; got:\n{}",
        text.stdout
    );
    assert!(
        text.stdout.contains("project invocations: 1 consumed + 0 outstanding"),
        "the dimension lines are unchanged; got:\n{}",
        text.stdout
    );
    assert!(
        !text.stdout.contains("damage"),
        "there is no damage sub-case to report on an account that verifies; got:\n{}",
        text.stdout
    );

    assert_success(&json);
    let report: serde_json::Value =
        serde_json::from_str(&json.stdout).expect("the sound report is JSON on stdout");
    assert_eq!(report["health"], "verified", "health keeps its only previous value");
    assert!(
        report.get("damage").is_none() || report["damage"].is_null(),
        "`damage` is absent where health is `verified`; got:\n{report:#}"
    );
    assert_eq!(report["project_id"], format!("panta:{uuid}"), "the JSON keys are unchanged");
    assert_eq!(report["invocations"]["consumed"], 1, "the JSON counts are unchanged");
}
