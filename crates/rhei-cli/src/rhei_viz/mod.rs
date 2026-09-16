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
pub fn build_with_history(rhei: &Rhei, machine: &StateMachine, workspace_root: &Path) -> VizModel {
    build_inner(
        rhei,
        machine,
        Some(HistoryRoots { default_root: workspace_root, task_roots: None }),
    )
}

/// Like [`build_with_history`], but each task's history is read under its
/// owning rhei's execution root — a Panta project keeps runtime ledgers per
/// rhei, not under the project root. §AR-rhei-panta.5
pub fn build_with_history_roots(
    rhei: &Rhei,
    machine: &StateMachine,
    default_root: &Path,
    task_roots: &HashMap<String, PathBuf>,
) -> VizModel {
    build_inner(rhei, machine, Some(HistoryRoots { default_root, task_roots: Some(task_roots) }))
}

/// Like [`build_with_history_roots`], for a project whose rheis run their own
/// machines: each subtree classifies under its owning rhei's machine, and the
/// legend is the union of every distinct machine. §DA-per-rhei-state-machines
pub fn build_set_with_history_roots(
    rhei: &Rhei,
    machines: &crate::rhei_validator::MachineSet,
    default_root: &Path,
    task_roots: &HashMap<String, PathBuf>,
) -> VizModel {
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
    VizModel {
        plan_title: Some(rhei.title.clone()),
        plan_state: Some(plan_state),
        about,
        tasks,
        machine: flatten_machine_union(machines),
    }
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

/// Load a task's history under its qualified id, falling back to the
/// rhei-local id: pre-qualification runtime records are keyed locally, and
/// an upgrade must not orphan an executed plan's history. §AR-rhei-panta.2
fn load_task_history(
    workspace_root: &Path,
    task_id: &str,
    legacy_id: Option<String>,
) -> Vec<StateHistoryEntry> {
    let history = load_task_history_for_id(workspace_root, task_id);
    if !history.is_empty() {
        return history;
    }
    legacy_id.map(|id| load_task_history_for_id(workspace_root, &id)).unwrap_or_default()
}

fn load_task_history_for_id(workspace_root: &Path, task_id: &str) -> Vec<StateHistoryEntry> {
    let central_history = load_central_transition_history(workspace_root, task_id);
    let journal_history = load_task_journal_history(workspace_root, task_id);
    if !central_history.is_empty() {
        if journal_history.is_empty() {
            return central_history;
        }
        return merge_history_sources(vec![journal_history, central_history]);
    }
    let path = workspace_root.join("runtime").join("results").join(format!("{task_id}.md"));
    let result_history =
        fs::read_to_string(path).map(|raw| parse_result_history(&raw)).unwrap_or_default();
    merge_history_sources(vec![journal_history, result_history])
}

fn load_central_transition_history(workspace_root: &Path, task_id: &str) -> Vec<StateHistoryEntry> {
    let path = workspace_root.join("runtime").join("state-transitions.log");
    let Ok(raw) = fs::read_to_string(path) else {
        return Vec::new();
    };
    parse_central_transition_history(&raw, task_id)
}

fn parse_central_transition_history(raw: &str, task_id: &str) -> Vec<StateHistoryEntry> {
    raw.lines()
        .filter_map(|line| {
            let (id, movement) = line.trim().split_once(char::is_whitespace)?;
            if id != task_id {
                return None;
            }
            let (from, to) = movement.trim().split_once('@')?;
            let from = from.trim();
            let to = to.trim();
            if from.is_empty() || to.is_empty() {
                return None;
            }
            Some(StateHistoryEntry { from: from.to_string(), to: to.to_string() })
        })
        .collect()
}

fn parse_result_history(raw: &str) -> Vec<StateHistoryEntry> {
    raw.lines()
        .filter_map(|line| {
            let heading = line.trim().strip_prefix("## ")?;
            let (from, to) = heading.split_once('\u{2192}').or_else(|| heading.split_once("->"))?;
            let from = from.trim();
            let to = to.trim();
            if from.is_empty() || to.is_empty() {
                return None;
            }
            Some(StateHistoryEntry { from: from.to_string(), to: to.to_string() })
        })
        .collect()
}

fn load_task_journal_history(workspace_root: &Path, task_id: &str) -> Vec<StateHistoryEntry> {
    let path = workspace_root.join("runtime").join("transitions.log");
    let Ok(raw) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let states = parse_journal_state_path(&raw, task_id);
    states
        .windows(2)
        .filter_map(|pair| match pair {
            [from, to] if from != to => {
                Some(StateHistoryEntry { from: from.clone(), to: to.clone() })
            }
            _ => None,
        })
        .collect()
}

fn merge_history_sources(sources: Vec<Vec<StateHistoryEntry>>) -> Vec<StateHistoryEntry> {
    let mut states = Vec::new();
    for source in sources {
        let source_states = history_to_states(&source);
        append_state_path(&mut states, &source_states);
    }
    states_to_history(states)
}

fn history_to_states(history: &[StateHistoryEntry]) -> Vec<String> {
    let mut states = Vec::new();
    for entry in history {
        push_history_state(&mut states, &entry.from);
        push_history_state(&mut states, &entry.to);
    }
    states
}

fn append_state_path(states: &mut Vec<String>, next: &[String]) {
    if next.is_empty() {
        return;
    }
    let max_overlap = states.len().min(next.len());
    let overlap = (1..=max_overlap)
        .rev()
        .find(|&len| states[states.len() - len..] == next[..len])
        .unwrap_or(0);
    for state in &next[overlap..] {
        push_history_state(states, state);
    }
}

fn states_to_history(states: Vec<String>) -> Vec<StateHistoryEntry> {
    states
        .windows(2)
        .filter_map(|pair| match pair {
            [from, to] if from != to => {
                Some(StateHistoryEntry { from: from.clone(), to: to.clone() })
            }
            _ => None,
        })
        .collect()
}

fn parse_journal_state_path(raw: &str, task_id: &str) -> Vec<String> {
    let mut states = Vec::new();
    for line in raw.lines() {
        let parts = line.split("  ").collect::<Vec<_>>();
        if parts.len() < 3 || parts[1] != task_id {
            continue;
        }
        let movement = parts[2];
        if let Some(state) =
            movement.strip_prefix("start@").or_else(|| movement.strip_prefix("end@"))
        {
            push_history_state(&mut states, state);
        } else if let Some((from, to)) = movement.split_once('\u{2192}') {
            push_history_state(&mut states, from.trim());
            push_history_state(&mut states, to.trim());
        } else if let Some((from, to)) = movement.split_once("->") {
            push_history_state(&mut states, from.trim());
            push_history_state(&mut states, to.trim());
        }
    }
    states
}

fn push_history_state(states: &mut Vec<String>, state: &str) {
    let state = state.trim();
    if state.is_empty() || states.last().is_some_and(|last| last == state) {
        return;
    }
    states.push(state.to_string());
}

/// Normalize a raw `**State:**` value through the machine (e.g. resolving the
/// `-N` visit suffix on counted states for a live render).
pub fn normalize_state(raw_state: &str, machine: &StateMachine) -> String {
    parse_task_state(raw_state, machine).state
}

/// The set of states that are the entry of at least one profile, unioned with
/// any state flagged `initial: true` directly. §FS-rhei-viz §8, §FS-rhei-states
fn initial_states(machine: &StateMachine) -> HashSet<String> {
    let mut set: HashSet<String> = machine
        .states
        .iter()
        .filter(|(_, def)| def.initial)
        .map(|(name, _)| name.clone())
        .collect();
    if let Some(profiles) = &machine.profiles {
        for profile in profiles.values() {
            set.insert(profile.initial.clone());
        }
    }
    set
}

/// Flatten a [`StateMachine`] into the model's [`Machine`]: states in declared
/// order, each with its outgoing transitions (explicit first, then applicable
/// `from: "*"` wildcard edges) and artifact contracts. §FS-rhei-viz.8
pub fn flatten_machine(machine: &StateMachine) -> Machine {
    let initials = initial_states(machine);

    let to_artifacts = |defs: &[StateArtifactDef]| {
        defs.iter()
            .map(|a| Artifact {
                name: a.name.clone(),
                path: a.path.clone(),
                description: a.description.clone(),
                optional: a.optional,
            })
            .collect::<Vec<_>>()
    };

    let states = machine
        .states
        .iter()
        .map(|(name, def)| {
            let mut transitions: Vec<Transition> = machine
                .transitions
                .iter()
                .filter(|rule| rule.from.0 == *name)
                .map(|rule| Transition {
                    to: rule.to.0.clone(),
                    condition: rule.condition.clone(),
                    wildcard: false,
                })
                .collect();
            // Attach `from: "*"` wildcard edges to every non-terminal state so
            // the inspector shows the real set of legal exits.
            if !def.terminal {
                for rule in machine.transitions.iter().filter(|rule| {
                    rule.from.0 == "*" && machine.transition_matches_source(rule, name)
                }) {
                    if rule.to.0 != *name && !transitions.iter().any(|t| t.to == rule.to.0) {
                        transitions.push(Transition {
                            to: rule.to.0.clone(),
                            condition: rule.condition.clone(),
                            wildcard: true,
                        });
                    }
                }
            }
            MachineState {
                name: name.clone(),
                description: def.description.clone(),
                instructions: machine.effective_instructions(def),
                visits: def.visits,
                initial: initials.contains(name),
                terminal: def.terminal,
                gating: def.gating,
                waiting_on: def.waiting_on_person().map(str::to_string),
                process: state_process_kind(def),
                transitions,
                inputs: to_artifacts(&def.inputs),
                outputs: to_artifacts(&def.outputs),
                template_context: template_context(def),
                template_contexts: fanout_template_contexts(def),
            }
        })
        .collect();

    Machine { name: machine.name.clone(), states }
}

fn state_process_kind(def: &crate::rhei_validator::StateDef) -> Option<MachineProcessKind> {
    if def.program.is_some() {
        Some(MachineProcessKind::Program)
    } else if def.agent.is_some()
        || def.model.is_some()
        || def.target.is_some()
        || !def.all_models.is_empty()
        || !def.all_targets.is_empty()
    {
        Some(MachineProcessKind::Agent)
    } else {
        None
    }
}

fn target_template_context(target: crate::rhei_validator::ExecutionTarget) -> TemplateContext {
    TemplateContext {
        target: Some(target.selector()),
        target_slug: Some(target.slug()),
        model_provider: target.provider.clone(),
        model_name: Some(target.model.clone()),
        model: Some(target.model),
        agent: Some(target.agent),
        agent_mode: target.mode,
    }
}

fn model_template_context(def: &crate::rhei_validator::StateDef, model: String) -> TemplateContext {
    TemplateContext {
        model: Some(model.clone()),
        model_name: Some(model),
        agent: def.agent.as_ref().map(|agent| agent.id().to_string()),
        agent_mode: def.agent_mode.clone(),
        ..TemplateContext::default()
    }
}

fn explicit_template_context(def: &crate::rhei_validator::StateDef) -> TemplateContext {
    if let Some(selector) = def.target.as_deref() {
        if let Ok(target) = parse_execution_target(selector) {
            return target_template_context(target);
        }
    }
    if let Some(model) = def.model.as_ref().map(|model| model.trim().to_string()) {
        return model_template_context(def, model);
    }
    TemplateContext {
        agent: def.agent.as_ref().map(|agent| agent.id().to_string()),
        agent_mode: def.agent_mode.clone(),
        ..TemplateContext::default()
    }
}

// Static prompt/artifact previews resolve only authored concrete values; multi
// fanout expands into per-target/model variants instead of guessing. §FS-rhei-viz.8
fn template_context(def: &crate::rhei_validator::StateDef) -> TemplateContext {
    let contexts = authored_fanout_template_contexts(def);
    if contexts.len() == 1 {
        contexts.into_iter().next().unwrap_or_default()
    } else {
        explicit_template_context(def)
    }
}

fn fanout_template_contexts(def: &crate::rhei_validator::StateDef) -> Vec<TemplateContext> {
    let contexts = authored_fanout_template_contexts(def);
    if contexts.len() > 1 {
        contexts
    } else {
        Vec::new()
    }
}

fn authored_fanout_template_contexts(
    def: &crate::rhei_validator::StateDef,
) -> Vec<TemplateContext> {
    if !def.all_targets.is_empty() {
        return def
            .all_targets
            .iter()
            .filter_map(|selector| parse_execution_target(selector).ok())
            .map(target_template_context)
            .collect();
    }
    if !def.all_models.is_empty() {
        return def
            .all_models
            .iter()
            .map(|model| model_template_context(def, model.trim().to_string()))
            .collect();
    }
    Vec::new()
}

/// Classify a persisted state into one of the seven categories: machine flags
/// first, state name second; `Active` is the catch-all. Mirrors the asset's
/// `category()`. §FS-rhei-viz.1.1
pub fn category(machine: &StateMachine, state: &str) -> Category {
    let def = machine.states.get(state);
    if state == "completed" {
        return Category::Done;
    }
    if state == "failed" {
        return Category::Failed;
    }
    if state == "blocked" {
        return Category::Blocked;
    }
    // A person wait shares the gate row rather than earning an eighth category:
    // same thing to the eye, differing only in who moves it on.
    // §FS-rhei-viz.1.1 §FS-rhei-states.2.5
    if def.map(|d| d.gating || d.waiting_on_person().is_some()).unwrap_or(false)
        || state == "human-review"
    {
        return Category::Gate;
    }
    if def.map(|d| d.terminal).unwrap_or(false) {
        return if state == "completed" { Category::Done } else { Category::Retired };
    }
    if state == "cancelled" || state == "archived" {
        return Category::Retired;
    }
    let is_initial =
        def.map(|d| d.initial).unwrap_or(false) || initial_states(machine).contains(state);
    if state == "draft" || state == "pending" || is_initial {
        return Category::Idle;
    }
    Category::Active
}

/// Derive the level-0 plan state from top-level task states: the pure derivation
/// over state names; the live host additionally promotes to `active` when a
/// top-level task is assigned to a running slot. §FS-rhei-viz.9
pub fn derive_plan_state(tasks: &[TaskRow], machine: &StateMachine) -> String {
    let roots: Vec<&str> =
        tasks.iter().filter(|t| t.depth == 0).map(|t| t.state.as_str()).collect();
    if roots.is_empty() {
        return "draft".into();
    }
    if roots.iter().all(|s| *s == "draft") {
        return "draft".into();
    }
    if roots.iter().all(|s| *s == "completed") {
        return "completed".into();
    }

    let terminal: HashSet<&str> = machine
        .states
        .iter()
        .filter_map(|(name, def)| def.terminal.then_some(name.as_str()))
        .collect();
    if roots.iter().all(|s| terminal.contains(s)) {
        return "archived".into();
    }

    // active-like = a non-terminal state that is not in the `idle` category.
    let any_active_like = roots.iter().any(|s| category(machine, s) == Category::Active);
    if any_active_like {
        "active".into()
    } else {
        "pending".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rhei_validator::StateMachine;
    use rhei_core::parse;

    fn builtin() -> StateMachine {
        StateMachine::builtin_default()
    }

    #[test]
    fn flat_tasks_carry_depth_and_parent() {
        let rhei = parse(
            "# Rhei: Deep\n**States:** rhei\n---\nstructure:\n  maxLevels: 4\n  nodeKinds: [task, bug]\n---\n\n## Tasks\n\n### Task api: Build API\n**State:** pending\n\n#### Bug api.cache: Cache issue\n**State:** in-progress\n",
        )
        .expect("parse");
        let model = build(&rhei, &builtin());
        assert_eq!(model.tasks.len(), 2);
        assert_eq!(model.tasks[0].id, "api");
        assert_eq!(model.tasks[0].depth, 0);
        assert_eq!(model.tasks[0].parent, None);
        assert_eq!(model.tasks[1].id, "api.cache");
        assert_eq!(model.tasks[1].depth, 1);
        assert_eq!(model.tasks[1].parent.as_deref(), Some("api"));
    }

    /// The two polls the ticket contrasts, classified: the one waiting on a
    /// person reads as a pause, the one watching CI stays active — and the
    /// label crosses the wire so the browser and the terminal can agree.
    // §FS-rhei-viz.1.1 §FS-rhei-states.2.5
    #[test]
    fn a_poll_waiting_on_a_person_is_a_pause_not_active_work() {
        let machine = StateMachine::from_yaml_str(
            r#"
name: approvals
version: 1.0
states:
  plan-approval:
    description: Wait for the author
    program: "./check-reply.sh"
    poll:
      interval: 10m
      max_attempts: 60
      waiting_on: author
  ci-watch:
    description: Watch CI
    program: "./check-ci.sh"
    poll:
      interval: 2m
      max_attempts: 30
  completed:
    final: true
transitions:
  - from: plan-approval
    to: plan-approval
  - from: plan-approval
    to: ci-watch
  - from: ci-watch
    to: ci-watch
  - from: ci-watch
    to: completed
"#,
        )
        .expect("states load");

        assert_eq!(category(&machine, "plan-approval"), Category::Gate);
        assert_eq!(category(&machine, "ci-watch"), Category::Active);

        let wire = flatten_machine(&machine);
        let state = |name: &str| {
            wire.states.iter().find(|s| s.name == name).expect("state present").clone()
        };
        assert_eq!(state("plan-approval").waiting_on.as_deref(), Some("author"));
        assert_eq!(state("ci-watch").waiting_on, None);
    }

    #[test]
    fn task_visit_and_unambiguous_template_context_are_exposed() {
        let machine = StateMachine::from_yaml_str(
            r#"
name: custom
version: 1.0
states:
  review:
    visits: 3
    target: codex:openai:gpt-5
    instructions: "Review {task_id} in {state}-{visit_count} using {target.slug}"
    outputs:
      - name: notes
        path: runtime/reviews/{task_id}-{state}-{visit_count}-{target.slug}.md
  completed:
    final: true
transitions:
  - from: review
    to: completed
"#,
        )
        .expect("states load");
        let rhei = parse(
            "# Rhei: Visits\n**States:** custom\n\n## Tasks\n\n### Task 1: A\n**State:** review-2\n",
        )
        .expect("parse");

        let model = build(&rhei, &machine);
        assert_eq!(model.tasks[0].state, "review");
        assert_eq!(model.tasks[0].visit_count, Some(2));
        let review = model.machine.states.iter().find(|s| s.name == "review").unwrap();
        assert_eq!(review.template_context.target.as_deref(), Some("codex:openai:gpt-5"));
        assert_eq!(review.template_context.target_slug.as_deref(), Some("codex-openai-gpt-5"));
        assert_eq!(review.template_context.model.as_deref(), Some("gpt-5"));
        assert_eq!(review.template_context.model_provider.as_deref(), Some("openai"));
    }

    #[test]
    fn multi_target_fanout_contexts_are_exposed_without_guessing() {
        let machine = StateMachine::from_yaml_str(
            r#"
name: custom
version: 1.0
states:
  product-run:
    all_targets:
      - claude-code[yolo]:anthropic:claude-opus-4-7
      - codex[xhigh]:openai:gpt-5.5
    instructions: "Write {output.notes.path} for {target}"
    outputs:
      - name: notes
        path: runtime/{target.slug}/{task_id}.md
  completed:
    final: true
transitions:
  - from: product-run
    to: completed
"#,
        )
        .expect("states load");
        let rhei = parse(
            "# Rhei: Fanout\n**States:** custom\n\n## Tasks\n\n### Task pm: Evaluate\n**State:** product-run\n",
        )
        .expect("parse");

        let model = build(&rhei, &machine);
        let product = model.machine.states.iter().find(|s| s.name == "product-run").unwrap();
        assert_eq!(product.template_context.target, None);
        assert_eq!(product.template_context.target_slug, None);
        assert_eq!(product.template_contexts.len(), 2);
        assert_eq!(
            product.template_contexts[0].target.as_deref(),
            Some("claude-code[yolo]:anthropic:claude-opus-4-7")
        );
        assert_eq!(
            product.template_contexts[0].target_slug.as_deref(),
            Some("claude-code-yolo-anthropic-claude-opus-4-7")
        );
        assert_eq!(
            product.template_contexts[1].target_slug.as_deref(),
            Some("codex-xhigh-openai-gpt-5.5")
        );
    }

    #[test]
    fn plan_state_pending_when_only_pending_roots() {
        let rhei = parse(
            "# Rhei: P\n**States:** rhei\n\n## Tasks\n\n### Task 1: A\n**State:** pending\n\n### Task 2: B\n**State:** pending\n",
        )
        .expect("parse");
        let model = build(&rhei, &builtin());
        assert_eq!(model.plan_state.as_deref(), Some("pending"));
    }

    #[test]
    fn plan_state_active_when_a_root_is_active_like() {
        let rhei = parse(
            "# Rhei: A\n**States:** rhei\n\n## Tasks\n\n### Task 1: A\n**State:** in-progress\n\n### Task 2: B\n**State:** pending\n",
        )
        .expect("parse");
        let model = build(&rhei, &builtin());
        assert_eq!(model.plan_state.as_deref(), Some("active"));
    }

    #[test]
    fn plan_state_completed_and_archived() {
        let completed = parse(
            "# Rhei: C\n**States:** rhei\n\n## Tasks\n\n### Task 1: A\n**State:** completed\n",
        )
        .expect("parse");
        assert_eq!(build(&completed, &builtin()).plan_state.as_deref(), Some("completed"));

        let archived = parse(
            "# Rhei: C\n**States:** archival\n\n## Tasks\n\n### Task 1: A\n**State:** completed\n\n### Task 2: B\n**State:** archived\n",
        )
        .expect("parse");
        let machine = StateMachine::from_yaml_str(
            r#"
name: archival
version: 1
states:
  pending: { description: "ready" }
  completed: { description: "done", final: true }
  archived: { description: "retired", final: true }
transitions:
  - from: pending
    to: completed
profiles:
  default: { initial: pending, allowed: [pending, completed, archived] }
node_policy:
  root: default
  default: default
"#,
        )
        .expect("machine");
        assert_eq!(build(&archived, &machine).plan_state.as_deref(), Some("archived"));
    }

    #[test]
    fn machine_flattening_marks_wildcard_and_initial() {
        let machine = builtin();
        let flat = flatten_machine(&machine);
        // The built-in `default-rhei` profile enters at `pending`, so the
        // profile-initial union must mark `pending` initial.
        let pending = flat.states.iter().find(|s| s.name == "pending").expect("pending state");
        assert!(pending.initial, "pending is the built-in profile's initial state");
        let completed = flat.states.iter().find(|s| s.name == "completed").expect("completed");
        assert!(completed.terminal);
        assert!(completed.transitions.is_empty(), "terminal states get no wildcard exits");
    }
}
