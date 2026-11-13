// Weighing one pool work item against the fourth bound, the last question before
// a spawn is staged.
//
// Its own part because admission only decides and reports: it stages nothing and
// starts nothing, which is what the spawn next door goes on to do.

// §AR-source-file-size.3 §FS-rhei-run.3.4

/// Whether the bound admits this arm, reported either way: a worker pool that
/// skipped it would be the exception that makes the count untrue. One arm at a
/// time, which is the stated residual. A refusal is the pool's stall: `None`.
/// An admission returns the arm it admitted, `Some(None)` where nothing is
/// accounted. §FS-rhei-run.3.4 §FS-rhei-budgets.6.1 §FS-rhei-budgets.4.2 §FS-rhei-budgets.6.2.1
#[allow(clippy::too_many_arguments)]
fn admit_parallel_agent_spawn(
    item: &AgentWorkItem,
    input: &Path,
    loaded: &LoadedPlan,
    workspace_root: &Path,
    machine: &rhei_validator::StateMachine,
    settings: &RheiSettings,
    task: &rhei_core::ast::Task,
    opts: &RunOptions,
    budget_run: &BudgetRun,
    sink: &Arc<dyn rhei_tui::EventSink>,
) -> MietteResult<Option<Option<String>>> {
    match budget_admit_spawn(
        input,
        loaded,
        workspace_root,
        machine,
        settings,
        task,
        &item.task_id_str,
        &item.current_state,
        &opts.price_book().currency,
        budget_run,
    )? {
        // §FS-rhei-budgets.9
        BudgetAdmission::Admitted { bounds, note, arm } => {
            sink.emit(rhei_tui::RunEvent::BudgetSnapshot { bounds });
            // Nothing is wrong, so it is a note and not a warning.
            // §FS-rhei-budgets.7.2
            if let Some(note) = note {
                emit_run_message(sink, rhei_tui::MessageLevel::Info, note);
            }
            Ok(Some(arm))
        }
        BudgetAdmission::NotAccounted => Ok(Some(None)),
        BudgetAdmission::Refused { halt, event, bounds } => {
            // The same current facts on a refused pool admission. §FS-rhei-budgets.9
            if !bounds.is_empty() {
                sink.emit(rhei_tui::RunEvent::BudgetSnapshot { bounds });
            }
            emit_run_message(sink, rhei_tui::MessageLevel::Error, halt);
            if let Some(event) = event {
                sink.emit(*event);
            }
            Ok(None)
        }
    }
}
