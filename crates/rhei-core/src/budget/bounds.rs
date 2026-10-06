//! A bound, where its value came from, and what a halt says when it is spent.
//!
//! Resolution itself belongs to the settings chain in the CLI, which is the one
//! place every other setting resolves. What lives here is the *shape* the
//! answer takes — a value, a value source, and a limiting source — because that
//! shape is printed identically by validation, by the run, and by a halt, and
//! three copies of it would drift.
//! §FS-rhei-budgets.2.3 §FS-rhei-budgets.8

use std::path::PathBuf;

use super::types::{Contract, Dimension, Exhaustion, SpendMarks};
use crate::money;

/// The settings key a machine delegates the count ceilings with, as every
/// row that reports a delegation names it. §FS-rhei-budgets.2.1
pub const CLAMP_PROJECTS: &str = "defaults.clamp_projects";

/// Who set the requested value. `plan` covers anything the plan's own state
/// machine declares, because from the operator's side the plan is what asked.
/// §FS-rhei-budgets.2.3
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundSource {
    BuiltIn,
    Machine,
    Project,
    Plan,
}

impl BoundSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BuiltIn => "built_in",
            Self::Machine => "machine",
            Self::Project => "project",
            Self::Plan => "plan",
        }
    }
}

/// What a bound's number *is*, which is what decides how every surface writes
/// it: a count is itself, an amount of money is micro-units of the account's
/// currency. §FS-rhei-budgets.1 §FS-rhei-budgets.2.1
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BoundUnit {
    #[default]
    Count,
    Money,
}

impl BoundUnit {
    /// The number on its own, with no currency: what a settings key takes and
    /// therefore what `rhei validate` and `rhei budget show` report.
    /// §FS-rhei-budgets.2.1
    fn bare(self, value: u64) -> String {
        match self {
            Self::Count => value.to_string(),
            Self::Money => money::format_bare(value),
        }
    }

    /// The same number as an amount, which is how a halt writes it.
    /// §FS-rhei-budgets.8
    fn amount(self, value: u64, currency: Option<&str>) -> String {
        match self {
            Self::Count => value.to_string(),
            Self::Money => money::format_micro(value, currency),
        }
    }
}

/// Whose number a bound is clamped to: the machine's by default, or the
/// project's where the machine delegated that dimension's ceiling to it.
/// §FS-rhei-budgets.2 §FS-rhei-budgets.2.3
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Limiter {
    #[default]
    Machine,
    Project,
}

impl Limiter {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Machine => "machine",
            Self::Project => "project",
        }
    }
}

/// The two settings files a bound's sentences can send a reader to, by the
/// paths rhei actually read: the machine file as the loader opens it, and the
/// project file the loader chose between the current and the deprecated home.
/// `None` where no path is known, which falls back to naming the file by role.
/// §FS-rhei-budgets.8
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SettingsFiles {
    pub machine: Option<PathBuf>,
    pub project: Option<PathBuf>,
}

impl SettingsFiles {
    /// The file a settings remedy names: its path where known, its role
    /// otherwise. §FS-rhei-budgets.8
    pub fn name(&self, holder: Limiter) -> String {
        let path = match holder {
            Limiter::Machine => &self.machine,
            Limiter::Project => &self.project,
        };
        match path {
            Some(path) => path.display().to_string(),
            None => format!("the {} settings file", holder.as_str()),
        }
    }
}

/// One bound in force: what it is, who asked for it, and whose ceiling
/// lowered it.
///
/// The two sources are kept apart because they answer different questions and
/// send a reader to different files: "the machine set this" and "the machine
/// lowered this" are not the same sentence. §FS-rhei-budgets.2.3
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bound {
    pub key: &'static str,
    pub effective: u64,
    pub source: BoundSource,
    /// The higher value an inner tier asked for, present only when the active
    /// ceiling clamped it. §FS-rhei-budgets.2
    pub requested: Option<u64>,
    /// Whether this number is a count or an amount of money.
    /// §FS-rhei-budgets.2.1
    pub unit: BoundUnit,
    /// The active ceiling's value: the machine's or the built-in, or the
    /// project's where the machine delegated it. §FS-rhei-budgets.2
    pub ceiling: u64,
    /// Who holds the active ceiling, and therefore who a clamp is reported as
    /// limited by and which file a remedy names. §FS-rhei-budgets.2.3
    pub limiter: Limiter,
    /// The files the phrases, rows and remedy name. §FS-rhei-budgets.8
    pub files: SettingsFiles,
}

impl Bound {
    /// Resolve one dimension: most specific request wins the value, and the
    /// machine's number is then applied as a ceiling.
    ///
    /// `machine` is the machine-global settings value; where it configures
    /// none, the built-in default *is* the machine value and therefore the
    /// ceiling, so there is no unconfigured state in which a count dimension is
    /// unbounded. §FS-rhei-budgets.2
    pub fn resolve(
        key: &'static str,
        built_in: u64,
        machine: Option<u64>,
        requested: Option<(u64, BoundSource)>,
    ) -> Self {
        Self::resolve_in(BoundUnit::Count, key, built_in, machine, requested)
    }

    /// The same resolution for a dimension whose unit is money. The clamp is
    /// the clamp: an amount is compared with an amount exactly as a count is
    /// compared with a count. §FS-rhei-budgets.2
    pub fn resolve_money(
        key: &'static str,
        built_in: u64,
        machine: Option<u64>,
        requested: Option<(u64, BoundSource)>,
    ) -> Self {
        Self::resolve_in(BoundUnit::Money, key, built_in, machine, requested)
    }

    /// Resolve a count dimension whose ceiling the machine delegated to the
    /// project: the project's declared value takes the machine's place as the
    /// ceiling, whether it is higher or lower, and a plan or profile may lower
    /// it but never raise it. Only the caller knows the delegation applies —
    /// the two count keys, the machine's switch, a project declaration — so it
    /// passes the project's number here rather than the machine's.
    /// §FS-rhei-budgets.2 §FS-rhei-budgets.2.2
    pub fn resolve_delegated(
        key: &'static str,
        project: u64,
        requested: Option<(u64, BoundSource)>,
    ) -> Self {
        let fallback = (project, BoundSource::Project);
        Self::clamp(BoundUnit::Count, key, project, Limiter::Project, fallback, requested)
    }

    /// The same bound, naming the files its phrases and remedy send a reader
    /// to by the paths rhei read. §FS-rhei-budgets.8
    pub fn with_files(mut self, files: SettingsFiles) -> Self {
        self.files = files;
        self
    }

    fn resolve_in(
        unit: BoundUnit,
        key: &'static str,
        built_in: u64,
        machine: Option<u64>,
        requested: Option<(u64, BoundSource)>,
    ) -> Self {
        let fallback = match machine {
            Some(value) => (value, BoundSource::Machine),
            None => (built_in, BoundSource::BuiltIn),
        };
        let ceiling = machine.unwrap_or(built_in);
        Self::clamp(unit, key, ceiling, Limiter::Machine, fallback, requested)
    }

    /// `effective bound = min(resolved value, active ceiling)`, whoever holds
    /// the ceiling. §FS-rhei-budgets.2
    fn clamp(
        unit: BoundUnit,
        key: &'static str,
        ceiling: u64,
        limiter: Limiter,
        fallback: (u64, BoundSource),
        requested: Option<(u64, BoundSource)>,
    ) -> Self {
        let (value, source) = requested.unwrap_or(fallback);
        let (effective, requested) =
            if value > ceiling { (ceiling, Some(value)) } else { (value, None) };
        let files = SettingsFiles::default();
        Self { key, effective, source, requested, unit, ceiling, limiter, files }
    }

    /// Whether the active ceiling, rather than the requester, is what decides
    /// this value — and therefore what a remedy has to name. A delegated
    /// project value is its own ceiling, so it limits when it is the source.
    /// §FS-rhei-budgets.8
    pub fn limited_by_ceiling(&self) -> bool {
        self.requested.is_some()
            || match self.limiter {
                Limiter::Machine => {
                    matches!(self.source, BoundSource::Machine | BoundSource::BuiltIn)
                }
                Limiter::Project => self.source == BoundSource::Project,
            }
    }

    /// Whether the machine handed this dimension's ceiling to the project.
    /// §FS-rhei-budgets.2
    pub fn delegated(&self) -> bool {
        self.limiter == Limiter::Project
    }

    /// `80 (built_in)`, or the clamped form naming requester and limiter.
    /// §FS-rhei-budgets.2.3
    pub fn value_phrase(&self) -> String {
        self.phrase(|value| self.unit.bare(value))
    }

    /// The same line written as an amount in the account's currency, which is
    /// how a halt reports a money bound: `$25.00 (machine)`.
    /// §FS-rhei-budgets.8
    pub fn amount_phrase(&self, currency: Option<&str>) -> String {
        self.phrase(|value| self.unit.amount(value, currency))
    }

    fn phrase(&self, write: impl Fn(u64) -> String) -> String {
        match self.requested {
            Some(asked) => format!(
                "{} (requested {} by the {}, limited by {} settings)",
                write(self.effective),
                write(asked),
                self.source.as_str(),
                self.limiter.as_str()
            ),
            None => format!("{} ({})", write(self.effective), self.source.as_str()),
        }
    }

    /// The one line every surface that reports a bound prints.
    /// §FS-rhei-budgets.2.3 §FS-rhei-validate.4
    pub fn report_line(&self) -> String {
        format!("{}: {}", self.key, self.value_phrase())
    }

    /// `transition_limit ceiling: 40 (project, <P>)`, present only where the
    /// machine delegated this dimension's ceiling. §FS-rhei-budgets.2.3
    pub fn ceiling_line(&self) -> Option<String> {
        self.delegated().then(|| {
            format!(
                "{} ceiling: {} (project, {})",
                self.key,
                self.unit.bare(self.ceiling),
                self.files.name(Limiter::Project)
            )
        })
    }

    /// The halt's `ceiling:` value, naming the project file that holds it and
    /// the machine file that delegated it; absent where nothing was delegated.
    /// §FS-rhei-budgets.8
    pub fn ceiling_halt_phrase(&self, currency: Option<&str>) -> Option<String> {
        self.delegated().then(|| {
            format!(
                "{} (project, {}; delegated by {CLAMP_PROJECTS}=false in {})",
                self.unit.amount(self.ceiling, currency),
                self.files.name(Limiter::Project),
                self.files.name(Limiter::Machine)
            )
        })
    }

    /// The single remedy: the one thing that raises the limiter that actually
    /// stopped the work, naming the settings file by the path rhei read. It
    /// never offers an inner value the ceiling would clamp, because sending an
    /// operator to a field that cannot take effect sends them to the wrong
    /// file. §FS-rhei-budgets.8
    pub fn remedy(&self) -> String {
        let holder = if self.limited_by_ceiling() {
            self.limiter
        } else if self.source == BoundSource::Project {
            Limiter::Project
        } else {
            return format!("set `{}` on the node's profile in the state machine", self.key);
        };
        format!("set `defaults.{}` in {}", self.key, self.files.name(holder))
    }
}

/// `count ceiling policy: delegated <keys> by defaults.clamp_projects=false in
/// <M>`, listing only the keys actually delegated; absent where none was.
/// §FS-rhei-budgets.2.3
pub fn ceiling_policy_line<'a>(bounds: impl IntoIterator<Item = &'a Bound>) -> Option<String> {
    let delegated: Vec<&Bound> = bounds.into_iter().filter(|bound| bound.delegated()).collect();
    let first = delegated.first()?;
    let keys: Vec<&str> = delegated.iter().map(|bound| bound.key).collect();
    Some(format!(
        "count ceiling policy: delegated {} by {CLAMP_PROJECTS}=false in {}",
        keys.join(", "),
        first.files.name(Limiter::Machine)
    ))
}

/// The one thing a halt tells an operator to do, chosen by which limiter
/// actually stopped the work rather than by which number is smallest.
/// §FS-rhei-budgets.8
pub enum Remedy {
    /// Raise the bound, wherever it was set.
    Raise,
    /// Wait: the window renews on its own at this instant, and naming a
    /// settings key for a wait would be the wrong instruction even though it
    /// would work.
    Renews(String),
    /// An explicit lifetime allowance is what limits, so the audited command
    /// is the remedy rather than any settings key. §FS-rhei-budgets.10
    Adjust,
}

/// Every label in a halt is padded to this column, so one halt reads the same
/// on every surface. §FS-rhei-budgets.8
const LABEL: usize = 13;

/// One continuation row of a refusal, indented under the `error: ` headline and
/// aligned on its values. Shared with the identity refusals, which print this
/// shape without the six-row bound report. §FS-rhei-budgets.8
pub(super) fn row(label: &str, value: &str) -> String {
    format!("       {label:<LABEL$}{value}")
}

/// The halt, in the fixed shape of §FS-rhei-budgets.8: what stopped, the
/// dimension, the bound with its provenance, the numbers, the accounting mode,
/// and exactly one remedy.
///
/// `marks` is what `estimated:` reports and is the one optional row: a fully
/// measured halt has exactly the six rows above it, so a count halt passes
/// [`SpendMarks::default`] and prints what it always printed.
/// §FS-rhei-budgets.8
pub fn halt_text(spent: &Exhaustion, bound: &Bound, remedy: &Remedy, marks: &SpendMarks) -> String {
    let currency = spent.currency.as_deref();
    // Money is written with the account's currency, a count with nothing at
    // all, and the same call decides both. §FS-rhei-budgets.8
    let amount = |value: u64| bound.unit.amount(value, currency);
    let headline = match spent.dimension {
        Dimension::Travel => {
            format!("error: ticket '{}' has spent its travel bound", spent.subject)
        }
        Dimension::Invocations => match spent.contract {
            Contract::Window => {
                format!("error: project '{}' has spent today's invocation capacity", spent.subject)
            }
            Contract::Lifetime { .. } => {
                format!("error: project '{}' has spent its invocation allowance", spent.subject)
            }
        },
        // Spend is bounded by a window whichever contract the account holds,
        // so it has one headline rather than two. §FS-rhei-budgets.3.4
        Dimension::Spend => {
            format!("error: project '{}' has spent today's measured budget", spent.subject)
        }
    };
    let mode = match spent.dimension {
        Dimension::Travel => "per ticket identity".to_string(),
        Dimension::Invocations => match spent.contract {
            Contract::Window => format!("window ({}Z)", spent.day),
            Contract::Lifetime { .. } => "lifetime".to_string(),
        },
        Dimension::Spend => format!("window ({}Z)", spent.day),
    };
    let mut lines = vec![
        headline,
        row("dimension:", spent.dimension.label()),
        row("bound:", &bound.amount_phrase(currency)),
    ];
    // Directly after `bound:`, and only where the machine delegated this
    // dimension's ceiling to the project. §FS-rhei-budgets.8
    if let Some(ceiling) = bound.ceiling_halt_phrase(currency) {
        lines.push(row("ceiling:", &ceiling));
    }
    lines.push(row(
        "consumed:",
        &format!(
            "{}  outstanding: {}  remaining: {}",
            amount(spent.counter.consumed),
            amount(spent.counter.reserved),
            amount(spent.counter.remaining(spent.bound).unwrap_or(0))
        ),
    ));
    // Between `consumed:` and `mode:`, because it qualifies the numbers above
    // it rather than the contract below. §FS-rhei-budgets.8
    if let Some(estimated) = marks.row(super::built_in::SPEND_RESERVE, currency) {
        lines.push(row("estimated:", &estimated));
    }
    lines.push(row("mode:", &mode));
    lines.push(match remedy {
        Remedy::Renews(instant) => row("renews at:", instant),
        Remedy::Adjust => row(
            "to raise it:",
            "run `rhei budget adjust <TARGET> --invocations <N> --reason <TEXT>`",
        ),
        Remedy::Raise => row("to raise it:", &bound.remedy()),
    });
    lines.join("\n")
}
