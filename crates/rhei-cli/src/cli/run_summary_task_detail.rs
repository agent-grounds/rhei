// Per-task activity and the shared tree detail column. §FS-rhei-run-report.3.2

/// Per-task activity accumulated from the run event stream. The tree shows the
/// driver and timing of the work that advanced each task. §FS-rhei-run-report.3.2
#[derive(Debug, Clone, Default)]
struct TaskActivity {
    /// Agent invocations in this run (fan-out targets count separately).
    /// §FS-rhei-run-report.3.2
    agent_invocations: u32,
    /// Program invocations in this run. §FS-rhei-run-report.3.2
    program_invocations: u32,
    /// Duration of the last invocation, milliseconds.
    last_duration_ms: u64,
    /// Direct accounting for usage reported against this task during the run.
    accounting: Option<rhei_tui::AccountingRunSummary>,
    /// Required artifacts the last worker left unwritten, rendered as
    /// `name (path)`, paired with the state it left them in. The halt
    /// classification uses them only while the ticket is still in that state,
    /// and a fresh spawn clears them, so an old stall never explains a new one.
    // §FS-rhei-run-report.3.1
    missing_outputs: Option<(String, Vec<String>)>,
    /// The code the task's last released invocation exited with, `None` before
    /// one has been released or when the run has no code for it. Paired with
    /// `missing_outputs` by the same spawn, which clears both, so a halt row
    /// never names one attempt's artifacts beside another's exit.
    // §FS-rhei-run-report.3.1 §FS-rhei-programs.3.2
    last_exit_code: Option<i32>,
}

/// Build the detail column for a task row: driver + timing when the run spawned
/// work, otherwise a short reason for halted tasks. §FS-rhei-run-report.3.2
fn task_detail(
    id: &str,
    state: &str,
    marker: Marker,
    halt_causes: &HashMap<String, HaltCause>,
    activity: &HashMap<String, TaskActivity>,
) -> Option<String> {
    if let Some(act) = activity.get(id) {
        let cost = act
            .accounting
            .as_ref()
            .map(|accounting| format!(" · {}", format_summary_cost(accounting)))
            .unwrap_or_default();
        // Order kinds consistently and attach each count to its kind. §FS-rhei-run-report.3.2
        let label = [("agent", act.agent_invocations), ("program", act.program_invocations)]
            .into_iter()
            .filter(|(_, count)| *count > 0)
            .map(|(driver, count)| {
                if count > 1 { format!("{driver}×{count}") } else { driver.to_string() }
            })
            .collect::<Vec<_>>()
            .join("+");
        if !label.is_empty() {
            return Some(format!(
                "{label}  {}{}",
                format_duration_short(act.last_duration_ms),
                cost
            ));
        }
        if !cost.is_empty() {
            return Some(cost.trim_start_matches(" · ").to_string());
        }
    }
    match marker {
        Marker::Gate | Marker::Attention => {
            Some(attention_reason(marker, id, state, halt_causes).0)
        }
        _ => None,
    }
}
