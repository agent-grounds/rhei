// What the project graph hands prompt composition about the *shape* of the task
// tree: the flattening every section walks in, the terminality test, where the
// reader stands in the tree, and what one line may say about a whole subtree it
// speaks for.
//
// Its own part because the sections next door disagree about which slice of
// memory they render and agree completely about how the tree under a node is
// read — a count that two renderers derived separately would be two counts.

// §AR-source-file-size.3 §FS-rhei-memory.4.3

/// Every task of the merged graph, in plan order, parents before children.
// §FS-rhei-plan-language.1.2
fn flatten_task_slice(tasks: &[rhei_core::ast::Task]) -> Vec<&rhei_core::ast::Task> {
    fn collect<'a>(task: &'a rhei_core::ast::Task, out: &mut Vec<&'a rhei_core::ast::Task>) {
        out.push(task);
        for child in &task.children {
            collect(child, out);
        }
    }
    let mut out = Vec::new();
    for task in tasks {
        collect(task, &mut out);
    }
    out
}

/// Whether a ticket's authored state is terminal under its machine.
fn task_state_is_terminal(
    task: &rhei_core::ast::Task,
    machine: &rhei_validator::StateMachine,
) -> bool {
    is_terminal_state(&memory_state_name(task, machine), machine)
}

/// Descendants of `task` whose results this prompt already pasted, which drop
/// out of `own` rather than repeating as one-liners. §FS-rhei-memory.4.3
fn pasted_descendant_ids(
    render_context: &RuntimeTemplateContext<'_>,
    pasted_in_full: &BTreeSet<String>,
) -> BTreeSet<String> {
    flatten_task_slice(&render_context.task.children)
        .into_iter()
        .map(|task| task.id.to_string())
        .filter(|id| pasted_in_full.contains(id))
        .collect()
}

/// Every descendant of `task`, at any depth, in plan order.
///
/// The same walk `pasted_descendant_ids` performs, named once so a fold clause
/// and a progress count cannot disagree about the size of a subtree.
// §FS-rhei-memory.4.3
fn task_descendants(task: &rhei_core::ast::Task) -> Vec<&rhei_core::ast::Task> {
    flatten_task_slice(&task.children)
}

/// `{n}` of a folded line: the size of the subtree, whatever state each node is
/// in and whatever rhei owns it. A count is navigation, so an excluded subtree
/// is still counted in full — no fold clause opens a file.
// §FS-rhei-memory.4.3 §FS-rhei-memory.1.1
fn descendant_count(task: &rhei_core::ast::Task) -> usize {
    task_descendants(task).len()
}

/// `{k} of {n}`: the terminal descendants, and all of them.
// §FS-rhei-memory.4.3
fn descendant_progress(
    task: &rhei_core::ast::Task,
    machine: &rhei_validator::StateMachine,
) -> (usize, usize) {
    let descendants = task_descendants(task);
    let finished = descendants
        .iter()
        .filter(|descendant| task_state_is_terminal(descendant, machine))
        .count();
    (finished, descendants.len())
}

/// `{breakdown}`: every descendant bucketed by its normalized state name, in
/// the resolved machine's declaration order, with empty buckets omitted.
///
/// *Every* descendant, not only the terminal ones, so the numbers always sum to
/// `{n}` — a parent cancelled with children left pending reads `1 pending, 3
/// completed, 1 cancelled` rather than a line that does not add up. The name
/// comes from the machine, which is why a custom terminal shows under its own
/// name with no lookup of its own. A state the machine does not declare has no
/// declared position, so it follows the declared ones in plan order.
// §FS-rhei-memory.4.3 §FS-rhei-memory.3.2
fn descendant_state_breakdown(
    descendants: &[&rhei_core::ast::Task],
    machine: &rhei_validator::StateMachine,
) -> String {
    let mut counted: Vec<(String, usize)> = Vec::new();
    for descendant in descendants {
        let name = memory_state_name(descendant, machine);
        match counted.iter_mut().find(|(seen, _)| seen == &name) {
            Some((_, count)) => *count += 1,
            None => counted.push((name, 1)),
        }
    }
    let declared = |name: &str| machine.states.get_index_of(name).unwrap_or(usize::MAX);
    counted.sort_by_key(|(name, _)| declared(name));
    counted
        .iter()
        .map(|(name, count)| format!("{count} {name}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The fold clause of a listed history line: ` — {n} subtasks: {breakdown}`,
/// and nothing at all for a leaf, whose line is the shape it has always had.
// §FS-rhei-memory.3.2 §FS-rhei-memory.4.3
fn subtree_fold_clause(
    task: &rhei_core::ast::Task,
    machine: &rhei_validator::StateMachine,
) -> String {
    let descendants = task_descendants(task);
    if descendants.is_empty() {
        return String::new();
    }
    format!(
        " \u{2014} {} subtasks: {}",
        descendants.len(),
        descendant_state_breakdown(&descendants, machine)
    )
}

/// ` — {k} of {n} subtasks finished`: what an `### In Flight` row says about an
/// open subtree, rendered even at `0 of {n}` — that a subtree exists and is
/// untouched is worth a reader knowing — and absent on a leaf.
// §FS-rhei-memory.3.2 §FS-rhei-memory.4.3
fn subtree_progress_clause(
    task: &rhei_core::ast::Task,
    machine: &rhei_validator::StateMachine,
) -> String {
    let (finished, total) = descendant_progress(task, machine);
    if total == 0 {
        return String::new();
    }
    format!(" \u{2014} {finished} of {total} subtasks finished")
}

/// Where this invocation stands in the tree, as the two questions the history
/// and `### In Flight` predicates ask about a candidate.
///
/// Built from `ancestor_chain`, the same walk `## Position` renders its chain
/// from, so the two sections cannot disagree about where the reader stands.
// §FS-rhei-memory.4.2 §FS-rhei-memory.4.3
struct ReaderPath {
    /// The reader and each of its ancestors: the tasks whose children are on
    /// the path, because a parent is what speaks for a child.
    speakers: BTreeSet<String>,
    /// The reader's ancestors alone — the nodes `## Position` already names in
    /// the chain, each with its state, the nearest one's body pasted in full.
    ancestors: BTreeSet<String>,
    /// Each task's parent, by qualified id. A top-level task has none.
    parents: HashMap<String, String>,
}

impl ReaderPath {
    /// §FS-rhei-memory.4.3 step 1: a candidate is on the path when it is a
    /// top-level task of the plan, or its parent is the reader or an ancestor
    /// of the reader. A predicate on the candidate rather than a walk, so a
    /// task of another rhei on the way down prunes nothing beneath it.
    fn lists(&self, candidate: &rhei_core::ast::Task) -> bool {
        match self.parents.get(&candidate.id.to_string()) {
            None => true,
            Some(parent) => self.speakers.contains(parent),
        }
    }

    /// §FS-rhei-memory.4.2: whether `## Position` already names the candidate —
    /// which is why neither `## Plan History` nor `### In Flight` repeats it.
    fn named_in_position(&self, candidate: &rhei_core::ast::Task) -> bool {
        self.ancestors.contains(&candidate.id.to_string())
    }
}

/// The path of one invocation through the plan it was composed from.
// §FS-rhei-memory.4.3
fn reader_path(
    plan_tasks: &[rhei_core::ast::Task],
    reader: &rhei_core::ast::Task,
) -> ReaderPath {
    let ancestors: BTreeSet<String> = ancestor_chain(plan_tasks, &reader.id)
        .into_iter()
        .map(|task| task.id.to_string())
        .collect();
    let mut speakers = ancestors.clone();
    speakers.insert(reader.id.to_string());
    let mut parents = HashMap::new();
    for parent in flatten_task_slice(plan_tasks) {
        for child in &parent.children {
            parents.insert(child.id.to_string(), parent.id.to_string());
        }
    }
    ReaderPath { speakers, ancestors, parents }
}
