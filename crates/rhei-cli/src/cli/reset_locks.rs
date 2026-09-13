// The complete stable-writer lock set held by `rhei reset` while it restores
// authored plan state and removes the corresponding runtime history.

// §AR-source-file-size.3 §AR-agent-orchestrator-workflow.3.3.1

/// All plan/metadata and ledger locks for one reset persistence boundary.
///
/// Paths are canonicalized, sorted, and deduplicated before acquisition.
/// Metadata precedes distinct task files, and every plan lock precedes every
/// ledger lock. The `Drop` implementation releases that stack in reverse.
struct ResetWriterLocks {
    plan_locks: Vec<(PathBuf, LockedPlanFile)>,
    ledger_locks: Vec<(PathBuf, LockedTransitionLedger)>,
}

impl ResetWriterLocks {
    fn acquire(loaded: &LoadedPlan, input: &Path, scope: &RheiScope) -> MietteResult<Self> {
        let (metadata_paths, task_paths) = reset_plan_lock_paths(loaded, input, scope)?;
        let ledger_roots = reset_ledger_roots(loaded, input, scope)?;

        let mut locks = Self {
            plan_locks: Vec::with_capacity(metadata_paths.len() + task_paths.len()),
            ledger_locks: Vec::with_capacity(ledger_roots.len()),
        };
        for path in metadata_paths.into_iter().chain(task_paths) {
            locks.plan_locks.push((path.clone(), LockedPlanFile::open(&path)?));
        }
        for root in ledger_roots {
            locks.ledger_locks.push((root.clone(), LockedTransitionLedger::lock(&root)?));
        }
        Ok(locks)
    }

    /// Refuse a concurrently changed project layout rather than touching a
    /// plan or ledger pathname outside the lock set derived before acquisition.
    fn verify_coverage(
        &self,
        loaded: &LoadedPlan,
        input: &Path,
        scope: &RheiScope,
    ) -> MietteResult<()> {
        let (metadata_paths, task_paths) = reset_plan_lock_paths(loaded, input, scope)?;
        let plan_paths = metadata_paths.into_iter().chain(task_paths).collect::<BTreeSet<_>>();
        let held_plan_paths =
            self.plan_locks.iter().map(|(path, _)| path.clone()).collect::<BTreeSet<_>>();
        let ledger_roots = reset_ledger_roots(loaded, input, scope)?;
        let held_ledger_roots =
            self.ledger_locks.iter().map(|(root, _)| root.clone()).collect::<BTreeSet<_>>();
        if plan_paths != held_plan_paths || ledger_roots != held_ledger_roots {
            return Err(miette!(
                help = "retry the reset after the concurrent plan-layout change has finished.",
                "the plan layout changed while reset acquired its writer locks"
            ));
        }
        Ok(())
    }

    fn plan(&self, path: &Path) -> MietteResult<&LockedPlanFile> {
        let path = canonical_reset_plan_path(path)?;
        self.plan_locks
            .iter()
            .find(|(held, _)| held == &path)
            .map(|(_, lock)| lock)
            .ok_or_else(|| {
                miette!(
                    help = "retry the reset so it can acquire the complete writer-lock set.",
                    "reset does not hold the plan lock for {}",
                    path.display()
                )
            })
    }

    fn ledger(&mut self, root: &Path) -> MietteResult<&mut LockedTransitionLedger> {
        let root = canonical_reset_root(root)?;
        self.ledger_locks
            .iter_mut()
            .find(|(held, _)| held == &root)
            .map(|(_, lock)| lock)
            .ok_or_else(|| {
                miette!(
                    help = "retry the reset so it can acquire the complete ledger-lock set.",
                    "reset does not hold the transition ledger lock for {}",
                    root.display()
                )
            })
    }
}

impl Drop for ResetWriterLocks {
    fn drop(&mut self) {
        while self.ledger_locks.pop().is_some() {}
        while self.plan_locks.pop().is_some() {}
    }
}

/// Canonical parent plus exact destination name, matching `plan_lock_path`.
fn canonical_reset_plan_path(path: &Path) -> MietteResult<PathBuf> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = rhei_core::platform::canonical_path(parent)
        .map_err(|err| file_io_report(parent, "failed to resolve reset lock path", err))?;
    let name = path.file_name().ok_or_else(|| {
        miette!(
            help = "the reset plan path must name a file.",
            "failed to derive reset lock path for {}",
            path.display()
        )
    })?;
    Ok(parent.join(name))
}

fn canonical_reset_root(path: &Path) -> MietteResult<PathBuf> {
    rhei_core::platform::canonical_path(path)
        .map_err(|err| file_io_report(path, "failed to resolve reset lock path", err))
}

/// Metadata and distinct task paths in their required acquisition groups.
fn reset_plan_lock_paths(
    loaded: &LoadedPlan,
    input: &Path,
    scope: &RheiScope,
) -> MietteResult<(BTreeSet<PathBuf>, BTreeSet<PathBuf>)> {
    let mut metadata_paths = BTreeSet::new();
    let mut task_paths = reset_target_files(loaded, input, scope)
        .into_iter()
        .map(|(path, _)| canonical_reset_plan_path(&path))
        .collect::<MietteResult<BTreeSet<_>>>()?;

    fn collect<'a>(task: &'a rhei_core::ast::Task, out: &mut Vec<&'a rhei_core::ast::Task>) {
        out.push(task);
        for child in &task.children {
            collect(child, out);
        }
    }
    let mut tasks = Vec::new();
    for task in &loaded.rhei.tasks {
        if task_in_rhei_scope(scope, &task.id.to_string()) {
            collect(task, &mut tasks);
        }
    }
    for task in tasks {
        let route = loaded.task_route(&task.id.to_string(), input);
        metadata_paths.insert(canonical_reset_plan_path(&route.metadata_file)?);
    }

    // Preserve the metadata files reset already clears even for empty scopes.
    if workspace::is_workspace(input) {
        metadata_paths.insert(canonical_reset_plan_path(&input.join("index.rhei.md"))?);
    }
    let mut roots = loaded.task_roots.values().collect::<BTreeSet<_>>();
    if scope.is_some() {
        roots.retain(|root| {
            loaded.task_roots.iter().any(|(task_id, candidate)| {
                candidate == *root && task_in_rhei_scope(scope, task_id)
            })
        });
    }
    for root in roots {
        if workspace::is_workspace(root) {
            metadata_paths.insert(canonical_reset_plan_path(&root.join("index.rhei.md"))?);
        }
    }

    task_paths.retain(|path| !metadata_paths.contains(path));
    Ok((metadata_paths, task_paths))
}

/// Execution roots whose ledger or containing runtime tree this reset changes.
fn reset_ledger_roots(
    loaded: &LoadedPlan,
    input: &Path,
    scope: &RheiScope,
) -> MietteResult<BTreeSet<PathBuf>> {
    let mut roots = BTreeSet::new();
    fn collect(task: &rhei_core::ast::Task, out: &mut Vec<String>) {
        out.push(task.id.to_string());
        for child in &task.children {
            collect(child, out);
        }
    }
    let mut task_ids = Vec::new();
    for task in &loaded.rhei.tasks {
        if task_in_rhei_scope(scope, &task.id.to_string()) {
            collect(task, &mut task_ids);
        }
    }
    for task_id in task_ids {
        roots.insert(canonical_reset_root(&loaded.task_route(&task_id, input).execution_root)?);
    }

    if scope.is_none() {
        if loaded.is_panta_project() || workspace::is_workspace(input) {
            roots.insert(canonical_reset_root(input)?);
        } else {
            roots.insert(canonical_reset_root(&execution_workspace_root(input))?);
        }
    }
    Ok(roots)
}

#[cfg(test)]
type ResetDecisionHook = Box<dyn FnOnce(&ResetDecision)>;

#[cfg(test)]
thread_local! {
    static RESET_BEFORE_LOCKS_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        const { std::cell::RefCell::new(None) };
    static RESET_AFTER_PREVIEW_HOOK: std::cell::RefCell<Option<ResetDecisionHook>> =
        const { std::cell::RefCell::new(None) };
    static RESET_BEFORE_UNLOCK_HOOK: std::cell::RefCell<Option<ResetDecisionHook>> =
        const { std::cell::RefCell::new(None) };
    static RESET_STDIN_INTERACTIVE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
    static RESET_CONFIRM_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce() -> bool>>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn set_reset_before_locks_hook(hook: impl FnOnce() + 'static) {
    RESET_BEFORE_LOCKS_HOOK.with(|installed| *installed.borrow_mut() = Some(Box::new(hook)));
}

#[cfg(test)]
fn run_reset_before_locks_hook() {
    let hook = RESET_BEFORE_LOCKS_HOOK.with(|installed| installed.borrow_mut().take());
    if let Some(hook) = hook {
        hook();
    }
}

/// Pause after the authoritative preview while the complete stack is held.
/// Focused tests start real writers in the consent window. §FS-rhei-reset.1.2
#[cfg(test)]
fn set_reset_after_preview_hook(hook: impl FnOnce(&ResetDecision) + 'static) {
    RESET_AFTER_PREVIEW_HOOK.with(|installed| *installed.borrow_mut() = Some(Box::new(hook)));
}

#[cfg(test)]
fn run_reset_after_preview_hook(decision: &ResetDecision) {
    let hook = RESET_AFTER_PREVIEW_HOOK.with(|installed| installed.borrow_mut().take());
    if let Some(hook) = hook {
        hook(decision);
    }
}

/// Pause after summary and cleanup but before the lock stack leaves scope.
/// The decision argument exposes the exact data the summary reported.
#[cfg(test)]
fn set_reset_before_unlock_hook(hook: impl FnOnce(&ResetDecision) + 'static) {
    RESET_BEFORE_UNLOCK_HOOK.with(|installed| *installed.borrow_mut() = Some(Box::new(hook)));
}

#[cfg(test)]
fn run_reset_before_unlock_hook(decision: &ResetDecision) {
    let hook = RESET_BEFORE_UNLOCK_HOOK.with(|installed| installed.borrow_mut().take());
    if let Some(hook) = hook {
        hook(decision);
    }
}

#[cfg(test)]
fn set_reset_confirmation(interactive: bool, hook: impl FnOnce() -> bool + 'static) {
    RESET_STDIN_INTERACTIVE.with(|value| value.set(Some(interactive)));
    RESET_CONFIRM_HOOK.with(|installed| *installed.borrow_mut() = Some(Box::new(hook)));
}

#[cfg(test)]
fn reset_stdin_interactive_override() -> Option<bool> {
    RESET_STDIN_INTERACTIVE.with(std::cell::Cell::get)
}

#[cfg(test)]
fn run_reset_confirm_hook() -> Option<bool> {
    RESET_CONFIRM_HOOK
        .with(|installed| installed.borrow_mut().take())
        .map(|hook| hook())
}
