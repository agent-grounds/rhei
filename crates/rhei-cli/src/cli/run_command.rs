/// Top-level tickets per owning plan file. Only top-level tickets are
/// independently schedulable — a subtask always executes inside its ticket's
/// slot — so descendants never add to a file's count. §FS-rhei-run.2.5
fn ticket_file_counts(loaded: &LoadedPlan) -> BTreeMap<&Path, usize> {
    let mut counts: BTreeMap<&Path, usize> = BTreeMap::new();
    for task in &loaded.rhei.tasks {
        if let Some(path) = loaded.task_sources.get(&task.id.to_string()) {
            *counts.entry(path.as_path()).or_default() += 1;
        }
    }
    counts
}

/// The artifact root a run composes every prompt against: the workspace the
/// input names, resolved to an absolute path before anything is loaded.
///
/// This is the one place a run turns what was typed into where things are.
/// Every root the run then derives — the execution root of each ticket, each
/// declared artifact, the `## Result` path, the snapshot session directory —
/// is a join onto this value, so a root left relative here reaches the worker
/// as a path only the launching shell could resolve: not the artifact root the
/// prompt names, not the worker's own working directory, and, with the
/// identity variables gone, nowhere in its environment either. Resolving it
/// once makes `rhei run ws` compose what `rhei run /absolute/ws` composes.
///
/// Absolute and nothing more. Canonicalizing here would also re-spell the
/// root: a caller who types `/var/…` on macOS, or a short path on Windows,
/// would be handed `/private/var/…` and a long one back — a path they never
/// typed, in every prompt and every error. Whether this root and the checkout
/// root are one directory is a separate question, and
/// `roots_are_one_directory` resolves both sides to answer it.
// §FS-rhei-agents.4.1
fn run_artifact_root(input: &Path) -> PathBuf {
    let input = normalize_workspace_input(input);
    std::path::absolute(&input).unwrap_or(input)
}

/// The execution root a run works under: its artifact root, or the plan file's
/// directory when the input named a single file.
///
/// Taken from the run's own input and not from the plan path resolved for
/// callbacks, which is canonical. The two name one directory, but only this
/// one is spelled the way the caller spelled it — and the run's `runtime/`
/// tree hangs off it, so the other would put the transcripts directory in
/// every prompt under a name nothing else in that prompt uses.
// §FS-rhei-agents.4.1
fn run_execution_root(input: &Path) -> PathBuf {
    execution_workspace_root(input)
}

/// Every execution root a run locks: its own plus each rhei's, so project
/// and member-rhei runs contend on the same lock. Canonicalized and sorted —
/// one global acquisition order, no lock-order deadlock. §FS-rhei-run.2.6
fn run_lock_roots(loaded: &LoadedPlan, workspace_root: &Path) -> BTreeSet<PathBuf> {
    // Dedup is what keeps this set safe to lock: `flock` on a second
    // descriptor for an inode this process already holds blocks forever. A
    // ticket root can arrive as the empty path — `parent()` of a bare
    // relative filename — which canonicalizes to nothing and so slips past
    // dedup as a second name for the run's own directory.
    let canonical = |root: &Path| {
        let root = if root.as_os_str().is_empty() { Path::new(".") } else { root };
        rhei_core::platform::canonical_path(root).unwrap_or_else(|_| root.to_path_buf())
    };
    let mut roots = BTreeSet::new();
    roots.insert(canonical(workspace_root));
    for root in loaded.task_roots.values() {
        roots.insert(canonical(root));
    }
    roots
}

/// Accounting roots whose agents can participate in this invocation: the run
/// root plus each in-scope ticket's owning rhei root. §FS-rhei-cost-accounting.5.1
fn run_accounting_roots(
    loaded: &LoadedPlan,
    workspace_root: &Path,
    rhei_scope: &RheiScope,
) -> BTreeSet<PathBuf> {
    let mut roots = BTreeSet::from([workspace_root.to_path_buf()]);
    for (task_id, root) in &loaded.task_roots {
        if task_in_rhei_scope(rhei_scope, task_id) {
            roots.insert(root.clone());
        }
    }
    roots
}

/// One run's identity: the id it is named by everywhere, and when it began.
/// Computed once, as soon as the run holds its locks, so the run report, the
/// descriptor, and `rhei attach <id>` all speak about the same run by the same
/// name — rather than each execution mode deriving its own. Stamping it before
/// the locks made `started_at` the time the command was typed, which is not
/// when a queued run began.
// §FS-rhei-run.2.7
struct RunIdentity {
    id: String,
    started: Instant,
    started_wall: std::time::SystemTime,
    /// Whether this process is the detached child of a `--headless` launch.
    headless: bool,
}

impl RunIdentity {
    fn new() -> Self {
        let started_wall = std::time::SystemTime::now();
        Self {
            id: short_run_id(started_wall),
            started: Instant::now(),
            started_wall,
            headless: is_headless_child(),
        }
    }
}

/// Stamp each opened lock inode with the stable Linux process identity that a
/// later read-only liveness probe needs after the pathname is displaced.
// §FS-rhei-run-headless.3
#[cfg(target_os = "linux")]
fn record_run_lock_ownership(
    locks: &mut [HeldRunLock],
    identity: &RunIdentity,
) -> MietteResult<()> {
    for lock in locks {
        write_run_lock_owner(lock, &identity.id, std::process::id()).map_err(|reason| {
            miette!(
                help = "check that /proc is mounted and the workspace lock file is writable",
                "could not record run-lock ownership for {}: {reason}",
                lock.workspace.display()
            )
        })?;
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn record_run_lock_ownership(
    _locks: &mut [HeldRunLock],
    _identity: &RunIdentity,
) -> MietteResult<()> {
    Ok(())
}

/// Take the run lock of every involved execution root.
///
/// A **detached child** must not wait: its launcher is holding a handshake
/// open, and a blocked child turns a lock refusal into a 30-second "did not
/// report itself ready" for a run that never had a chance. It fails fast with
/// the same diagnostic the launcher's own pre-check gives — which is what
/// catches the case a per-workspace launch lock cannot: two launches on
/// different member plans that share a root.
///
/// A **foreground** run keeps blocking, because waiting on a contended lock is
/// a queueing idiom people use on purpose. It says whose run it is waiting for
/// first: silent blocking is indistinguishable from a hang.
// §FS-rhei-run.2.6 §FS-rhei-run-headless.1.1
fn acquire_run_locks(
    loaded: &LoadedPlan,
    workspace_root: &Path,
    opts: &RunOptions,
) -> MietteResult<Vec<HeldRunLock>> {
    let roots = run_lock_roots(loaded, workspace_root);
    acquire_run_locks_for_roots(roots.iter(), opts)
}

/// How often a queued run re-tries the lock it is waiting for.
const RUN_LOCK_WAIT_POLL: Duration = Duration::from_millis(200);

/// Wait for a contended run lock the way the rest of the run waits: in slices
/// the interrupt handler can end.
///
/// A blocking `flock` is not one of them. The handler sets a flag and returns,
/// so a process parked inside `flock` goes straight back into the syscall and
/// the operator's Ctrl+C does nothing at all — which is what turned the wait
/// this run now announces into a wait it could not cancel. The queueing
/// behaviour is unchanged: it still blocks until the lock frees.
// §FS-rhei-run.2.6 §FS-rhei-run.3.2
fn wait_for_run_lock(root: &Path) -> MietteResult<HeldRunLock> {
    loop {
        if let Some(lock) = try_acquire_run_lock(root)? {
            return Ok(lock);
        }
        if interrupt_requested() {
            return Err(miette!(
                help = "the run that holds it is untouched; `rhei runs` shows what is live",
                "stopped waiting for the run lock on {}",
                root.display()
            ));
        }
        interruptible_sleep(RUN_LOCK_WAIT_POLL);
    }
}

/// One line naming the holder before a foreground run blocks on its lock.
// §FS-rhei-run.2.6 §FS-rhei-run-json.1
fn announce_run_lock_wait(root: &Path, json: bool) {
    let holder = read_descriptor(&run_descriptor_path(root))
        .filter(|run| !run.liveness().has_ended())
        .map(|run| format!("run {} (pid {})", run.id, run.pid))
        .unwrap_or_else(|| "another run".to_string());
    let line = format!("Waiting for {holder} to release the run lock on {}...", root.display());
    if json || stdout_carries_json_records() {
        eprintln!("{line}");
    } else {
        println!("{line}");
    }
}

/// Execute the `run` subcommand: advance tasks through the state machine
/// in dependency order.
///
/// In agent mode (the default when an agent is configured), spawns coding
/// agents for each task. In callback-only mode (`--no-agent`), advances
/// tasks through transition callbacks only.
fn run_command(
    input: &Path,
    state_machine_path: Option<&Path>,
    mut opts: RunOptions,
) -> MietteResult<()> {
    // `--headless` re-executes this same command in a detached session and
    // returns its id; everything below then runs in the *child*. Checked first
    // so a launch takes no locks and starts no frontend of its own.

    // §FS-rhei-run-headless.1
    if opts.headless() && !is_headless_child() {
        return launch_headless_run(input, &opts);
    }
    // From here on, a human-oriented line goes to stderr. §FS-rhei-run-json.1
    if opts.json() {
        reserve_stdout_for_json_records();
    }
    // Installed for every `run`, before anything can be spawned: from here on a
    // SIGINT/SIGTERM/SIGHUP interrupts the run instead of killing the
    // supervisor out from under its subprocesses. §FS-rhei-run.3.2
    install_interrupt_handlers();
    let input_buf = run_artifact_root(input);
    let input = input_buf.as_path();
    let loaded = load_plan_for_run(input, &opts, state_machine_path)?;
    let rhei_scope = resolve_rhei_scope(&loaded, opts.rhei_scope())?;
    report_panta_scope_narrowed(&loaded, "run", &rhei_scope);
    let resolved = resolve_state_machines_for_loaded_plan(input, &loaded, state_machine_path)?;
    let machines =
        ExecutionMachines::build(&resolved, input, &loaded)?.with_state_machine_override(state_machine_path);
    let workspace_root = run_execution_root(input);
    let custom_price_book = opts.prices_path().map(Path::to_path_buf);
    if let Some(path) = custom_price_book.as_deref() {
        opts.select_price_book(load_price_book(path)?);
    }
    let settings = load_merged_settings(&workspace_root)?;
    // §FS-rhei-run.2.6: one live run per rhei — lock every involved
    // execution root, not just the run's own.
    let mut run_locks =
        if opts.dry_run() { Vec::new() } else { acquire_run_locks(&loaded, &workspace_root, &opts)? };
    // Stamped only now: a run that queued behind someone else's lock began
    // when it got the lock, not when it was typed, and `rhei runs` orders by
    // this. §FS-rhei-run.2.7
    let identity = RunIdentity::new();
    record_run_lock_ownership(&mut run_locks, &identity)?;
    // §FS-rhei-run.3.1: detect subprocess commits that leave run-owned state dirty.
    let git_consistency = RunGitConsistencyGuard::capture(&workspace_root, input, !opts.dry_run());

    // §FS-rhei-run.2.5: when every ticket lives in one plan file, parallelism
    // can only schedule same-file tickets against one checkout — fall back to
    // sequential, as for a bare single-file plan.
    let ticket_counts = ticket_file_counts(&loaded);
    let shared_file =
        ticket_counts.iter().find(|(_, count)| **count > 1).map(|(path, _)| (*path).to_path_buf());
    let single_shared_file = ticket_counts.len() == 1 && shared_file.is_some();
    let effective_parallel = if opts.parallel() > 1 && single_shared_file {
        eprintln!(
            "warning: --parallel > 1 is not supported when every ticket lives in one \
             plan file (risk of conflicting edits). Falling back to sequential execution."
        );
        1
    } else {
        // A project keeps parallelism, but two tickets of one rhei file are
        // still concurrent work against a single checkout — say so instead
        // of silently dropping the single-file warning. §FS-rhei-run.2.5
        if opts.parallel() > 1 {
            if let Some(shared) = &shared_file {
                eprintln!(
                    "warning: --parallel > 1 schedules tickets from the same rhei file \
                     concurrently ({}); plan-file writes serialize on the sibling sidecar, but \
                     agents may still collide in the shared checkout.",
                    shared.display()
                );
            }
        }
        opts.parallel()
    };

    // Initial validation pass.
    let mut report = rhei_validator::validate_with_machine_set(&loaded.rhei, &machines.set);
    report
        .errors
        .extend(validate_plan_settings_references(&loaded.rhei, &machines.set, &settings));
    report.errors.extend(validate_snapshot_plan_context(&loaded, &resolved));
    if report.has_errors() {
        return Err(validation_report(
            input,
            &resolved.validation_sources(&loaded.rhei_ids), // §FS-rhei-validate.6
            &report.errors,
            &report.help,
        ));
    }
    // A warning is legal, so the run proceeds. Keep one ordered copy of each
    // initial warning for the selected frontend to emit after `RunStarted`.
    // §FS-rhei-validate.4 §FS-rhei-run.3
    report.warnings.dedup();

    if !opts.dry_run() {
        // §FS-rhei-cost-accounting.11 §FS-rhei-panta.6.5: identity spans the selected root union.
        validate_accounting_identity(
            &accounting_roots(&loaded, &workspace_root, &rhei_scope),
            &rhei_scope,
        )?;
        let accounting_roots = run_accounting_roots(&loaded, &workspace_root, &rhei_scope);
        // Every root is preflighted while all run locks are held and before
        // any root is mutated or execution starts. §FS-rhei-cost-accounting.5.1
        for root in &accounting_roots {
            validate_price_book_currency(&root.join("runtime/accounting"), opts.price_book())?;
        }
        // The caller-owned book is durable everywhere this run can start an
        // agent before the first such process is spawned. §FS-rhei-cost-accounting.5.1
        if custom_price_book.is_some() {
            for root in accounting_roots {
                write_price_book(&root.join("runtime/accounting"), opts.price_book())?;
            }
        }
    }

    let roots = ReadySetRoots { workspace_root: &workspace_root, task_roots: &loaded.task_roots };
    let use_standalone_mode = should_use_agent_mode(
        &loaded.rhei,
        &machines.set,
        &settings,
        &opts,
        &roots,
    )? || (loaded.is_panta_project()
        && opts.rhei_scope().is_empty()
        && !opts.dry_run()
        && !(opts.no_agent() && opts.no_program()));
    // Agent mode also advances callback-only work and can execute agents or
    // programs introduced by admitted members. §FS-rhei-panta.6.2 §FS-rhei-run.3

    let mut live = LiveRunContext::new(
        &loaded,
        machines,
        settings,
        run_locks,
        &workspace_root,
        state_machine_path,
    );
    let result = if use_standalone_mode {
        run_agent_mode(
            input,
            &mut live,
            &opts,
            effective_parallel,
            &report.warnings,
            &identity,
        )
    } else {
        run_callback_mode(
            input,
            &mut live,
            &opts,
            effective_parallel,
            &report.warnings,
            &identity,
        )
    };
    result?;
    // An interrupted run made no claim of durable success, so the commit
    // postcondition has nothing to check; a run torn down by its own failure
    // still owes it. §FS-rhei-run.3.1 §FS-rhei-run.3.2
    if interrupted_by_signal() {
        return Ok(());
    }
    git_consistency.verify_after_success()
}

// §AR-rhei-panta.5: the mode probe reads the same ready set the run will, so it
// resolves a member's inputs at the owning rhei's execution root too.
fn should_use_agent_mode(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    settings: &RheiSettings,
    opts: &RunOptions,
    roots: &ReadySetRoots<'_>,
) -> MietteResult<bool> {
    if !opts.no_agent()
        && machines.distinct().iter().any(|machine| {
            machine
                .states
                .values()
                .any(|def| !def.terminal && !def.gating && state_declares_autonomous_agent_work(def))
        })
    {
        return Ok(true);
    }

    // A future retry deadline chooses when the program runs, not whether this
    // invocation keeps the subprocess engine capable of running it. The probe
    // shares every non-temporal readiness constraint. §FS-rhei-run.3
    if !opts.no_program()
        && !narrow_to_rhei_scope(
            find_runnable_program_polls_for_mode_selection(rhei, machines, roots, opts),
            &rhei_scope_set(opts.rhei_scope()),
        )
        .is_empty()
    {
        return Ok(true);
    }

    for task in narrow_to_rhei_scope(
        find_runnable_tasks_with_options(rhei, machines, roots, &HashSet::new(), opts),
        &rhei_scope_set(opts.rhei_scope()),
    ) {
        let machine = machines.for_task(&task.id);
        let state_name = normalized_state_name(task.state.as_str(), machine);
        let Some(def) = machine.states.get(&state_name) else {
            continue;
        };
        if def.terminal || def.gating {
            continue;
        }
        if def.program.is_some() && !opts.no_program() {
            return Ok(true);
        }
        if !opts.no_agent() {
            let invocations = resolve_agent_invocations_for_task(
                machine,
                &state_name,
                settings,
                opts,
                Some(task),
            )?;
            if !invocations.is_empty() || state_declares_autonomous_agent_work(def) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
