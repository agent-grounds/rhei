//! One serialized account beneath every neural start.
//!
//! The scheduler does not construct a neural subprocess directly: it submits a
//! resolved launch here and gets back an owned reservation or a typed refusal.
//! That one boundary sits beneath every scheduler, retry, poll attempt, fanout
//! arm, and nested run, which is what makes the counts true of *every* start
//! rather than of the ones someone remembered to route through it.
//!
//! This component counts, and it reads one amount it did not produce. It
//! still does not price, broker, confine, or qualify anything: pricing belongs
//! to cost accounting §FS-rhei-cost-accounting, and the rest to the
//! provider-spend obligation on `agent-grounds/rhei#107`. The reason is
//! unchanged for the counts — importing that evidence here would make a count
//! depend on evidence a count does not need — and it is why the spend
//! dimension consumes a record written elsewhere rather than measuring
//! anything of its own.
//! §AR-neural-admission §REQ-bounded-neural-work

mod account;
mod admission;
mod authority;
mod bounds;
mod events;
mod identity;
mod journal;
mod replay;
mod types;
mod window;

#[cfg(test)]
mod adjust_tests;
#[cfg(test)]
mod ancestry_tests;
#[cfg(test)]
mod ledger_tests;
#[cfg(test)]
mod nonstart_tests;
#[cfg(test)]
mod spend_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod travel_tests;
#[cfg(test)]
mod window_tests;

pub use account::{Account, ACCOUNT_DIR};
pub use admission::{AdmissionRequest, AppliedEdge, Arm, EffectiveBounds, ReservationGroup};
pub use bounds::{halt_text, Bound, BoundSource, BoundUnit, Remedy};
pub use events::{BudgetEvent, BudgetLine};
pub use journal::{Audit, Journal, Receipt};
pub use types::{
    BudgetError, Contract, Counter, Dimension, Exhaustion, Snapshot, SpendBasis, SpendMarks,
};
pub use window::{day_key, instant, now, renewal_instant, CLOCK_ENV};

pub(crate) type Result<T> = std::result::Result<T, BudgetError>;

/// The built-in default for each dimension, and the worst case a spawn
/// reserves against the day.
///
/// These are measured rather than chosen: the definition of a healthy history,
/// the observed maxima, the multiplier, and what the evidence could not see are
/// recorded in §REQ-bounded-neural-work.7 so that a re-measurement can supersede
/// them honestly rather than argue with a number nobody can source. The two
/// amounts are measured over their own corpus under a restated definition of
/// healthy, which §REQ-bounded-neural-work.7.5 states separately for exactly
/// that reason.
/// §FS-rhei-budgets.2.1
pub mod built_in {
    /// Applied transitions one ticket may make over the life of its identity.
    /// The largest healthy travel ever observed in the source corpus was 17,
    /// and the largest of any history 20; this is that times four.
    pub const TRANSITION_LIMIT: u64 = 80;

    /// Neural starts a project may be admitted per UTC day. The busiest real
    /// day observed was 51 healthy, 58 of any kind; this is that times four.
    pub const INVOCATIONS_PER_DAY: u64 = 200;

    /// The ceiling on an explicit lifetime allowance: thirty days at the
    /// built-in rate. Nothing consumes it and no project receives it.
    pub const INVOCATION_LIFETIME_MAX: u64 = 6000;

    /// Measured spend a project may be charged per UTC day, in micro-units of
    /// its account's currency. The busiest project-day in the accounting
    /// archive was $96.37 over eight healthy days whose median was $10.94;
    /// this is that maximum times four. §REQ-bounded-neural-work.7.5
    pub const SPEND_PER_DAY: u64 = 400 * crate::money::MICRO;

    /// The worst case every neural start reserves against the day, released
    /// by the settle that replaces it with what the request actually cost.
    ///
    /// Flat, because no agent profile declares a token or a context limit to
    /// derive a per-model estimate from, and carrying no margin of its own:
    /// it is the largest single invocation the archive holds, $17.71, rounded
    /// up. The margin is the ceiling's, and multiplying here would charge it
    /// twice — hardest against the class that could not be measured at all.
    /// §REQ-bounded-neural-work.7.5 §FS-rhei-budgets.6.2
    pub const SPEND_RESERVE: u64 = 20 * crate::money::MICRO;
}
