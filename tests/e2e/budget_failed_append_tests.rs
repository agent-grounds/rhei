//! A charge whose append fails partway.
//!
//! A write cut short reaches the file before its error comes back, so today the
//! failed append leaves part of a receipt in the witness, or a whole receipt in
//! the witness that the journal never got, and every later edge on the project
//! is refused (`agent-grounds/rhei#536`, the reproducer's part A). A failed
//! append leaves both files as they were before the edge, and the same edge
//! goes through once the cause is gone, with nothing to repair
//! ([§FS-rhei-budgets.5.4.3](../../docs/functional-spec/rhei-budgets.spec.md)).

// §FS-rhei-budgets.5.4 §FS-rhei-budgets.5.4.3

use std::fs;

use super::budget_torn_witness_support::*;
use super::*;

/// The witness's write is cut short the way a full disk cuts it: one
/// `rhei transition` under `RLIMIT_FSIZE`, 60 bytes past the witness, with
/// `SIGXFSZ` ignored, so the kernel writes what fits and then fails the write.
/// The limit is set in bytes, so it sits inside the next receipt without
/// charging edges up to a 4096-byte block as the shell reproducer did.
// §FS-rhei-budgets.5.4.3
// A file-size limit and SIGXFSZ are POSIX; Windows has no per-process limit on
// the size of a file, and the failed journal append below is the portable form.
#[cfg(unix)]
#[test]
#[ignore = "red until #536: a witness write cut short leaves a fragment in the witness; implement removes this"]
fn a_witness_write_cut_short_leaves_both_files_as_they_were() {
    use nix::sys::resource::{setrlimit, Resource};
    use nix::sys::signal::{signal, SigHandler, Signal};
    use std::os::unix::process::CommandExt;

    let world = world("budget-append-fsize");
    let (witness, journal, plan) =
        (read(&world.witness()), read(&world.journal()), read(&world.plan()));
    assert!(witness == journal, "the account verifies before the tear");
    let limit = witness.len() as u64 + 60;
    let mut limited = world.edge_command("work", "review");
    // SAFETY: both calls are single system calls, safe between fork and exec.
    unsafe {
        limited.pre_exec(move || {
            signal(Signal::SIGXFSZ, SigHandler::SigIgn).map_err(std::io::Error::from)?;
            setrlimit(Resource::RLIMIT_FSIZE, limit, limit).map_err(std::io::Error::from)
        });
    }

    let failed = CliRun::from(&limited.output().expect("rhei transition should run"));

    assert!(!failed.status.success(), "the edge fails under the limit:\n{}", said(&failed));
    assert!(said(&failed).contains("File too large"), "the limit bit:\n{}", said(&failed));
    let after = read(&world.witness());
    assert!(
        after == witness,
        "the witness is as it was: {} bytes before, {} after",
        witness.len(),
        after.len()
    );
    assert!(read(&world.journal()) == journal, "the journal is as it was");
    assert!(read(&world.plan()) == plan, "no edge applied: the task is still at work");
    assert_success(&world.edge("work", "review"));
}

/// The witness's append succeeds and the journal's fails: a journal that
/// cannot be opened for writing. Both files are as they were before the edge.
// §FS-rhei-budgets.5.4.3
#[test]
#[ignore = "red until #536: a failed journal append leaves the witness a receipt ahead; implement removes this"]
fn a_failed_journal_append_leaves_both_files_as_they_were() {
    let world = world("budget-append-journal");
    let journal_path = world.journal();
    let (witness, journal, plan) =
        (read(&world.witness()), read(&journal_path), read(&world.plan()));
    let writable = fs::metadata(&journal_path).expect("journal metadata").permissions();
    let mut sealed = writable.clone();
    sealed.set_readonly(true);
    fs::set_permissions(&journal_path, sealed).expect("make the journal read-only");
    if fs::OpenOptions::new().append(true).open(&journal_path).is_ok() {
        fs::set_permissions(&journal_path, writable).expect("give the journal back");
        panic!(
            "{} is still writable while read-only, so this test cannot stage the ticket's case; \
             do not run the suite as root",
            journal_path.display()
        );
    }

    let failed = world.edge("work", "review");
    fs::set_permissions(&journal_path, writable).expect("give the journal back");

    assert!(!failed.status.success(), "the edge fails:\n{}", said(&failed));
    let after = read(&world.witness());
    assert!(
        after == witness,
        "the witness is cut back: {} bytes before, {} after",
        witness.len(),
        after.len()
    );
    assert!(read(&journal_path) == journal, "the journal is as it was");
    assert!(read(&world.plan()) == plan, "no edge applied: the task is still at work");
    assert_success(&world.edge("work", "review"));
}

/// The account's first receipt, whose journal the append creates: where that
/// fails, the account is **absent** again — no journal and no witness — and the
/// next charge establishes it. The account directory is the one a refused
/// establish leaves, holding only the lock file.
// §FS-rhei-budgets.5.4 §FS-rhei-budgets.5.4.3
// A directory that refuses a new file is a Unix mode: Windows's read-only attribute on a
// directory does not stop one being created, and the first journal is not there to seal.
#[cfg(unix)]
#[test]
#[ignore = "red until #536: a failed first journal append leaves a witness and no journal; implement removes this"]
fn a_failed_first_journal_append_leaves_the_account_absent() {
    use std::os::unix::fs::PermissionsExt;

    let world = bare("budget-append-first");
    let uuid = uuid::Uuid::new_v4().to_string();
    let account = world.account_dir(&uuid);
    fs::create_dir_all(&account).expect("the account directory");
    fs::write(account.join("journal.jsonl.lock"), b"").expect("its lock file");
    fs::set_permissions(&account, fs::Permissions::from_mode(0o555)).expect("seal it");
    if fs::write(account.join("probe"), b"probe").is_ok() {
        let _ = fs::remove_file(account.join("probe"));
        fs::set_permissions(&account, fs::Permissions::from_mode(0o755)).expect("unseal it");
        panic!(
            "{} is still writable at mode 0555, so this test cannot stage the ticket's case; \
             do not run the suite as root",
            account.display()
        );
    }

    let failed = world.edge("work", "review");
    fs::set_permissions(&account, fs::Permissions::from_mode(0o755)).expect("unseal it");

    assert!(!failed.status.success(), "the first charge fails:\n{}", said(&failed));
    assert!(!account.join("journal.jsonl").exists(), "no journal was left");
    let witness = world.authority().join(&uuid).join("history.jsonl");
    assert!(
        !witness.exists(),
        "the witness is absent again; it holds {:?}",
        fs::read_to_string(&witness).unwrap_or_default()
    );
    assert!(
        fs::read_to_string(world.plan()).expect("read the plan").contains("**State:** work"),
        "no edge applied"
    );
    assert_success(&world.edge("work", "review"));
}
