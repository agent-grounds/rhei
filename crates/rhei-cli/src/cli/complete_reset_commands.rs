/// Split the `<TICKET_OR_PLAN>` positional shared by every command that acts
/// on a single ticket between the two things it can name: a plan path or the
/// ticket id itself.
///
/// Returns the plan target (if one was named) and the ticket (if one was
/// resolved). A caller that requires a ticket reports its own absence, because
/// the sentence that helps depends on the command.
// §FS-rhei-usage.2: one ticket-argument shape across the command family.
fn split_ticket_target(
    input: Option<PathBuf>,
    task: Option<String>,
) -> MietteResult<(Option<PathBuf>, Option<String>)> {
    // With --task present the positional is the plan path — legacy behavior.
    if let Some(task) = task {
        return Ok((input, Some(task)));
    }
    let Some(positional) = input else {
        return Ok((None, None));
    };
    // An existing path wins over id shape, so a plan named like an id never
    // silently selects a ticket. §FS-rhei-complete.2.1
    if positional.exists() {
        return Ok((Some(positional), None));
    }
    let raw = positional.to_string_lossy();
    if is_ticket_id_shaped(&raw) {
        return Ok((None, Some(raw.into_owned())));
    }
    Err(miette!(
help = io_error_help(&positional, std::io::ErrorKind::NotFound),
"plan '{}' does not exist", positional.display()))
}

/// Split `rhei complete`'s positional, where the ticket is mandatory.
// §FS-rhei-complete.2.1: `rhei complete auth.1 --result "…"` works as pasted.
fn split_complete_ticket_target(
    input: Option<PathBuf>,
    task: Option<String>,
) -> MietteResult<(Option<PathBuf>, String)> {
    match split_ticket_target(input, task)? {
        (plan, Some(task)) => Ok((plan, task)),
        (Some(plan), None) => Err(miette!(
            help = ticket_id_required_help(),
            "'{}' is a plan path; name the ticket too: \
             `rhei complete <ticket-id> --result <message>` \
             (or `rhei complete {} --task <ticket-id> --result <message>`)",
            plan.display(),
            plan.display(),
        )),
        (None, None) => Err(miette!(
help = ticket_id_required_help(),

            "name the ticket to complete: `rhei complete <ticket-id> --result <message>` \
             (or `--task <ticket-id>`)"
        )),
    }
}

/// Split `rhei transition`'s positional, where the ticket is mandatory.
/// §FS-rhei-transition-cmd.1
fn split_transition_ticket_target(
    input: Option<PathBuf>,
    task: Option<String>,
) -> MietteResult<(Option<PathBuf>, String)> {
    match split_ticket_target(input, task)? {
        (plan, Some(task)) => Ok((plan, task)),
        (Some(plan), None) => Err(miette!(
            help = ticket_id_required_help(),
            "'{}' is a plan path; name the ticket too: \
             `rhei transition <ticket-id> --from <state> --to <state>` \
             (or `rhei transition {} --task <ticket-id> --from <state> --to <state>`)",
            plan.display(),
            plan.display(),
        )),
        (None, None) => Err(miette!(
help = ticket_id_required_help(),

            "name the ticket to transition: \
             `rhei transition <ticket-id> --from <state> --to <state>` \
             (or `--task <ticket-id>`)"
        )),
    }
}

/// Whether `raw` has the shape of a ticket id (`3`, `auth.1`): dot-separated
/// segments that are numbers or names, no path separators, and not a markdown
/// file name. §FS-rhei-complete.2.1
fn is_ticket_id_shaped(raw: &str) -> bool {
    if raw.is_empty() || raw.contains(['/', '\\']) || raw.ends_with(".md") {
        return false;
    }
    raw.split('.').all(|segment| {
        let mut chars = segment.chars();
        match chars.next() {
            Some(c) if c.is_ascii_digit() => segment.chars().all(|c| c.is_ascii_digit()),
            Some(c) if c.is_ascii_alphabetic() => {
                chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            }
            _ => false,
        }
    })
}

/// Execute the `complete` subcommand.
///
/// It is sugar, and deliberately thin: the readiness-class checks a scheduling
/// verb owns (already terminal, gating, unsatisfied `**Prior:**`), plus the one
/// thing genuinely its own — inferring the one-hop non-cancelled terminal
/// target — plus the shared transition carrying `--result`. The ledger line,
/// the result file, its link, the dropped assignee, and the refusal of a
/// terminal entry with no result all belong to the shared path, so the same
/// edge driven by `rhei transition --result` or by `rhei run` leaves the
/// identical trail.
// §FS-rhei-complete.4 §FS-rhei-complete.4.1
#[allow(clippy::too_many_arguments)]
fn complete_command(
    input: &Path,
    rhei_scope: &[String],
    state_machine_path: Option<&Path>,
    task_id_str: &str,
    result_msg: &str,
    no_callbacks: bool,
) -> MietteResult<()> {
    // §FS-rhei-complete.4: a blank `--result` is rejected before anything is
    // written — the flag is mandatory here precisely so the ticket records why.
    let result_msg = require_non_blank_result(Some(result_msg), "complete")?
        .expect("a Some input yields a Some result");
    let input_buf = normalize_workspace_input(input);
    let input = input_buf.as_path();
    let loaded = load_plan(input)?;
    // No `--rhei` on this command: the explicit ticket target is the scope,
    // narrowed by the rhei the invocation was pointed at. §FS-rhei-panta.6
    let scope = resolve_rhei_scope(&loaded, rhei_scope)?;
    let task_id_str = &resolve_cli_task_id(&loaded, task_id_str, &scope)?;
    let resolved = resolve_state_machines_for_loaded_plan(input, &loaded, state_machine_path)?;
    let machines = ExecutionMachines::build(&resolved, input)?;
    // One ticket is the whole scope: its machine and callback base govern.
    let machine = machines.for_task_str(task_id_str).clone();
    let callback_paths = machines.callbacks_for_str(task_id_str).clone();

    // Validate the plan first.
    let report = rhei_validator::validate_with_machine_set(&loaded.rhei, &machines.set);
    if report.has_errors() {
        return Err(validation_report(
            input,
            &resolved.validation_sources(&loaded.rhei_ids), // §FS-rhei-validate.6
            &report.errors,
            &report.help,
        ));
    }

    // Find the task and its current state.
    let target_id = parse_task_id(task_id_str);
    let task = find_task_by_id(&loaded.rhei.tasks, &target_id)
        .ok_or_else(|| miette!(
            help = task_id_help(),
            "task '{}' not found in the plan", task_id_str
        ))?;
    let current_state_raw = task.state.as_str();
    let current_state = normalized_state_name(current_state_raw, &machine);

    // Reject tasks already in a terminal state.
    if is_terminal_state(current_state_raw, &machine) {
        return Err(miette!(
            help = "nothing to do — the task is finished. Reopen it with: rhei reset <plan> <task>",
            "Task {} is already in terminal state '{}'",
            task_id_str,
            current_state_raw
        ));
    }
    if machine.states.get(&current_state).map(|def| def.gating).unwrap_or(false) {
        return Err(miette!(
            help = "a human gate is released explicitly: rhei transition <plan> <task> --to <state>",
            "Task {} cannot be completed from gating state '{}'; use an explicit human transition",
            task_id_str,
            current_state
        ));
    }

    // Descendants-first is not checked here: it is the shared transition path's
    // guard, so the rejection `rhei complete` produces is the one every other
    // verb produces. §FS-rhei-complete.4 §FS-rhei-transition-cmd.3.1

    // Completing ahead of a prerequisite makes the ticket terminal, which drops
    // it out of readiness and out of `rhei list --blocked` — the violation would
    // never surface again. §FS-rhei-complete.4
    let mut all_tasks = Vec::new();
    collect_plan_tasks(&loaded.rhei.tasks, &mut all_tasks);
    let state_map = plan_state_map(&all_tasks, &machines.set);
    let blocked_by = blocking_priors(task, &state_map, &machines.set);
    if !blocked_by.is_empty() {
        return Err(miette!(
help = "finish the blocking priors first, or move this ticket deliberately with: rhei transition <ticket-id> --from <state> --to <state>",

            "Task {} cannot be completed while its prerequisites are unsatisfied.\nBlocking priors: {}\n\
             Complete them first, or use `rhei transition` for a deliberate out-of-order move.",
            task_id_str,
            blocked_by.join(", ")
        ));
    }

    // Find the completion target: a non-cancelled terminal state reachable via
    // a single declared transition from the current state.
    let to_state = find_completion_state(&current_state, &machine).ok_or_else(|| {
        miette!(
            help = "the machine declares no terminal edge from that state. List the edges with: rhei states",
            "no transition to a terminal state available from '{}' for Task {}",
            current_state_raw,
            task_id_str
        )
    })?;

    // The shared path carries the message and owns everything else: callbacks,
    // artifact contracts, the guards, the ledger, the finalization.
    // §FS-rhei-complete.4
    let route = loaded.task_route(task_id_str, input);
    let effective_to = execute_transition(
        TransitionFiles {
            task_file: &route.task_file,
            metadata_file: &route.metadata_file,
            metadata_id: &route.metadata_id,
            artifact_root: &route.execution_root,
            artifact_id: task_id_str,
        },
        &callback_paths,
        &machine,
        &route.local_id,
        &current_state,
        &to_state,
        Some(result_msg),
        no_callbacks,
    )?;
    if !is_successful_completion_state(&effective_to, &machine) {
        // The redirect is the machine's decision and is already applied, as is
        // its finalization: a ticket redirected to `cancelled` is left cancelled
        // with the caller's message recorded against it. §FS-rhei-complete.4
        return Err(miette!(
            help = "inspect the machine and the task's state with: rhei states",
            "Task {} was redirected to '{}', which is not a successful completion state",
            task_id_str,
            effective_to
        ));
    }

    let result_link = format!("runtime/results/{}.md", task_id_str);
    println!(
        "Task {} completed: '{}' → '{}' ({})",
        task_id_str, current_state_raw, effective_to, result_link
    );

    Ok(())
}

/// Execute the `reset` subcommand: return every task in the tree to the
/// state it was authored in, recovered from the transition ledger.
///
/// For directory workspaces, this also removes the generated `runtime/`
/// directory so logs and artifacts do not survive the reset.
// §FS-rhei-reset.2.2
fn reset_command(
    input: &Path,
    state_machine_path: Option<&Path>,
    rhei_scope: &[String],
    dry_run: bool,
    assume_yes: bool,
) -> MietteResult<()> {
    let input_buf = normalize_workspace_input(input);
    let input = input_buf.as_path();
    let loaded = load_plan(input)?;
    let scope = resolve_rhei_scope(&loaded, rhei_scope)?;

    // A dry run is read-only and reports the snapshot it just loaded. A
    // destructive reset instead takes the complete writer stack below before
    // it decides what its preview promises. §FS-rhei-reset.1.2
    if dry_run {
        report_panta_scope_narrowed(&loaded, "reset", &scope);
        let resolved =
            resolve_state_machines_for_loaded_plan(input, &loaded, state_machine_path)?;
        let machines = resolved.validator_set();
        let decision = collect_reset_decision(&loaded, input, &scope, &machines);
        report_reset_preview(&decision);
        println!("\nDry run — nothing was changed.");
        return Ok(());
    }

    #[cfg(test)]
    run_reset_before_locks_hook();

    // Reset participates in the same metadata, distinct-task, then ledger
    // order as every ordinary writer. Its authoritative decision is made only
    // after the whole stack is held. §AR-agent-orchestrator-workflow.3.3.1
    let mut reset_locks = ResetWriterLocks::acquire(&loaded, input, &scope)?;
    let loaded = load_plan(input)?;
    let scope = resolve_rhei_scope(&loaded, rhei_scope)?;
    reset_locks.verify_coverage(&loaded, input, &scope)?;
    let resolved = resolve_state_machines_for_loaded_plan(input, &loaded, state_machine_path)?;
    let machines = resolved.validator_set();
    let decision = collect_reset_decision(&loaded, input, &scope, &machines);

    report_panta_scope_narrowed(&loaded, "reset", &decision.scope);
    // This preview, the mutation below, and the success summary all use the
    // same decision while every writer lock stays held. §FS-rhei-reset.1.2
    // §FS-rhei-reset.4
    report_reset_preview(&decision);
    #[cfg(test)]
    run_reset_after_preview_hook(&decision);

    if !assume_yes {
        // §FS-rhei-reset.1.2: with no terminal there is no one to answer, and
        // the destroyed material is typically gitignored with no VCS copy. Ask
        // for `-y` instead of assuming consent nobody gave.
        if !stdin_is_interactive() {
            return Err(miette!(
help = "re-run with -y to confirm, or --dry-run to preview what it would clear.",

                "`rhei reset` destroys runtime state and stdin is not a terminal, so it cannot \
                 ask for confirmation. Re-run with `-y` to confirm, or `--dry-run` to preview."
            ));
        }
        if !confirm("\nProceed?")? {
            println!("Cancelled — nothing was changed.");
            return Ok(());
        }
    }

    // Each plan file's tasks return to the states *that file* authored them
    // in; a file whose tasks never moved still has its runtime lines
    // (assignee, result links) stripped. §FS-rhei-reset.2.2
    let no_moves: BTreeMap<String, String> = BTreeMap::new();
    for (file, _sample_task_id) in reset_target_files(&loaded, input, &decision.scope) {
        let file_authored = decision.authored.by_file.get(&file).unwrap_or(&no_moves);
        reset_plan_file_states(&file, file_authored, reset_locks.plan(&file)?)?;
    }
    if workspace::is_workspace(input) {
        let index = input.join("index.rhei.md");
        clear_runtime_metadata_in_file(&index, true, reset_locks.plan(&index)?)?;
    }

    // §FS-rhei-panta.6.4: a narrowed reset removes per-ticket artifacts, never
    // whole `runtime/` trees — sibling rheis share one execution root.
    if decision.scope.is_some() {
        // §FS-rhei-panta.6.4: runtime ticket metadata (visit counts, poll
        // timers) in an in-scope workspace rhei's index is ticket-owned
        // state; leaving it would be a silent partial reset.
        let scoped_roots: BTreeSet<&PathBuf> = loaded
            .task_roots
            .iter()
            .filter(|(task_id, _)| task_in_rhei_scope(&decision.scope, task_id))
            .map(|(_, root)| root)
            .collect();
        for root in scoped_roots {
            if workspace::is_workspace(root) && root.as_path() != input {
                let index = root.join("index.rhei.md");
                clear_runtime_metadata_in_file(&index, true, reset_locks.plan(&index)?)?;
            }
        }
        let removed = remove_scoped_runtime_artifacts(
            &loaded,
            input,
            &decision.scope,
            &machines,
            &mut reset_locks,
        )?;
        report_reset_summary(&decision, removed);
        // A narrowed reset can only speak for ticket-owned artifacts; run-scoped
        // rollups belong to the run, not the ticket. Say so rather than leaving
        // the operator to discover the difference. §FS-rhei-panta.6.4
        println!(
            "Kept run-scoped output not owned by any ticket (run report, dashboard, \
             accounting rollups). Reset without `--rhei` to clear it."
        );
        #[cfg(test)]
        run_reset_before_unlock_hook(&decision);
        return Ok(());
    }

    if loaded.is_panta_project() {
        let mut roots: BTreeSet<PathBuf> = loaded.task_roots.values().cloned().collect();
        roots.insert(input.to_path_buf());
        for root in roots {
            if workspace::is_workspace(&root) {
                let index = root.join("index.rhei.md");
                clear_runtime_metadata_in_file(&index, true, reset_locks.plan(&index)?)?;
            }
        }
    }

    let mut removed_runtime = false;
    // Delete exactly the runtime directories named in the confirmed preview.
    // A cooperating writer cannot create another one until these locks leave.
    // §FS-rhei-reset.1.2 §FS-rhei-reset.4
    for runtime_dir in &decision.runtime_targets {
        if runtime_dir.exists() {
            fs::remove_dir_all(runtime_dir).map_err(|err| {
                file_io_report(runtime_dir, "failed to remove runtime directory", err)
            })?;
            removed_runtime = true;
        }
    }

    report_reset_summary(&decision, removed_runtime);
    #[cfg(test)]
    run_reset_before_unlock_hook(&decision);
    Ok(())
}

/// True when there is a human on stdin to answer a prompt.
fn stdin_is_interactive() -> bool {
    #[cfg(test)]
    if let Some(interactive) = reset_stdin_interactive_override() {
        return interactive;
    }

    use std::io::IsTerminal;
    std::io::stdin().is_terminal()
}

/// Ask a yes/no question, defaulting to no.
fn confirm(question: &str) -> MietteResult<bool> {
    #[cfg(test)]
    if let Some(answer) = run_reset_confirm_hook() {
        return Ok(answer);
    }

    use std::io::Write;
    print!("{question} [y/N] ");
    std::io::stdout().flush().map_err(|err| miette!(
help = internal_error_help(),
"failed to write prompt: {err}"))?;
    let mut answer = String::new();
    std::io::stdin()
        .read_line(&mut answer)
        .map_err(|err| miette!(
help = "re-run with -y to confirm without a prompt.",
"failed to read confirmation: {err}"))?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes" | "Yes"))
}

fn initial_state_for_node(
    machine: &rhei_validator::StateMachine,
    kind: &str,
    level: u8,
) -> MietteResult<String> {
    if let Some(profile) = machine.profile_for_node(kind, level) {
        return Ok(profile.initial.clone());
    }
    initial_state_name(machine)
}

fn initial_state_name(machine: &rhei_validator::StateMachine) -> MietteResult<String> {
    let initial_states = machine
        .states
        .iter()
        .filter(|(_, def)| def.initial)
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();

    match initial_states.as_slice() {
        [] => Err(miette!(
            help = state_machine_help(),
            "state machine '{}' does not declare an initial state", machine.name
        )),
        [initial] => Ok(initial.clone()),
        many => Err(miette!(
            help = state_machine_help(),
            "state machine '{}' declares multiple legacy initial states: {}",
            machine.name,
            many.join(", ")
        )),
    }
}
