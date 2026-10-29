// `rhei seal`: append a generation to each in-scope rhei's run-input seal,
// and the sealed-set resolution it shares with a strict run.

// §FS-rhei-seal.5

/// `rhei seal`'s arguments. §FS-rhei-seal.5
#[derive(Args, Debug)]
struct SealOptions {
    /// Path to a states YAML file; it is sealed in place of the machine it replaces
    #[arg(long, value_name = "PATH", add = ArgValueCompleter::new(complete_yaml_path))]
    state_machine: Option<PathBuf>,
    /// Plan or project to seal; omitted, the nearest enclosing one is used
    #[arg(value_name = "RHEI_PLAN", add = ArgValueCompleter::new(complete_rhei_plan_path))]
    input: Option<PathBuf>,
    /// Seal only the named rhei (repeatable; one id per flag)
    #[arg(long = "rhei", value_name = "RHEI_ID", add = ArgValueCompleter::new(complete_rhei_id))]
    rhei: Vec<String>,
}

/// One seal scope per rhei in `rhei_scope`, sorted by rhei id: each rhei's own
/// machine or the default it inherits, its execution root, and the settings
/// files and price book at `settings_root`. §FS-rhei-seal.2 §FS-rhei-seal.3.1
fn seal_scopes(
    loaded: &LoadedPlan,
    resolved: &ResolvedMachineSet,
    rhei_scope: &RheiScope,
    workspace_root: &Path,
    price_book: Option<&Path>,
) -> Vec<SealScope> {
    let mut ids: Vec<&String> = loaded.rhei_ids.iter().collect();
    ids.sort();
    ids.dedup();
    ids.into_iter()
        .filter(|id| rhei_scope.as_ref().is_none_or(|scope| scope.contains(id.as_str())))
        .map(|id| {
            let machine = resolved.per_rhei.get(id).unwrap_or(&resolved.default);
            let root = loaded.rhei_roots.get(id).map_or(workspace_root, PathBuf::as_path);
            // `parent()` of a bare file name is the empty path, which is the cwd.
            let root = if root.as_os_str().is_empty() { Path::new(".") } else { root };
            SealScope::new(id, root, machine.path.as_deref(), workspace_root, price_book)
        })
        .collect()
}

/// Execute `rhei seal`. It takes every run root without waiting, so a live run
/// is refused by name and nothing is written. §FS-rhei-seal.5
fn seal_command(
    input: &Path,
    state_machine_path: Option<&Path>,
    rhei: &[String],
) -> MietteResult<()> {
    let loaded = load_plan(input)?;
    let workspace_root = execution_workspace_root(input);
    let rhei_scope = resolve_rhei_scope(&loaded, rhei)?;
    let mut locks = Vec::new();
    for root in run_lock_roots(&loaded, &workspace_root) {
        match try_acquire_run_lock(&root)? {
            Some(lock) => locks.push(lock),
            None => return Err(run_lock_conflict(&root)),
        }
    }
    let resolved = resolve_state_machines_for_loaded_plan(input, &loaded, state_machine_path)?;
    // §FS-rhei-seal.2.2: the price book `defaults.prices` names, as a run resolves it.
    let settings = load_merged_settings(&workspace_root)?;
    let price_book = settings.prices.as_ref().map(|book| book.path.clone());
    for scope in seal_scopes(&loaded, &resolved, &rhei_scope, &workspace_root, price_book.as_deref()) {
        let taken = take_seal(&scope, SealTaker::Verb)?;
        println!("{}", sealed_line(&scope.rhei, &taken));
        if let Some((_, previous)) = &taken.previous {
            for change in seal_changes(previous, &taken.snapshot) {
                println!("  {:<9} {}", change.kind, change.path);
            }
        }
    }
    drop(locks);
    Ok(())
}
