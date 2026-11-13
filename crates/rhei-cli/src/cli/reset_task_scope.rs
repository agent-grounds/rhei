// What a reset reaches, asked by every step of it, and the closure `--task`
// computes over the plan's own edges to answer that question below a rhei.
//
// Its own part because the closure is a walk no other reset step makes —
// children, `**Prior:**` dependents, finished ancestors — while every step only
// asks whether a ticket is in scope.

// §AR-source-file-size.3 §FS-rhei-reset.2.5

/// What one reset reaches: the whole project, the rheis `--rhei` names, or the
/// tickets a `--task` closure reaches. Every step of a reset asks it, so the
/// states it restores, the files it locks and the records it removes cannot
/// disagree on what is in scope. §FS-rhei-reset.2 §FS-rhei-reset.2.5
#[derive(Clone, Copy)]
enum ResetScope<'a> {
    /// The whole project (`None`), or the rheis `--rhei` names.
    Rheis(&'a RheiScope),
    /// The tickets a `--task` closure reaches.
    Tickets(&'a TicketScope),
}

impl<'a> From<&'a RheiScope> for ResetScope<'a> {
    fn from(scope: &'a RheiScope) -> Self {
        Self::Rheis(scope)
    }
}

impl<'a> ResetScope<'a> {
    /// The tickets `tickets` names where `--task` was given, else the rheis `rheis` names.
    fn of(rheis: &'a RheiScope, tickets: Option<&'a TicketScope>) -> Self {
        tickets.map_or(Self::Rheis(rheis), Self::Tickets)
    }

    /// Whether the reset reaches `task_id`, a project-qualified id.
    fn contains(self, task_id: &str) -> bool {
        match self {
            Self::Rheis(scope) => task_in_rhei_scope(scope, task_id),
            Self::Tickets(tickets) => tickets.contains(task_id),
        }
    }

    /// Whether it is narrower than the project, so it removes what is keyed by
    /// its tickets rather than whole `runtime/` trees. §FS-rhei-reset.2.1
    fn is_narrowed(self) -> bool {
        match self {
            Self::Rheis(scope) => scope.is_some(),
            Self::Tickets(_) => true,
        }
    }

    fn tickets(self) -> Option<&'a TicketScope> {
        match self {
            Self::Rheis(_) => None,
            Self::Tickets(tickets) => Some(tickets),
        }
    }

    /// Every task node the reset reaches, descendants included, in plan order.
    fn tasks(self, loaded: &LoadedPlan) -> Vec<&rhei_core::ast::Task> {
        let mut tasks = Vec::new();
        collect_plan_tasks(&loaded.rhei.tasks, &mut tasks);
        tasks.retain(|task| self.contains(&task.id.to_string()));
        tasks
    }

    /// The rheis its records sit in: the named ones, or the ones a ticket
    /// scope's tickets belong to; `None` project-wide. §FS-rhei-evidence.10.1
    fn rheis(self) -> RheiScope {
        match self {
            Self::Rheis(scope) => scope.clone(),
            Self::Tickets(tickets) => Some(tickets.rheis()),
        }
    }
}

/// The edge that first brought a ticket into a `--task` scope, which the preview
/// prints beside its move. §FS-rhei-reset.2.5.1 §FS-rhei-reset.4
#[derive(Debug, Clone, PartialEq, Eq)]
enum ScopeEdge {
    Named,
    Prior(String),
    ChildOf(String),
    FinishedParentOf(String),
}

impl ScopeEdge {
    fn label(&self) -> Option<String> {
        match self {
            Self::Named => None,
            Self::Prior(id) => Some(format!("(prior: {id})")),
            Self::ChildOf(id) => Some(format!("(child of {id})")),
            Self::FinishedParentOf(id) => Some(format!("(finished parent of {id})")),
        }
    }
}

/// The tickets a `--task` reset reaches, and what it says about them.
/// §FS-rhei-reset.2.5
#[derive(Debug, Clone, PartialEq, Eq)]
struct TicketScope {
    /// Every ticket the closure reaches, by qualified id, with the edge that
    /// first reached it.
    reached: BTreeMap<String, ScopeEdge>,
    /// The rheis `--rhei` or a member target bounded the ids to, which the
    /// preview names when the closure leaves them. §FS-rhei-reset.2.5.4
    resolved_in: RheiScope,
    /// The nearest supervising ancestor of a ticket in scope that the scope
    /// leaves out, by qualified id, with the tickets in scope it owns, in plan
    /// order. §FS-rhei-reset.2.5.3
    supervisors: BTreeMap<String, Vec<String>>,
}

impl TicketScope {
    /// Read each id `--task` names as `rhei list --parent` reads one, bounded by
    /// `resolved_in`, into its qualified id. An unknown id or an ambiguous shorthand
    /// is refused here, before any preview. §FS-rhei-reset.2.5.4
    fn resolve_ids(
        loaded: &LoadedPlan,
        named: &[String],
        resolved_in: &RheiScope,
    ) -> MietteResult<Vec<String>> {
        named.iter().map(|id| resolve_cli_task_id(loaded, id.trim(), resolved_in)).collect()
    }

    /// The scope `named` closes to, or `None` where `--task` named nothing.
    /// §FS-rhei-reset.2.5
    fn of_named(
        loaded: &LoadedPlan,
        named: &[String],
        resolved_in: &RheiScope,
        resolved: &ResolvedMachineSet,
    ) -> Option<Self> {
        if named.is_empty() {
            return None;
        }
        Some(Self::close(loaded, named, resolved_in.clone(), &resolved.validator_set()))
    }

    /// The closure of `named` (qualified ids): every descendant, every ticket
    /// whose `**Prior:**` names one in scope, across rheis, and every finished
    /// ancestor alone, to a fixpoint. `**Consumes:**` adds nothing, and a named
    /// ticket's priors are never added. §FS-rhei-reset.2.5.1 §FS-rhei-reset.2.5.2
    fn close(
        loaded: &LoadedPlan,
        named: &[String],
        resolved_in: RheiScope,
        machines: &rhei_validator::MachineSet,
    ) -> Self {
        let mut all = Vec::new();
        collect_plan_tasks(&loaded.rhei.tasks, &mut all);
        let by_id: HashMap<String, &rhei_core::ast::Task> =
            all.iter().map(|task| (task.id.to_string(), *task)).collect();
        let mut dependents: HashMap<String, Vec<String>> = HashMap::new();
        for task in &all {
            for prior in &task.prior {
                dependents.entry(prior.to_string()).or_default().push(task.id.to_string());
            }
        }

        let mut walk = ClosureWalk::default();
        // Named tickets first, in plan order, so each is listed as named.
        for task in &all {
            let id = task.id.to_string();
            if named.contains(&id) {
                walk.reach(id, ScopeEdge::Named, true);
            }
        }
        while let Some(id) = walk.queue.pop_front() {
            let Some(task) = by_id.get(&id) else { continue };
            // A finished ancestor is taken alone: its other children stay.
            if walk.whole.contains(&id) {
                for child in &task.children {
                    walk.reach(child.id.to_string(), ScopeEdge::ChildOf(id.clone()), true);
                }
            }
            for dependent in dependents.get(&id).into_iter().flatten() {
                walk.reach(dependent.clone(), ScopeEdge::Prior(id.clone()), true);
            }
            let machine = machines.for_task(&task.id);
            if let Some(ancestor) = terminal_ancestors(&loaded.rhei.tasks, &task.id, machine).first() {
                walk.reach(ancestor.id.to_string(), ScopeEdge::FinishedParentOf(id.clone()), false);
            }
        }

        let supervisors = left_out_supervisors(loaded, &all, &walk.reached, machines);
        TicketScope { reached: walk.reached, resolved_in, supervisors }
    }

    fn contains(&self, task_id: &str) -> bool {
        self.reached.contains_key(task_id)
    }

    fn edge(&self, task_id: &str) -> Option<&ScopeEdge> {
        self.reached.get(task_id)
    }

    /// The rheis the reached tickets belong to, sorted. §FS-rhei-evidence.10.1
    fn rheis(&self) -> BTreeSet<String> {
        self.reached.keys().map(|id| owning_rhei(id).to_string()).collect()
    }

    /// The reached tickets outside the rheis their ids were resolved in.
    /// §FS-rhei-reset.2.5.4
    fn beyond_resolution(&self) -> Vec<&str> {
        let Some(bound) = &self.resolved_in else { return Vec::new() };
        self.reached
            .keys()
            .filter(|id| !bound.contains(owning_rhei(id)))
            .map(String::as_str)
            .collect()
    }
}

/// The closure as it is walked: what is reached, which of those bring their
/// descendants, and what is still to visit. §FS-rhei-reset.2.5.1
#[derive(Default)]
struct ClosureWalk {
    reached: BTreeMap<String, ScopeEdge>,
    whole: BTreeSet<String>,
    queue: std::collections::VecDeque<String>,
}

impl ClosureWalk {
    /// Reach `id` by `edge`; the first edge is the one listed. A ticket first
    /// reached as a finished ancestor and later by another edge brings its
    /// descendants after all, so it is visited again.
    fn reach(&mut self, id: String, edge: ScopeEdge, whole: bool) {
        let first = !self.reached.contains_key(&id);
        if first {
            self.reached.insert(id.clone(), edge);
        }
        let widened = whole && self.whole.insert(id.clone());
        if first || widened {
            self.queue.push_back(id);
        }
    }
}

/// For each ticket in scope, the supervisor a transition of it would checkpoint
/// (§FS-rhei-supervision.2.2), where the scope leaves that supervisor out.
/// §FS-rhei-reset.2.5.3
fn left_out_supervisors(
    loaded: &LoadedPlan,
    all: &[&rhei_core::ast::Task],
    reached: &BTreeMap<String, ScopeEdge>,
    machines: &rhei_validator::MachineSet,
) -> BTreeMap<String, Vec<String>> {
    let mut supervisors: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for task in all.iter().filter(|task| reached.contains_key(&task.id.to_string())) {
        let machine = machines.for_task(&task.id);
        let ancestors = ancestor_chain(&loaded.rhei.tasks, &task.id);
        let Some(owner) = nearest_in_scope_supervising_owner(machine, ancestors) else {
            continue;
        };
        let owner = owner.id.to_string();
        if !reached.contains_key(&owner) {
            supervisors.entry(owner).or_default().push(task.id.to_string());
        }
    }
    supervisors
}
