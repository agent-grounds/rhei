// What an agent's exit does when its provider made a wait of it: a refusal
// parks the ticket until its reset, and a `pi` session that lost its provider
// has its spawn record rewritten and waits out a backoff, or halts a spent
// visit.
//
// Its own part because both ways an agent exit is judged — the single worker
// and the pool — reach the same decisions and must say the same thing there,
// and neither of those files is where the answer belongs.

// §AR-source-file-size.3 §FS-rhei-run.3.3 §FS-rhei-agents.2.5 §FS-rhei-agents.3.2.1

/// Park a recognized refusal: its wait is persisted only while the ticket still
/// occupies the reporting state, and the line says so.
// §FS-rhei-agents.5.2.1 §FS-rhei-run.3.3
fn park_refusal(
    input: &Path,
    reloaded: &LoadedPlan,
    (task_id, state): (&str, &str),
    stayed_in_state: bool,
    observed: ProviderLimit,
    release: &mut PendingSlotRelease,
    sink: &Arc<dyn rhei_tui::EventSink>,
) -> MietteResult<()> {
    let effective = if stayed_in_state {
        persist_provider_limit(input, reloaded, task_id, state, &observed)?
    } else {
        observed
    };
    release.provider_limited(&effective);
    emit_run_message(
        sink,
        rhei_tui::MessageLevel::Info,
        format!(
            "  Task {} provider-limited by {} until {}; state unchanged.",
            task_id, effective.identity.provider, effective.next_attempt_at
        ),
    );
    Ok(())
}

/// Whether a zero exit that stayed in its state resumed the provider: not when
/// it is about to settle as an unavailability, whose wait must not be cleared
/// first. §FS-rhei-run.3.3
fn resumed_provider(verdict: Option<&ProviderUnavailable>, missing: &[String]) -> bool {
    verdict.is_none() || missing.is_empty()
}

/// One agent exit that stayed in its state with a required output missing.
struct AgentStall<'a> {
    input: &'a Path,
    reloaded: &'a LoadedPlan,
    task_id: &'a str,
    state: &'a str,
    /// Where the spawn recorded itself, `exited` and charged. §FS-rhei-agents.8.4
    spawn_record: &'a Path,
    missing: &'a [String],
    /// Decided at the spawn, counting this attempt as charged. §FS-rhei-agents.3.2.1
    outlook: RetryOutlook,
    /// pi's verdict, read at the exit. §FS-rhei-agents.2.5
    verdict: Option<&'a ProviderUnavailable>,
}

/// Say what the stall is and what the run does next: a recognized provider
/// unavailability settles as one, and anything else — including a record that
/// cannot be rewritten, which stays `exited` and charged — is a missing output.
// §FS-rhei-agents.3.2.1
fn stall_agent_exit(
    stall: AgentStall<'_>,
    release: &mut PendingSlotRelease,
    sink: &Arc<dyn rhei_tui::EventSink>,
) -> MietteResult<()> {
    if let Some(verdict) = stall.verdict.filter(|_| !stall.missing.is_empty()) {
        if let Some(record) = record_provider_unavailable(stall.spawn_record, verdict) {
            return settle_provider_unavailable(&stall, verdict, &record, release, sink);
        }
    }
    // An agent reaches this only on a zero exit. §FS-rhei-agents.3.2
    let (task_id, state) = (stall.task_id, stall.state);
    emit_missing_required_outputs_warning(
        "agent",
        task_id,
        state,
        0,
        stall.missing,
        stall.outlook,
        sink,
    );
    Ok(())
}

/// The warning, then the wait or — when the charge spent the visit's last
/// attempt — the exhaustion halt with no wait, because no spawn follows it.
// §FS-rhei-agents.3.2.1 §FS-rhei-run.3.3
fn settle_provider_unavailable(
    stall: &AgentStall<'_>,
    verdict: &ProviderUnavailable,
    record: &SpawnRecord,
    release: &mut PendingSlotRelease,
    sink: &Arc<dyn rhei_tui::EventSink>,
) -> MietteResult<()> {
    // The metrics note was written at the exit; it says what the record now says.
    if let Some(runtime_dir) = stall.spawn_record.parent().and_then(Path::parent) {
        amend_metric_pending_ending(runtime_dir, &record.log, PROVIDER_UNAVAILABLE_ENDING);
    }
    let warn = |text: String| emit_run_message(sink, rhei_tui::MessageLevel::Warn, text);
    warn(format!(
        "  warning: the provider was unavailable for task {} in state '{}': pi gave up \
         retrying ({}); {}",
        stall.task_id,
        stall.state,
        verdict.signal,
        unavailable_charge(record, verdict)
    ));
    if record.attempt_charged && !matches!(stall.outlook, RetryOutlook::AttemptsLeft) {
        warn(stall.outlook.halt_line(stall.task_id, stall.state, stall.missing));
        release.provider_unavailable(None);
        return Ok(());
    }
    let wait = verdict
        .wait(record.unavailable)
        .ok_or_else(|| diagnostic!("cannot date the wait of task {}", stall.task_id))?;
    // The later deadline wins, whichever kind set it. §FS-rhei-run.3.3
    let effective =
        persist_provider_limit(stall.input, stall.reloaded, stall.task_id, stall.state, &wait)?;
    emit_run_message(
        sink,
        rhei_tui::MessageLevel::Info,
        format!(
            "  waiting: {} on {} starts again after {}",
            effective.identity.agent, effective.identity.provider, effective.next_attempt_at
        ),
    );
    release.provider_unavailable(Some(&effective));
    Ok(())
}

/// The warning's `{charge}` clause, from the record as rewritten.
// §FS-rhei-agents.3.2.1 §FS-rhei-agents.3.2.3
fn unavailable_charge(record: &SpawnRecord, verdict: &ProviderUnavailable) -> String {
    if !record.attempt_charged {
        format!(
            "no attempt charged (free {} of {FREE_UNAVAILABLE_ENDINGS} on this visit)",
            record.unavailable
        )
    } else if verdict.answered {
        "attempt charged, because the provider answered this session".to_string()
    } else {
        format!(
            "attempt charged, because this visit's {FREE_UNAVAILABLE_ENDINGS} free endings are \
             spent"
        )
    }
}
