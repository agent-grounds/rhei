// `rhei remove`'s write half: every lock it takes without waiting, the
// pending-removal marker that makes it resumable, and the install, validate and
// clean sequence that commits it or puts every byte back.
//
// Its own part because a resumed removal runs this exact sequence from a
// marker, never from a fresh decision.

// §FS-rhei-remove.6.1 §FS-rhei-remove.6.2

/// Everything removal holds from its re-read to its last cleanup, released in
/// reverse. §FS-rhei-remove.6.1
struct RemovalLocks {
    _sidecars: Vec<fs::File>,
    _guards: Vec<rhei_core::root_access::RootAccessGuard>,
    _run_locks: Vec<HeldRunLock>,
}

impl Drop for RemovalLocks {
    fn drop(&mut self) {
        for sidecar in &self._sidecars {
            let _ = fs2::FileExt::unlock(sidecar);
        }
    }
}

/// Try-acquire, in order, every run lock, every exclusive owner guard and every
/// sidecar; refuse on the first one held. §FS-rhei-remove.3.3 §FS-rhei-remove.6.1
fn acquire_removal_locks(
    run_roots: BTreeSet<PathBuf>,
    input: &Path,
    id: &str,
    files: &[PathBuf],
    ledger_roots: &[PathBuf],
) -> MietteResult<RemovalLocks> {
    let refuse = |what: String| {
        miette!(
            help = "wait for it to finish, or stop the run holding it, then remove again.",
            "{id} cannot be removed: {what}"
        )
    };
    let mut run_locks = Vec::new();
    for root in run_roots {
        match try_acquire_run_lock(&root)? {
            Some(lock) => run_locks.push(lock),
            None => return Err(refuse(format!("{}", run_lock_conflict(&root)))),
        }
    }
    let roots = rhei_core::root_access::input_roots(input).map_err(|err| diagnostic!("{err}"))?;
    let mut guards = Vec::new();
    for root in roots {
        match rhei_core::root_access::RootAccessGuard::try_exclusive(&root) {
            Ok(Some(guard)) => guards.push(guard),
            Ok(None) => {
                return Err(refuse(format!("another rhei process holds {}", display_path(&root))))
            }
            Err(err) => return Err(refuse(format!("{err}"))),
        }
    }
    let mut sidecar_paths = Vec::new();
    for file in files {
        sidecar_paths.push(plan_lock_path(file)?);
    }
    for root in ledger_roots {
        sidecar_paths.push(root.join("runtime.state-transitions.log.lock"));
    }
    let mut seen = BTreeSet::new();
    sidecar_paths.retain(|path| seen.insert(path.clone()));
    let mut sidecars = Vec::new();
    for path in sidecar_paths {
        let sidecar = fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .map_err(|err| file_io_report(&path, "failed to open lock file", err))?;
        match sidecar.try_lock_exclusive() {
            Ok(()) => sidecars.push(sidecar),
            Err(err) if lock_is_contended(&err) => {
                return Err(refuse(format!("a writer holds {}", display_path(&path))))
            }
            Err(err) => return Err(file_io_report(&path, "failed to acquire lock", err)),
        }
    }
    Ok(RemovalLocks { _sidecars: sidecars, _guards: guards, _run_locks: run_locks })
}

/// Where an unfinished removal is recorded. §FS-rhei-remove.6.2
fn removal_marker_path(project_root: &Path) -> PathBuf {
    project_root.join(rhei_core::root_access::REMOVAL_MARKER)
}

/// What the marker records: the ticket, every image, every residue path.
/// §FS-rhei-remove.6.2
fn removal_marker_json(id: &str, images: &RemovalImages, residue: &[PathBuf]) -> serde_json::Value {
    let encode = |text: &str| rhei_core::transition_history::encode(text.as_bytes());
    serde_json::json!({
        "version": 1,
        "ticket": id,
        "files": images.0.iter().map(|image| serde_json::json!({
            "path": image.path.to_string_lossy(),
            "before": image.before.as_deref().map(encode),
            "after": encode(&image.after),
        })).collect::<Vec<_>>(),
        "residue": residue.iter().map(|path| path.to_string_lossy()).collect::<Vec<_>>(),
    })
}

/// Read a marker back into images and residue. §FS-rhei-remove.6.2
fn read_removal_marker(marker: &Path) -> MietteResult<(String, RemovalImages, Vec<PathBuf>)> {
    let corrupt = |why: String| {
        miette!(
            help = "the marker is rhei's record of an unfinished removal; restore it from a backup \
                    or ask for help before deleting it.",
            "cannot read pending removal {}: {why}",
            display_path(marker)
        )
    };
    let raw = fs::read(marker).map_err(|err| file_io_report(marker, "failed to read", err))?;
    let value: serde_json::Value =
        serde_json::from_slice(&raw).map_err(|err| corrupt(err.to_string()))?;
    let decode = |value: &serde_json::Value| -> MietteResult<String> {
        let bytes = rhei_core::transition_history::decode(value.as_str().unwrap_or_default())
            .map_err(|err| corrupt(err.to_string()))?;
        String::from_utf8(bytes).map_err(|err| corrupt(err.to_string()))
    };
    let ticket = value["ticket"].as_str().ok_or_else(|| corrupt("no ticket".into()))?.to_string();
    let mut images = RemovalImages::default();
    for file in value["files"].as_array().ok_or_else(|| corrupt("no files".into()))? {
        let path = PathBuf::from(file["path"].as_str().ok_or_else(|| corrupt("a file has no path".into()))?);
        let before = if file["before"].is_null() { None } else { Some(decode(&file["before"])?) };
        images.0.push(RemovalImage { path, before, after: decode(&file["after"])? });
    }
    let residue = value["residue"]
        .as_array()
        .map(|paths| paths.iter().filter_map(|path| path.as_str().map(PathBuf::from)).collect())
        .unwrap_or_default();
    Ok((ticket, images, residue))
}

/// The validation errors the project has right now, as `rhei new` reads them.
/// §FS-rhei-remove.4.1
fn removal_validation_errors(input: &Path, state_machine: Option<&Path>) -> Vec<String> {
    match validation_pass(input, state_machine) {
        Ok(pass) => pass.errors,
        Err(report) => vec![report.to_string()],
    }
}

fn install_image(path: &Path, image: Option<&str>) -> MietteResult<()> {
    match image {
        Some(text) => forced_replace(path, text.as_bytes()),
        None => forced_remove(path),
    }
}

/// Install the after-images, validate, then clean the residue and clear the
/// marker — or put every before-image back if the result does not validate.
/// The caller holds every lock and has written the marker. §FS-rhei-remove.4.1
/// §FS-rhei-remove.6.2
fn commit_removal(
    input: &Path,
    state_machine: Option<&Path>,
    marker: &Path,
    id: &str,
    images: &RemovalImages,
    residue: &[PathBuf],
    baseline: &[String],
) -> MietteResult<Vec<PathBuf>> {
    for (index, image) in images.0.iter().enumerate() {
        forced_boundary(&format!("removal-image-{index}"))?;
        install_image(&image.path, Some(&image.after))?;
    }
    forced_boundary("removal-validate")?;
    let after = match validation_pass(input, state_machine) {
        Ok(pass) => pass.errors,
        Err(report) => vec![report.to_string()],
    };
    let introduced = errors_introduced_over(baseline, after);
    if !introduced.is_empty() {
        for image in images.0.iter().rev() {
            install_image(&image.path, image.before.as_deref())?;
        }
        forced_remove(marker)?;
        return Err(miette!(
            help = "nothing was changed; fix what the errors name, then remove again.",
            "{id} cannot be removed: the project would not validate without it:\n  {}",
            introduced.join("\n  ")
        ));
    }
    let mut deleted = Vec::new();
    for path in residue {
        forced_boundary("removal-residue")?;
        if remove_residue_path(path, id)? {
            deleted.push(path.clone());
        }
    }
    forced_boundary("removal-clear")?;
    forced_remove(marker)?;
    Ok(deleted)
}

/// Finish the removal a marker records, accepting only its before or after
/// image of each file. §FS-rhei-remove.6.2
fn resume_removal(
    input: &Path,
    state_machine: Option<&Path>,
    marker: &Path,
) -> MietteResult<(String, Vec<PathBuf>)> {
    let (id, images, residue) = read_removal_marker(marker)?;
    let project_root = execution_workspace_root(input);
    // Read only for its roots, then dropped: its shared guards must be gone
    // before the exclusive ones are taken. §FS-rhei-remove.6.1
    let run_roots = run_lock_roots(&load_plan(input)?, &project_root);
    let files: Vec<PathBuf> = images.0.iter().map(|image| image.path.clone()).collect();
    let _locks = acquire_removal_locks(run_roots, input, &id, &files, &[project_root])?;
    for image in &images.0 {
        let current = match fs::read_to_string(&image.path) {
            Ok(raw) => Some(raw),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
            Err(err) => return Err(file_io_report(&image.path, "failed to read", err)),
        };
        if current != image.before && current.as_deref() != Some(image.after.as_str()) {
            return Err(miette!(
                help = "something other than this removal changed it; restore it, then run the \
                        same `rhei remove` again.",
                "cannot resume the removal of {id}: {} is neither its recorded before nor after \
                 image",
                display_path(&image.path)
            ));
        }
    }
    for image in images.0.iter().rev() {
        install_image(&image.path, image.before.as_deref())?;
    }
    let baseline = removal_validation_errors(input, state_machine);
    let deleted = commit_removal(input, state_machine, marker, &id, &images, &residue, &baseline)?;
    Ok((id, deleted))
}
