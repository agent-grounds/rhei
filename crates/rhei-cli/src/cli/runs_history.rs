// `rhei runs` as a question about history rather than only about right now,
// and the retention boundary that makes some windows unanswerable.
//
// The registry's cap on ended entries is right for resolving an id after the
// fact and wrong for a window: past it the entries were unlinked and there is
// nothing left to count. Saying so is the difference between an incomplete
// answer and a wrong one.

// §FS-rhei-run-headless.6.1 §FS-rhei-run-headless.6.2

/// The object `--json` emits once history has been asked for. The bare array
/// stays the answer to the live-run question §6 already defined.
// §FS-rhei-run-headless.6.1
const RUN_HISTORY_SCHEMA: &str = "rhei.run-history.v1";

/// What `--all`, `--since`, and `--until` asked of the run list. A window
/// implies `--all`, because a window is a question about history.
// §FS-rhei-run-headless.6.1
pub(crate) struct RunHistoryQuery {
    since: Option<SelectionInstant>,
    until: Option<SelectionInstant>,
}

impl RunHistoryQuery {
    /// `None` when nothing was asked beyond the live listing.
    // §FS-rhei-run-headless.6.1
    pub(crate) fn resolve(
        all: bool,
        since: Option<&str>,
        until: Option<&str>,
    ) -> MietteResult<Option<Self>> {
        if !all && since.is_none() && until.is_none() {
            return Ok(None);
        }
        // The same time vocabulary `rhei cost` uses, and the same refusal to
        // read an unparsable one as an empty window.
        // §FS-rhei-cost-accounting.8.2
        Ok(Some(Self {
            since: since.map(|text| parse_selection_time(text, "--since")).transpose()?,
            until: until.map(|text| parse_selection_time(text, "--until")).transpose()?,
        }))
    }

    /// `[since, until)` on `started_at`, the same half-open interval a cost
    /// window uses. §FS-rhei-run-headless.6.1
    fn selects(&self, descriptor: &RunDescriptor) -> bool {
        if self.since.is_none() && self.until.is_none() {
            return true;
        }
        let Some(secs) = rhei_tui::parse_rfc3339(&descriptor.started_at)
            .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|since_epoch| since_epoch.as_secs() as i64)
        else {
            // A run whose start cannot be read is not a run this window can
            // place, and dropping it silently is what §6.2 exists to prevent —
            // so it is kept and the listing shows it.
            return true;
        };
        self.since.as_ref().is_none_or(|since| secs >= since.secs())
            && self.until.as_ref().is_none_or(|until| secs < until.secs())
    }

    /// Whether the window reaches back past what the registry still holds.
    ///
    /// Two facts together: the registry is standing at its cap, so anything
    /// older than what it holds is gone if it ever existed; and the window
    /// begins before the earliest entry still held. A window entirely inside
    /// what is held is answerable in full. The registry keeps no record of what
    /// it unlinked, so the notice says the window *may* reach past its history
    /// rather than asserting entries were dropped — true whether or not the
    /// sweep has ever run.
    // §FS-rhei-run-headless.6.2
    fn truncation(&self, ended: &[RunDescriptor]) -> Option<RetentionTruncation> {
        if ended.len() < RETAINED_ENDED_RUNS {
            return None;
        }
        // Sorted newest first, so the earliest still held is the last one.
        let earliest = ended.last()?.started_at.clone();
        let reaches_past = self.since.as_ref().is_none_or(|since| {
            rhei_tui::parse_rfc3339(&earliest)
                .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok())
                .is_none_or(|held| since.secs() < held.as_secs() as i64)
        });
        reaches_past.then_some(RetentionTruncation { earliest_started_at: earliest })
    }
}

/// What the registry could not answer about, and the two facts that say why.
// §FS-rhei-run-headless.6.2
struct RetentionTruncation {
    earliest_started_at: String,
}

/// List the runs a window contains, live and ended, and say what retention hid.
// §FS-rhei-run-headless.6.1 §FS-rhei-run-headless.6.2
pub(crate) fn runs_history_command(json: bool, query: &RunHistoryQuery) -> MietteResult<()> {
    let sweep = sweep_run_registry();
    sweep.ensure_access()?;
    let truncated = query.truncation(&sweep.ended);
    let live: Vec<&RunDescriptor> =
        sweep.live.iter().filter(|run| query.selects(run)).collect();
    let ended: Vec<&RunDescriptor> =
        sweep.ended.iter().filter(|run| query.selects(run)).collect();

    if json {
        print_run_history_json(&live, &ended, truncated.as_ref())?;
        for entry in &sweep.undecided {
            eprintln!("warning: could not check {}: {}", entry.summary_line(), entry.reason);
        }
        return Ok(());
    }

    if live.is_empty() {
        println!("No runs are live on this machine.");
    } else {
        println!("{} live run{}:", live.len(), if live.len() == 1 { "" } else { "s" });
        for run in &live {
            print_run_history_entry(run);
        }
    }
    println!();
    // A heading of their own, so a finished run is never read as a live one.
    // §FS-rhei-run-headless.6.1
    if ended.is_empty() {
        println!("No ended runs in this window.");
    } else {
        println!("{} ended run{}:", ended.len(), if ended.len() == 1 { "" } else { "s" });
        for run in &ended {
            print_run_history_entry(run);
        }
    }
    report_undecided_runs(&sweep.undecided);
    if let Some(truncated) = &truncated {
        println!();
        print_retention_truncation(truncated);
    }
    Ok(())
}

fn print_run_history_entry(run: &RunDescriptor) {
    println!("  {}", run.summary_line());
    println!("      started {}  {}", run.started_at, run.workspace.display());
}

/// The line that keeps a shortened window from reading as a whole one.
// §FS-rhei-run-headless.6.2
fn print_retention_truncation(truncated: &RetentionTruncation) {
    println!(
        "Retention truncated this window: the registry keeps {RETAINED_ENDED_RUNS} ended \
         runs, and the earliest it still holds started {}.",
        truncated.earliest_started_at
    );
    println!("  This window may reach past what the registry still holds, so it cannot be answered in full.");
}

fn print_run_history_json(
    live: &[&RunDescriptor],
    ended: &[&RunDescriptor],
    truncated: Option<&RetentionTruncation>,
) -> MietteResult<()> {
    let runs: Vec<serde_json::Value> = live
        .iter()
        .map(|run| run_history_entry_json(run, "live"))
        .chain(ended.iter().map(|run| run_history_entry_json(run, "ended")))
        .collect();
    let payload = serde_json::json!({
        "schema": RUN_HISTORY_SCHEMA,
        "runs": runs,
        // Null when the window is answerable in full; the same two facts the
        // text line names when it is not. §FS-rhei-run-headless.6.2
        "truncated": truncated.map(|truncated| {
            serde_json::json!({
                "retained_ended_runs": RETAINED_ENDED_RUNS,
                "earliest_started_at": truncated.earliest_started_at,
            })
        }),
    });
    let rendered = serde_json::to_string_pretty(&payload).map_err(|err| {
        miette!(
            help = "read the run list as text instead: rhei runs --all",
            "could not render the run history as JSON: {err}"
        )
    })?;
    println!("{rendered}");
    Ok(())
}

/// One entry: the descriptor as `rhei runs --json` already emits it, plus the
/// liveness that keeps a finished run from being read as a live one.
// §FS-rhei-run-headless.6.1
fn run_history_entry_json(run: &RunDescriptor, liveness: &str) -> serde_json::Value {
    let mut value = serde_json::to_value(run).unwrap_or(serde_json::Value::Null);
    if let Some(object) = value.as_object_mut() {
        object.insert("liveness".to_string(), serde_json::Value::String(liveness.to_string()));
    }
    value
}
