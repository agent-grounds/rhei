/// Avoid a parent/shared → child/shared → force/exclusive wait cycle on fair locks. §FS-rhei-recover.4
fn headless_startup_run_locks(input: &Path) -> MietteResult<(BTreeSet<PathBuf>, Vec<HeldRunLock>)> {
    // §FS-rhei-errors.6
    let roots = rhei_core::root_access::input_roots(input).map_err(|err| root_access_report(input, err))?
        .into_iter().collect::<BTreeSet<_>>();
    // §FS-rhei-errors.6
    for root in &roots { rhei_core::root_access::check_pending(root).map_err(|err| root_access_report(root, err))?; }
    let mut locks = Vec::new();
    for root in &roots {
        let Some(lock) = try_acquire_run_lock(root)? else {
            // Lock-owner evidence is available without reading an in-doubt runtime descriptor.
            let owner = read_run_lock_owner(&root.join(".rhei/run.lock"))
                .unwrap_or_else(|err| format!("owner record unreadable: {err}"));
            // §FS-rhei-errors.6
            return Err(miette!(help = format!("attach to the existing run: rhei attach {}; wait for it to finish before starting another run.", shell_quote(&root.display().to_string())), "a run is already live on {} and holds its .rhei/run.lock; recorded owner: {}",
                root.display(), owner.trim()));
        };
        locks.push(lock);
    }
    Ok((roots, locks))
}
