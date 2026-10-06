// What `rhei remove` finds under `runtime/` for one ticket, sorted into history
// that refuses it and empty residue it may clean.
//
// Its own part because ownership has to be exact — `auth.1` never reaches
// `auth.10` — and a path that cannot be proven owned, safe and empty is never
// cleaned, which is a rule about paths rather than about tickets.

// §FS-rhei-remove.4.2

/// Classify every path the ticket owns under one execution root's `runtime/`.
/// §FS-rhei-remove.2 §FS-rhei-remove.4.2
fn runtime_evidence(
    root: &Path,
    id: &str,
    machine: &rhei_validator::StateMachine,
    others: &[String],
    assessment: &mut RemovalAssessment,
) -> MietteResult<()> {
    let runtime = root.join("runtime");
    match fs::symlink_metadata(&runtime) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => {
            assessment.history.push(("unreadable".into(), format!("{}: {err}", display_path(&runtime))));
            return Ok(());
        }
        Ok(meta) if meta.file_type().is_symlink() => {
            assessment.history.push(("unsafe".into(), format!("{} is a symlink", display_path(&runtime))));
            return Ok(());
        }
        Ok(_) => {}
    }
    let mut targets = scoped_runtime_targets(&runtime, id, machine);
    // A ticket's export home is named by its exact id. §FS-rhei-plan-language.3.12
    targets.push(ScopedTarget::Exact(runtime.join("exports").join(id)));
    for target in targets {
        match target {
            ScopedTarget::Exact(path) => classify_owned_path(root, &path, id, assessment),
            ScopedTarget::Prefixed { dir, prefix } => {
                let entries = match fs::read_dir(&dir) {
                    Ok(entries) => entries,
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(err) => {
                        assessment
                            .history
                            .push(("unreadable".into(), format!("{}: {err}", display_path(&dir))));
                        continue;
                    }
                };
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if !name.starts_with(&prefix) {
                        continue;
                    }
                    if prefix_is_ambiguous(&name, &prefix, id, others) {
                        assessment.history.push((
                            "unsafe".into(),
                            format!(
                                "{} may belong to another ticket",
                                relative_to(root, &entry.path())
                            ),
                        ));
                        continue;
                    }
                    classify_owned_path(root, &entry.path(), id, assessment);
                }
            }
        }
    }
    Ok(())
}

/// Whether a loose prefix match could equally be another ticket's: `auth.fix-`
/// is a prefix of `auth.fix-cache-`'s files. §FS-rhei-remove.4.2
fn prefix_is_ambiguous(name: &str, prefix: &str, id: &str, others: &[String]) -> bool {
    others.iter().filter(|other| other.starts_with(id)).any(|other| {
        let theirs = prefix.replacen(id, other, 1);
        theirs != prefix && name.starts_with(&theirs)
    })
}

/// Sort one owned path into history or residue; refuse a symlink or a path that
/// escapes the execution root rather than decide about it. §FS-rhei-remove.4.2
fn classify_owned_path(root: &Path, path: &Path, id: &str, assessment: &mut RemovalAssessment) {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return,
        Err(err) => {
            assessment.history.push(("unreadable".into(), format!("{}: {err}", display_path(path))));
            return;
        }
    };
    let shown = relative_to(root, path);
    if meta.file_type().is_symlink() {
        assessment.history.push(("unsafe".into(), format!("{shown} is a symlink")));
        return;
    }
    if !path_stays_under(root, path) {
        assessment.history.push(("unsafe".into(), format!("{shown} escapes the execution root")));
        return;
    }
    let kind = path
        .strip_prefix(root.join("runtime"))
        .ok()
        .and_then(|rest| rest.components().next())
        .map(|first| first.as_os_str().to_string_lossy().into_owned())
        .unwrap_or_default();
    let label = match kind.as_str() {
        "results" => "result",
        "spawns" => "spawn",
        "reports" => "report",
        "snapshot-sessions" => "snapshot",
        "worktree-refs" => "worktree",
        "accounting" => "accounting",
        "logs" => {
            match fs::read_to_string(path) {
                Ok(text) if is_pre_spawn_header(&text, id) => {
                    assessment.residue.push(path.to_path_buf());
                }
                Ok(_) => assessment.history.push(("log".into(), shown)),
                Err(err) => assessment.history.push(("unreadable".into(), format!("{shown}: {err}"))),
            }
            return;
        }
        _ => {
            match is_empty_tree(path) {
                Ok(true) => assessment.residue.push(path.to_path_buf()),
                Ok(false) => assessment.history.push(("output".into(), shown)),
                Err(err) => assessment.history.push(("unreadable".into(), format!("{shown}: {err}"))),
            }
            return;
        }
    };
    assessment.history.push((label.into(), shown));
}

/// The path's directory resolves inside the execution root. §FS-rhei-remove.4.2
fn path_stays_under(root: &Path, path: &Path) -> bool {
    let Some(parent) = path.parent() else { return false };
    match (rhei_core::platform::canonical_path(root), rhei_core::platform::canonical_path(parent)) {
        (Ok(root), Ok(parent)) => parent.starts_with(root),
        _ => false,
    }
}

/// An empty file, or a directory holding nothing but empty directories and
/// files — and no symlink anywhere. §FS-rhei-remove.4.2
fn is_empty_tree(path: &Path) -> std::io::Result<bool> {
    let meta = fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() {
        return Ok(false);
    }
    if meta.is_file() {
        return Ok(meta.len() == 0);
    }
    for entry in fs::read_dir(path)? {
        if !is_empty_tree(&entry?.path())? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// A log holding only the header rhei writes before it starts the process, for
/// this ticket: nothing ran, so nothing happened. §FS-rhei-remove.4.2
fn is_pre_spawn_header(text: &str, id: &str) -> bool {
    let mut lines = text.lines();
    let first = lines.next().map(str::trim);
    if first != Some("=== rhei agent log v1 ===") && first != Some("=== rhei program log v1 ===") {
        return false;
    }
    let mut names_ticket = false;
    let mut closed = false;
    for line in lines {
        if closed {
            if !line.trim().is_empty() {
                return false;
            }
            continue;
        }
        if line.trim() == "===" {
            closed = true;
            continue;
        }
        let Some((key, value)) = line.split_once(": ") else { return false };
        if key.is_empty() || key.contains(char::is_whitespace) {
            return false;
        }
        if key == "task" {
            names_ticket = value.trim() == id;
        }
    }
    names_ticket
}

/// Delete one recorded residue path, re-checking it is still residue first.
/// §FS-rhei-remove.4.2
fn remove_residue_path(path: &Path, id: &str) -> MietteResult<bool> {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(file_io_report(path, "failed to inspect residue", err)),
    };
    let still_residue = !meta.file_type().is_symlink()
        && (is_empty_tree(path).unwrap_or(false)
            || fs::read_to_string(path).is_ok_and(|text| is_pre_spawn_header(&text, id)));
    if !still_residue {
        return Err(miette!(
            help = "something wrote to it after removal checked it; inspect it and run the same \
                    `rhei remove` again.",
            "{} is no longer empty residue",
            display_path(path)
        ));
    }
    let removed = if meta.is_dir() { fs::remove_dir_all(path) } else { fs::remove_file(path) };
    removed.map_err(|err| file_io_report(path, "failed to remove residue", err))?;
    Ok(true)
}
