/// Preview decision derived exclusively from exact ledger evidence. §FS-rhei-recover.3
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ForcedDecision { Rollback, Forward }

impl ForcedDecision {
    fn word(self) -> &'static str { match self { Self::Rollback => "rollback", Self::Forward => "forward" } }
}

/// Ambiguity and third values leave every byte for the operator to inspect. §FS-rhei-recover.3
fn forced_decision(root: &Path, marker: &ForcedMarker) -> MietteResult<ForcedDecision> {
    let path = forced_image_path(root, &marker.ledger.path)?;
    let bytes = ForcedImage::read(&path)?.bytes()?.unwrap_or_default();
    let offset = usize::try_from(marker.ledger.offset).map_err(|err| miette!("invalid ledger offset: {err}"))?;
    if bytes.len() < offset || forced_digest(&bytes[..offset]) != marker.ledger.prefix_sha256 {
        return Err(miette!("ledger prefix digest mismatch; restore evidence at {}", path.display()));
    }
    let pair = format!("{}{}", marker.ledger.metadata_line, marker.ledger.movement_line);
    let tail = &bytes[offset..];
    let decision = if tail == pair.as_bytes() {
        ForcedDecision::Forward
    } else if pair.as_bytes().starts_with(tail) && std::str::from_utf8(tail).is_ok() {
        ForcedDecision::Rollback
    } else {
        return Err(miette!("ambiguous ledger evidence (not the exact pair or a torn prefix); restore {}", path.display()));
    };
    // Parse the prefix to reject an earlier copy of this recovery id.
    let prefix = std::str::from_utf8(&bytes[..offset]).map_err(|err| miette!("invalid ledger prefix: {err}"))?;
    let history = rhei_core::transition_history::parse(prefix).map_err(|err| miette!("{err}"))?;
    if history.iter().any(|movement| movement.audit.as_ref().is_some_and(|audit| audit.recovery_id == marker.recovery_id)) {
        return Err(miette!("duplicate recovery id in saved ledger prefix"));
    }
    for file in &marker.files {
        let path = forced_file_path(root, file)?;
        let current = ForcedImage::read(&path)?;
        if current != file.before && current != file.after {
            return Err(miette!("recovery image has a third value; restore evidence at {}", path.display()));
        }
    }
    Ok(decision)
}

/// Marker errors always carry exactly one explicit recovery invocation. §FS-rhei-recover.4
fn forced_recovery_error(root: &Path, error: miette::Report) -> miette::Report {
    miette!("forced-recovery marker is corrupt or its evidence is ambiguous: {error}\nmarker: {}\nrhei recover {}",
        root.join(rhei_core::root_access::MARKER).display(), shell_quote(&root.display().to_string()))
}

/// Recovery never loads the potentially inconsistent plan. §FS-rhei-recover.1
fn recover_command(root: &Path) -> MietteResult<()> {
    let root = rhei_core::platform::canonical_path(root)
        .map_err(|err| file_io_report(root, "failed to resolve recovery root", err))?;
    let marker_path = root.join(rhei_core::root_access::MARKER);
    forced_marker_location(&root).map_err(|err| forced_recovery_error(&root, err))?;
    let preview = match fs::read(&marker_path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            println!("no forced recovery pending");
            return Ok(());
        }
        Err(err) => return Err(file_io_report(&marker_path, "failed to read recovery marker", err)),
    };
    let marker = ForcedMarker::parse(&root, &preview).map_err(|err| forced_recovery_error(&root, err))?;
    let decision = forced_decision(&root, &marker).map_err(|err| forced_recovery_error(&root, err))?;
    let expected = format!("recover {} {} {} -> {} {}", marker.recovery_id, marker.hop.task_id,
        marker.hop.from, marker.hop.to, decision.word());
    let account = confirm_operator_hop(&expected, "recover")?;
    forced_replay(&root, &marker, decision, &account)?;
    println!("{} {} {} -> {} rolled {}", marker.recovery_id, marker.hop.task_id,
        marker.hop.from, marker.hop.to, if decision == ForcedDecision::Forward { "forward" } else { "back" });
    Ok(())
}

/// Repeatable replay with the same fault boundaries as the initial writer. §FS-rhei-recover.3
fn forced_replay(root: &Path, marker: &ForcedMarker, decision: ForcedDecision, account: &str) -> MietteResult<()> {
    let preview = marker.bytes()?;
    let roots = forced_owner_roots(root, &marker.files)?;
    forced_boundary("recovery-confirmed-before-locks")?;
    let _run_locks = operator_run_locks(&roots, account)?;
    let _guards = forced_root_guards(&roots)?;
    forced_marker_location(root)?;
    let _file_locks = forced_lock_images(root, &marker.files)?;
    let _ledger = LockedTransitionLedger::lock(root)?;
    let marker_path = root.join(rhei_core::root_access::MARKER);
    let current = fs::read(&marker_path).map_err(|err| file_io_report(&marker_path, "confirmed marker vanished", err))?;
    if current != preview { return Err(miette!("forced-recovery marker changed after confirmation; retry with fresh confirmation")); }
    let locked_marker = ForcedMarker::parse(root, &current).map_err(|err| forced_recovery_error(root, err))?;
    if forced_owner_roots(root, &locked_marker.files)? != roots {
        return Err(miette!("recovery owners changed after confirmation; retry with fresh confirmation"));
    }
    let locked_decision = forced_decision(root, &locked_marker).map_err(|err| forced_recovery_error(root, err))?;
    if locked_decision != decision { return Err(miette!("recovery decision changed after confirmation; retry with fresh confirmation")); }
    let path = forced_image_path(root, &marker.ledger.path)?;
    if path.exists() {
        forced_boundary("recovery-ledger-before")?;
        let ledger = fs::OpenOptions::new().write(true).open(&path)
            .map_err(|err| file_io_report(&path, "failed to open recovery ledger", err))?;
        if decision == ForcedDecision::Rollback {
            forced_boundary("recovery-ledger-truncate-before")?;
            ledger.set_len(marker.ledger.offset).map_err(|err| file_io_report(&path, "failed to truncate torn force pair", err))?;
            forced_boundary("recovery-ledger-truncate-after")?;
        }
        forced_boundary("recovery-ledger-sync-before")?;
        ledger.sync_all().map_err(|err| file_io_report(&path, "failed to sync recovery ledger", err))?;
        forced_sync_directory(path.parent().expect("ledger parent"))?;
        forced_boundary("recovery-ledger-sync-after")?;
    }
    forced_install_images(root, marker, decision == ForcedDecision::Forward)?;
    forced_boundary("marker-remove-before")?;
    forced_remove(&root.join(rhei_core::root_access::MARKER))?;
    forced_boundary("marker-remove-after")
}
