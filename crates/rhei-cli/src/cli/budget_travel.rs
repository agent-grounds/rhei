// The travel charge on the shared transition path, and the halt text every
// surface prints when a bound is spent.
//
// Its own part because the charge sits where a move is *recorded* rather than
// where one is decided: every verb that moves a ticket — the run's
// auto-advance, `rhei transition`, `rhei complete`, a callback redirect —
// reaches the same line, which is what makes the count true of all of them
// rather than of the ones that remembered.

// §AR-source-file-size.3 §FS-rhei-budgets.4.1 §FS-rhei-budgets.8

use rhei_core::budget::{halt_text, AppliedEdge, Contract, Dimension, Remedy};

/// Render a refusal as the halt of §FS-rhei-budgets.8, or as itself when what
/// refused was the account rather than a bound.
///
/// The remedy is chosen by which limiter actually stopped the work: the machine
/// settings key when the ceiling limits, the audited command when an explicit
/// lifetime allowance does, and the renewal instant when the window does —
/// because nothing an operator does is needed for a wait, and sending them to a
/// settings file for one is the wrong instruction even though it would work.
/// §FS-rhei-budgets.8
fn budget_halt_text(refusal: &BudgetError, bounds: &CountBounds, journal: &Journal) -> String {
    let Some(spent) = refusal.exhaustion.as_deref() else {
        return format!("error: {}", refusal.message);
    };
    match spent.dimension {
        Dimension::Travel => halt_text(spent, &bounds.travel, &Remedy::Raise),
        Dimension::Invocations => match &spent.contract {
            // A window renews on its own, so the instant *is* the remedy.
            Contract::Window => match journal.renewal_instant() {
                Ok(Some(instant)) => halt_text(spent, &bounds.per_day, &Remedy::Renews(instant)),
                _ => halt_text(spent, &bounds.per_day, &Remedy::Raise),
            },
            // An explicit allowance is the ledger's own number rather than a
            // settings value, so it is reported as one and raised by the
            // audited command. §FS-rhei-budgets.10
            Contract::Lifetime { allowance } => halt_text(
                spent,
                &Bound::resolve("invocation_allowance", *allowance, None, None),
                &Remedy::Adjust,
            ),
        },
    }
}

/// The ticket's budget identity, taken from the plan metadata the caller is
/// already holding, and minted into a copy of it when the ticket has none.
///
/// The transition path already holds this ticket's metadata lock, so the
/// identity is folded into the update it is about to write rather than taken
/// through a second lock — which is the same lock, and would be a deadlock.
/// §AR-neural-admission.3
fn budget_ticket_id_in_metadata(
    metadata: Option<&Metadata>,
    metadata_key: &TaskId,
) -> (Option<Metadata>, String) {
    if let Some(existing) = task_metadata_map(metadata, metadata_key)
        .and_then(|task| task.get(yaml_key(BUDGET_TICKET_KEY)))
        .and_then(YamlValue::as_str)
    {
        return (None, existing.to_string());
    }
    let minted = uuid::Uuid::new_v4().to_string();
    let mut root = metadata.cloned().unwrap_or_default();
    let metadata_section = ensure_mapping(&mut root, yaml_key("metadata"));
    let tasks = ensure_mapping(metadata_section, yaml_key("tasks"));
    let task_entry = ensure_mapping(tasks, task_id_yaml_key(metadata_key));
    task_entry.insert(yaml_key(BUDGET_TICKET_KEY), YamlValue::String(minted.clone()));
    (Some(root), minted)
}

/// What one applied edge needs to be charged, gathered where the transition
/// path already has it.
struct TransitionCharge<'a> {
    workspace_root: &'a Path,
    machine: &'a rhei_validator::StateMachine,
    settings: &'a RheiSettings,
    task: Option<&'a rhei_core::ast::Task>,
    metadata_key: &'a TaskId,
    /// The file the identity is bound to, and the one the caller is about to
    /// rewrite.
    metadata_file: &'a Path,
    task_id_str: &'a str,
    from: &'a str,
    to: &'a str,
}

/// Charge one travel unit for an applied edge, returning the metadata the
/// caller should write when the ticket had to be given an identity.
///
/// Every applied edge spends one, whoever selected it: an agent outcome, a
/// program's declared route, a callback redirect, an ordinary self-loop, or a
/// person running `rhei transition`. A manual path that moved for free would be
/// the bypass. §FS-rhei-budgets.4.1
///
/// A project with **no account** is charged nothing, because there is nothing
/// to charge against: an account is established by the first admission
/// ([§FS-rhei-budgets.5.4](../../../docs/functional-spec/rhei-budgets.spec.md)),
/// and a project that has never admitted a neural start has never had the
/// runaway this bound exists for. It is also what keeps a plan that never runs
/// an agent exactly as it is today, down to the bytes of its metadata.
fn budget_charge_applied_edge(
    charge: &TransitionCharge<'_>,
    metadata: Option<&Metadata>,
) -> MietteResult<Option<Metadata>> {
    let project_root = budget_project_root(charge.workspace_root);
    let Some(account) = budget_result(Account::locate(&project_root))? else {
        return Ok(None);
    };
    let (minted, ticket_uuid) = budget_ticket_id_in_metadata(metadata, charge.metadata_key);
    let ticket = account.ticket_identity(&ticket_uuid);
    let bounds =
        resolve_count_bounds(charge.settings, node_transition_limit(charge.machine, charge.task));
    // The unit admission is holding for this very edge, where the move came
    // from a spawn this run admitted. Taken rather than read: an edge is
    // applied once. §FS-rhei-budgets.4.1
    let held =
        with_claims(|claims| claims.get_mut(charge.task_id_str).and_then(|c| c.travel.take()));
    let audit = budget_audit("apply an edge")?;
    let mut journal = budget_result(account.open(true))?;
    let charged = (|| -> Result<(), BudgetError> {
        journal.bind_ticket(&ticket, charge.task_id_str, charge.metadata_file, &audit)?;
        journal.charge_travel(
            &AppliedEdge {
                ticket: &ticket,
                display_id: charge.task_id_str,
                from: charge.from,
                to: charge.to,
                reservation: held.as_deref(),
                transition_limit: bounds.travel.effective,
            },
            &audit,
        )
    })();
    match charged {
        Ok(()) => Ok(minted),
        Err(refusal) => Err(miette!(
            help = budget_inspect_help(),
            "{}",
            budget_halt_text(&refusal, &bounds, &journal)
        )),
    }
}

/// Where both counts stand, as the surfaces that show them want it.
///
/// The travel line is per ticket, so it is reported for the ticket being
/// admitted rather than for the project: a run that showed one ticket's travel
/// as the project's would be reporting a number that is true of nothing.
/// §FS-rhei-budgets.9
fn budget_reports(
    journal: &Journal,
    ticket: &str,
    bounds: &CountBounds,
) -> Result<Vec<rhei_tui::BoundReport>, BudgetError> {
    let snapshot = journal.snapshot()?;
    let invocation_bound = snapshot.invocation_bound(bounds.per_day.effective);
    let travel = snapshot.travel_for(ticket);
    Ok(vec![
        rhei_tui::BoundReport {
            dimension: Dimension::Invocations.label().into(),
            effective: invocation_bound,
            value_source: bounds.per_day.source.as_str().into(),
            limiting_source: bounds.per_day.requested.map(|_| "machine".into()),
            consumed: snapshot.invocations.consumed,
            outstanding: snapshot.invocations.reserved,
            remaining: snapshot.invocations.remaining(invocation_bound)?,
            mode: snapshot.contract.name().into(),
            window: matches!(snapshot.contract, Contract::Window).then(|| snapshot.day.clone()),
        },
        rhei_tui::BoundReport {
            dimension: Dimension::Travel.label().into(),
            effective: bounds.travel.effective,
            value_source: bounds.travel.source.as_str().into(),
            limiting_source: bounds.travel.requested.map(|_| "machine".into()),
            consumed: travel.consumed,
            outstanding: travel.reserved,
            remaining: travel.remaining(bounds.travel.effective)?,
            mode: "per ticket identity".into(),
            window: None,
        },
    ])
}

/// The refused admission as a record, carrying the same facts the halt text
/// prints so a reader can route on them without parsing prose.
/// §FS-rhei-run-json.2.1
fn budget_halt_event(
    refusal: &BudgetError,
    bounds: &CountBounds,
    journal: &Journal,
    task_id_str: &str,
) -> Option<rhei_tui::RunEvent> {
    let spent = refusal.exhaustion.as_deref()?;
    let (bound, renews_at) = match spent.dimension {
        Dimension::Travel => (&bounds.travel, None),
        Dimension::Invocations => (
            &bounds.per_day,
            match spent.contract {
                Contract::Window => journal.renewal_instant().ok().flatten(),
                Contract::Lifetime { .. } => None,
            },
        ),
    };
    Some(rhei_tui::RunEvent::BudgetHalt {
        task: task_id_str.to_string(),
        reason_code: refusal.reason_code.clone(),
        bound: rhei_tui::BoundReport {
            dimension: spent.dimension.label().into(),
            effective: spent.bound,
            value_source: bound.source.as_str().into(),
            limiting_source: bound.requested.map(|_| "machine".into()),
            consumed: spent.counter.consumed,
            outstanding: spent.counter.reserved,
            remaining: spent.counter.remaining(spent.bound).unwrap_or(0),
            mode: match spent.dimension {
                Dimension::Travel => "per ticket identity".into(),
                Dimension::Invocations => spent.contract.name().into(),
            },
            window: matches!(
                (spent.dimension, &spent.contract),
                (Dimension::Invocations, Contract::Window)
            )
            .then(|| spent.day.clone()),
        },
        // Never an inner value the machine ceiling would clamp: telling an
        // operator to raise a field that cannot take effect sends them to the
        // wrong file. §FS-rhei-budgets.8
        remedy: match &renews_at {
            Some(instant) => format!("wait: the window renews at {instant}"),
            None => bound.remedy(),
        },
        renews_at,
    })
}
