// `rhei remove <ticket>` — take back a ticket nothing has acted on, and retire
// its id so no later create reissues it.
//
// This part resolves the ticket and runs the command's order: read everything
// first, refuse or preview without a single write, then lock, re-read, record
// the pending removal and commit it.

// §FS-rhei-remove

/// Execute the `remove` subcommand. §FS-rhei-remove.1
fn remove_command(
    input: Option<PathBuf>,
    task: Option<String>,
    rhei_scope: &[String],
    dry_run: bool,
    state_machine: Option<&Path>,
) -> MietteResult<()> {
    let (plan, ticket) = split_ticket_target(input, task)?;
    let Some(ticket) = ticket else {
        return Err(miette!(
            help = ticket_id_required_help(),
            "name the ticket to remove: `rhei remove <ticket-id>` (or `--task <ticket-id>`)"
        ));
    };
    let target = resolve_plan_target(plan)?;
    // Absolute, so a bare `plan.rhei.md` still has a directory to write beside.
    let input_buf = normalize_workspace_input(target.path());
    let input_buf = std::path::absolute(&input_buf).unwrap_or(input_buf);
    let input = input_buf.as_path();
    let project_root = execution_workspace_root(input);

    // An unfinished removal is finished by its own invocation and blocks every
    // other one. §FS-rhei-remove.6.2
    let marker = removal_marker_path(&project_root);
    if marker.exists() {
        let pending = rhei_core::root_access::pending_removal_ticket(&marker);
        let same = pending == ticket || pending.split_once('.').is_some_and(|(_, local)| local == ticket);
        if !same || dry_run {
            return Err(miette!(
                help = format!("finish it first: rhei remove {}", shell_quote(&pending)),
                "removal pending: {pending}; marker {}",
                display_path(&marker)
            ));
        }
        let _owned = rhei_core::root_access::own_pending_removal(&project_root);
        let (id, deleted) = resume_removal(input, state_machine, &marker)?;
        print_removed(&id, &deleted, &project_root);
        return Ok(());
    }

    let loaded = load_plan(input)?;
    // `--rhei` narrows which rhei a local id resolves in, never what is loaded.
    // §FS-rhei-remove.1.2
    let scope = resolve_rhei_scope(&loaded, &target.scope_with(rhei_scope))?;
    let Some(id) = resolve_removal_target(&loaded, &ticket, &scope)? else {
        return Ok(());
    };
    let resolved = resolve_state_machines_for_loaded_plan(input, &loaded, state_machine)?;
    let machines = resolved.validator_set();
    let assessment = assess_removal(&loaded, input, &id, machines.for_task_str(&id))?;
    if assessment.refused() {
        return Err(assessment.refusal(&id));
    }
    let images = plan_removal_images(&loaded, input, &id)?;

    // §FS-rhei-remove.7: every check above, and not one write.
    if dry_run {
        println!("Would remove {id}; would retire {id}");
        for image in &images.0 {
            println!("  would rewrite {}", relative_to(&project_root, &image.path));
        }
        for path in &assessment.residue {
            println!("  would delete {}", relative_to(&project_root, path));
        }
        println!("\nDry run — nothing was changed.");
        return Ok(());
    }

    // §FS-rhei-remove.6.1: the shared guards go before the exclusive ones come.
    let run_roots = run_lock_roots(&loaded, &project_root);
    let route = loaded.task_route(&id, input);
    let mut ledger_roots = vec![route.execution_root.clone(), project_root.clone()];
    ledger_roots.dedup();
    let files: Vec<PathBuf> = images.0.iter().map(|image| image.path.clone()).collect();
    drop(loaded);
    let _locks = acquire_removal_locks(run_roots, input, &id, &files, &ledger_roots)?;

    // Re-read and re-decide under the locks. §FS-rhei-remove.6.1
    let loaded = load_plan(input)?;
    if find_task_by_id_str(&loaded.rhei.tasks, &id).is_none() {
        return Err(miette!(
            help = "re-run it to see where the ticket went.",
            "{id} cannot be removed: it changed while removal took its locks"
        ));
    }
    let assessment = assess_removal(&loaded, input, &id, machines.for_task_str(&id))?;
    if assessment.refused() {
        return Err(assessment.refusal(&id));
    }
    let images = plan_removal_images(&loaded, input, &id)?;
    let held: BTreeSet<PathBuf> = files.into_iter().collect();
    if images.0.iter().any(|image| !held.contains(&image.path)) {
        return Err(miette!(
            help = "retry the removal after the concurrent plan-layout change has finished.",
            "{id} cannot be removed: the plan layout changed while removal took its locks"
        ));
    }
    let baseline = removal_validation_errors(input, state_machine);
    drop(loaded);

    // §FS-rhei-remove.6.2: the marker is synced before the first effect.
    let _owned = rhei_core::root_access::own_pending_removal(&project_root);
    let json = removal_marker_json(&id, &images, &assessment.residue);
    let bytes = serde_json::to_vec_pretty(&json)
        .map_err(|err| miette!(help = "report this as a rhei bug.", "failed to encode marker: {err}"))?;
    forced_replace(&marker, &bytes)?;
    let deleted =
        commit_removal(input, state_machine, &marker, &id, &images, &assessment.residue, &baseline)?;
    print_removed(&id, &deleted, &project_root);
    Ok(())
}

/// §FS-rhei-remove.8
fn print_removed(id: &str, deleted: &[PathBuf], project_root: &Path) {
    println!("removed {id}; id retired");
    for path in deleted {
        println!("  deleted {}", relative_to(project_root, path));
    }
}

/// The one qualified ticket the argument names, or `None` when it names a
/// ticket already removed — a repeat that exits 0. §FS-rhei-remove.1.2
/// §FS-rhei-remove.6.2
fn resolve_removal_target(
    loaded: &LoadedPlan,
    ticket: &str,
    scope: &RheiScope,
) -> MietteResult<Option<String>> {
    let ticket = ticket.trim();
    // §FS-rhei-remove.3.2
    if let Some(plan) = loaded.rhei_ids.iter().find(|id| *id == ticket).map(|id| loaded.rhei_plans.get(id)) {
        let named = plan.map(|plan| format!("; its plan is {}", display_path(plan))).unwrap_or_default();
        return Err(miette!(
            help = "name one of its tickets instead: `rhei list` shows them.",
            "{ticket} cannot be removed: it is a rhei, not a ticket{named}"
        ));
    }
    let live = |qualified: &str| find_task_by_id_str(&loaded.rhei.tasks, qualified).is_some();
    let retired = rhei_core::retired::retired_tickets(loaded.rhei.metadata.as_ref())
        .map_err(|err| miette!(help = "fix or remove the malformed retirement record.", "{err}"))?;
    let in_scope = |rhei_id: &String| scope.as_ref().is_none_or(|scope| scope.contains(rhei_id));
    let qualified: Vec<String> = loaded
        .rhei_ids
        .iter()
        .filter(|rhei_id| in_scope(rhei_id))
        .map(|rhei_id| format!("{rhei_id}.{ticket}"))
        .collect();
    // A retired id is no longer a ticket, so it only answers a local id nothing
    // live matches: the repeat that exits 0. §FS-rhei-remove.1.2
    let live_matches: Vec<String> = qualified.iter().filter(|q| live(q)).cloned().collect();
    let candidates: Vec<String> = if live(ticket) || retired.contains_key(ticket) {
        vec![ticket.to_string()]
    } else if !live_matches.is_empty() {
        live_matches
    } else {
        qualified.into_iter().filter(|q| retired.contains_key(q)).collect()
    };
    if candidates.len() > 1 {
        return Err(miette!(
            help = "name it qualified, or narrow the rhei with --rhei <id>.",
            "ticket '{ticket}' is ambiguous: {}",
            candidates.join(", ")
        ));
    }
    if let Some(candidate) = candidates.first() {
        if !live(candidate) && retired.contains_key(candidate) {
            println!("{candidate} is already removed; id retired");
            return Ok(None);
        }
    }
    let id = resolve_cli_task_id(loaded, ticket, scope)?;
    Ok(Some(id))
}
