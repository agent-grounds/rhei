// Previewing, reporting, and removing the runtime state `rhei reset` owns.

// §AR-source-file-size.3 §FS-rhei-reset.2.1 §FS-rhei-reset.4

/// Runtime directories a full reset would delete, in report order. A narrowed
/// reset removes per-ticket artifacts rather than whole trees, so it lists
/// none and the preview says so in words. §FS-rhei-panta.6.4
fn reset_runtime_preview(loaded: &LoadedPlan, input: &Path, scope: ResetScope<'_>) -> Vec<PathBuf> {
    if scope.is_narrowed() {
        return Vec::new();
    }
    let mut dirs: Vec<PathBuf> = Vec::new();
    if loaded.is_panta_project() {
        let mut roots: BTreeSet<PathBuf> = loaded.task_roots.values().cloned().collect();
        roots.insert(input.to_path_buf());
        dirs.extend(roots.into_iter().map(|root| root.join("runtime")));
    } else if workspace::is_workspace(input) {
        dirs.push(input.join("runtime"));
    } else if let Some(parent) = input.parent() {
        dirs.push(parent.join("runtime"));
    }
    dirs.retain(|dir| dir.exists());
    dirs
}

/// The one authoritative reset decision used for preview, mutation, cleanup,
/// and summary while the complete writer stack remains held.
// §FS-rhei-reset.1.2 §FS-rhei-reset.4
struct ResetDecision {
    /// The rheis it reaches: the named ones, or those a ticket scope's tickets are in.
    scope: RheiScope,
    /// The tickets `--task` reaches, when it narrowed the reset. §FS-rhei-reset.2.5
    tickets: Option<TicketScope>,
    task_count: usize,
    descendant_count: usize,
    authored: AuthoredStates,
    runtime_targets: Vec<PathBuf>,
    /// What a ticket scope says after its moves. §FS-rhei-reset.4
    left: TicketScopeLeft,
}

impl ResetDecision {
    /// The scope every step of the reset asks.
    fn reset_scope(&self) -> ResetScope<'_> {
        ResetScope::of(&self.scope, self.tickets.as_ref())
    }
}

fn collect_reset_decision(
    loaded: &LoadedPlan,
    input: &Path,
    scope: ResetScope<'_>,
    resolved: &ResolvedMachineSet,
) -> MietteResult<ResetDecision> {
    // §FS-rhei-reset.2.1: corrupt pairs refuse before any destructive rewrite.
    for root in loaded.rhei_roots.values().collect::<BTreeSet<_>>() { read_ledger(root)?; }
    // Step 1 judges the whole plan as it stands, whatever `--rhei` names, and without its links,
    // as `rhei complete` does; a refusal is validate's own report, before any preview.
    // §FS-rhei-reset.2 §FS-rhei-reset.4
    let machines = &resolved.validator_set();
    let report = rhei_validator::validate_with_machine_set(&loaded.rhei, machines);
    if report.has_errors() {
        let sources = resolved.validation_sources(&loaded.rhei_ids);
        return Err(validation_report(input, &sources, &report.errors, &report.help));
    }
    fn count_nodes(task: &rhei_core::ast::Task) -> usize {
        1 + task.children.iter().map(count_nodes).sum::<usize>()
    }

    let in_scope: Vec<&rhei_core::ast::Task> = loaded
        .rhei
        .tasks
        .iter()
        .filter(|task| scope.contains(&task.id.to_string()))
        .collect();
    // A ticket scope counts its tickets, at any depth, as the tasks it resets.
    // §FS-rhei-reset.4
    let (task_count, total_nodes) = match scope.tickets() {
        Some(tickets) => (tickets.reached.len(), tickets.reached.len()),
        None => (in_scope.len(), in_scope.iter().map(|task| count_nodes(task)).sum()),
    };

    let authored = collect_authored_states(loaded, input, scope, machines);
    // A reset that would leave a plan validation refuses is refused here, before any
    // preview, `--dry-run` included, with nothing changed; so is a plan the reset
    // cannot rewrite, through the `?`. §FS-rhei-reset.2.3
    if let Some(refusal) = reset_refusal(loaded, input, scope, machines, &authored)? {
        return Err(refusal.report());
    }
    // So is one the filesystem would stop, with the same text from both runs. §FS-rhei-reset.2.3
    if let Some(refusal) = reset_filesystem_refusal(loaded, input, scope, machines, &authored)? {
        return Err(refusal);
    }
    let left = TicketScopeLeft::collect(loaded, input, scope, machines, &authored);
    Ok(ResetDecision {
        scope: scope.rheis(),
        tickets: scope.tickets().cloned(),
        task_count,
        descendant_count: total_nodes.saturating_sub(task_count),
        authored,
        runtime_targets: reset_runtime_preview(loaded, input, scope),
        left,
    })
}

/// Describe what a reset is about to destroy, and which tasks it would move.
/// The preview and the summary print the same move list, so what the dry run
/// promises and what the reset reports are the same text. §FS-rhei-reset.4
fn report_reset_preview(decision: &ResetDecision) {
    if decision.descendant_count == 0 {
        println!("Would reset {} task(s) to their authored states.", decision.task_count);
    } else {
        println!(
            "Would reset {} task(s) and {} subtask(s) to their authored states.",
            decision.task_count, decision.descendant_count
        );
    }
    report_state_moves(&decision.authored, "Would move", decision.tickets.as_ref());
    decision.left.report("Would hold");
    if decision.runtime_targets.is_empty() {
        // §FS-rhei-reset.2.1: the preview includes ticket-owned export directories.
        println!("Would remove per-ticket runtime artifacts (results, exports, ledgers).");
    } else {
        println!("Would delete, with every result and ledger inside:");
        for dir in &decision.runtime_targets {
            println!("  {}", dir.display());
        }
    }
}

/// §FS-rhei-reset.4
fn report_reset_summary(decision: &ResetDecision, removed_runtime: bool) {
    if decision.descendant_count == 0 {
        println!("Reset {} task(s) to their authored states.", decision.task_count);
    } else {
        println!(
            "Reset {} task(s) (and {} descendant task(s)) to their authored states.",
            decision.task_count, decision.descendant_count
        );
    }
    report_state_moves(&decision.authored, "Moved", decision.tickets.as_ref());
    decision.left.report("Held");
    if removed_runtime {
        println!("Removed runtime output.");
    } else {
        println!("No runtime output was present.");
    }
}

/// One runtime path a narrowed reset removes for a ticket: a fully resolved path,
/// a name rhei writes itself matched by its prefix, or a declared path that still
/// carries run-time placeholders (`{state}`, `{visit_count}`, `{model}`, …), matched
/// in full, component by component. §FS-rhei-reset.2.1
enum ScopedTarget {
    Exact(PathBuf),
    Prefixed(NamedPrefix),
    Pattern(ComponentPattern),
}

/// Everything keyed by an in-scope ticket id that a narrowed reset removes, and
/// nothing else: sibling rheis share one execution root. §FS-rhei-reset.2.1
struct ScopedRemovals {
    targets: Vec<ScopedTarget>,
    /// Ledger lines are pruned per execution root, once, after the per-ticket
    /// sweep: sibling rheis share one `state-transitions.log`.
    ledgers: BTreeMap<PathBuf, BTreeSet<String>>,
    /// Which tickets the reset keeps, so the removal leaves a path one could own.
    owners: RuntimeOwners,
    /// Roots holding a record keyed by an in-scope ticket's bare local id that a
    /// ticket scope did not sweep. §FS-rhei-reset.2.5.5
    bare_kept: BTreeSet<PathBuf>,
    /// Under a ticket scope, the tickets it removes anything for, a ledger row
    /// included, read before it does. §FS-rhei-reset.2.5.3
    losing: BTreeSet<String>,
}

/// The one enumeration of [`ScopedRemovals`], for the removal and for the check that it
/// can be made. §FS-rhei-reset.2.1 §FS-rhei-reset.2.3
fn scoped_removals(
    loaded: &LoadedPlan,
    input: &Path,
    scope: ResetScope<'_>,
    machines: &rhei_validator::MachineSet,
) -> ScopedRemovals {
    let mut targets = Vec::new();
    let mut task_ids: Vec<String> = Vec::new();
    fn collect(task: &rhei_core::ast::Task, out: &mut Vec<String>) {
        out.push(task.id.to_string());
        for child in &task.children {
            collect(child, out);
        }
    }
    for task in &loaded.rhei.tasks {
        collect(task, &mut task_ids);
    }

    let mut ledgers: BTreeMap<PathBuf, BTreeSet<String>> = BTreeMap::new();

    // Pre-qualification records are keyed by the rhei-local id; a local-id sweep at a root is
    // only unambiguous when every ticket rooted there is in scope — shared roots collide on
    // local ids. §FS-rhei-panta.6.4 §FS-rhei-reset.2.5.5
    let mut sweep_ok: BTreeMap<&PathBuf, bool> = BTreeMap::new();
    for (task_id, root) in &loaded.task_roots {
        *sweep_ok.entry(root).or_insert(true) &= scope.contains(task_id);
    }
    let legacy_sweep_ok = |root: &PathBuf| sweep_ok.get(root).copied().unwrap_or(false);
    let owners = RuntimeOwners::index(loaded, scope);
    let mut bare_kept = BTreeSet::new();
    let mut losing = BTreeSet::new();
    let mut ledger_rows: BTreeMap<PathBuf, BTreeMap<String, String>> = BTreeMap::new();

    // Run-orchestrated logs and captures land under the project execution
    // root even for tickets whose own rhei root is a subdirectory, so a
    // narrowed reset must sweep both roots. §FS-rhei-reset.2.1
    let project_root = execution_workspace_root(input);
    for task_id in task_ids.iter().filter(|id| scope.contains(id)) {
        let root = loaded.task_root(task_id, input);
        let ledger_ids = ledgers.entry(root.clone()).or_default();
        ledger_ids.insert(task_id.clone());
        let local_id = rhei_local_id_str(task_id);
        if local_id != task_id && legacy_sweep_ok(&root) {
            ledger_ids.insert(local_id.to_string());
        }
        let mut base_roots = vec![root.clone()];
        if root != project_root {
            base_roots.push(project_root.clone());
        }
        let mut own = Vec::new();
        for base in base_roots {
            let runtime = base.join("runtime");
            if !runtime.exists() {
                continue;
            }
            // Artifact-name patterns come from the owning ticket's machine.
            // §DA-per-rhei-state-machines
            let machine = machines.for_task_str(task_id);
            own.extend(scoped_runtime_targets(&runtime, task_id, machine));
            if local_id != task_id && legacy_sweep_ok(&base) {
                own.extend(scoped_runtime_targets(&runtime, local_id, machine));
            } else if local_id != task_id && scope.tickets().is_some() {
                // Kept, and said so where there is one to keep. §FS-rhei-reset.2.5.5
                let bare = scoped_runtime_targets(&runtime, local_id, machine);
                if bare.iter().any(|target| owners.takes_any(target)) {
                    bare_kept.insert(base);
                }
            }
        }
        // What the ticket loses decides whether its supervisor is held for it, and a
        // file it takes where a longer in-scope id could stand is lost by that ticket
        // too. §FS-rhei-reset.2.5.3
        if scope.tickets().is_some() {
            let rows = ledger_rows.entry(root.clone()).or_insert_with(|| ledger_first_departures(&root));
            let mut loses = rows.contains_key(task_id.as_str());
            for target in &own {
                let paths = owners.entries(target).unwrap_or_default();
                loses |= !paths.is_empty();
                for path in &paths {
                    losing.extend(owners.standing(target, path).cloned());
                }
            }
            if loses {
                losing.insert(task_id.clone());
            }
        }
        targets.extend(own);
    }
    if scope.tickets().is_some() {
        // A ledger row keyed by a bare local id is such a record too.
        for (root, ids) in ledgers.iter().filter(|(root, _)| !legacy_sweep_ok(root)) {
            let rows = ledger_first_departures(root);
            let bare = |id: &String| {
                let local = rhei_local_id_str(id);
                local != id.as_str() && rows.contains_key(local)
            };
            if ids.iter().any(bare) {
                bare_kept.insert(root.clone());
            }
        }
    }
    ScopedRemovals { targets, ledgers, owners, bare_kept, losing }
}

/// Remove what [`scoped_removals`] names, recording in `progress` what has landed.
/// §FS-rhei-reset.2.1 §FS-rhei-reset.4
fn remove_scoped_runtime_artifacts(
    loaded: &LoadedPlan,
    input: &Path,
    scope: ResetScope<'_>,
    machines: &rhei_validator::MachineSet,
    reset_locks: &mut ResetWriterLocks,
    progress: &mut ResetProgress,
) -> MietteResult<bool> {
    let removals = scoped_removals(loaded, input, scope, machines);
    let mut removed = false;
    for target in &removals.targets {
        removed |= remove_scoped_target(target, &removals.owners, &mut progress.removed_paths)?;
    }
    for (root, ids) in &removals.ledgers {
        let pruned = prune_transition_ledger(root, ids, reset_locks)?;
        progress.landed |= pruned;
        removed |= pruned;
    }
    Ok(removed)
}

/// Every runtime path keyed by `task_id` under one execution root's `runtime/`.
/// §FS-rhei-reset.2.1
fn scoped_runtime_targets(
    runtime: &Path,
    task_id: &str,
    machine: &rhei_validator::StateMachine,
) -> Vec<ScopedTarget> {
    let accounting_id = safe_accounting_file_segment(task_id);
    let mut targets = vec![
        // §FS-rhei-complete.4: the completion result file.
        ScopedTarget::Exact(runtime.join("results").join(format!("{task_id}.md"))),
        // §FS-rhei-reset.2.1: exact export homes are independent of machine artifact contracts.
        ScopedTarget::Exact(runtime.join("exports").join(task_id)),
        // §FS-rhei-agents.9 / §FS-rhei-programs.5: `task-<id>-<state>[-…].log`.
        ScopedTarget::Prefixed(NamedPrefix::new(runtime.join("logs"), "task-", task_id)),
        // The record of every spawn those logs came from. §FS-rhei-agents.8.4
        ScopedTarget::Prefixed(NamedPrefix::new(runtime.join("spawns"), "task-", task_id)),
        // The session reports rendered from those logs, `.orphaned-…` ones
        // included. §FS-rhei-reset.2.1
        ScopedTarget::Prefixed(NamedPrefix::new(runtime.join("reports"), "task-", task_id)),
        // §FS-rhei-snapshots.4: `<id>-<state>-<slug>-<nonce>/` session dirs.
        ScopedTarget::Prefixed(NamedPrefix::new(runtime.join("snapshot-sessions"), "", task_id)),
        ScopedTarget::Exact(runtime.join("worktree-refs").join(format!("{task_id}.yaml"))),
        // §FS-rhei-cost-accounting.2: per-ticket captures and task index.
        ScopedTarget::Prefixed(NamedPrefix::capture(runtime.join("accounting").join("captures"), task_id)),
        ScopedTarget::Exact(
            runtime.join("accounting").join("tasks").join(format!("{accounting_id}.json")),
        ),
    ];

    // §FS-rhei-states.6: artifact contracts are the machine's own declaration
    // of what a ticket writes, so a reset that leaves them behind would let a
    // stale output satisfy a required input on the next run.
    let root = runtime.parent().unwrap_or(runtime);
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for state in machine.states.values() {
        for artifact in state.inputs.iter().chain(state.outputs.iter()) {
            if !artifact.path.contains("{task_id}") || !seen.insert(artifact.path.clone()) {
                continue;
            }
            // A placeholder a reset cannot resolve, before the id or after it, leaves a
            // path matched in full, one component at a time. §FS-rhei-reset.2.1
            targets.push(if ComponentPattern::applies_to(&artifact.path) {
                ScopedTarget::Pattern(ComponentPattern::new(root, &artifact.path, task_id))
            } else {
                ScopedTarget::Exact(root.join(artifact.path.replace("{task_id}", task_id)))
            });
        }
    }
    targets
}

/// Remove what `target` names and its ticket owns, where no ticket the reset keeps
/// could own it. §FS-rhei-reset.2.1
fn remove_scoped_target(
    target: &ScopedTarget,
    owners: &RuntimeOwners,
    removed_paths: &mut Vec<PathBuf>,
) -> MietteResult<bool> {
    let paths = owners
        .entries(target)
        .map_err(|(dir, err)| file_io_report(&dir, "failed to read runtime directory", err))?;
    let mut removed = false;
    for path in paths {
        removed |= remove_runtime_path(&path, removed_paths)?;
    }
    Ok(removed)
}

/// The entries of `dir` whose name starts with `prefix`; none where `dir` is not a
/// directory. §FS-rhei-reset.2.1
fn matching_entries(dir: &Path, prefix: &str) -> std::io::Result<Vec<PathBuf>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    Ok(fs::read_dir(dir)?
        .flatten()
        .filter(|entry| entry.file_name().to_str().is_some_and(|name| name.starts_with(prefix)))
        .map(|entry| entry.path())
        .collect())
}

/// Remove one runtime path, adding to `removed_paths` each file that went, those
/// a failed removal took before it stopped included: they have landed. §FS-rhei-reset.4
fn remove_runtime_path(path: &Path, removed_paths: &mut Vec<PathBuf>) -> MietteResult<bool> {
    let files = files_under(path);
    if path.is_dir() {
        if let Err(err) = fs::remove_dir_all(path) {
            removed_paths.extend(files.into_iter().filter(|file| path_is_gone(file)));
            return Err(file_io_report(path, "failed to remove runtime directory", err));
        }
        removed_paths.extend(files);
        return Ok(true);
    }
    if path.exists() {
        fs::remove_file(path)
            .map_err(|err| file_io_report(path, "failed to remove runtime artifact", err))?;
        removed_paths.extend(files);
        return Ok(true);
    }
    Ok(false)
}

/// Drop the in-scope tickets' lines from one execution root's transition
/// ledger, so a reset ticket's recorded history matches its plan state.
/// Lines read `<task-id> <from>@<to>`. §FS-rhei-panta.6.4
fn prune_transition_ledger(
    root: &Path,
    task_ids: &BTreeSet<String>,
    reset_locks: &mut ResetWriterLocks,
) -> MietteResult<bool> {
    reset_locks.ledger(root)?.prune(task_ids)
}

/// Whether `path` itself is gone. A link is there until it is removed, wherever it
/// points, and a path that cannot be asked may still be there.
fn path_is_gone(path: &Path) -> bool {
    fs::symlink_metadata(path).is_err_and(|err| err.kind() == std::io::ErrorKind::NotFound)
}

/// Every file at or under `path`, as a removal is about to take them. A symlink is one
/// entry, as it is to the removal: following it would walk out of the tree, and round
/// and round where it leads back up. §FS-rhei-reset.2
fn files_under(path: &Path) -> Vec<PathBuf> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => fs::read_dir(path)
            .map(|entries| entries.flatten().flat_map(|entry| files_under(&entry.path())).collect())
            .unwrap_or_default(),
        Ok(_) => vec![path.to_path_buf()],
        Err(_) => Vec::new(),
    }
}
