// Comparing invocation identities and explaining contradictory historical records.
// §AR-source-file-size.3 §FS-rhei-cost-accounting.3.7 §FS-rhei-cost-accounting.11

/// Whether a repeated `invocation_id` is one record read twice or two different
/// records sharing an id. §FS-rhei-panta.6.5
fn same_stored_record(
    left: &AccountingInvocationRecord,
    right: &AccountingInvocationRecord,
) -> bool {
    matches!(
        (serde_json::to_value(left), serde_json::to_value(right)),
        (Ok(left), Ok(right)) if left == right
    )
}

/// Whether this id already carries the run/move/attempt suffix written by
/// current Rhei. A repeated current id always means the same attempt; only a
/// legacy visit-level id is eligible for inference.
// §FS-rhei-cost-accounting.3.7
fn is_attempt_scoped_invocation_id(id: &str) -> bool {
    invocation_attempt_identity(id).is_some()
}

/// The `(moves, attempt)` an attempt-scoped id names, or nothing for a legacy
/// visit-level id. §FS-rhei-cost-accounting.3.7
fn invocation_attempt_identity(id: &str) -> Option<(u64, u64)> {
    invocation_attempt_parts(id).map(|(_, moves, attempt)| (moves, attempt))
}

/// Run, move and attempt components share one parser for identity and reservation.
/// §FS-rhei-cost-accounting.3.7 §FS-rhei-run.2.7
fn invocation_attempt_parts(id: &str) -> Option<(&str, u64, u64)> {
    let (before_attempt, attempt) = id.rsplit_once("::attempt-")?;
    let (before_move, moves) = before_attempt.rsplit_once("::move-")?;
    let (_, run_id) = before_move.rsplit_once("::run-")?;
    if run_id.is_empty() {
        return None;
    }
    Some((run_id, moves.parse().ok()?, attempt.parse().ok()?))
}

/// Stable facts carried both beside and inside a legacy visit-level id. A
/// disagreement means the records cannot describe retries of one visit.
// §FS-rhei-cost-accounting.3.7
fn same_legacy_base_facts(
    left: &AccountingInvocationRecord,
    right: &AccountingInvocationRecord,
) -> bool {
    left.task_id == right.task_id
        && left.state == right.state
        && left.visit == right.visit
        && left.target_slug == right.target_slug
}

/// A stored interval that can distinguish one legacy process from another.
// §FS-rhei-cost-accounting.3.7
fn valid_invocation_interval(
    record: &AccountingInvocationRecord,
) -> Option<(std::time::SystemTime, std::time::SystemTime)> {
    let started = rhei_tui::parse_rfc3339(&record.started_at)?;
    let ended = rhei_tui::parse_rfc3339(&record.ended_at)?;
    (ended >= started).then_some((started, ended))
}

/// Independent run evidence or non-overlapping forward intervals distinguish
/// two legacy attempts. Missing run attribution is deliberately not failure:
/// interval evidence can still retain that history.
// §FS-rhei-cost-accounting.3.5 §FS-rhei-cost-accounting.3.7
fn distinct_legacy_attempts(
    left: &AccountingInvocationRecord,
    right: &AccountingInvocationRecord,
) -> bool {
    if !same_legacy_base_facts(left, right) {
        return false;
    }
    if matches!((&left.run_id, &right.run_id), (Some(left), Some(right)) if left != right) {
        return true;
    }
    match (valid_invocation_interval(left), valid_invocation_interval(right)) {
        (Some((left_start, left_end)), Some((right_start, right_end))) => {
            left_end <= right_start || right_end <= left_start
        }
        _ => false,
    }
}

/// Explain reuse only when retained run evidence establishes distinct starts.
/// An invocation's timestamps alone distinguish attempts, not runs. §FS-rhei-cost-accounting.11
fn reused_run_identity_note(
    left: &AccountingInvocationRecord,
    right: &AccountingInvocationRecord,
    roots: &[AccountingRoot],
    run_root: Option<&Path>,
) -> Option<String> {
    let (id, _, _) = invocation_attempt_parts(&left.invocation_id)?;
    let (other, _, _) = invocation_attempt_parts(&right.invocation_id)?;
    if id != other
        || left.run_id.as_deref().is_some_and(|held| held != id)
        || right.run_id.as_deref().is_some_and(|held| held != id)
    {
        return None;
    }
    let execution_roots = roots
        .iter()
        .filter_map(|root| root.path.parent()?.parent())
        .chain(run_root)
        .map(canonical_root_key)
        .collect();
    let history = RunIdentityHistory::read_run_evidence(&execution_roots).ok()?;
    (history.starts.get(id)?.len() > 1).then(|| format!("; distinct runs reused run id '{id}'"))
}
