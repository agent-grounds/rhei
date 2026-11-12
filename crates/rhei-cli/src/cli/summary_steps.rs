// The steps of a summary: the run's attempts in `started` order, records and
// unrecorded spawns merged, with the consecutive attempts at one visit folded
// into one numbered `×N` entry. Its own part because the lead, the accounting
// and the checkpoint all count members, and only this list counts entries.

// §FS-rhei-summary.2.2 §FS-rhei-summary.2.2.1

/// One numbered entry per step of the run, in `started` order: an invocation
/// record, an agent spawn no record answers for, or a run of consecutive
/// attempts at one visit folded into one `×N` entry. The reading sorts the
/// whole union of its roots, so the numbering is the `started_at` order the
/// spec asks for however many roots the scope selected.
/// §FS-rhei-summary.2.2 §FS-rhei-summary.2.2.1 §FS-rhei-cost-accounting.6.2.1
fn summary_steps(inspection: &CostInspection) -> String {
    let mut per_task: BTreeMap<&str, usize> = BTreeMap::new();
    for held in &inspection.invocations {
        *per_task.entry(held.record.task_id.as_str()).or_default() += 1;
    }
    let steps = summary_step_order(inspection);
    let mut out = String::new();
    // Numbered after folding, so the last number counts entries. §FS-rhei-summary.2.2.1
    for (index, attempts) in steps.chunk_by(SummaryStep::folds_with).enumerate() {
        let first = &attempts[0];
        let (task_id, state) = first.task_and_state();
        // A repeated visit and a task with several records both need the visit
        // spelled out; a one-shot step stays clean. A spawn record carries no
        // `visit`, so an unrecorded entry has none to spell. §FS-rhei-summary.2.2
        let sibling_records = per_task.get(task_id).copied().unwrap_or(0);
        let visit = match first {
            SummaryStep::Record(held) if held.record.visit > 1 || sibling_records > 1 => {
                format!(" (visit {})", held.record.visit)
            }
            _ => String::new(),
        };
        // A run of one prints as an unfolded entry does, byte for byte. §FS-rhei-summary.2.2.1
        let folded = match attempts.len() {
            1 => String::new(),
            count => format!(" ×{count}"),
        };
        out.push_str(&format!("{}. `{task_id}` {state}{visit}{folded}", index + 1));
        let duration = summary_entry_duration(attempts);
        match first {
            SummaryStep::Record(held) => {
                out.push_str(&format!(" — {}", summary_step_actor(&held.record)));
                if let Some(duration) = duration {
                    out.push_str(&format!(" — {duration}"));
                }
                let records = attempts.iter().filter_map(|step| match step {
                    SummaryStep::Record(held) => Some((&held.record, inspection.books_of(held))),
                    SummaryStep::Unrecorded(_) => None,
                });
                if let Some(tokens) = summary_entry_tokens(records) {
                    out.push_str(&format!(" — {tokens}"));
                }
            }
            SummaryStep::Unrecorded(spawn) => {
                out.push_str(&format!(" — {}", spawn.worker));
                if let Some(duration) = duration {
                    out.push_str(&format!(" — {duration}"));
                }
                out.push_str(" — no accounting record");
            }
        }
        out.push('\n');
    }
    out
}

/// The records in the reading's own order, with each unrecorded spawn merged in
/// ahead of the first record that started after it. Merged rather than sorted
/// together, so the records keep the order they always had.
/// §FS-rhei-summary.2.2 §FS-rhei-cost-accounting.6.2.1
fn summary_step_order(inspection: &CostInspection) -> Vec<SummaryStep<'_>> {
    let mut spawns = inspection.unrecorded.iter().peekable();
    let mut steps = Vec::new();
    for held in &inspection.invocations {
        if let Some(at) = record_started_at_secs(&held.record) {
            while let Some(spawn) =
                spawns.next_if(|spawn| spawn.started_secs().is_some_and(|started| started < at))
            {
                steps.push(SummaryStep::Unrecorded(spawn));
            }
        }
        steps.push(SummaryStep::Record(held));
    }
    steps.extend(spawns.map(SummaryStep::Unrecorded));
    steps
}

/// One attempt in the step order: an invocation record, or an agent spawn no
/// record answers for. Consecutive attempts at one visit print as one entry.
/// §FS-rhei-summary.2.2 §FS-rhei-summary.2.2.1
enum SummaryStep<'a> {
    Record(&'a InspectedRecord),
    /// An agent spawn no record answers for. §FS-rhei-cost-accounting.6.2.1
    Unrecorded(&'a UnrecordedSpawn),
}

impl<'a> SummaryStep<'a> {
    /// The task id and the state the attempt ran.
    fn task_and_state(&self) -> (&'a str, &'a str) {
        match self {
            Self::Record(held) => (held.record.task_id.as_str(), held.record.state.as_str()),
            Self::Unrecorded(spawn) => (spawn.task.as_str(), spawn.state.as_str()),
        }
    }

    /// Whether `next`, the attempt right after this one, is another attempt at
    /// the same visit by the same actor, and so shares this one's entry: records
    /// by task, state, `visit`, agent, provider and model; spawns by task, state
    /// and `worker`; a record and a spawn never. §FS-rhei-summary.2.2.1
    fn folds_with(&self, next: &Self) -> bool {
        match (self, next) {
            (Self::Record(held), Self::Record(next)) => {
                let (record, next) = (&held.record, &next.record);
                record.task_id == next.task_id
                    && record.state == next.state
                    && record.visit == next.visit
                    && record.agent == next.agent
                    && record.provider == next.provider
                    && record.model == next.model
            }
            (Self::Unrecorded(spawn), Self::Unrecorded(next)) => {
                spawn.task == next.task && spawn.state == next.state && spawn.worker == next.worker
            }
            _ => false,
        }
    }

    /// The attempt's own duration, by the arithmetic every reading of its kind
    /// shares. §FS-rhei-summary.2.2 §FS-rhei-cost-accounting.3.4.1
    fn elapsed_ms(&self) -> Option<u64> {
        match self {
            Self::Record(held) => invocation_elapsed_ms(&held.record),
            Self::Unrecorded(spawn) => spawn.elapsed_ms(),
        }
    }
}

/// `<agent>, <provider>/<model>`, dropping whatever the record did not carry.
fn summary_step_actor(record: &AccountingInvocationRecord) -> String {
    match (record.provider.as_deref(), record.model.as_deref()) {
        (Some(provider), Some(model)) => format!("{}, {provider}/{model}", record.agent),
        (None, Some(model)) => format!("{}, {model}", record.agent),
        _ => record.agent.clone(),
    }
}

/// `ended_at - started_at` of one record, humanized as an entry of one prints
/// it; `None` when either timestamp is missing or unparseable, because a
/// duration is not worth guessing. §FS-rhei-summary.2.2 §FS-rhei-cost-accounting.3.4.1
#[cfg(test)]
fn summary_step_duration(record: &AccountingInvocationRecord) -> Option<String> {
    invocation_elapsed_ms(record).map(format_duration_short)
}

/// The sum of every attempt's own duration, humanized; `None` when any attempt
/// has none, never a partial sum shown as the whole. §FS-rhei-summary.2.2.1
fn summary_entry_duration(attempts: &[SummaryStep]) -> Option<String> {
    let total: Option<u64> = attempts.iter().map(SummaryStep::elapsed_ms).sum();
    total.map(format_duration_short)
}

/// Humanized `in`/`out` counts of one record, as an entry of one prints them.
/// §FS-rhei-summary.2.2
#[cfg(test)]
fn summary_step_tokens(
    record: &AccountingInvocationRecord,
    books: &ReachablePriceBooks,
) -> Option<String> {
    summary_entry_tokens([(record, books)])
}

/// Humanized `in`/`out` sums over an entry's records, and only the sides every
/// one of them measured. §FS-rhei-summary.2.2 §FS-rhei-summary.2.2.1
fn summary_entry_tokens<'r>(
    records: impl IntoIterator<Item = (&'r AccountingInvocationRecord, &'r ReachablePriceBooks)>,
) -> Option<String> {
    // A record written before the convention existed is read under the one its
    // own `agent` implies here too. §FS-rhei-cost-accounting.5.2
    let usages: Vec<rhei_tui::UsageSummary> = records
        .into_iter()
        .map(|(record, books)| usage_summary_from_record(record, books))
        .collect();
    let input: Option<u64> = usages.iter().map(|usage| usage.input_total.value).sum();
    let output: Option<u64> = usages.iter().map(|usage| usage.output_total.value).sum();
    let mut parts = Vec::new();
    for (total, side) in [(input, "in"), (output, "out")] {
        let summed = rhei_tui::DimensionSummary { value: total, ..Default::default() };
        if summed.value.is_some() {
            parts.push(format!("{} {side}", format_dimension_value(&summed)));
        }
    }
    (!parts.is_empty()).then(|| parts.join(" / "))
}
