// Previewing, reporting, and removing the runtime state `rhei reset` owns.

// §AR-source-file-size.3 §FS-rhei-reset.2.1 §FS-rhei-reset.4

/// Runtime directories a full reset would delete, in report order. A narrowed
/// reset removes per-ticket artifacts rather than whole trees, so it lists
/// none and the preview says so in words. §FS-rhei-panta.6.4
fn reset_runtime_preview(loaded: &LoadedPlan, input: &Path, scope: &RheiScope) -> Vec<PathBuf> {
    if scope.is_some() {
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

/// Describe what a reset is about to destroy, and which tasks it would move.
/// The preview and the summary print the same move list, so what the dry run
/// promises and what the reset reports are the same text. §FS-rhei-reset.4
fn report_reset_preview(
    task_count: usize,
    descendant_count: usize,
    authored: &AuthoredStates,
    runtime_dirs: &[PathBuf],
) {
    if descendant_count == 0 {
        println!("Would reset {task_count} task(s) to their authored states.");
    } else {
        println!(
            "Would reset {task_count} task(s) and {descendant_count} subtask(s) to their \
             authored states."
        );
    }
    report_state_moves(authored, "Would move");
    if runtime_dirs.is_empty() {
        println!("Would remove per-ticket runtime artifacts (results, ledgers).");
    } else {
        println!("Would delete, with every result and ledger inside:");
        for dir in runtime_dirs {
            println!("  {}", dir.display());
        }
    }
}

/// §FS-rhei-reset.4
fn report_reset_summary(
    task_count: usize,
    descendant_count: usize,
    authored: &AuthoredStates,
    removed_runtime: bool,
) {
    if descendant_count == 0 {
        println!("Reset {task_count} task(s) to their authored states.");
    } else {
        println!(
            "Reset {task_count} task(s) (and {descendant_count} descendant task(s)) to their \
             authored states."
        );
    }
    report_state_moves(authored, "Moved");
    if removed_runtime {
        println!("Removed runtime output.");
    } else {
        println!("No runtime output was present.");
    }
}

/// One runtime path a narrowed reset removes for a ticket: either a fully
/// resolved path, or a literal prefix within a directory when the artifact
/// template still carries run-time placeholders (`{state}`, `{visit_count}`,
/// `{model}`, …) that a reset cannot resolve.
enum ScopedTarget {
    Exact(PathBuf),
    Prefixed { dir: PathBuf, prefix: String },
}

/// Remove everything keyed by an in-scope ticket id — results, logs, declared
/// artifacts, snapshots, worktree refs, accounting, ledger lines — and nothing
/// else: sibling rheis share one execution root. §FS-rhei-reset.2.1
fn remove_scoped_runtime_artifacts(
    loaded: &LoadedPlan,
    input: &Path,
    scope: &RheiScope,
    machines: &rhei_validator::MachineSet,
    reset_locks: &mut ResetWriterLocks,
) -> MietteResult<bool> {
    let mut removed = false;
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

    // Ledger lines are pruned per execution root, once, after the per-ticket
    // sweep: sibling rheis share one `state-transitions.log`.
    let mut ledger_roots: BTreeMap<PathBuf, BTreeSet<String>> = BTreeMap::new();

    // Pre-qualification runtime records are keyed by the rhei-local id; a
    // local-id sweep at a root is only unambiguous when every rhei rooted
    // there is in scope — shared roots collide on local ids. §FS-rhei-panta.6.4
    let mut root_owners: BTreeMap<&PathBuf, BTreeSet<&str>> = BTreeMap::new();
    for (task_id, root) in &loaded.task_roots {
        let owner = task_id.split_once('.').map(|(head, _)| head).unwrap_or(task_id);
        root_owners.entry(root).or_default().insert(owner);
    }
    let legacy_sweep_ok = |root: &PathBuf| {
        root_owners
            .get(root)
            .is_some_and(|owners| owners.iter().all(|owner| task_in_rhei_scope(scope, owner)))
    };

    // Run-orchestrated logs and captures land under the project execution
    // root even for tickets whose own rhei root is a subdirectory, so a
    // narrowed reset must sweep both roots. §FS-rhei-reset.2.1
    let project_root = execution_workspace_root(input);
    for task_id in task_ids.iter().filter(|id| task_in_rhei_scope(scope, id)) {
        let root = loaded.task_root(task_id, input);
        let ledger_ids = ledger_roots.entry(root.clone()).or_default();
        ledger_ids.insert(task_id.clone());
        let local_id = rhei_local_id_str(task_id);
        if local_id != task_id && legacy_sweep_ok(&root) {
            ledger_ids.insert(local_id.to_string());
        }
        let mut base_roots = vec![root.clone()];
        if root != project_root {
            base_roots.push(project_root.clone());
        }
        for base in base_roots {
            let runtime = base.join("runtime");
            if !runtime.exists() {
                continue;
            }
            // Artifact-name patterns come from the owning ticket's machine.
            // §DA-per-rhei-state-machines
            let machine = machines.for_task_str(task_id);
            for target in scoped_runtime_targets(&runtime, task_id, machine) {
                removed |= remove_scoped_target(&target)?;
            }
            if local_id != task_id && legacy_sweep_ok(&base) {
                for target in scoped_runtime_targets(&runtime, local_id, machine) {
                    removed |= remove_scoped_target(&target)?;
                }
            }
        }
    }

    for (root, ids) in ledger_roots {
        removed |= prune_transition_ledger(&root, &ids, reset_locks)?;
    }
    Ok(removed)
}

/// Every runtime path keyed by `task_id` under one execution root's `runtime/`.
fn scoped_runtime_targets(
    runtime: &Path,
    task_id: &str,
    machine: &rhei_validator::StateMachine,
) -> Vec<ScopedTarget> {
    let accounting_id = safe_accounting_file_segment(task_id);
    let mut targets = vec![
        // §FS-rhei-complete.4: the completion result file.
        ScopedTarget::Exact(runtime.join("results").join(format!("{task_id}.md"))),
        // §FS-rhei-agents.9 / §FS-rhei-programs.5: `task-<id>-<state>[-…].log`.
        ScopedTarget::Prefixed { dir: runtime.join("logs"), prefix: format!("task-{task_id}-") },
        // The record of every spawn those logs came from. §FS-rhei-agents.8.4
        ScopedTarget::Prefixed { dir: runtime.join("spawns"), prefix: format!("task-{task_id}-") },
        // §FS-rhei-snapshots.4: `<id>-<state>-<slug>-<nonce>/` session dirs.
        ScopedTarget::Prefixed {
            dir: runtime.join("snapshot-sessions"),
            prefix: format!("{task_id}-"),
        },
        ScopedTarget::Exact(runtime.join("worktree-refs").join(format!("{task_id}.yaml"))),
        // §FS-rhei-cost-accounting.2: per-ticket captures and task index.
        ScopedTarget::Prefixed {
            dir: runtime.join("accounting").join("captures"),
            prefix: format!("{accounting_id}-"),
        },
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
            let resolved = artifact.path.replace("{task_id}", task_id);
            match resolved.split_once('{') {
                // A template with placeholders a reset cannot resolve becomes a
                // literal prefix; the text between `{task_id}` and the next
                // placeholder keeps `auth.1` from matching `auth.10`.
                Some((literal, _)) => {
                    let literal = root.join(literal);
                    let Some(dir) = literal.parent().map(Path::to_path_buf) else { continue };
                    let Some(prefix) =
                        literal.file_name().and_then(|name| name.to_str()).map(str::to_string)
                    else {
                        continue;
                    };
                    if !prefix.is_empty() {
                        targets.push(ScopedTarget::Prefixed { dir, prefix });
                    }
                }
                None => targets.push(ScopedTarget::Exact(root.join(resolved))),
            }
        }
    }
    targets
}

fn remove_scoped_target(target: &ScopedTarget) -> MietteResult<bool> {
    match target {
        ScopedTarget::Exact(path) => remove_runtime_path(path),
        ScopedTarget::Prefixed { dir, prefix } => {
            if !dir.is_dir() {
                return Ok(false);
            }
            let mut removed = false;
            for entry in fs::read_dir(dir)
                .map_err(|err| file_io_report(dir, "failed to read runtime directory", err))?
                .flatten()
            {
                let name = entry.file_name();
                let Some(name) = name.to_str() else { continue };
                if name.starts_with(prefix.as_str()) {
                    removed |= remove_runtime_path(&entry.path())?;
                }
            }
            Ok(removed)
        }
    }
}

fn remove_runtime_path(path: &Path) -> MietteResult<bool> {
    if path.is_dir() {
        fs::remove_dir_all(path)
            .map_err(|err| file_io_report(path, "failed to remove runtime directory", err))?;
        return Ok(true);
    }
    if path.exists() {
        fs::remove_file(path)
            .map_err(|err| file_io_report(path, "failed to remove runtime artifact", err))?;
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
