//! Builds the [`VizModel`] from a parsed plan and its resolved machine — the
//! single source of truth for the spec's derivation rules (flattening, plan
//! state, classification), shared by the static and live paths. §AR-rhei-viz-flow.8

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::rhei_validator::{
    parse_execution_target, parse_task_state, StateArtifactDef, StateMachine,
};
use crate::rhei_viz_model::{
    Artifact, Machine, MachineProcessKind, MachineState, StateHistoryEntry, TaskRow,
    TemplateContext, Transition, VizModel,
};
use rhei_core::ast::{Rhei, Task as AstTask};

mod collect;
pub use collect::{collect_plans, Bundle};

/// Narrow every plan in `bundle` to the named rheis, keeping the one-hop
/// neighbours their `**Prior:**` edges point at.
///
/// Pointing `rhei viz` at one rhei of a project asks about that rhei, but a
/// cross-rhei prior is part of what governs it: dropping the far end would
/// erase the dependency rather than scope it. Keeping the referenced ticket
/// draws the edge with something to anchor to. An empty `rhei_ids` leaves the
/// bundle untouched.
// §FS-rhei-viz.7.3: a member rhei renders its project narrowed to itself.
pub fn narrow_bundle(bundle: Bundle, rhei_ids: &[String]) -> Bundle {
    if rhei_ids.is_empty() {
        return bundle;
    }
    let scope: HashSet<&str> = rhei_ids.iter().map(String::as_str).collect();
    fn owner(id: &str) -> &str {
        id.split_once('.').map_or(id, |(head, _)| head)
    }

    bundle
        .into_iter()
        .map(|(key, mut model)| {
            let in_scope: HashSet<String> = model
                .tasks
                .iter()
                .filter(|task| scope.contains(owner(&task.id)))
                .map(|task| task.id.clone())
                .collect();
            if in_scope.is_empty() {
                // Nothing of this plan is in scope; an empty page is a clearer
                // answer than one silently showing everything.
                model.tasks.clear();
                return (key, model);
            }
            let mut keep = in_scope.clone();
            for task in &model.tasks {
                if !in_scope.contains(&task.id) {
                    continue;
                }
                keep.extend(task.prior.iter().cloned());
                // An ancestor gives a kept descendant somewhere to hang.
                let mut parent = task.parent.clone();
                while let Some(id) = parent {
                    parent = model
                        .tasks
                        .iter()
                        .find(|candidate| candidate.id == id)
                        .and_then(|candidate| candidate.parent.clone());
                    keep.insert(id);
                }
            }
            model.tasks.retain(|task| keep.contains(&task.id));
            (key, model)
        })
        .collect()
}

/// Coarse status category a persisted state reduces to (§FS-rhei-viz §1.1). The
/// rows are evaluated top to bottom, first match wins, so `Active` is the
/// catch-all. The live dashboard overlays runtime slot assignment separately.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Done,
    Blocked,
    Failed,
    Gate,
    Retired,
    Idle,
    Active,
}

/// Build the static [`VizModel`] from a parsed plan and its resolved machine.
///
/// `tasks` is flattened to source order — each top-level task followed by its
/// descendants — each carrying its tree `depth` (`0` for a top-level task) and
/// `parent` id. The asset renders the outline and graph from this list.
pub fn build(rhei: &Rhei, machine: &StateMachine) -> VizModel {
    build_inner(rhei, machine, None)
}

/// Build a [`VizModel`] and attach best-effort per-task state history from the
/// central `runtime/state-transitions.log` ledger. §FS-rhei-viz.4
pub fn build_with_history(
    rhei: &Rhei,
    machine: &StateMachine,
    workspace_root: &Path,
) -> std::io::Result<VizModel> {
    let _guards = history_guards(workspace_root, &HashMap::new())?;
    Ok(build_inner(
        rhei,
        machine,
        Some(HistoryRoots { default_root: workspace_root, task_roots: None }),
    ))
}

/// Like [`build_with_history`], but each task's history is read under its
/// owning rhei's execution root — a Panta project keeps runtime ledgers per
/// rhei, not under the project root. §AR-rhei-panta.5
pub fn build_with_history_roots(
    rhei: &Rhei,
    machine: &StateMachine,
    default_root: &Path,
    task_roots: &HashMap<String, PathBuf>,
) -> std::io::Result<VizModel> {
    let _guards = history_guards(default_root, task_roots)?;
    Ok(build_inner(
        rhei,
        machine,
        Some(HistoryRoots { default_root, task_roots: Some(task_roots) }),
    ))
}

/// Like [`build_with_history_roots`], for a project whose rheis run their own
/// machines: each subtree classifies under its owning rhei's machine, and the
/// legend is the union of every distinct machine. §DA-per-rhei-state-machines
pub fn build_set_with_history_roots(
    rhei: &Rhei,
    machines: &crate::rhei_validator::MachineSet,
    default_root: &Path,
    task_roots: &HashMap<String, PathBuf>,
) -> std::io::Result<VizModel> {
    let _guards = history_guards(default_root, task_roots)?;
    let roots = Some(HistoryRoots { default_root, task_roots: Some(task_roots) });
    let mut tasks = Vec::new();
    for task in &rhei.tasks {
        // A subtree lives inside one rhei; the top task's machine covers it.
        collect_task(task, 0, None, machines.for_task(&task.id), roots, &mut tasks);
    }
    let plan_state = derive_plan_state_set(&tasks, machines);
    let about = rhei
        .content_sections
        .iter()
        .find(|s| s.title.eq_ignore_ascii_case("overview"))
        .or_else(|| rhei.content_sections.first())
        .map(|s| s.content.trim().to_string())
        .filter(|s| !s.is_empty());
    Ok(VizModel {
        plan_title: Some(rhei.title.clone()),
        plan_state: Some(plan_state),
        about,
        tasks,
        machine: flatten_machine_union(machines),
    })
}

/// Presentation legend for a heterogeneous project: every distinct machine's
/// states, first definition of a name winning. Per-ticket semantics must use
/// [`MachineSet::for_task`], never this union. §DA-per-rhei-state-machines
pub fn flatten_machine_union(machines: &crate::rhei_validator::MachineSet) -> Machine {
    let distinct = machines.distinct();
    let mut union = flatten_machine(distinct[0]);
    for machine in distinct.iter().skip(1) {
        let flattened = flatten_machine(machine);
        for state in flattened.states {
            if !union.states.iter().any(|existing| existing.name == state.name) {
                union.states.push(state);
            }
        }
    }
    union.name =
        distinct.iter().map(|machine| machine.name.as_str()).collect::<Vec<_>>().join(" + ");
    union
}

/// [`derive_plan_state`] with each top-level task judged under its owning
/// rhei's machine. §DA-per-rhei-state-machines
pub fn derive_plan_state_set(
    tasks: &[TaskRow],
    machines: &crate::rhei_validator::MachineSet,
) -> String {
    let roots: Vec<&TaskRow> = tasks.iter().filter(|t| t.depth == 0).collect();
    if roots.is_empty() {
        return "draft".into();
    }
    if roots.iter().all(|t| t.state == "draft") {
        return "draft".into();
    }
    if roots.iter().all(|t| t.state == "completed") {
        return "completed".into();
    }
    let is_terminal = |row: &TaskRow| {
        machines
            .for_task_str(&row.id)
            .states
            .get(&row.state)
            .map(|def| def.terminal)
            .unwrap_or(false)
    };
    if roots.iter().all(|t| is_terminal(t)) {
        return "archived".into();
    }
    // active-like = a non-terminal state that is not in the `idle` category.
    let any_active_like =
        roots.iter().any(|t| category(machines.for_task_str(&t.id), &t.state) == Category::Active);
    if any_active_like {
        "active".into()
    } else {
        "pending".into()
    }
}

/// Where each task's runtime history is read from: its owning rhei's
/// execution root when known, the plan's own root otherwise.
#[derive(Clone, Copy)]
struct HistoryRoots<'a> {
    default_root: &'a Path,
    task_roots: Option<&'a HashMap<String, PathBuf>>,
}

impl HistoryRoots<'_> {
    fn root_for(&self, task_id: &str) -> &Path {
        self.task_roots
            .and_then(|roots| roots.get(task_id))
            .map(PathBuf::as_path)
            .unwrap_or(self.default_root)
    }
}

fn build_inner(rhei: &Rhei, machine: &StateMachine, roots: Option<HistoryRoots<'_>>) -> VizModel {
    let mut tasks = Vec::new();
    for task in &rhei.tasks {
        collect_task(task, 0, None, machine, roots, &mut tasks);
    }
    let plan_state = derive_plan_state(&tasks, machine);
    let about = rhei
        .content_sections
        .iter()
        .find(|s| s.title.eq_ignore_ascii_case("overview"))
        .or_else(|| rhei.content_sections.first())
        .map(|s| s.content.trim().to_string())
        .filter(|s| !s.is_empty());
    VizModel {
        plan_title: Some(rhei.title.clone()),
        plan_state: Some(plan_state),
        about,
        tasks,
        machine: flatten_machine(machine),
    }
}

fn collect_task(
    task: &AstTask,
    depth: u8,
    parent: Option<String>,
    machine: &StateMachine,
    roots: Option<HistoryRoots<'_>>,
    out: &mut Vec<TaskRow>,
) {
    let id = task.id.to_string();
    let parsed = parse_task_state(&task.state, machine);
    let history = roots
        .map(|roots| load_task_history(roots.root_for(&id), &id, rhei_local_task_id(task)))
        .unwrap_or_default();
    out.push(TaskRow {
        id: id.clone(),
        title: task.title.clone(),
        parent,
        depth,
        state: parsed.state,
        visit_count: parsed.visit,
        prior: task.prior.iter().map(ToString::to_string).collect(),
        history,
    });
    for child in &task.children {
        collect_task(child, depth + 1, Some(id.clone()), machine, roots, out);
    }
}

/// The ticket id as it reads inside its own rhei file: the project-qualified
/// id with the rhei-prefix segments removed. `None` when the task carries no
/// prefix (already rhei-local). §AR-rhei-panta.3
fn rhei_local_task_id(task: &AstTask) -> Option<String> {
    let prefix = task.profile_depth_offset as usize;
    if prefix == 0 || task.id.segments.len() <= prefix {
        return None;
    }
    Some(
        task.id.segments[prefix..]
            .iter()
            .map(|segment| segment.to_string())
            .collect::<Vec<_>>()
            .join("."),
    )
}

include!("history.rs");

include!("machine.rs");

#[cfg(test)]
mod tests;
