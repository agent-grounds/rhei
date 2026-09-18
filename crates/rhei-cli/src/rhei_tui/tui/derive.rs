//! Pure derivations over the run model: the inspector's navigable sections, the
//! machine's disjoint-workflow grouping, and cost rollups — shared by input and
//! rendering so focus navigates exactly where it is drawn. §FS-rhei-run-tui.1.5

use crate::rhei_viz_model::{Machine, TaskRow, VizModel};

use super::state::{UiState, UsageRecord};

/// What following an inspector chip does (§FS-rhei-run-tui.1.5.2): select a
/// neighbor task, or mark a target state in the Machine view.
#[derive(Clone)]
pub(super) enum ChipAction {
    SelectTask(String),
    MarkState(String),
    None,
}

#[derive(Clone)]
pub(super) struct Chip {
    pub(super) label: String,
    pub(super) action: ChipAction,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum InspectorSectionKind {
    Dependencies,
    PreviousStates,
    NextState,
    Prompt,
    LiveAgent,
    Artifacts,
    Children,
}

#[derive(Clone)]
pub(super) struct InspectorSection {
    pub(super) kind: InspectorSectionKind,
    pub(super) title: String,
    pub(super) items: Vec<Chip>,
}

/// The navigable sections of the selected task's surroundings inspector. Header
/// order follows §FS-rhei-viz.4; each section's item order matches rendering.
pub(super) fn inspector_sections(state: &UiState, task_id: &str) -> Vec<InspectorSection> {
    let plan = &state.plan;
    let Some(task) = state.task(task_id) else {
        return Vec::new();
    };
    let mut sections = Vec::new();

    let mut dependency_items = Vec::new();
    for prior in &task.prior {
        dependency_items.push(Chip {
            label: format!("◂ {prior}"),
            action: ChipAction::SelectTask(prior.clone()),
        });
    }
    for other in &plan.tasks {
        if other.prior.iter().any(|p| p == task_id) {
            dependency_items.push(Chip {
                label: format!("▸ {}", other.id),
                action: ChipAction::SelectTask(other.id.clone()),
            });
        }
    }
    sections.push(InspectorSection {
        kind: InspectorSectionKind::Dependencies,
        title: "depends on / unblocks".to_string(),
        items: dependency_items,
    });

    sections.push(InspectorSection {
        kind: InspectorSectionKind::PreviousStates,
        title: "state history".to_string(),
        items: previous_state_names(task)
            .into_iter()
            .map(|state| {
                // §FS-rhei-viz.4: audit text belongs to the label, never the state identity.
                let reason = task
                    .history
                    .iter()
                    .rev()
                    .find(|entry| entry.from == state)
                    .and_then(|entry| entry.forced_reason.as_deref());
                let label = reason
                    .map_or_else(|| state.clone(), |reason| format!("{state} — forced: {reason}"));
                Chip { label, action: ChipAction::MarkState(state) }
            })
            .collect(),
    });

    if let Some(machine_state) = state.machine_state(&task.state) {
        sections.push(InspectorSection {
            kind: InspectorSectionKind::NextState,
            title: "next states".to_string(),
            items: machine_state
                .transitions
                .iter()
                .map(|tr| Chip {
                    label: format!("⮞ {}", tr.to),
                    action: ChipAction::MarkState(tr.to.clone()),
                })
                .collect(),
        });

        if let Some(prompt) = &machine_state.instructions {
            sections.push(InspectorSection {
                kind: InspectorSectionKind::Prompt,
                title: "prompt".to_string(),
                items: prompt
                    .lines()
                    .map(|line| Chip { label: line.to_string(), action: ChipAction::None })
                    .collect(),
            });
        }
    }

    if let Some((_, slot_state)) = state.running_slot(&task.id) {
        let title = if slot_state.agent.is_some() { "live agent" } else { "live program" };
        sections.push(InspectorSection {
            kind: InspectorSectionKind::LiveAgent,
            title: title.to_string(),
            items: Vec::new(),
        });
    }

    let artifacts = artifact_rows(state, task);
    if !artifacts.is_empty() {
        sections.push(InspectorSection {
            kind: InspectorSectionKind::Artifacts,
            title: "artifacts".to_string(),
            items: artifacts
                .iter()
                .map(|row| Chip {
                    label: format!("{} {}", row.marker(), row.name),
                    action: ChipAction::None,
                })
                .collect(),
        });
    }

    let children: Vec<Chip> = state
        .plan
        .tasks
        .iter()
        .filter(|candidate| candidate.parent.as_deref() == Some(task.id.as_str()))
        .map(|child| Chip {
            label: child.id.clone(),
            action: ChipAction::SelectTask(child.id.clone()),
        })
        .collect();
    if !children.is_empty() {
        let title = match subtree_progress(&state.plan, task) {
            Some((done, total)) => format!("children  {done}/{total} ✓"),
            None => "children".to_string(),
        };
        sections.push(InspectorSection {
            kind: InspectorSectionKind::Children,
            title,
            items: children,
        });
    }

    sections
}

/// One inspector artifact row: a declared contract of the current state, or —
/// via the §FS-rhei-viz.4 fallback — an output borrowed from the previous
/// state, which then names that state in `from_state`.
#[derive(Clone)]
pub(super) struct ArtifactRow {
    pub(super) input: bool,
    pub(super) name: String,
    pub(super) path: String,
    pub(super) optional: bool,
    pub(super) from_state: Option<String>,
}

impl ArtifactRow {
    pub(super) fn marker(&self) -> &'static str {
        if self.input {
            "in ◂"
        } else {
            "out ▸"
        }
    }
}

/// The artifact rows for a task: the current state's contracts, or — when the
/// state declares none — the previous state's outputs, labeled, so a parked
/// node still shows the report whose transition parked it. §FS-rhei-viz.4
pub(super) fn artifact_rows(state: &UiState, task: &TaskRow) -> Vec<ArtifactRow> {
    let Some(current) = state.machine_state(&task.state) else {
        return Vec::new();
    };
    if !current.inputs.is_empty() || !current.outputs.is_empty() {
        let inputs = current.inputs.iter().map(|artifact| ArtifactRow {
            input: true,
            name: artifact.name.clone(),
            path: artifact.path.clone(),
            optional: artifact.optional,
            from_state: None,
        });
        let outputs = current.outputs.iter().map(|artifact| ArtifactRow {
            input: false,
            name: artifact.name.clone(),
            path: artifact.path.clone(),
            optional: artifact.optional,
            from_state: None,
        });
        return inputs.chain(outputs).collect();
    }
    let Some(previous) = previous_state_names(task).into_iter().next() else {
        return Vec::new();
    };
    let Some(previous_state) = state.machine_state(&previous) else {
        return Vec::new();
    };
    previous_state
        .outputs
        .iter()
        .map(|artifact| ArtifactRow {
            input: false,
            name: artifact.name.clone(),
            path: artifact.path.clone(),
            optional: artifact.optional,
            from_state: Some(previous.clone()),
        })
        .collect()
}

fn previous_state_names(task: &TaskRow) -> Vec<String> {
    task.history
        .iter()
        .rev()
        .filter_map(|entry| {
            let from = entry.from.trim();
            if from.is_empty() {
                None
            } else {
                Some(from.to_string())
            }
        })
        .take(3)
        .collect()
}

/// Group machine states into disjoint workflows: connected components over
/// explicit (non-wildcard) transitions, ordered by declaration. Isolated states
/// and wildcard-terminal states fold into a trailing group. §FS-rhei-run-tui.1.5.4
pub(super) fn machine_groups(machine: &Machine) -> Vec<Vec<usize>> {
    let n = machine.states.len();
    if n == 0 {
        return Vec::new();
    }
    let index_of = |name: &str| machine.states.iter().position(|s| s.name == name);

    // Union-find over explicit edges.
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    let union = |parent: &mut Vec<usize>, a: usize, b: usize| {
        let ra = find(parent, a);
        let rb = find(parent, b);
        if ra != rb {
            parent[ra] = rb;
        }
    };
    let mut connected = vec![false; n];
    for (i, st) in machine.states.iter().enumerate() {
        for tr in &st.transitions {
            if tr.wildcard {
                continue;
            }
            if let Some(j) = index_of(&tr.to) {
                union(&mut parent, i, j);
                connected[i] = true;
                connected[j] = true;
            }
        }
    }

    // Multi-state components first, in declaration order of their first member.
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut seen_root: Vec<Option<usize>> = vec![None; n];
    let connected_snapshot = connected.clone();
    for (i, is_connected) in connected_snapshot.iter().enumerate() {
        if !is_connected {
            continue;
        }
        let root = find(&mut parent, i);
        match seen_root[root] {
            Some(g) => groups[g].push(i),
            None => {
                seen_root[root] = Some(groups.len());
                groups.push(vec![i]);
            }
        }
    }
    groups.retain(|g| g.len() > 1);

    // Trailing group: isolated + wildcard-only-terminal states.
    let mut trailing: Vec<usize> =
        (0..n).filter(|i| !groups.iter().any(|g| g.contains(i))).collect();
    trailing.sort_unstable();
    if !trailing.is_empty() {
        groups.push(trailing);
    }
    groups
}

/// Compact per-key cost rollup for the Cost view.
#[derive(Default, Clone)]
pub(super) struct CostRollup {
    pub(super) cost_micro: Option<u64>,
    pub(super) total_tokens: u64,
    pub(super) input_tokens: u64,
    pub(super) input_cached_read_tokens: u64,
    pub(super) output_tokens: u64,
    pub(super) invocations: u64,
}

impl CostRollup {
    pub(super) fn add(&mut self, usage: &crate::rhei_tui::event::UsageSummary) {
        self.invocations += 1;
        if let Some(c) = usage.cost_micro.or(usage.priced_cost_micro) {
            self.cost_micro = Some(self.cost_micro.unwrap_or(0) + c);
        }
        self.total_tokens += usage.total.value.unwrap_or(0);
        self.input_tokens += usage.input_total.value.unwrap_or(0);
        self.input_cached_read_tokens += usage.input_cached_read.value.unwrap_or(0);
        self.output_tokens += usage.output_total.value.unwrap_or(0);
    }
}

/// Run-level rollup across every recorded invocation.
pub(super) fn run_rollup(invocations: &[UsageRecord]) -> CostRollup {
    let mut roll = CostRollup::default();
    for rec in invocations {
        roll.add(&rec.usage);
    }
    roll
}

/// Direct cost of one task: invocations recorded against that exact id.
pub(super) fn task_direct(invocations: &[UsageRecord], task_id: &str) -> CostRollup {
    let mut roll = CostRollup::default();
    for rec in invocations.iter().filter(|r| r.task == task_id) {
        roll.add(&rec.usage);
    }
    roll
}

/// Subtree cost: the task plus every id-descendant (`<id>.` prefix).
pub(super) fn task_subtree(
    plan: &VizModel,
    invocations: &[UsageRecord],
    task_id: &str,
) -> CostRollup {
    let prefix = format!("{task_id}.");
    let ids: Vec<&str> = plan
        .tasks
        .iter()
        .filter(|t| t.id == task_id || t.id.starts_with(&prefix))
        .map(|t| t.id.as_str())
        .collect();
    let mut roll = CostRollup::default();
    for rec in invocations.iter().filter(|r| ids.contains(&r.task.as_str())) {
        roll.add(&rec.usage);
    }
    roll
}

/// Whether a task has any descendant.
pub(super) fn has_children(plan: &VizModel, task: &TaskRow) -> bool {
    let prefix = format!("{}.", task.id);
    plan.tasks.iter().any(|t| t.id.starts_with(&prefix))
}

/// `done/total` over a task's descendants (completed leaves vs all). Returns
/// `None` for a leaf.
pub(super) fn subtree_progress(plan: &VizModel, task: &TaskRow) -> Option<(usize, usize)> {
    let prefix = format!("{}.", task.id);
    let descendants: Vec<&TaskRow> =
        plan.tasks.iter().filter(|t| t.id.starts_with(&prefix)).collect();
    if descendants.is_empty() {
        return None;
    }
    let done = descendants.iter().filter(|t| t.state == "completed").count();
    Some((done, descendants.len()))
}
