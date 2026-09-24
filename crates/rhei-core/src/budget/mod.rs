//! One serialized account beneath every neural start.
//!
//! The scheduler does not construct a neural subprocess directly: it submits a
//! resolved launch here and gets back an owned reservation or a typed refusal.
//! That one boundary sits beneath every scheduler, retry, poll attempt, fanout
//! arm, and nested run, which is what makes the counts true of *every* start
//! rather than of the ones someone remembered to route through it.
//!
//! This component counts. It does not price, broker, confine, or qualify
//! anything: those belong to the provider-spend obligation on
//! `agent-grounds/rhei#107`, and importing them here would make a count depend
//! on evidence a count does not need.
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

pub use account::{Account, ACCOUNT_DIR};
pub use admission::{AdmissionRequest, Arm, EffectiveBounds, ReservationGroup};
pub use bounds::{halt_text, Bound, BoundSource, Remedy};
pub use events::{BudgetEvent, BudgetLine};
pub use journal::{Audit, Journal, Receipt};
pub use types::{BudgetError, Contract, Counter, Dimension, Exhaustion, Snapshot};
pub use window::{day_key, instant, now, renewal_instant, CLOCK_ENV};

pub(crate) type Result<T> = std::result::Result<T, BudgetError>;

/// The built-in default for each count dimension.
///
/// These are measured rather than chosen: the definition of a healthy history,
/// the observed maxima, the multiplier, and what the evidence could not see are
/// recorded in §REQ-bounded-neural-work.7 so that a re-measurement can supersede
/// them honestly rather than argue with a number nobody can source.
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
}
