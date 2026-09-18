/// Stable input for one attended correction; never accepted by machine executors. §FS-rhei-transition-cmd.6
struct ForcedRequest<'a> {
    input: &'a Path,
    scope: &'a [String],
    machine_path: Option<&'a Path>,
    task: &'a str,
    from: &'a str,
    to: &'a str,
    reason: &'a str,
    result: Option<&'a str>,
}

/// §FS-rhei-transition-cmd.6
struct PreparedForce {
    root: PathBuf,
    roots: Vec<PathBuf>,
    task_id: String,
    files: Vec<ForcedFile>,
}

/// Lock the same stable plan sidecars ordinary writers use, in sorted order. §FS-rhei-recover.3
fn forced_lock_images(root: &Path, files: &[ForcedFile]) -> MietteResult<Vec<fs::File>> {
    let mut locks = Vec::new();
    for image in files {
        if image.roles.iter().all(|role| role == "result") { continue; }
        let path = forced_image_path(root, &image.path)?;
        let lock_path = plan_lock_path(&path)?;
        let lock = fs::OpenOptions::new().create(true).read(true).write(true).truncate(false).open(&lock_path)
            .map_err(|err| file_io_report(&lock_path, "failed to open recovery writer lock", err))?;
        lock_plan_writer(&lock, &lock_path)?;
        locks.push(lock);
    }
    Ok(locks)
}

/// Confirmation precedes run/root/file locks; all mutable guards are then repeated. §FS-rhei-transition-cmd.6
#[allow(clippy::too_many_arguments)]
fn force_transition_command(input: &Path, scope: &[String], machine_path: Option<&Path>,
    task: &str, from: &str, to: &str, reason: &str, result: Option<&str>) -> MietteResult<()> {
    let request = ForcedRequest { input, scope, machine_path, task, from, to, reason, result };
    let preview = prepare_forced_transition(&request)?;
    let account = confirm_operator_hop(&format!("force {} {from} -> {to}", preview.task_id), "--force")?;
    commit_confirmed_force(&request, preview, &account)
}

/// Only the command ceremony calls this coordinator; unit tests supply isolated roots. §FS-rhei-transition-cmd.6
fn commit_confirmed_force(request: &ForcedRequest<'_>, preview: PreparedForce, account: &str) -> MietteResult<()> {
    let ForcedRequest { from, to, reason, result, .. } = *request;
    forced_boundary("confirmed-before-locks")?;
    let _run_locks = operator_run_locks(&preview.roots, account)?;
    let mut _guards = Vec::new();
    for root in &preview.roots {
        let guard = match rhei_core::root_access::RootAccessGuard::try_exclusive(root)
            .map_err(|err| miette!("{err}"))? {
            Some(guard) => guard,
            None => {
                forced_boundary("root-contended")?;
                rhei_core::root_access::RootAccessGuard::exclusive(root).map_err(|err| miette!("{err}"))?
            }
        };
        _guards.push(guard);
    }
    for root in &preview.roots { rhei_core::root_access::check_pending(root).map_err(|err| miette!("{err}"))?; }
    let _file_locks = forced_lock_images(&preview.root, &preview.files)?;
    let _ledger = LockedTransitionLedger::lock(&preview.root)?;
    let prepared = prepare_forced_transition(request)?;
    if prepared.roots != preview.roots || prepared.root != preview.root || prepared.task_id != preview.task_id
        || prepared.files.iter().map(|file| &file.path).collect::<Vec<_>>() != preview.files.iter().map(|file| &file.path).collect::<Vec<_>>() {
        return Err(miette!("recovery scope changed after confirmation; retry with fresh confirmation"));
    }
    let id = uuid::Uuid::now_v7().to_string();
    let audit = rhei_core::transition_history::ForceAudit {
        confirmation: "typed-hop-v1".into(), from: from.into(), to: to.into(),
        os_user: account.to_string(), reason: reason.into(), recovery_id: id.clone(),
        schema_version: 1, task_id: prepared.task_id.clone(),
        timestamp: rhei_tui::format_rfc3339(std::time::SystemTime::now()),
        result_sha256: result.map(|message| forced_digest(message.as_bytes())),
    };
    let (metadata_line, movement_line) = audit.pair().map_err(|err| miette!("{err}"))?;
    let ledger_path = prepared.root.join("runtime/state-transitions.log");
    let prefix = ForcedImage::read(&ledger_path)?.bytes()?.unwrap_or_default();
    rhei_core::transition_history::parse(std::str::from_utf8(&prefix).map_err(|err| miette!("{err}"))?)
        .map_err(|err| miette!("{err}"))?;
    if !prefix.is_empty() && !prefix.ends_with(b"\n") { return Err(miette!("state ledger has an incomplete final row")); }
    let marker = ForcedMarker { version: 1, recovery_id: id,
        hop: ForcedHop { task_id: prepared.task_id.clone(), from: from.into(), to: to.into() },
        files: prepared.files,
        ledger: ForcedLedger { path: "runtime/state-transitions.log".into(), offset: prefix.len() as u64,
            prefix_sha256: forced_digest(&prefix), metadata_line, movement_line } };
    ForcedMarker::parse(&prepared.root, &marker.bytes()?)?;
    forced_commit(&prepared.root, &marker).map_err(|err| {
        match rhei_core::root_access::check_pending(&prepared.root) {
            Err(pending) => miette!("{err}\n{pending}"),
            Ok(()) => err,
        }
    })?;
    println!("Task {} forced: '{}' → '{}' (recovery {})", prepared.task_id, from, to, marker.recovery_id);
    Ok(())
}

/// One canonical path may carry metadata, state and checkpoint changes. §FS-rhei-recover.2
fn forced_file(root: &Path, path: &Path, roles: &[&str], after: &[u8]) -> MietteResult<ForcedFile> {
    let path = if path.exists() { rhei_core::platform::canonical_path(path) }
        else { Ok(path.to_path_buf()) }.map_err(|err| file_io_report(path, "failed to resolve recovery file", err))?;
    let relative = path.strip_prefix(root).map_err(|_| miette!(
        "forced recovery cannot represent metadata outside execution root: {}; marker version 1 requires contained images", path.display()))?
        .to_string_lossy().replace('\\', "/");
    forced_image_path(root, &relative)?;
    let mut roles = roles.iter().map(|role| role.to_string()).collect::<Vec<_>>();
    roles.sort(); roles.dedup();
    Ok(ForcedFile { path: relative, roles, before: ForcedImage::read(&path)?, after: ForcedImage::present(after) })
}

/// Completion/reopening changes only the task's result link, preserving history. §FS-rhei-plan-language.3.8
fn forced_result_link(raw: &str, task: &str, qualified: &str, terminal: bool) -> String {
    let mut lines = Vec::new();
    let mut target = false;
    let mut code = false;
    let link = format!("> **Result:** [{qualified}](runtime/results/{qualified}.md)");
    for line in raw.lines() {
        if let Some((_, id)) = node_heading_outside_code(line, &mut code) {
            if target && terminal { trim_trailing_blank_lines(&mut lines); lines.extend([String::new(), link.clone(), String::new()]); }
            target = id == task;
        }
        if target && !code && (line.starts_with("> **Result:**") || line.starts_with("**Assignee:**")) { continue; }
        lines.push(line.to_string());
    }
    if target && terminal { trim_trailing_blank_lines(&mut lines); lines.extend([String::new(), link]); }
    let mut text = lines.join("\n");
    if raw.ends_with('\n') { text.push('\n'); }
    if raw.contains("\r\n") { text = text.replace('\n', "\r\n"); }
    text
}
