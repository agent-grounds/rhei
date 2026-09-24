use std::collections::HashSet;
use std::path::PathBuf;

use serde::Serialize;

use crate::rhei_tui::event::{
    summarize_usage_summaries, AccountingRunSummary, AgentStream, MessageLevel, RunEvent,
    RunSummary, Slot, TaskOutcome, UsageSummary,
};

use super::{now_ms, system_time_ms, RECENT_LIMIT, SLOT_TRAFFIC_LIMIT};

#[derive(Clone, Serialize)]
pub(super) struct DashboardState {
    pub(super) workspace: String,
    pub(super) parallel: u16,
    pub(super) total_tasks: usize,
    pub(super) pass: u32,
    pub(super) ready: Vec<String>,
    /// Task ids deferred during the *current* pass. Cleared on `PassStarted`.
    pub(super) deferred: Vec<String>,
    pub(super) slots: Vec<DashboardSlot>,
    pub(super) recent: Vec<JournalLine>,
    pub(super) links: Vec<DashboardLink>,
    pub(super) accounting: Option<AccountingRunSummary>,
    #[serde(skip)]
    pub(super) invocations: Vec<DashboardUsageRecord>,
    pub(super) finished: bool,
    pub(super) summary: Option<DashboardSummary>,
    pub(super) started_at_ms: u128,
    pub(super) updated_at_ms: u128,
}

#[derive(Clone, Serialize)]
pub(super) struct JournalLine {
    pub(super) level: &'static str,
    pub(super) text: String,
    pub(super) ts_ms: u128,
}

impl DashboardState {
    pub(super) fn new(workspace: PathBuf, parallel: u16, total_tasks: usize) -> Self {
        let now = now_ms();
        Self {
            workspace: workspace.display().to_string(),
            parallel,
            total_tasks,
            pass: 0,
            ready: Vec::new(),
            deferred: Vec::new(),
            slots: vec![DashboardSlot::default(); parallel as usize],
            recent: Vec::new(),
            links: Vec::new(),
            accounting: None,
            invocations: Vec::new(),
            finished: false,
            summary: None,
            started_at_ms: now,
            updated_at_ms: now,
        }
    }

    pub(super) fn apply(&mut self, event: &RunEvent) {
        let now = now_ms();
        self.updated_at_ms = now;
        match event {
            RunEvent::RunStarted { workspace, parallel, total_tasks, .. } => {
                self.workspace = workspace.display().to_string();
                self.parallel = (*parallel).max(1);
                self.total_tasks = *total_tasks;
                self.slots = vec![DashboardSlot::default(); self.parallel as usize];
                self.started_at_ms = now;
                self.push_recent(
                    "info",
                    format!("run started: parallel={} total={}", self.parallel, self.total_tasks),
                );
            }
            RunEvent::PassStarted { pass, ready } => {
                self.pass = *pass;
                self.ready = ready.clone();
                self.deferred.clear();
                self.push_recent("info", format!("pass {pass}: {} ready", ready.len()));
            }
            RunEvent::SlotAssigned {
                slot,
                task,
                from,
                to,
                agent,
                template_context,
                log_path,
                wall_clock,
                ..
            } => {
                let slot_state = self.slot_mut(*slot);
                slot_state.active = true;
                slot_state.task = Some(task.clone());
                slot_state.agent = agent.clone();
                slot_state.template_context = template_context.clone();
                slot_state.state = Some(to.clone());
                // Only record a transition when the worker actually moved
                // states. `from == to` means the engine started a worker in
                // an autonomous state — there was no transition.
                slot_state.transition =
                    if from == to { None } else { Some(format!("{from}->{to}")) };
                slot_state.log_path = Some(log_path.display().to_string());
                slot_state.started_at_ms = Some(system_time_ms(*wall_clock));
                slot_state.finished_at_ms = None;
                slot_state.duration_ms = None;
                slot_state.exit_code = None;
                slot_state.outcome = None;
                slot_state.usage = None;
                slot_state.traffic.clear();
                if from == to {
                    self.push_recent("info", format!("slot {slot}: task {task} started in {to}"));
                } else {
                    self.push_recent("info", format!("slot {slot}: task {task} {from}->{to}"));
                }
            }
            RunEvent::AgentOutput { slot, stream, line, wall_clock, .. } => {
                let slot_state = self.slot_mut(*slot);
                let stream_name = match stream {
                    AgentStream::Stdout => "stdout",
                    AgentStream::Stderr => "stderr",
                };
                let ts = system_time_ms(*wall_clock);
                // Dedup consecutive identical lines: bump a counter on the
                // last entry instead of pushing a duplicate.
                if let Some(last) = slot_state.traffic.last_mut() {
                    if last.stream == stream_name && last.text == *line {
                        last.repeat += 1;
                        last.ts_ms = ts;
                        return;
                    }
                }
                if slot_state.traffic.len() == SLOT_TRAFFIC_LIMIT {
                    slot_state.traffic.remove(0);
                }
                slot_state.traffic.push(DashboardTraffic {
                    stream: stream_name,
                    text: line.clone(),
                    ts_ms: ts,
                    repeat: 1,
                });
            }
            RunEvent::SlotReleased {
                slot,
                task,
                from,
                to,
                outcome,
                wall_clock,
                duration_ms,
                exit_code,
                ..
            } => {
                let slot_state = self.slot_mut(*slot);
                slot_state.active = false;
                slot_state.finished_at_ms = Some(system_time_ms(*wall_clock));
                slot_state.duration_ms = Some(*duration_ms);
                slot_state.exit_code = *exit_code;
                slot_state.outcome = Some(match outcome {
                    TaskOutcome::Completed => "completed".to_string(),
                    TaskOutcome::Failed(reason) => format!("failed: {reason}"),
                    // Carries no reason: the exit that matched the self-loop is
                    // in the cell beside it. §FS-rhei-states.2.2
                    TaskOutcome::Waiting => "waiting".to_string(),
                    TaskOutcome::ProviderLimited { provider, next_attempt_at } => {
                        format!("provider limited: {provider} until {next_attempt_at}")
                    }
                    TaskOutcome::Cancelled => "cancelled".to_string(),
                    TaskOutcome::TimedOut => "timed out".to_string(),
                    TaskOutcome::Interrupted => "interrupted".to_string(),
                    // Shown under the name the stream gave it rather than
                    // rewritten to a word this build knows.
                    // §FS-rhei-run-json.2.2
                    TaskOutcome::Unrecognized(name) => name.clone(),
                });
                if from != to {
                    slot_state.transition = Some(format!("{from}->{to}"));
                }
                let outcome_label = slot_state.outcome.as_deref().unwrap_or("unknown").to_string();
                let where_label =
                    if from == to { format!("in {to}") } else { format!("{from}->{to}") };
                self.push_recent(
                    "info",
                    format!("slot {slot}: task {task} finished {where_label} ({outcome_label})"),
                );
            }
            RunEvent::PassEnded { pass, progressed } => {
                self.push_recent("info", format!("pass {pass} ended: progressed={progressed}"));
            }
            RunEvent::TasksDeferred { pass, tasks } => {
                let mut seen: HashSet<String> = self.deferred.iter().cloned().collect();
                for t in tasks {
                    if seen.insert(t.clone()) {
                        self.deferred.push(t.clone());
                    }
                }
                self.push_recent(
                    "info",
                    format!("pass {pass} deferred {} task(s): {}", tasks.len(), tasks.join(", ")),
                );
            }
            RunEvent::RunFinished { summary } => {
                self.finished = true;
                self.summary = Some(DashboardSummary::from(summary));
                self.push_recent(
                    "info",
                    format!(
                        "run finished: terminal={}/{}",
                        summary.terminal_tasks, summary.total_tasks
                    ),
                );
            }
            RunEvent::Message { level, text } => {
                let level = match level {
                    MessageLevel::Info => "info",
                    MessageLevel::Warn => "warn",
                    MessageLevel::Error => "error",
                };
                self.push_recent(level, text.clone());
            }
            RunEvent::RunLink { label, url } => {
                if !self.links.iter().any(|link| link.url == *url) {
                    self.links.push(DashboardLink {
                        label: label.clone(),
                        url: url.clone(),
                        source: "callback",
                    });
                }
                self.push_recent("info", format!("{label}: {url}"));
            }
            RunEvent::UsageReported { slot, task, invocation_id, usage, .. } => {
                // §FS-rhei-cost-accounting.7: Usage updates task, slot, and run totals.
                // Keyed by invocation id, so both reports upsert the same row.
                let record = DashboardUsageRecord {
                    slot: *slot,
                    task: task.clone(),
                    invocation_id: invocation_id.clone(),
                    usage: usage.clone(),
                };
                if let Some(existing) =
                    self.invocations.iter_mut().find(|entry| entry.invocation_id == *invocation_id)
                {
                    *existing = record;
                } else {
                    self.invocations.push(record);
                }
                self.accounting =
                    summarize_usage_summaries(self.invocations.iter().map(|entry| &entry.usage));
                if let Some(slot) = slot {
                    let slot_state = self.slot_mut(*slot);
                    slot_state.usage = Some(usage.clone());
                }
                self.push_recent(
                    "info",
                    format!("task {task}: usage reported for {}", usage.agent),
                );
            }
            // Data for the run report's halt classification; the operator-facing
            // warning arrives separately as a `Message`, so rendering it here
            // would show the same stall twice. §FS-rhei-run-report.3.1
            RunEvent::TaskOutputsMissing { .. } => {}
            // §FS-rhei-run-tui.1.1 §FS-rhei-budgets.9
            RunEvent::BudgetSnapshot { bounds } => {
                for bound in bounds {
                    self.push_recent("info", crate::rhei_tui::event::bound_journal_line(bound));
                }
            }
            RunEvent::BudgetHalt { task, bound, .. } => {
                self.push_recent("error", format!("{task} halted on {}", bound.dimension));
            }
        }
    }

    fn slot_mut(&mut self, slot: Slot) -> &mut DashboardSlot {
        let idx = slot as usize;
        if idx >= self.slots.len() {
            self.slots.resize_with(idx + 1, DashboardSlot::default);
        }
        &mut self.slots[idx]
    }

    fn push_recent(&mut self, level: &'static str, text: String) {
        if self.recent.len() == RECENT_LIMIT {
            self.recent.remove(0);
        }
        self.recent.push(JournalLine { level, text, ts_ms: now_ms() });
    }
}

#[derive(Clone, Default, Serialize)]
pub(super) struct DashboardSlot {
    pub(super) active: bool,
    pub(super) task: Option<String>,
    pub(super) agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) template_context: Option<crate::rhei_viz_model::TemplateContext>,
    pub(super) state: Option<String>,
    pub(super) transition: Option<String>,
    pub(super) log_path: Option<String>,
    pub(super) started_at_ms: Option<u128>,
    pub(super) finished_at_ms: Option<u128>,
    pub(super) duration_ms: Option<u64>,
    pub(super) exit_code: Option<i32>,
    pub(super) outcome: Option<String>,
    pub(super) traffic: Vec<DashboardTraffic>,
    pub(super) usage: Option<UsageSummary>,
}

#[derive(Clone, Serialize)]
pub(super) struct DashboardUsageRecord {
    pub(super) slot: Option<Slot>,
    pub(super) task: String,
    pub(super) invocation_id: String,
    pub(super) usage: UsageSummary,
}

#[derive(Clone, Serialize)]
pub(super) struct DashboardTraffic {
    pub(super) stream: &'static str,
    pub(super) text: String,
    pub(super) ts_ms: u128,
    pub(super) repeat: u32,
}

#[derive(Clone, Serialize)]
pub(super) struct DashboardLink {
    pub(super) label: String,
    pub(super) url: String,
    /// `"callback"` for links emitted by the run process; `"workspace"` for
    /// the fixed entries the dashboard injects (workspace dir, runtime/logs,
    /// runtime/results). The frontend renders this string as-is in the
    /// source-chip column.
    pub(super) source: &'static str,
}

#[derive(Clone, Serialize)]
pub(super) struct DashboardSummary {
    pub(super) agents_spawned: u32,
    pub(super) programs_spawned: u32,
    pub(super) terminal_tasks: usize,
    pub(super) total_tasks: usize,
    pub(super) accounting: Option<AccountingRunSummary>,
}

impl From<&RunSummary> for DashboardSummary {
    fn from(summary: &RunSummary) -> Self {
        Self {
            agents_spawned: summary.agents_spawned,
            programs_spawned: summary.programs_spawned,
            terminal_tasks: summary.terminal_tasks,
            total_tasks: summary.total_tasks,
            accounting: summary.accounting.clone(),
        }
    }
}

/// Compact per-task accounting rollups carried in the snapshot's `task_runtime`
/// overlay. §FS-rhei-cost-accounting §6, §10.
#[derive(Clone, Serialize)]
pub(crate) struct TaskAccounting {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) direct: Option<AccountingRunSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) subtree: Option<AccountingRunSummary>,
}

pub(super) fn task_accounting_for_tasks(
    tasks: &[crate::rhei_viz_model::TaskRow],
    invocations: &[DashboardUsageRecord],
) -> std::collections::BTreeMap<String, TaskAccounting> {
    // §FS-rhei-cost-accounting.6: Dashboard derives direct and subtree totals.
    let mut direct = std::collections::BTreeMap::<String, Vec<&UsageSummary>>::new();
    for entry in invocations {
        direct.entry(entry.task.clone()).or_default().push(&entry.usage);
    }

    let mut descendants = std::collections::BTreeMap::<String, Vec<String>>::new();
    for task in tasks {
        let id = task.id.as_str();
        for candidate in tasks {
            if candidate.id == task.id {
                continue;
            }
            if candidate.id.starts_with(id) && candidate.id.as_bytes().get(id.len()) == Some(&b'.')
            {
                descendants.entry(task.id.clone()).or_default().push(candidate.id.clone());
            }
        }
    }

    let mut out = std::collections::BTreeMap::new();
    for task in tasks {
        let direct_summary =
            direct.get(&task.id).and_then(|items| summarize_usage_summaries(items.iter().copied()));
        let mut subtree_items: Vec<&UsageSummary> =
            direct.get(&task.id).into_iter().flatten().copied().collect();
        if let Some(children) = descendants.get(&task.id) {
            for child in children {
                if let Some(items) = direct.get(child) {
                    subtree_items.extend(items.iter().copied());
                }
            }
        }
        let subtree_summary = summarize_usage_summaries(subtree_items.into_iter());
        if direct_summary.is_some() || subtree_summary.is_some() {
            out.insert(
                task.id.clone(),
                TaskAccounting { direct: direct_summary, subtree: subtree_summary },
            );
        }
    }
    out
}
