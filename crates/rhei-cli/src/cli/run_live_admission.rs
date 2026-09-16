// Admission of Panta members discovered after an unrestricted run starts.
//
// The graph was always reloaded at scheduling boundaries, but everything that
// makes a task executable was formerly fixed at startup. This context keeps
// graph discovery and executable initialization as one scheduler operation.

// §FS-rhei-panta.6.2 §FS-rhei-run.2.5 §FS-rhei-run.2.6 §FS-rhei-run.3

struct LiveRunContext {
    machines: ExecutionMachines,
    settings: RheiSettings,
    run_locks: Vec<HeldRunLock>,
    locked_roots: BTreeSet<PathBuf>,
    initialized_rheis: BTreeSet<String>,
    state_machine_override: Option<PathBuf>,
}

impl LiveRunContext {
    fn new(
        loaded: &LoadedPlan,
        machines: ExecutionMachines,
        settings: RheiSettings,
        run_locks: Vec<HeldRunLock>,
        workspace_root: &Path,
        state_machine_override: Option<&Path>,
    ) -> Self {
        Self {
            machines,
            settings,
            run_locks,
            locked_roots: run_lock_roots(loaded, workspace_root),
            initialized_rheis: loaded.rhei_ids.iter().cloned().collect(),
            state_machine_override: state_machine_override.map(Path::to_path_buf),
        }
    }

    /// Strictly reload the project and completely initialize newly visible
    /// members before returning their tasks to a ready-set scan. Explicit
    /// `--rhei` scope is still applied by the caller, after this full-project
    /// validation and lock step. §FS-rhei-run.2.5 §FS-rhei-run.3
    fn checkpoint(
        &mut self,
        input: &Path,
        workspace_root: &Path,
        opts: &RunOptions,
        identity: &RunIdentity,
    ) -> MietteResult<(LoadedPlan, Vec<String>)> {
        let loaded = load_plan(input)?;
        let current: BTreeSet<String> = loaded.rhei_ids.iter().cloned().collect();
        let admitted: Vec<String> = current.difference(&self.initialized_rheis).cloned().collect();
        if admitted.is_empty() {
            return Ok((loaded, admitted));
        }

        // Resolve and validate the entire prospective graph before any of its
        // tasks can enter a ready set. §AR-rhei-panta.2 §AR-rhei-panta.4
        let resolved = resolve_state_machines_for_loaded_plan(
            input,
            &loaded,
            self.state_machine_override.as_deref(),
        )?;
        let machines = ExecutionMachines::build(&resolved, input, &loaded)?
            .with_state_machine_override(self.state_machine_override.as_deref());
        let settings = load_merged_settings(workspace_root)?;
        let mut report = rhei_validator::validate_with_machine_set(&loaded.rhei, &machines.set);
        report
            .errors
            .extend(validate_plan_settings_references(&loaded.rhei, &machines.set, &settings));
        report.errors.extend(validate_snapshot_plan_context(&loaded, &resolved));
        if report.has_errors() {
            return Err(validation_report(
                input,
                &resolved.validation_sources(&loaded.rhei_ids),
                &report.errors,
                &report.help,
            ));
        }
        report.warnings.dedup();
        for warning in &report.warnings {
            eprintln!("warning: {warning}");
        }

        // A project run already owns the project root, so adding the sorted
        // root delta cannot deadlock with a project-aware direct run. Every
        // root is held before the member becomes schedulable. §FS-rhei-run.2.6
        if !opts.dry_run() {
            let new_roots: BTreeSet<PathBuf> = run_lock_roots(&loaded, workspace_root)
                .difference(&self.locked_roots)
                .cloned()
                .collect();
            let mut new_locks = acquire_run_locks_for_roots(new_roots.iter(), opts)?;
            record_run_lock_ownership(&mut new_locks, identity)?;

            let scope = rhei_scope_set(opts.rhei_scope());
            validate_accounting_identity(&accounting_roots(&loaded, workspace_root, &scope), &scope)?;
            let roots = run_accounting_roots(&loaded, workspace_root, &scope);
            for root in &roots {
                validate_price_book_currency(&root.join("runtime/accounting"), opts.price_book())?;
            }
            if opts.prices_path().is_some() {
                for root in roots {
                    write_price_book(&root.join("runtime/accounting"), opts.price_book())?;
                }
            }

            self.locked_roots.extend(new_roots);
            self.run_locks.append(&mut new_locks);
        }

        self.machines = machines;
        self.machines.member_local_runtimes.extend(admitted.iter().cloned());
        self.settings = settings;
        self.initialized_rheis = current;
        Ok((loaded, admitted))
    }
}

/// Acquire a sorted root delta with the same foreground wait and detached-child
/// refusal as startup acquisition. §FS-rhei-run.2.6
fn acquire_run_locks_for_roots<'a>(
    roots: impl IntoIterator<Item = &'a PathBuf>,
    opts: &RunOptions,
) -> MietteResult<Vec<HeldRunLock>> {
    let mut locks = Vec::new();
    for root in roots {
        match try_acquire_run_lock(root)? {
            Some(lock) => locks.push(lock),
            None if is_headless_child() => return Err(run_lock_conflict(root)),
            None => {
                announce_run_lock_wait(root, opts.json());
                locks.push(wait_for_run_lock(root)?);
            }
        }
    }
    Ok(locks)
}
