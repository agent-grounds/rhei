// Recovering the state a task was *authored* in, so `rhei reset` can put it
// back there instead of at the machine's `initial: true` state.
//
// Its own part because it is a read of runtime history, not a rewrite: the
// ledger it parses is deleted by the same command a moment later, so the read
// has to happen before the rewrite next door and cannot live inside it.

// §AR-source-file-size.3 §FS-rhei-reset.2.2

/// One task `rhei reset` moves back, as the preview and the summary print it.
// §FS-rhei-reset.4
struct StateMove {
    /// Project-qualified, because that is the id the operator types.
    task_id: String,
    /// The state the plan holds now.
    from: String,
    /// The state the task was authored in.
    to: String,
}

/// The state to write on each task's `**State:**` line, keyed by the file
/// holding the heading and then by the task's *file-local* id — headings
/// inside a rhei are rhei-local even when ledger keys are project-qualified.
///
/// Every in-scope task has an entry, so every line is rewritten in normalized
/// form even when the state name does not change: a counted-visit suffix is
/// runtime state, and reset clears it either way.
// §AR-rhei-panta.3 §FS-rhei-reset.2
type AuthoredByFile = BTreeMap<PathBuf, BTreeMap<String, String>>;

/// What reset learned from the ledgers before it deletes them.
struct AuthoredStates {
    by_file: AuthoredByFile,
    /// Every recovered move, sorted by task id.
    moves: Vec<StateMove>,
    /// Tasks whose execution root has no ledger, that show a trace of having
    /// run, and that sit outside their profile's `initial` state — the ones
    /// whose position reset cannot account for. A task with no trace of a run
    /// is authored where it stands and is not listed: for a pre-authored chain
    /// that is every child, and naming them all would bury the real one.
    stranded: Vec<(String, String)>,
    /// `(task, state now, state recorded)` where the ledger names a state the
    /// task's machine no longer declares — renamed since the run. Writing it
    /// back would leave a plan that no longer validates.
    undeclared: Vec<(String, String, String)>,
}

/// Parse one execution root's ledger into `task-id → the first state it left`.
///
/// Lines are `<task-id> <from>@<to>` in the order the moves happened, so the
/// first `from` recorded for a task is the state that task started in. A
/// missing or unreadable ledger is not an error: it means no history, which
/// the caller answers by changing nothing.
// §FS-rhei-viz.4 §FS-rhei-reset.2.2
fn ledger_first_departures(root: &Path) -> BTreeMap<String, String> {
    let mut first: BTreeMap<String, String> = BTreeMap::new();
    let Ok(raw) = fs::read_to_string(root.join("runtime").join("state-transitions.log")) else {
        return first;
    };
    // §FS-rhei-reset.2.2: only movements establish authored state.
    for movement in rhei_core::transition_history::parse(&raw).unwrap_or_default() {
        first.entry(movement.task_id).or_insert(movement.from);
    }
    first
}

/// Whether this task carries a trace of having been run: a claim, a result
/// link, or a counted-visit suffix on its state. Used only to keep the
/// "cannot account for this" report off a plan that plainly never ran.
///
/// Best effort, and it has to be: a `rhei transition` into a non-final state
/// leaves nothing behind but the state itself, which is exactly what a hand
/// authored plan looks like. Missing such a task costs a line of report; a
/// false positive would put every child of a pre-authored chain in the list.
fn shows_run_trace(task: &rhei_core::ast::Task, normalized_state: &str) -> bool {
    task.assignee.is_some()
        || task.state.as_str() != normalized_state
        || task.content.contains("> **Result:**")
}

/// Recover every in-scope task's authored state from the ledger of the
/// execution root that owns it, and list the tasks that moved away from it.
/// Reads only — the caller rewrites, then deletes the ledgers.
// §FS-rhei-reset.2.2
fn collect_authored_states(
    loaded: &LoadedPlan,
    input: &Path,
    scope: &RheiScope,
    machines: &rhei_validator::MachineSet,
) -> AuthoredStates {
    let mut in_scope: Vec<&rhei_core::ast::Task> = Vec::new();
    fn collect<'a>(task: &'a rhei_core::ast::Task, out: &mut Vec<&'a rhei_core::ast::Task>) {
        out.push(task);
        for child in &task.children {
            collect(child, out);
        }
    }
    for task in &loaded.rhei.tasks {
        if task_in_rhei_scope(scope, &task.id.to_string()) {
            collect(task, &mut in_scope);
        }
    }

    // One read per execution root: sibling rheis share one ledger, and a task
    // graph can hold thousands of nodes. Ledger presence is judged per root,
    // never across them — one rhei's history says nothing about another's.
    let mut ledgers: BTreeMap<PathBuf, BTreeMap<String, String>> = BTreeMap::new();
    // A root holding runtime output but no ledger is a run whose history was
    // removed — evidence for every task there, not just the ones that left a
    // mark in the plan.
    let mut root_ran: BTreeMap<PathBuf, bool> = BTreeMap::new();
    let mut result = AuthoredStates {
        by_file: BTreeMap::new(),
        moves: Vec::new(),
        stranded: Vec::new(),
        undeclared: Vec::new(),
    };

    for task in in_scope {
        let task_id = task.id.to_string();
        let route = loaded.task_route(&task_id, input);
        let ledger = ledgers
            .entry(route.execution_root.clone())
            .or_insert_with(|| ledger_first_departures(&route.execution_root));
        let ran_here = *root_ran
            .entry(route.execution_root.clone())
            .or_insert_with(|| route.execution_root.join("runtime").is_dir());

        let machine = machines.for_task_str(&task_id);
        let current = normalized_state_name(task.state.as_str(), machine);

        // The project-qualified key only. A pre-qualification ledger keys by
        // the rhei-local id, but a local id can equal another rhei's qualified
        // id at the same root, and taking that line would move a task that
        // never ran — the very defect this command is being fixed for.
        let recorded = ledger.get(&task_id);
        let recovered = match recorded {
            Some(state) if !machine.states.contains_key(state.as_str()) => {
                result.undeclared.push((task_id.clone(), current.clone(), state.clone()));
                None
            }
            other => other,
        };

        let target = recovered.cloned().unwrap_or_else(|| current.clone());
        if target != current {
            result.moves.push(StateMove {
                task_id: task_id.clone(),
                from: current.clone(),
                to: target.clone(),
            });
        } else if recovered.is_none()
            && ledger.is_empty()
            && (ran_here || shows_run_trace(task, &current))
        {
            // No ledger at this root, so "no line" carries no information —
            // unlike a root whose ledger exists, where it means "never moved".
            let initial = initial_state_for_node(machine, &task.kind, task.profile_level());
            if initial.is_ok_and(|initial| initial != current) {
                result.stranded.push((task_id.clone(), current.clone()));
            }
        }

        // Every in-scope task gets an entry, so its line is rewritten in
        // normalized form and loses any counted-visit suffix.
        result.by_file.entry(route.task_file.clone()).or_default().insert(route.local_id, target);
    }

    result.moves.sort_by(|a, b| a.task_id.cmp(&b.task_id));
    result.stranded.sort();
    result.undeclared.sort();
    result
}

/// Print the moves a reset makes, under `verb` ("Would move" for the preview,
/// "Moved" for the summary). A count alone was true of the run that corrupted
/// a supervised chain and of the run that did nothing.
// §FS-rhei-reset.4
fn report_state_moves(authored: &AuthoredStates, verb: &str) {
    if authored.moves.is_empty() {
        println!("No task had moved from its authored state.");
    } else {
        println!("{} {} task(s) back:", verb, authored.moves.len());
        for mv in &authored.moves {
            println!("  Task {}: {} → {}", mv.task_id, mv.from, mv.to);
        }
    }

    if !authored.stranded.is_empty() {
        // Naming them is the whole recourse: with nothing recording where
        // these came from, only the operator knows.
        println!(
            "Nothing records where these {} task(s) came from, so they were left as they \
             stand, without the results and logs the rest of this reset removed:",
            authored.stranded.len()
        );
        for (task_id, state) in &authored.stranded {
            println!("  Task {task_id}: {state}");
        }
        println!("Edit their **State:** lines directly if that is not where they should be.");
    }

    for (task_id, current, recorded) in &authored.undeclared {
        println!(
            "Task {task_id} started in '{recorded}', which this state machine no longer \
             declares; left in '{current}'."
        );
    }
}
