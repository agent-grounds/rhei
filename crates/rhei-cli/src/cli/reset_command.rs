// `rhei reset`'s own flow: the read, the decision taken again under the writer
// locks, the confirmation, and what it says once it is done.
//
// Its own part because the flow is the one place a reset's scope is resolved:
// every step below it is handed the scope and only asks what is in it.

// §AR-source-file-size.3 §FS-rhei-reset.2

/// The two switches every reset takes. §FS-rhei-reset.1.1
#[derive(Clone, Copy)]
struct ResetFlags {
    dry_run: bool,
    assume_yes: bool,
}

/// Execute the `reset` subcommand: return every task in the tree to the
/// state it was authored in, recovered from the transition ledger.
///
/// For directory workspaces, this also removes the generated `runtime/`
/// directory so logs and artifacts do not survive the reset. The CLI runs
/// [`reset_scoped_command`]; this is the reset of whole rheis the tests drive.
// §FS-rhei-reset.2.2
#[cfg(test)]
fn reset_command(
    input: &Path,
    state_machine_path: Option<&Path>,
    rhei_scope: &[String],
    dry_run: bool,
    assume_yes: bool,
) -> MietteResult<()> {
    let flags = ResetFlags { dry_run, assume_yes };
    reset_scoped_command(input, state_machine_path, rhei_scope, &[], flags)
}

/// [`reset_command`], narrowed to the tickets `tasks` reaches when it names any. Then
/// `rhei_scope` bounds where those ids resolve, not how far the reset reaches.
/// §FS-rhei-reset.2.5 §FS-rhei-reset.2.5.4
fn reset_scoped_command(
    input: &Path,
    state_machine_path: Option<&Path>,
    rhei_scope: &[String],
    tasks: &[String],
    flags: ResetFlags,
) -> MietteResult<()> {
    let input_buf = normalize_workspace_input(input);
    let input = input_buf.as_path();
    let loaded = load_plan(input)?;
    let scope = resolve_rhei_scope(&loaded, rhei_scope)?;
    // §FS-rhei-reset.2.5.4: an unknown or ambiguous id is refused before any preview.
    let named = TicketScope::resolve_ids(&loaded, tasks, &scope)?;

    // A dry run is read-only and reports the snapshot it just loaded. A
    // destructive reset instead takes the complete writer stack below before
    // it decides what its preview promises. §FS-rhei-reset.1.2
    if flags.dry_run {
        if named.is_empty() {
            report_panta_scope_narrowed(&loaded, "reset", &scope);
        }
        let resolved =
            resolve_state_machines_for_loaded_plan(input, &loaded, state_machine_path)?;
        let tickets = TicketScope::of_named(&loaded, &named, &scope, &resolved);
        if let Some(tickets) = &tickets {
            report_ticket_scope(&loaded, tickets);
        }
        let reset_scope = ResetScope::of(&scope, tickets.as_ref());
        let decision = collect_reset_decision(&loaded, input, reset_scope, &resolved)?;
        report_reset_preview(&decision);
        println!("\nDry run — nothing was changed.");
        return Ok(());
    }

    #[cfg(test)]
    run_reset_before_locks_hook();

    // The first read's closure decides which files the locks below cover. §FS-rhei-reset.2.5.5
    let first_tickets = if named.is_empty() {
        None
    } else {
        let resolved = resolve_state_machines_for_loaded_plan(input, &loaded, state_machine_path)?;
        TicketScope::of_named(&loaded, &named, &scope, &resolved)
    };
    let first_scope = ResetScope::of(&scope, first_tickets.as_ref());
    // A lock it cannot create would stop it below without the dry run's refusal, so
    // that case asks the unlocked snapshot, as the dry run does. §FS-rhei-reset.2.3
    if reset_sidecars_are_blocked(&loaded, input, first_scope)? {
        let resolved =
            resolve_state_machines_for_loaded_plan(input, &loaded, state_machine_path)?;
        collect_reset_decision(&loaded, input, first_scope, &resolved)?;
    }

    // Reset participates in the same metadata, distinct-task, then ledger
    // order as every ordinary writer. Its authoritative decision is made only
    // after the whole stack is held. §AR-agent-orchestrator-workflow.3.3.1
    let mut reset_locks = ResetWriterLocks::acquire(&loaded, input, first_scope)?;
    let loaded = load_plan(input)?;
    let scope = resolve_rhei_scope(&loaded, rhei_scope)?;
    let named = TicketScope::resolve_ids(&loaded, tasks, &scope)?;
    if named.is_empty() {
        reset_locks.verify_coverage(&loaded, input, (&scope).into())?;
    }
    let resolved = resolve_state_machines_for_loaded_plan(input, &loaded, state_machine_path)?;
    // The closure taken again from this re-read, which the held stack has to cover.
    // §FS-rhei-reset.2.5.5
    let tickets = TicketScope::of_named(&loaded, &named, &scope, &resolved);
    if let Some(tickets) = &tickets {
        reset_locks.verify_coverage(&loaded, input, ResetScope::Tickets(tickets))?;
    }
    // Judges this re-read, taken under the locks, not the snapshot above. §FS-rhei-reset.2
    let reset_scope = ResetScope::of(&scope, tickets.as_ref());
    let decision = collect_reset_decision(&loaded, input, reset_scope, &resolved)?;
    let machines = resolved.validator_set();

    match &tickets {
        Some(tickets) => report_ticket_scope(&loaded, tickets),
        None => report_panta_scope_narrowed(&loaded, "reset", &decision.scope),
    }
    // This preview, the mutation below, and the success summary all use the
    // same decision while every writer lock stays held. §FS-rhei-reset.1.2
    // §FS-rhei-reset.4
    report_reset_preview(&decision);
    #[cfg(test)]
    run_reset_after_preview_hook(&decision);

    if !flags.assume_yes {
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

    let mut progress = ResetProgress::default();
    let removed = apply_reset(&loaded, input, &decision, &machines, &mut reset_locks, &mut progress)
        .map_err(|err| progress.stopped(err))?;
    report_reset_summary(&decision, removed);

    if decision.reset_scope().is_narrowed() {
        report_narrowed_reset_kept(&loaded, input, &decision, &progress.removed_paths);
    }
    #[cfg(test)]
    run_reset_before_unlock_hook(&decision);
    Ok(())
}

/// What a narrowed reset kept and why, after its summary. §FS-rhei-reset.2.1 §FS-rhei-reset.2.5
fn report_narrowed_reset_kept(
    loaded: &LoadedPlan,
    input: &Path,
    decision: &ResetDecision,
    removed_paths: &[PathBuf],
) {
    // The store is kept and the removals recorded for verify. §FS-rhei-evidence.10.1
    if let Some(event) = write_reset_event(loaded, input, decision.reset_scope(), removed_paths) {
        println!("Kept run evidence in runtime/evidence/; recorded the removed paths as reset event {event}.");
    }
    // A narrowed reset can only speak for ticket-owned artifacts; run-scoped
    // rollups belong to the run, not the ticket. Say so rather than leaving
    // the operator to discover the difference. §FS-rhei-panta.6.4

    // The note store is kept for the same reason and named beside them: one
    // shared journal charged to every plan, not keyed by a ticket id.
    // §FS-rhei-reset.2.1 §FS-rhei-note.3.1 §FS-rhei-seal.6: seals are keyed by rhei.

    // §FS-rhei-reset.2.5: the flag to drop is the one that narrowed it.
    let flag = if decision.tickets.is_some() { "--task" } else { "--rhei" };
    println!(
        "Kept run-scoped output not owned by any ticket (run report, dashboard, \
         accounting rollups), the project note store (runtime/notes.md), and the \
         run-input seals (runtime/seal/). Reset without `{flag}` to clear them; for a \
         fresh seal of one rhei, run `rhei seal --rhei <id>`."
    );
}
