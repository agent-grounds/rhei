// Which invocation records an aggregate is computed over, and what that
// aggregate is then allowed to claim about itself.
//
// Selection happens before aggregation, and two rules make the result honest.
// A record that names no run is never dropped and never folded into a named
// run — it is the history a workspace held before the field existed. And a run
// selection standing beside such a record cannot call itself complete, because
// one of them may belong to the run that was asked for.

// §FS-rhei-cost-accounting.6.1 §FS-rhei-cost-accounting.6.2
// §FS-rhei-cost-accounting.8.2 §FS-rhei-cost-accounting.8.3

/// The reserved `--run` id that asks for the records naming no run.
// §FS-rhei-cost-accounting.8.2
const UNATTRIBUTED_RUN_SELECTOR: &str = "unattributed";

/// The group key those records are reported under. Parenthesized so it cannot
/// collide with a run id, which is hex. §FS-rhei-cost-accounting.8.3
const UNATTRIBUTED_GROUP_KEY: &str = "(unattributed)";

/// One resolved `<TIME>`: the instant it names, and the RFC 3339 spelling
/// reported back, so a caller can see what its `7d` was taken to mean.
// §FS-rhei-cost-accounting.8.2
#[derive(Clone, Debug)]
pub(crate) struct SelectionInstant {
    secs: i64,
    label: String,
}

impl SelectionInstant {
    pub(crate) fn label(&self) -> &str {
        &self.label
    }

    pub(crate) fn secs(&self) -> i64 {
        self.secs
    }
}

/// What `--run` asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
enum RunSelector {
    Named(String),
    /// `--run unattributed`: the records that name no run, asked for directly
    /// rather than only found by grouping. §FS-rhei-cost-accounting.8.2
    Unattributed,
}

/// `--run`, `--since`, and `--until`, resolved. They compose.
// §FS-rhei-cost-accounting.8.2
#[derive(Clone, Debug, Default)]
pub(crate) struct CostSelection {
    run: Option<RunSelector>,
    since: Option<SelectionInstant>,
    until: Option<SelectionInstant>,
}

impl CostSelection {
    pub(crate) fn resolve(
        run: Option<&str>,
        since: Option<&str>,
        until: Option<&str>,
    ) -> MietteResult<Self> {
        let run = run.map(|id| {
            if id == UNATTRIBUTED_RUN_SELECTOR {
                RunSelector::Unattributed
            } else {
                RunSelector::Named(id.to_string())
            }
        });
        Ok(Self {
            run,
            since: since.map(|text| parse_selection_time(text, "--since")).transpose()?,
            until: until.map(|text| parse_selection_time(text, "--until")).transpose()?,
        })
    }

    /// Whether the caller narrowed anything. Nothing about the unselected
    /// reading may move, so the extra text output is gated on this.
    // §FS-rhei-cost-accounting.8.4
    pub(crate) fn is_active(&self) -> bool {
        self.run.is_some() || self.since.is_some() || self.until.is_some()
    }

    fn selects_window(&self) -> bool {
        self.since.is_some() || self.until.is_some()
    }

    pub(crate) fn since_label(&self) -> Option<&str> {
        self.since.as_ref().map(SelectionInstant::label)
    }

    pub(crate) fn until_label(&self) -> Option<&str> {
        self.until.as_ref().map(SelectionInstant::label)
    }

    /// The `--run` value as the caller spelled it.
    pub(crate) fn run_label(&self) -> Option<&str> {
        match self.run.as_ref()? {
            RunSelector::Named(id) => Some(id),
            RunSelector::Unattributed => Some(UNATTRIBUTED_RUN_SELECTOR),
        }
    }

    /// `[since, until)`. Half-open, so two adjacent windows neither
    /// double-count a record nor lose one. §FS-rhei-cost-accounting.6.1
    fn window_contains(&self, secs: i64) -> bool {
        self.since.as_ref().is_none_or(|since| secs >= since.secs)
            && self.until.as_ref().is_none_or(|until| secs < until.secs)
    }

    /// Apply the selection to every record the reading holds, and to the agent
    /// spawns none of them answers for.
    ///
    /// `roots_unreadable` says that one of the accounting roots existed and
    /// could not be read, so nothing computed here may claim `complete`.
    // §FS-rhei-cost-accounting.6.2 §FS-rhei-cost-accounting.6.2.1
    pub(crate) fn apply<'a>(
        &self,
        records: impl IntoIterator<Item = ScopedRecord<'a>>,
        roots_unreadable: bool,
        spawns: &'a [UnrecordedSpawn],
    ) -> CostSelectionResult<'a> {
        // The window comes first: it decides the *scope* the run filter is then
        // applied to, and it is also the scope run attribution is reported
        // over, because that is the set a named run could have drawn from.
        let mut scope = Vec::new();
        let mut undated = 0u64;
        for record in records {
            if !self.selects_window() {
                scope.push(record);
                continue;
            }
            match record_started_at_secs(record.record) {
                Some(secs) if self.window_contains(secs) => scope.push(record),
                Some(_) => {}
                // Unplaceable, so counted rather than dropped: it demotes the
                // window's coverage instead. §FS-rhei-cost-accounting.6.2
                None => undated += 1,
            }
        }

        // A spawn names no run, so every run selection keeps it and only the
        // window places it; one it cannot place is a doubt. §FS-rhei-cost-accounting.6.2.1
        let mut unrecorded = Vec::new();
        let mut spawn_undated = false;
        for spawn in spawns {
            if !self.selects_window() {
                unrecorded.push(spawn);
                continue;
            }
            match spawn.started_secs() {
                Some(secs) if self.window_contains(secs) => unrecorded.push(spawn),
                Some(_) => {}
                None => spawn_undated = true,
            }
        }

        let unattributed: Vec<ScopedRecord<'a>> =
            scope.iter().copied().filter(|held| held.record.run_id.is_none()).collect();
        let attributed_count = scope.len() as u64 - unattributed.len() as u64;
        let selected = match &self.run {
            None => scope,
            Some(RunSelector::Unattributed) => unattributed.clone(),
            Some(RunSelector::Named(id)) => scope
                .into_iter()
                .filter(|held| held.record.run_id.as_deref() == Some(id.as_str()))
                .collect(),
        };

        CostSelectionResult {
            records: selected,
            unattributed,
            roots_unreadable,
            attributed_count,
            // Only a *named* run is uncertain. `--run unattributed` asked for
            // exactly the records it got, so it carries ordinary coverage like
            // any other group. §FS-rhei-cost-accounting.6.2
            selects_named_run: matches!(self.run, Some(RunSelector::Named(_))),
            window_uncertain: undated > 0 || spawn_undated,
            undated,
            unrecorded,
        }
    }
}

/// One selection, resolved: the records it matched, and what it could not be
/// sure of. §FS-rhei-cost-accounting.6.1 §FS-rhei-cost-accounting.6.2
pub(crate) struct CostSelectionResult<'a> {
    /// What the selection matched, and what every aggregate is computed from.
    /// Each record carries the books of the root it was read from.
    /// §FS-rhei-cost-accounting.5.2
    pub(crate) records: Vec<ScopedRecord<'a>>,
    /// The records in the run filter's scope that name no run. Reported
    /// whatever the coverage is, so an unattributed history cannot be mistaken
    /// for an attributed one. §FS-rhei-cost-accounting.3.5
    pub(crate) unattributed: Vec<ScopedRecord<'a>>,
    /// An accounting root in scope could not be read, so this selection saw
    /// less than it was asked for. §FS-rhei-cost-accounting.6.2
    roots_unreadable: bool,
    /// The records in that scope that do name one.
    pub(crate) attributed_count: u64,
    /// A named run was asked for, so records that name no run put its total in
    /// doubt. §FS-rhei-cost-accounting.6.2
    selects_named_run: bool,
    /// A window could not place every record it was asked about.
    window_uncertain: bool,
    /// How many records the window could not place.
    pub(crate) undated: u64,
    /// The agent spawns in this selection that no record answers for.
    /// §FS-rhei-cost-accounting.6.2.1
    pub(crate) unrecorded: Vec<&'a UnrecordedSpawn>,
}

impl CostSelectionResult<'_> {
    fn has_unattributed(&self) -> bool {
        !self.unattributed.is_empty()
    }

    /// Whether the whole selection may still report `complete`.
    // §FS-rhei-cost-accounting.6.2
    fn is_uncertain(&self) -> bool {
        self.roots_unreadable
            || self.window_uncertain
            || (self.selects_named_run && self.has_unattributed())
    }

    /// The rollup over the selection, with §6.2's demotion applied and its
    /// unrecorded spawns counted. §FS-rhei-cost-accounting.6.2.1
    pub(crate) fn summary(&self) -> Option<rhei_tui::AccountingRunSummary> {
        let summary = summarize_records(self.records.iter().copied())?;
        Some(count_unrecorded(demote_if(summary, self.is_uncertain()), self.unrecorded.len()))
    }

    /// The rollup over the records that name no run, beside which every
    /// unrecorded spawn stands, because it names none either.
    /// §FS-rhei-cost-accounting.6.2.1
    pub(crate) fn unattributed_summary(&self) -> Option<rhei_tui::AccountingRunSummary> {
        summarize_records(self.unattributed.iter().copied())
            .map(|summary| count_unrecorded(summary, self.unrecorded.len()))
    }

    /// How many unrecorded spawns the plan-tree axis places at a node, or in
    /// its subtree. §FS-rhei-cost-accounting.6.2.1
    pub(crate) fn unrecorded_at(&self, task_id: &str, subtree: bool) -> usize {
        self.unrecorded
            .iter()
            .filter(|spawn| {
                if subtree {
                    spawn.in_subtree(task_id)
                } else {
                    spawn.task == task_id
                }
            })
            .count()
    }

    /// Whether one group of the selection may report `complete`.
    ///
    /// Under `--by run` every *named* group stands in the same doubt a `--run`
    /// selection does — an unattributed record may belong to it. The
    /// `(unattributed)` group does not: it is exactly the records nothing
    /// could attribute. §FS-rhei-cost-accounting.6.2
    fn group_is_uncertain(&self, by: CostGroup, group_is_unattributed: bool) -> bool {
        self.is_uncertain()
            || (matches!(by, CostGroup::Run) && !group_is_unattributed && self.has_unattributed())
    }

    /// One group's rollup, with the same demotion rule applied to it and the
    /// unrecorded spawns its key places in it counted.
    /// §FS-rhei-cost-accounting.6.2.1
    pub(crate) fn group_summary(
        &self,
        by: CostGroup,
        key: &CostGroupKey,
        records: &[ScopedRecord<'_>],
    ) -> Option<rhei_tui::AccountingRunSummary> {
        let summary = summarize_records(records.iter().copied())?;
        let summary = demote_if(summary, self.group_is_uncertain(by, key.unattributed));
        let placed = self.unrecorded.iter().filter(|spawn| spawn.in_group(by, &key.key)).count();
        Some(count_unrecorded(summary, placed))
    }
}

/// `complete` becomes `partial` when the aggregate could not see everything it
/// should have. Nothing else moves: an already-partial, unpriced, or empty
/// reading is not made worse by a doubt it already carries.
// §FS-rhei-cost-accounting.6.2
fn demote_if(
    mut summary: rhei_tui::AccountingRunSummary,
    uncertain: bool,
) -> rhei_tui::AccountingRunSummary {
    if uncertain && summary.coverage == rhei_tui::UsageCoverage::Complete {
        summary.coverage = rhei_tui::UsageCoverage::Partial;
    }
    summary
}

/// What one group of a `--by` partition is keyed on.
///
/// The `(unattributed)` group is marked as well as keyed, so a machine reader
/// need not match on the key text to find it.
// §FS-rhei-cost-accounting.8.3
pub(crate) struct CostGroupKey {
    pub(crate) key: String,
    pub(crate) unattributed: bool,
}

/// The group a record belongs to under `--by`.
pub(crate) fn cost_group_key(record: &AccountingInvocationRecord, by: CostGroup) -> CostGroupKey {
    match by {
        CostGroup::Agent => CostGroupKey { key: record.agent.clone(), unattributed: false },
        CostGroup::Model => CostGroupKey {
            key: record.model.clone().unwrap_or_else(|| "(unknown)".to_string()),
            unattributed: false,
        },
        CostGroup::State => CostGroupKey { key: record.state.clone(), unattributed: false },
        CostGroup::Node => CostGroupKey { key: record.task_id.clone(), unattributed: false },
        // §FS-rhei-cost-accounting.8.3: the records that name no run get one
        // explicit group, never a fold into whichever run happens to be there.
        CostGroup::Run => match record.run_id.as_deref() {
            Some(id) => CostGroupKey { key: id.to_string(), unattributed: false },
            None => {
                CostGroupKey { key: UNATTRIBUTED_GROUP_KEY.to_string(), unattributed: true }
            }
        },
        CostGroup::Day => CostGroupKey { key: record_utc_day(record), unattributed: false },
    }
}

/// The UTC calendar day a record started on, `YYYY-MM-DD`.
// §FS-rhei-cost-accounting.8.3
fn record_utc_day(record: &AccountingInvocationRecord) -> String {
    // Read off the text the instant was parsed from, but with `get` rather than
    // a byte slice: an unreadable `started_at` already has a group of its own.
    record_started_at_secs(record)
        .and_then(|_| record.started_at.get(..10))
        .map_or_else(|| "(unknown)".to_string(), str::to_string)
}

/// A record's `started_at` as an epoch second, or nothing when it will not
/// parse as the UTC RFC 3339 instant every writer produces.
fn record_started_at_secs(record: &AccountingInvocationRecord) -> Option<i64> {
    epoch_secs(rhei_tui::parse_rfc3339(&record.started_at)?)
}

fn epoch_secs(at: std::time::SystemTime) -> Option<i64> {
    at.duration_since(std::time::UNIX_EPOCH).ok().map(|since| since.as_secs() as i64)
}

/// An RFC 3339 instant (`2026-09-01T00:00:00Z`), a bare UTC date
/// (`2026-09-01`, that date's midnight UTC), or a duration before now (`7d`,
/// `24h`, `90m`).
///
/// Refusing what it cannot read is the point: silently selecting nothing is how
/// a caller reads zero as an answer. §FS-rhei-cost-accounting.8.2
pub(crate) fn parse_selection_time(text: &str, flag: &str) -> MietteResult<SelectionInstant> {
    let instant = parse_instant(text)
        .or_else(|| parse_bare_utc_date(text))
        .or_else(|| parse_duration_before_now(text))
        .ok_or_else(|| {
            miette!(
                help = "give an RFC 3339 instant (2026-09-01T00:00:00Z), a UTC date \
                        (2026-09-01), or a duration before now (7d, 24h, 90m)",
                "could not read '{text}' as a time for {flag}"
            )
        })?;
    let secs = epoch_secs(instant).ok_or_else(|| {
        miette!(
            help = "give an instant on or after 1970-01-01T00:00:00Z",
            "'{text}' names an instant before 1970, which {flag} cannot select from"
        )
    })?;
    Ok(SelectionInstant { secs, label: rhei_tui::format_rfc3339(instant) })
}

fn parse_instant(text: &str) -> Option<std::time::SystemTime> {
    rhei_tui::parse_rfc3339(text)
}

fn parse_bare_utc_date(text: &str) -> Option<std::time::SystemTime> {
    (text.len() == 10).then(|| rhei_tui::parse_rfc3339(&format!("{text}T00:00:00Z")))?
}

fn parse_duration_before_now(text: &str) -> Option<std::time::SystemTime> {
    // The unit is the last *character*, not the last byte: splitting on a byte
    // index panics on any text ending in a multi-byte one — a pasted trailing
    // non-breaking space, say — instead of refusing it. §FS-rhei-cost-accounting.8.2
    let mut chars = text.chars();
    let unit = chars.next_back()?;
    let count: u64 = chars.as_str().parse().ok()?;
    let seconds = match unit {
        'd' => 86_400,
        'h' => 3_600,
        'm' => 60,
        's' => 1,
        _ => return None,
    };
    // A duration too large to hold is unreadable, not a window: wrapping the
    // multiply would answer over a span nobody asked for.
    // §FS-rhei-cost-accounting.8.2
    let span = count.checked_mul(seconds)?;
    std::time::SystemTime::now().checked_sub(std::time::Duration::from_secs(span))
}
