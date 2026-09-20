// Refuse before task mutation and before subprocess error-routing can invent
// an outcome for work that was never admitted. §FS-rhei-budgets.4 §FS-rhei-budgets.9

fn budget_preflight(
    input: &Path,
    loaded: &LoadedPlan,
    machines: &rhei_validator::MachineSet,
    settings: &RheiSettings,
    scope: &RheiScope,
    events: &mut Vec<rhei_core::budget::BudgetEvent>,
    dry_run: bool,
) -> MietteResult<()> {
    let tasks = budget_task_list(&loaded.rhei.tasks)
        .into_iter()
        .filter(|task| {
            if !task_in_rhei_scope(scope, &task.id.to_string()) {
                return false;
            }
            let machine = machines.for_task(&task.id);
            let state = normalized_state_name(&task.state, machine);
            !machine.states.get(&state).is_some_and(|d| d.terminal || d.gating)
        })
        .collect::<Vec<_>>();
    if tasks.is_empty() {
        return Ok(());
    }
    let project = BudgetProject::resolve(input)?;
    let mut transaction = if dry_run { None } else { Some(budget_ensure_ticket_identities(&project)?) };
    let refreshed = load_plan(input)?;
    let loaded = &refreshed;
    if let Some(transaction) = &mut transaction {
        let collected = Arc::new(Mutex::new(Vec::new()));
        transaction.journal.set_event_sink(Arc::new(PreflightBudgetEvents(collected.clone())))
            .map_err(budget_error)?;
        let reservations = transaction.journal.snapshot().map_err(budget_error)?.reservations;
        for (id, reservation) in reservations {
            if reservation["settled_charge_micro"].is_null() {
                if let Err(error) = transaction.journal.reconcile_inbox(
                    &id, &budget_audit("collect delayed final provider evidence before admission")?,
                ) {
                    eprintln!("budget settlement retained exposure: {error}");
                }
            }
        }
        budget_recover_central_edges(&project, loaded, &mut transaction.journal)?;
        events.extend(collected.lock().expect("preflight budget events").drain(..));
    }
    let identity = project.required_identity()?;
    let read_only;
    let ledger = if let Some(transaction) = &transaction {
        &transaction.journal
    } else {
        read_only = rhei_core::budget::Journal::open(&project.root, &identity, false)
            .map_err(budget_error)?;
        &read_only
    };
    let mut snapshot = ledger.snapshot().map_err(budget_error)?;
    budget_add_travel_limits(&mut snapshot, loaded, machines, &identity)?;
    events.push(rhei_core::budget::BudgetEvent::BudgetSnapshot { budget: snapshot.clone() });
    // The fixture driver uses production admission; the ordinary binary has
    // no deterministic authority and stays fail-closed. §AR-neural-admission.8
    #[cfg(feature = "budget-fixtures")]
    if budget_fixture_active() {
        return budget_fixture_validate_project(&project.root, dry_run);
    }
    let mut refusals = Vec::new();
    let task_count = tasks.len();
    for task in tasks {
        let machine = machines.for_task(&task.id);
        let state = normalized_state_name(&task.state, machine);
        let def = machine.states.get(&state);
        let threshold = def
            .and_then(|s| s.budget_threshold.as_ref())
            .or(settings.defaults.budget_threshold.as_ref());
        let limit = machine
            .profile_for_node(&task.kind, task.profile_level())
            .and_then(|p| p.transition_limit);
        let ticket = budget_ticket_identity(loaded, &task.id, &identity);
        let travel = ticket
            .as_ref()
            .ok()
            .and_then(|id| snapshot.travel.get(id))
            .cloned()
            .unwrap_or_default();
        let reason = if snapshot.breached {
            ("breach", "qualification breach retained; requalification required".to_string())
        } else if limit.is_none_or(|n| n == 0) {
            (
                "missing_bound",
                "missing finite profile transition_limit; migrate the state machine".into(),
            )
        } else if let Err(error) = &ticket {
            ("identity_conflict", error.to_string())
        } else if travel.exposure().map_err(budget_error)? >= limit.expect("checked above") {
            (
                "travel_exhausted",
                format!(
                    "ticket travel exhausted ({} applied + {} reserved / {})",
                    travel.consumed,
                    travel.reserved,
                    limit.expect("checked above")
                ),
            )
        } else if threshold.is_none() {
            ("missing_bound", "missing budget_threshold on the state or in defaults".into())
        } else if let Err(error) = threshold.expect("checked above").validate() {
            ("missing_bound", error.to_string())
        } else if threshold.expect("checked above").currency != snapshot.allowance.spend.currency {
            ("missing_bound", "budget_threshold currency differs from the Panta allowance".into())
        } else if snapshot.remaining.invocations == 0 {
            (
                "invocation_exhausted",
                format!(
                    "Panta invocation allowance exhausted ({} consumed + {} reserved / {})",
                    snapshot.consumed.invocations,
                    snapshot.reserved.invocations,
                    snapshot.allowance.invocations
                ),
            )
        } else if snapshot.remaining.spend.amount_micro
            < threshold.expect("checked above").amount_micro
        {
            (
                "spend_exhausted",
                format!(
                    "Panta spend allowance exhausted ({} consumed + {} reserved / {} micro-{})",
                    snapshot.consumed.spend.amount_micro,
                    snapshot.reserved.spend.amount_micro,
                    snapshot.allowance.spend.amount_micro,
                    snapshot.allowance.spend.currency
                ),
            )
        } else if rhei_core::budget::Registry::available().is_empty() {
            // Say which half is missing: a host that cannot confine and a
            // confinable host with no qualified transport are repaired
            // differently. §FS-rhei-budgets.6.4 §FS-rhei-budgets.9
            ("missing_qualification", format!("missing qualification: {}", native_confinement_gap()))
        } else {
            // Exact qualification/debit belongs to the scheduler boundary;
            // command preflight never spends or guesses an arm. §AR-neural-admission.1
            continue;
        };
        events.push(rhei_core::budget::BudgetEvent::BudgetHalt {
            project_id: Some(snapshot.project_id.clone()),
            task_id: Some(task.id.to_string()),
            ticket_identity: ticket.ok(),
            reason_code: reason.0.into(),
            message: reason.1.clone(),
            budget: Some(snapshot.clone()),
            dry_run,
        });
        refusals.push(format!("{}: {}", task.id, reason.1));
    }
    if refusals.len() < task_count {
        // Exact per-ticket checks run again immediately before reservation.
        // §FS-rhei-budgets.9
        return Ok(());
    }
    Err(miette!(help = "no subprocess started and no task outcome changed; qualification evidence is a rollout prerequisite, not an operator opt-in", "budget admission halted:\n{}", refusals.join("\n")))
}

// The preflight front end has not installed the live tee yet; retain the same
// real settlement/breach receipts for it to publish. §FS-rhei-budgets.10
struct PreflightBudgetEvents(Arc<Mutex<Vec<rhei_core::budget::BudgetEvent>>>);
impl rhei_core::budget::BudgetEventSink for PreflightBudgetEvents {
    fn emit(&self, event: rhei_core::budget::BudgetEvent) {
        self.0.lock().expect("preflight budget events").push(event);
    }
}
