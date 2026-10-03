//! Refusals a test arranges for this thread's accesses to the witness index,
//! in place of the other process's open handle that makes Windows refuse one,
//! so the waiting out is pinned on every platform.
//!
//! The same shape as the plan writers' `REPLACE_REFUSALS` seam in `rhei-cli`:
//! a count of attempts to refuse, the error each refused attempt reports, and a
//! count of every attempt made while the refusals were installed.
//! §AR-agent-orchestrator-workflow.3.3.1.1.1

use std::cell::RefCell;

/// Which access to the index a refusal stands in front of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum IndexAccess {
    /// Reading `roots.json` by its pathname.
    Read,
    /// Renaming the staged pending file over `roots.json`.
    Replace,
}

struct IndexRefusals {
    access: IndexAccess,
    /// Attempts of that access to let through before the first refusal.
    skip: usize,
    /// Attempts still to refuse after the skipped ones; `None` refuses all.
    remaining: Option<usize>,
    refusal: fn() -> std::io::Error,
    /// Every attempt of that access made while installed, refused or not.
    attempts: usize,
}

thread_local! {
    static INDEX_REFUSALS: RefCell<Option<IndexRefusals>> = const { RefCell::new(None) };
}

/// Let this thread's next `skip` attempts at `access` through, then refuse the
/// following `count` (`None`: every one after them) with `refusal`.
pub(super) fn refuse_index(
    access: IndexAccess,
    skip: usize,
    count: Option<usize>,
    refusal: fn() -> std::io::Error,
) {
    INDEX_REFUSALS.with(|installed| {
        *installed.borrow_mut() =
            Some(IndexRefusals { access, skip, remaining: count, refusal, attempts: 0 })
    });
}

/// Remove the arranged refusals and say how many attempts were made under them.
pub(super) fn index_attempts() -> usize {
    INDEX_REFUSALS.with(|installed| installed.borrow_mut().take().map_or(0, |r| r.attempts))
}

/// The refusal arranged for this attempt at `access`, if there is one.
pub(super) fn take_index_refusal(access: IndexAccess) -> Option<std::io::Error> {
    INDEX_REFUSALS.with(|installed| {
        let mut installed = installed.borrow_mut();
        let refusals = installed.as_mut().filter(|r| r.access == access)?;
        refusals.attempts += 1;
        if refusals.skip > 0 {
            refusals.skip -= 1;
            return None;
        }
        match refusals.remaining.as_mut() {
            Some(0) => None,
            Some(remaining) => {
                *remaining -= 1;
                Some((refusals.refusal)())
            }
            None => Some((refusals.refusal)()),
        }
    })
}
