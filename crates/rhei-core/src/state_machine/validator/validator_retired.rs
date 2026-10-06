// The project's retirement record, judged against the live graph: a record rhei
// cannot read, a removed id made live again, a retired budget identity rebound.
// Every run and transition validates first, so these checks are what keep a
// hand edit from undoing a removal.

/// Refuse a retirement record a removal could not honour.
/// §FS-rhei-validate.4 §FS-rhei-remove.5.2
fn validate_retired_tickets(
    rhei: &Rhei,
    index: &HashMap<TaskId, &Task>,
    report: &mut ValidationReport,
) {
    let retired = match crate::retired::retired_tickets(rhei.metadata.as_ref()) {
        Ok(retired) => retired,
        Err(message) => {
            report.errors.push(message);
            return;
        }
    };
    if retired.is_empty() {
        return;
    }
    let mut resurrected: Vec<String> = index
        .keys()
        .map(ToString::to_string)
        .filter(|id| retired.contains_key(id))
        .collect();
    resurrected.sort();
    for id in resurrected {
        report.errors.push(format!(
            "Task {id} is declared, but {id} was removed and its id is retired \
             (metadata.retiredTickets); a retired id is never live again — give the ticket a \
             new id"
        ));
    }
    validate_retired_budget_identities(rhei, &retired, report);
}

/// A retired ticket's budget identity stays bound to it; a live ticket claiming
/// it would inherit the removed ticket's account. §FS-rhei-budgets.5.2.1
fn validate_retired_budget_identities(
    rhei: &Rhei,
    retired: &crate::retired::RetiredTickets,
    report: &mut ValidationReport,
) {
    let Some(tasks) = rhei
        .metadata
        .as_ref()
        .and_then(|root| root.get("metadata"))
        .and_then(serde_yaml::Value::as_mapping)
        .and_then(|section| section.get("tasks"))
        .and_then(serde_yaml::Value::as_mapping)
    else {
        return;
    };
    for (key, entry) in tasks {
        let Some(identity) = entry
            .get(crate::metadata::BUDGET_TICKET_ID_KEY)
            .and_then(serde_yaml::Value::as_str)
        else {
            continue;
        };
        let holder = retired
            .iter()
            .find(|(_, record)| record.budget_ticket_id.as_deref() == Some(identity));
        if let Some((retired_id, _)) = holder {
            let live = match key {
                serde_yaml::Value::String(text) => text.clone(),
                other => serde_yaml::to_string(other).unwrap_or_default().trim().to_string(),
            };
            report.errors.push(format!(
                "Task {live} claims budgetTicketId {identity}, which belongs to retired Task \
                 {retired_id} (metadata.retiredTickets); a retired budget identity is never \
                 rebound — delete the key and let the next run bind a fresh one"
            ));
        }
    }
}
