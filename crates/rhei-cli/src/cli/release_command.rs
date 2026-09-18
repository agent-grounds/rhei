// `rhei release` — drop a ticket's `**Assignee:**` so abandoned work can be
// picked up again, without touching its state, artifacts, or ledger.
// §FS-rhei-release

/// Execute the `release` subcommand.
fn release_command(
    input: &Path,
    state_machine_path: Option<&Path>,
    task_id_str: Option<&str>,
    all: bool,
    rhei_scope: &[String],
    dry_run: bool,
) -> MietteResult<()> {
    if task_id_str.is_some() == all {
        return Err(miette!(
help = ticket_id_required_help(),

            "`rhei release` takes either one ticket — `rhei release <ticket-id>`, or \
             `--task <id>` — or `--all`, which sweeps every claimed ticket in scope"
        ));
    }

    let input_buf = normalize_workspace_input(input);
    let input = input_buf.as_path();
    let loaded = load_plan(input)?;
    let scope = resolve_rhei_scope(&loaded, rhei_scope)?;
    let resolved = resolve_state_machines_for_loaded_plan(input, &loaded, state_machine_path)?;
    let machines = resolved.validator_set();

    let targets = match task_id_str {
        Some(id) => vec![release_target_by_id(&loaded, id, &scope, &machines)?],
        None => {
            report_panta_scope_narrowed(&loaded, "release", &scope);
            claimed_tickets(&loaded, &machines, &scope)
        }
    };

    if targets.is_empty() {
        println!("No claimed tickets to release.");
        return Ok(());
    }

    for target in &targets {
        let verb = if dry_run { "Would release" } else { "Released" };
        println!("{verb} Task {} (was assigned to {})", target.id, target.assignee);
        // Automatic `next` only claims from the initial state. A released ticket remains
        // explicitly reclaimable or can be deliberately reset, without `release` moving it.
        // §FS-rhei-release.3.1
        if let Some(initial) = target.initial_state.as_deref() {
            let machine = machines.for_task_str(&target.id);
            let state = normalized_state_name(&target.state, machine);
            // A supervisor is claimed where it stands, and a name the machine
            // does not declare cannot be transitioned to.
            // §FS-rhei-supervision.3.4 §FS-rhei-release
            let claimable_where_it_stands = execute_on_of(machine, &state).is_some();
            let target_exists = machine.states.contains_key(initial);
            if state != initial && !claimable_where_it_stands && target_exists {
                let plan = shell_quote(&input.display().to_string());
                println!(
                    "  note: still in '{}'. Automatic `rhei next` claims from '{}'; reclaim it \
                     explicitly with `rhei next {} --task {}`, or move it back with \
                     `rhei transition --task {} --from {} --to {}`.",
                    target.state,
                    initial,
                    plan,
                    target.id,
                    target.id,
                    target.state,
                    initial
                );
            }
        }
    }

    if dry_run {
        println!("\nDry run — nothing was changed.");
        return Ok(());
    }

    for target in &targets {
        remove_task_assignee(&target.file, &target.local_id, &target.id)?;
    }
    Ok(())
}

/// A ticket whose claim `rhei release` will drop.
struct ReleaseTarget {
    id: String,
    local_id: String,
    assignee: String,
    state: String,
    /// The state `rhei next` claims this ticket's node kind from, when the
    /// machine defines one.
    initial_state: Option<String>,
    file: PathBuf,
}

/// Resolve an explicit `--task` target, refusing a ticket that holds no claim.
fn release_target_by_id(
    loaded: &LoadedPlan,
    task_id_str: &str,
    scope: &RheiScope,
    machines: &rhei_validator::MachineSet,
) -> MietteResult<ReleaseTarget> {
    let task_id_str = resolve_cli_task_id(loaded, task_id_str, scope)?;
    let target_id = parse_task_id(&task_id_str);
    let task = find_task_by_id(&loaded.rhei.tasks, &target_id)
        .ok_or_else(|| miette!(
help = task_id_help(),
"task '{}' not found in the plan", task_id_str))?;
    let Some(assignee) = task.assignee.clone() else {
        return Err(miette!(
help = "nothing to release. See who holds what with: rhei list <plan>",

            "Task {} holds no claim — it has no **Assignee:** to release",
            task_id_str
        ));
    };
    Ok(release_target(loaded, task, assignee, machines.for_task(&task.id)))
}

/// Every claimed, non-terminal ticket in scope.
fn claimed_tickets(
    loaded: &LoadedPlan,
    machines: &rhei_validator::MachineSet,
    scope: &RheiScope,
) -> Vec<ReleaseTarget> {
    let mut all = Vec::new();
    collect_plan_tasks(&loaded.rhei.tasks, &mut all);
    all.into_iter()
        .filter(|task| task_in_rhei_scope(scope, &task.id.to_string()))
        // A terminal ticket keeping an assignee is a record of who finished it,
        // not a claim blocking anyone; a sweep must not erase that.
        .filter(|task| !is_terminal_state(task.state.as_str(), machines.for_task(&task.id)))
        .filter_map(|task| {
            task.assignee
                .clone()
                .map(|assignee| release_target(loaded, task, assignee, machines.for_task(&task.id)))
        })
        .collect()
}

fn release_target(
    loaded: &LoadedPlan,
    task: &rhei_core::ast::Task,
    assignee: String,
    machine: &rhei_validator::StateMachine,
) -> ReleaseTarget {
    let id = task.id.to_string();
    let file = loaded.task_file(&id, Path::new("."));
    ReleaseTarget {
        local_id: rhei_local_id_of(task),
        id,
        assignee,
        state: task.state.clone(),
        initial_state: initial_state_for_node(machine, &task.kind, task.profile_level()).ok(),
        file,
    }
}

/// One task's markdown without its `**Assignee:**` line, and whether there was
/// one to drop.
///
/// Task files are addressed by the rhei-local id they were authored with, so a
/// heading match uses that rather than the project-qualified id. Kept apart
/// from the file write so the transition path — which rewrites the same text in
/// memory and may still roll it back — can drop a claim through it too.
fn without_task_assignee(raw: &str, local_id: &str) -> (String, bool, bool) {
    let mut out: Vec<String> = Vec::with_capacity(raw.lines().count());
    let mut in_target_task = false;
    let mut target_found = false;
    let mut removed = false;
    let mut in_code_block = false;

    for line in raw.lines() {
        if let Some((_, id)) = node_heading_outside_code(line, &mut in_code_block) {
            in_target_task = id == local_id;
            target_found |= in_target_task;
        }
        if !in_code_block && in_target_task && line.starts_with("**Assignee:**") {
            removed = true;
            continue;
        }
        out.push(line.to_string());
    }

    let mut output = out.join("\n");
    if raw.ends_with('\n') {
        output.push('\n');
    }
    (output, target_found, removed)
}

/// Drop the target task's `**Assignee:**` line under the shared writer lock,
/// leaving every other line alone. §AR-agent-orchestrator-workflow.3.3.1
fn remove_task_assignee(
    task_file: &Path,
    local_id: &str,
    qualified_id: &str,
) -> MietteResult<()> {
    let locked = LockedPlanFile::open(task_file)?;
    let raw = locked.read_to_string("failed to read plan file")?;

    let (output, target_found, removed) = without_task_assignee(&raw, local_id);

    if !target_found {
        return Err(miette!(
help = task_id_help(),
"task '{}' not found in {}", qualified_id, task_file.display()));
    }
    if !removed {
        // The parse said the ticket was claimed, so a missing line means the
        // file moved under us — better to say so than to report a no-op write.
        return Err(miette!(
help = task_moved_help(),

            "task '{}' has no **Assignee:** line in {}; the file changed since it was read",
            qualified_id,
            task_file.display()
        ));
    }

    write_file_atomic_locked(task_file, &output, Some(&locked))
}
