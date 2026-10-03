//! The witness index waits out a refusal of the kind another process's open
//! handle causes, on both its read and its replacement, and reports only one
//! that outlasts the bound.
//!
//! Each case establishes a second project beside the [`Case`]'s own, because
//! establishing an absent account is what reads the index twice (`locate`, then
//! `record_root`) and replaces it once (`write_roots`) — the three accesses
//! `agent-grounds/rhei#401` showed a single refusal of halting a run.
//! §AR-agent-orchestrator-workflow.3.3.1.1.1

use super::index_refusals::{index_attempts, refuse_index, IndexAccess};
use super::test_support::*;
use super::*;
use std::path::Path;

/// What Windows's os error 5 and Unix's EACCES both read as.
fn permission_denied() -> std::io::Error {
    std::io::Error::from(std::io::ErrorKind::PermissionDenied)
}

/// A failure that says nothing about another handle.
fn not_a_refusal() -> std::io::Error {
    std::io::Error::other("the disk is on fire")
}

/// Establish a fresh project's account with `access` refused as arranged, and
/// say what came of it and how many attempts at `access` were made.
fn establish_refused(
    access: IndexAccess,
    skip: usize,
    count: Option<usize>,
    refusal: fn() -> std::io::Error,
) -> (tempfile::TempDir, super::Result<Account>, usize) {
    let project = tempfile::tempdir().expect("second project root");
    refuse_index(access, skip, count, refusal);
    let established = Account::establish(project.path(), &audit()).map(|(account, _)| account);
    (project, established, index_attempts())
}

/// `skip` is the attempts let through before the one refusal, so a retry is
/// any attempt past `skip + 1`.
fn assert_established_and_witnessed(
    established: super::Result<Account>,
    attempts: usize,
    skip: usize,
) {
    let account = established.unwrap_or_else(|refused| {
        panic!(
            "one refusal is waited out, not reported ({attempts} attempt(s)): {}",
            refused.message
        )
    });
    assert!(attempts > skip + 1, "the refused access was tried again: {attempts} attempt(s)");
    assert_eq!(
        witnessed_root(account.uuid()).expect("read the index").as_deref(),
        Some(account.root()),
        "the index records the root the account was established for"
    );
}

/// The first read, `Account::locate`'s, refused once.
// §AR-agent-orchestrator-workflow.3.3.1.1.1
#[test]
fn one_refused_read_of_the_index_by_locate_is_waited_out() {
    let _case = Case::new();
    let (_project, established, attempts) =
        establish_refused(IndexAccess::Read, 0, Some(1), permission_denied);
    assert_established_and_witnessed(established, attempts, 0);
}

/// The second read, `record_root`'s, refused once.
// §AR-agent-orchestrator-workflow.3.3.1.1.1
#[test]
fn one_refused_read_of_the_index_by_record_root_is_waited_out() {
    let _case = Case::new();
    let (_project, established, attempts) =
        establish_refused(IndexAccess::Read, 1, Some(1), permission_denied);
    assert_established_and_witnessed(established, attempts, 1);
}

/// The rename of the pending file over the index refused once.
// §AR-agent-orchestrator-workflow.3.3.1.1.1
#[test]
fn one_refused_replacement_of_the_index_is_waited_out() {
    let _case = Case::new();
    let (_project, established, attempts) =
        establish_refused(IndexAccess::Replace, 0, Some(1), permission_denied);
    assert_established_and_witnessed(established, attempts, 0);
}

/// Windows's sharing violation reads as no `ErrorKind` of its own, so it is
/// classified by its raw code there. §AR-agent-orchestrator-workflow.3.3.1.1
#[cfg(windows)]
#[test]
fn one_sharing_violation_on_the_index_replacement_is_waited_out() {
    fn sharing_violation() -> std::io::Error {
        std::io::Error::from_raw_os_error(32)
    }
    let _case = Case::new();
    let (_project, established, attempts) =
        establish_refused(IndexAccess::Replace, 0, Some(1), sharing_violation);
    assert_established_and_witnessed(established, attempts, 0);
}

/// A read refused for longer than the bound is reported as it always was, but
/// only after the bound: more than one attempt was made first.
// §AR-agent-orchestrator-workflow.3.3.1.1.1
#[test]
fn a_read_refused_past_the_bound_is_unreachable_after_retrying() {
    let _case = Case::new();
    let (_project, established, attempts) =
        establish_refused(IndexAccess::Read, 0, None, permission_denied);
    let refused = established.expect_err("a refusal that never ends is reported");
    assert_eq!(refused.reason_code, "unreachable_budget_path");
    assert!(refused.message.contains("roots.json"), "it names the index: {}", refused.message);
    assert!(attempts >= 2, "the refusal was waited out before it was reported: {attempts}");
}

/// A replacement refused for longer than the bound is reported the same way,
/// with the index's previous bytes intact.
// §AR-agent-orchestrator-workflow.3.3.1.1.1
#[test]
fn a_replacement_refused_past_the_bound_is_unreachable_and_keeps_the_index() {
    let case = Case::new();
    let witness = case.witness_path();
    let authority = witness.parent().and_then(Path::parent).expect("the authority directory");
    let index = authority.join("roots.json");
    let before = std::fs::read(&index).expect("the case's own account wrote the index");
    let (_project, established, attempts) =
        establish_refused(IndexAccess::Replace, 0, None, permission_denied);
    let refused = established.expect_err("a refusal that never ends is reported");
    assert_eq!(refused.reason_code, "unreachable_budget_path");
    assert!(refused.message.contains("roots.json"), "it names the index: {}", refused.message);
    assert_eq!(std::fs::read(&index).expect("index"), before, "the index is untouched");
    assert!(attempts >= 2, "the refusal was waited out before it was reported: {attempts}");
}

/// Any other failure is reported at once, after a single attempt.
// §AR-agent-orchestrator-workflow.3.3.1.1.1
#[test]
fn a_read_failing_for_another_reason_is_reported_at_once() {
    let _case = Case::new();
    let (_project, established, attempts) =
        establish_refused(IndexAccess::Read, 0, Some(1), not_a_refusal);
    let refused = established.expect_err("a failure that is not a refusal is reported");
    assert_eq!(refused.reason_code, "unreachable_budget_path");
    assert_eq!(attempts, 1, "a failure that is not a refusal is not retried");
}
