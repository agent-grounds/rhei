// Admission errors are ticket halts, never completed invocation outcomes.
// §FS-rhei-budgets.9 §FS-rhei-budgets.10

fn budget_emit_admission_halt(
    input: &Path,
    task: &rhei_core::ast::Task,
    error: &miette::Report,
    sink: &Arc<dyn rhei_tui::EventSink>,
) {
    let project = BudgetProject::resolve(input).ok();
    let identity = project.as_ref().and_then(|p| p.required_identity().ok());
    let budget = project.as_ref().zip(identity.as_ref()).and_then(|(p, id)| {
        rhei_core::budget::Journal::open(&p.root, id, false).ok()?.snapshot().ok()
    });
    let ticket_identity = identity
        .as_ref()
        .and_then(|id| budget_ticket_identity(&load_plan(input).ok()?, &task.id, id).ok());
    let reason_code = error
        .downcast_ref::<BudgetDiagnostic>()
        .map(|d| d.0.reason_code.clone())
        .unwrap_or_else(|| "missing_qualification".into());
    sink.emit(rhei_tui::RunEvent::Budget {
        event: Box::new(rhei_core::budget::BudgetEvent::BudgetHalt {
            project_id: identity.map(|id| format!("panta:{id}")),
            task_id: Some(task.id.to_string()),
            ticket_identity,
            reason_code,
            message: error.to_string(),
            budget,
            dry_run: false,
        }),
    });
}

fn budget_refusal(reason: &str, message: &str) -> miette::Report {
    budget_error(rhei_core::budget::BudgetError {
        reason_code: reason.into(),
        message: message.into(),
    })
}
