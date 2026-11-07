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
    let mut paths = files.iter().filter(|file| !file.roles.iter().all(|role| role == "result"))
        .map(|file| forced_file_path(root, file)).collect::<MietteResult<Vec<_>>>()?;
    paths.sort(); paths.dedup();
    let mut locks = Vec::new();
    for path in paths {
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
    let _guards = forced_root_guards(&preview.roots)?;
    // §FS-rhei-errors.6
    for root in &preview.roots { rhei_core::root_access::check_pending(root).map_err(|err| root_access_report(root, err))?; }
    let _file_locks = forced_lock_images(&preview.root, &preview.files)?;
    let _ledger = LockedTransitionLedger::lock(&preview.root)?;
    let prepared = prepare_forced_transition(request)?;
    if prepared.roots != preview.roots || prepared.root != preview.root || prepared.task_id != preview.task_id
        || prepared.files.iter().map(ForcedFile::key).collect::<Vec<_>>() != preview.files.iter().map(ForcedFile::key).collect::<Vec<_>>() {
        // §FS-rhei-errors.6
        return Err(miette!(help = FRESH_OPERATOR_CONFIRMATION_HELP, "recovery scope changed after confirmation; retry with fresh confirmation"));
    }
    let id = uuid::Uuid::now_v7().to_string();
    let audit = rhei_core::transition_history::ForceAudit {
        confirmation: "typed-hop-v1".into(), from: from.into(), to: to.into(),
        os_user: account.to_string(), reason: reason.into(), recovery_id: id.clone(),
        schema_version: 1, task_id: prepared.task_id.clone(),
        timestamp: rhei_tui::format_rfc3339(std::time::SystemTime::now()),
        result_sha256: result.map(|message| forced_digest(message.as_bytes())),
    };
    // §FS-rhei-errors.6
    let (metadata_line, movement_line) = audit.pair().map_err(|err| miette!(help = INTERNAL_DIAGNOSTIC_HELP, "{err}"))?;
    let ledger_path = prepared.root.join("runtime/state-transitions.log");
    let prefix = ForcedImage::read(&ledger_path)?.bytes()?.unwrap_or_default();
    // §FS-rhei-errors.6
    rhei_core::transition_history::parse(std::str::from_utf8(&prefix).map_err(|err| ledger_report(&ledger_path, err))?)
        // §FS-rhei-errors.6
        .map_err(|err| ledger_report(&ledger_path, err))?;
    // §FS-rhei-errors.6
    if !prefix.is_empty() && !prefix.ends_with(b"\n") {
        return Err(ledger_report(&ledger_path, "state ledger has an incomplete final row")
            .wrap_err("state ledger has an incomplete final row"));
    }
    let marker = ForcedMarker { version: if prepared.files.iter().any(|file| file.owner.is_some()) { 2 } else { 1 }, recovery_id: id,
        hop: ForcedHop { task_id: prepared.task_id.clone(), from: from.into(), to: to.into() },
        files: prepared.files,
        ledger: ForcedLedger { path: "runtime/state-transitions.log".into(), offset: prefix.len() as u64,
            prefix_sha256: forced_digest(&prefix), metadata_line, movement_line } };
    ForcedMarker::parse(&prepared.root, &marker.bytes()?)?;
    forced_commit(&prepared.root, &marker).map_err(|err| {
        match rhei_core::root_access::check_pending(&prepared.root) {
            // §FS-rhei-errors.6
            Err(pending) => root_access_report(&prepared.root, pending).wrap_err(err),
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
    let (owner, relative) = match path.strip_prefix(root) {
        Ok(relative) => (None, relative.to_string_lossy().replace('\\', "/")),
        Err(_) if path == forced_basin_manifest(root)? =>
            (Some(ForcedOwner::BasinProjectMetadata), "index.panta.md".to_string()),
        // §FS-rhei-errors.6
        Err(_) => return Err(miette!(help = "keep recovery images inside their execution root; a basin project-metadata image may refer only to its owning index.panta.md. Preserve any pending marker while restoring its paths.", "recovery image is outside its permitted owner: {}", path.display())),
    };
    let mut roles = roles.iter().map(|role| role.to_string()).collect::<Vec<_>>();
    roles.sort(); roles.dedup();
    let file = ForcedFile { owner, path: relative, roles, before: ForcedImage::read(&path)?, after: ForcedImage::present(after) };
    forced_file_path(root, &file)?;
    Ok(file)
}

/// Completion/reopening changes only the task's result link, preserving history. §FS-rhei-plan-language.3.8
fn forced_result_link(raw: &str, kind: PlanFileKind, task: &str, qualified: &str, terminal: bool) -> String {
    let all_lines: Vec<&str> = raw.lines().collect();
    // §FS-rhei-complete.3.3: a fence the task's text ends inside is below the link, unread.
    let (raw_lines, final_fence) = split_at_a_tasks_final_fence(&all_lines, kind, task);
    let mut lines = Vec::new();
    let mut target = false;
    // §FS-rhei-plan-language.2.1: quoted result and assignee lines remain content.
    let mut code = rhei_core::fence::FenceTracker::default();
    let link = format!("> **Result:** [{qualified}](runtime/results/{qualified}.md)");
    for line in raw_lines.iter().copied() {
        if let Some((_, id)) = node_heading_outside_code(line, &mut code) {
            if target && terminal { trim_trailing_blank_lines(&mut lines); lines.extend([String::new(), link.clone(), String::new()]); }
            target = id == task;
        }
        if target && !code.is_open() && (line.starts_with("> **Result:**") || line.starts_with("**Assignee:**")) { continue; }
        lines.push(line.to_string());
    }
    if target && terminal { append_result_link(&mut lines, link, !final_fence.is_empty()); }
    lines.extend(final_fence.iter().map(ToString::to_string));
    let mut text = lines.join("\n");
    if raw.ends_with('\n') { text.push('\n'); }
    if raw.contains("\r\n") { text = text.replace('\n', "\r\n"); }
    text
}
