// Accounting selections never narrow the Panta's lifetime balance.
// §FS-rhei-budgets.10 §FS-rhei-panta.6.5

fn budget_inspection(input: &Path) -> serde_json::Value {
    let inspect = || -> MietteResult<serde_json::Value> {
        let project = BudgetProject::resolve(input)?;
        let Some(id) = project.identity()? else {
            return Ok(serde_json::Value::Null);
        };
        let journal =
            rhei_core::budget::Journal::open(&project.root, &id, false).map_err(budget_error)?;
        let mut snapshot = journal.snapshot().map_err(budget_error)?;
        let loaded = load_plan(&project.input)?;
        if let Ok(resolved) = resolve_state_machines_for_loaded_plan(&project.input, &loaded, None)
        {
            if let Ok(machines) = ExecutionMachines::build(&resolved, &project.input, &loaded) {
                budget_add_travel_limits(&mut snapshot, &loaded, &machines.set, &id)?;
            }
        }
        let mut value = serde_json::to_value(snapshot)
            .map_err(|e| miette!(help = "recover the ledger with `rhei budget recover` and read it again", "cannot render the budget snapshot: {e}"))?;
        value["ledger_health"] = "verified".into();
        value["journal"] = journal.path().to_string_lossy().into_owned().into();
        value["authority"] = journal.authority_path().to_string_lossy().into_owned().into();
        Ok(value)
    };
    inspect().unwrap_or_else(|error| {
        serde_json::json!({
            "ledger_health": "untrustworthy", "reason_code": "untrustworthy_ledger",
            "message": error.to_string(),
        })
    })
}

/// Current authored ceilings are a read-only view over lifetime consumption.
/// §FS-rhei-budgets.10
fn budget_add_travel_limits(
    snapshot: &mut rhei_core::budget::Snapshot,
    loaded: &LoadedPlan,
    machines: &rhei_validator::MachineSet,
    project_uuid: &str,
) -> MietteResult<()> {
    for task in budget_task_list(&loaded.rhei.tasks) {
        if let (Ok(ticket), Some(limit)) = (
            budget_ticket_identity(loaded, &task.id, project_uuid),
            machines
                .for_task(&task.id)
                .profile_for_node(&task.kind, task.profile_level())
                .and_then(|p| p.transition_limit),
        ) {
            snapshot.set_travel_limit(&ticket, limit).map_err(budget_error)?;
        }
    }
    Ok(())
}

fn budget_inspection_text(value: &serde_json::Value) -> String {
    if value.is_null() {
        return String::new();
    }
    if value["ledger_health"] != "verified" {
        return format!(
            "Panta lifetime budget: untrustworthy history; {}\n",
            value["message"].as_str().unwrap_or("inspect `rhei budget show`")
        );
    }
    format!("Panta lifetime budget {} (verified history):\nInvocations: {} consumed + {} reserved / {}; {} remaining\nSpend: {} consumed + {} reserved / {} micro-{}; {} remaining\n",
        value["project_id"].as_str().unwrap_or("unknown"), value["consumed"]["invocations"],
        value["reserved"]["invocations"], value["allowance"]["invocations"], value["remaining"]["invocations"],
        value["consumed"]["spend"]["amount_micro"], value["reserved"]["spend"]["amount_micro"],
        value["allowance"]["spend"]["amount_micro"], value["allowance"]["spend"]["currency"].as_str().unwrap_or("unknown"),
        value["remaining"]["spend"]["amount_micro"])
}
