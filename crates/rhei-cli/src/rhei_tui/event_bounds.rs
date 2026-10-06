// The bound records a run emits: one dimension with its provenance and where
// it stands, the ceiling a machine delegated, and the line a terminal prints.
//
// Their own part because they are the one group of run records the budget
// owns rather than the engine: every field is a fact of a bound's provenance or
// standing, and the record grows with the budget rather than with the run loop.

// §AR-source-file-size.3 §FS-rhei-run-tui.1.1 §FS-rhei-budgets.9

/// One count dimension as every surface reports it: what it is, what bounds it,
/// who set that bound, who lowered it, and where it stands.
///
/// The two sources stay apart because they answer different questions: "the
/// machine set this" and "the machine lowered this" send a reader to different
/// files. §FS-rhei-budgets.2.3 §FS-rhei-budgets.9
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundReport {
    pub dimension: String,
    pub effective: u64,
    pub value_source: String,
    /// Whoever holds the ceiling that clamped a higher request — `"machine"`,
    /// or `"project"` where the machine delegated it — and `None` where
    /// nothing was clamped. §FS-rhei-budgets.2.3
    pub limiting_source: Option<String>,
    pub consumed: u64,
    pub outstanding: u64,
    pub remaining: u64,
    /// `per ticket identity`, `window`, or `lifetime`.
    pub mode: String,
    /// The UTC day key, under the window contract only.
    pub window: Option<String>,
    /// The account's currency, present exactly on the dimension whose
    /// amounts are money in micro-units rather than counts.
    /// §FS-rhei-budgets.5.5 §FS-rhei-run-tui.1.1
    pub currency: Option<String>,
    /// How much of this dimension was charged at the worst case rather than
    /// measured, present on the spend dimension and zeroed rather than absent
    /// where nothing was. §FS-rhei-budgets.6.2
    pub marks: Option<rhei_core::budget::SpendMarks>,
    /// The ceiling the machine delegated to the project for this dimension,
    /// present exactly where it did. §FS-rhei-budgets.2.3 §FS-rhei-run-tui.1.1
    pub ceiling: Option<BoundCeiling>,
}

/// A count ceiling the machine delegated to the project: the settings key, the
/// project's value, the project file holding it and the machine file that
/// delegated it, so a frontend can say whose ceiling is in force.
/// §FS-rhei-budgets.2.3 §FS-rhei-run-json.2.1
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundCeiling {
    pub key: String,
    pub value: u64,
    pub path: Option<String>,
    pub delegated_by: Option<String>,
}

impl BoundCeiling {
    /// The delegated ceiling of `bound`, or `None` where the machine's
    /// ceiling stands. §FS-rhei-budgets.2.3
    pub fn of(bound: &rhei_core::budget::Bound) -> Option<Self> {
        let path = |path: &Option<std::path::PathBuf>| {
            path.as_ref().map(|path| path.display().to_string())
        };
        bound.delegated().then(|| Self {
            key: bound.key.to_string(),
            value: bound.ceiling,
            path: path(&bound.files.project),
            delegated_by: path(&bound.files.machine),
        })
    }
}

/// One dimension on a terminal surface: the bound with its provenance, and
/// where it stands. §FS-rhei-budgets.9
pub fn bound_journal_line(bound: &BoundReport) -> String {
    let source = match &bound.limiting_source {
        Some(limiter) => format!("{} limited by {limiter}", bound.value_source),
        None => bound.value_source.clone(),
    };
    // Money carries its currency; a count is written as itself.
    // §FS-rhei-run-tui.1.1
    let write = |value: u64| match &bound.currency {
        Some(currency) => rhei_core::money::format_micro(value, Some(currency)),
        None if bound.marks.is_some() => rhei_core::money::format_micro(value, None),
        None => value.to_string(),
    };
    format!(
        "{}: {} consumed + {} outstanding / {} ({source}); {} remaining [{}]",
        bound.dimension,
        write(bound.consumed),
        write(bound.outstanding),
        write(bound.effective),
        write(bound.remaining),
        bound.window.as_deref().unwrap_or(&bound.mode)
    )
}
