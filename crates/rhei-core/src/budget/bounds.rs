//! A bound, where its value came from, and what a halt says when it is spent.
//!
//! Resolution itself belongs to the settings chain in the CLI, which is the one
//! place every other setting resolves. What lives here is the *shape* the
//! answer takes — a value, a value source, and a limiting source — because that
//! shape is printed identically by validation, by the run, and by a halt, and
//! three copies of it would drift.
//! §FS-rhei-budgets.2.3 §FS-rhei-budgets.8

use super::types::{Contract, Dimension, Exhaustion};

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

/// One bound in force: what it is, who asked for it, and whether the machine
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
    /// The higher value an inner tier asked for, present only when the machine
    /// clamped it. §FS-rhei-budgets.2
    pub requested: Option<u64>,
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
        let ceiling = machine.unwrap_or(built_in);
        let (value, source) = requested.unwrap_or(match machine {
            Some(value) => (value, BoundSource::Machine),
            None => (built_in, BoundSource::BuiltIn),
        });
        if value > ceiling {
            return Self { key, effective: ceiling, source, requested: Some(value) };
        }
        Self { key, effective: value, source, requested: None }
    }

    /// Whether the machine's ceiling, rather than the requester, is what
    /// decides this value — and therefore what a remedy has to name.
    /// §FS-rhei-budgets.8
    pub fn limited_by_machine(&self) -> bool {
        self.requested.is_some()
            || matches!(self.source, BoundSource::Machine | BoundSource::BuiltIn)
    }

    /// `80 (built_in)`, or the clamped form naming requester and limiter.
    /// §FS-rhei-budgets.2.3
    pub fn value_phrase(&self) -> String {
        match self.requested {
            Some(asked) => format!(
                "{} (requested {asked} by the {}, limited by machine settings)",
                self.effective,
                self.source.as_str()
            ),
            None => format!("{} ({})", self.effective, self.source.as_str()),
        }
    }

    /// The one line every surface that reports a bound prints.
    /// §FS-rhei-budgets.2.3 §FS-rhei-validate.4
    pub fn report_line(&self) -> String {
        format!("{}: {}", self.key, self.value_phrase())
    }

    /// The single remedy: the one thing that raises the limiter that actually
    /// stopped the work. It never offers an inner value the ceiling would
    /// clamp, because sending an operator to a field that cannot take effect
    /// sends them to the wrong file. §FS-rhei-budgets.8
    pub fn remedy(&self) -> String {
        if self.limited_by_machine() {
            return format!("set `defaults.{}` in the machine settings file", self.key);
        }
        match self.source {
            BoundSource::Project => {
                format!("set `defaults.{}` in the project settings file", self.key)
            }
            _ => format!("set `{}` on the node's profile in the state machine", self.key),
        }
    }
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

fn row(label: &str, value: &str) -> String {
    format!("       {label:<LABEL$}{value}")
}

/// The halt, in the fixed shape of §FS-rhei-budgets.8: what stopped, the
/// dimension, the bound with its provenance, the numbers, the accounting mode,
/// and exactly one remedy.
/// §FS-rhei-budgets.8
pub fn halt_text(spent: &Exhaustion, bound: &Bound, remedy: &Remedy) -> String {
    let headline = match spent.dimension {
        Dimension::Travel => {
            format!("error: ticket '{}' has spent its travel bound", spent.subject)
        }
        Dimension::Invocations => match spent.contract {
            Contract::Window => format!(
                "error: project '{}' has spent today's invocation capacity",
                spent.subject
            ),
            Contract::Lifetime { .. } => {
                format!("error: project '{}' has spent its invocation allowance", spent.subject)
            }
        },
    };
    let mode = match spent.dimension {
        Dimension::Travel => "per ticket identity".to_string(),
        Dimension::Invocations => match spent.contract {
            Contract::Window => format!("window ({}Z)", spent.day),
            Contract::Lifetime { .. } => "lifetime".to_string(),
        },
    };
    let mut lines = vec![
        headline,
        row("dimension:", spent.dimension.label()),
        row("bound:", &bound.value_phrase()),
        row(
            "consumed:",
            &format!(
                "{}  outstanding: {}  remaining: {}",
                spent.counter.consumed,
                spent.counter.reserved,
                spent.counter.remaining(spent.bound).unwrap_or(0)
            ),
        ),
        row("mode:", &mode),
    ];
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
