// The preflight halt is part of a run, including one that starts no worker.
// §FS-rhei-budgets.10

#[allow(clippy::too_many_arguments)]
fn report_budget_preflight(
    input: &Path,
    loaded: &LoadedPlan,
    machines: &ExecutionMachines,
    settings: &RheiSettings,
    scope: &RheiScope,
    opts: &RunOptions,
    identity: &RunIdentity,
) -> MietteResult<()> {
    use rhei_core::budget::BudgetEvent;
    let mut events = Vec::new();
    let result = budget_preflight(
        input,
        loaded,
        &machines.set,
        settings,
        scope,
        &mut events,
        opts.dry_run(),
    );
    if result.is_ok() {
        return result;
    }
    if !events.iter().any(|event| matches!(event, BudgetEvent::BudgetHalt { .. })) {
        let id = BudgetProject::resolve(input).ok().and_then(|p| p.identity().ok().flatten());
        let error = result.as_ref().expect_err("refusal");
        let reason_code = error
            .downcast_ref::<BudgetDiagnostic>()
            .map(|d| d.0.reason_code.clone())
            .unwrap_or_else(|| {
                if id.is_some() { "untrustworthy_ledger" } else { "missing_bound" }.into()
            });
        events.push(BudgetEvent::BudgetHalt {
            reason_code,
            project_id: id.map(|id| format!("panta:{id}")),
            task_id: None,
            ticket_identity: None,
            message: result.as_ref().expect_err("refusal").to_string(),
            budget: events.iter().rev().find_map(|event| event.snapshot().cloned()),
            dry_run: opts.dry_run(),
        });
    }
    let workspace_root = run_execution_root(input);
    if opts.dry_run() {
        let sink = dry_run_sink(&workspace_root, opts);
        for event in events {
            sink.emit(rhei_tui::RunEvent::Budget { event: Box::new(event) });
        }
        return result;
    }
    let mut report = RunReportGuard {
        input,
        machines: machines.set.clone(),
        runtime_dir: workspace_root.join("runtime"),
        run_started: identity.started,
        run_started_wall: identity.started_wall,
        run_id: identity.id.clone(),
        workspace_root: workspace_root.clone(),
        command: current_command_line(),
        parallel: opts.parallel(),
        mode: "admission",
        initial_states: collect_initial_states(&loaded.rhei, &machines.set),
        dry_run: false,
        summary: None,
        armed: true,
    };
    let shutdown = RunShutdown::default();
    let frontend = start_run_frontend(
        &workspace_root,
        input,
        machines,
        opts,
        opts.parallel().max(1).min(u16::MAX as usize) as u16,
        total_task_count(&loaded.rhei),
        &shutdown,
        identity,
    );
    // Error teardown restores a TUI without parking on its finished screen.
    let _guard = RunSubprocessGuard::install(shutdown);
    report.summary = Some(frontend.summary.clone());
    frontend.sink.emit(rhei_tui::RunEvent::RunStarted {
        run_id: identity.id.clone(),
        workspace: workspace_root,
        parallel: opts.parallel().max(1).min(u16::MAX as usize) as u16,
        total_tasks: total_task_count(&loaded.rhei),
    });
    for event in events {
        frontend.sink.emit(rhei_tui::RunEvent::Budget { event: Box::new(event) });
    }
    frontend.sink.emit(rhei_tui::RunEvent::RunFinished {
        summary: rhei_tui::RunSummary {
            agents_spawned: 0,
            programs_spawned: 0,
            terminal_tasks: terminal_task_count(&loaded.rhei, &machines.set),
            total_tasks: total_task_count(&loaded.rhei),
            accounting: None,
            workspace_accounting: None,
        },
    });
    frontend.write_frozen_dashboard();
    result
}

impl SummarySink {
    fn budget_events(&self) -> Vec<rhei_core::budget::BudgetEvent> {
        self.inner.lock().map(|state| state.budget_events.clone()).unwrap_or_default()
    }
}

impl RunSummaryReport {
    fn apply_budget_halts(&mut self) {
        use rhei_core::budget::BudgetEvent;
        for event in &self.budget_events {
            let BudgetEvent::BudgetHalt { task_id, reason_code, message, dry_run, .. } = event
            else {
                continue;
            };
            self.result = if *dry_run {
                "budget admission preview refused"
            } else {
                "budget admission halted"
            }
            .into();
            let Some(task) = task_id else { continue };
            let Some(row) = self.rows.iter_mut().find(|row| &row.id == task) else { continue };
            row.detail = Some(format!("{reason_code}: {message}"));
            self.attention.retain(|row| &row.id != task);
            self.waiting.retain(|row| &row.id != task);
            self.attention.push(AttentionRow {
                id: task.clone(),
                state: row.state.clone(),
                reason: format!("{reason_code}: {message}"),
                next: "inspect `rhei budget show`; repair the named admission bound".into(),
                is_gate: false,
                waits_on_person: false,
                provider_limited: false,
            });
            for edge in self.ledger.iter_mut().filter(|edge| &edge.task == task) {
                edge.reason = format!("budget halt: {reason_code}: {message}");
            }
        }
    }

    fn budget_markdown(&self) -> String {
        if self.budget_events.is_empty() {
            return String::new();
        }
        let mut text = String::from("## Panta lifetime budget\n\n");
        for event in &self.budget_events {
            text.push_str(&event.message().replace('`', "\\`").replace('<', "&lt;"));
            text.push_str("\n\n");
        }
        // Retain exact snapshots and receipts in the report's immutable copy.
        // JSON escaping keeps authored strings outside Markdown structure.
        // §FS-rhei-budgets.10
        let json =
            serde_json::to_string_pretty(&self.budget_events).expect("budget events serialize");
        let json = json.replace('<', "\\u003c").replace('`', "\\u0060");
        text.push_str("<details><summary>Budget snapshots and receipts</summary>\n\n```json\n");
        text.push_str(&json);
        text.push_str("\n```\n\n</details>\n\n");
        text
    }
}
