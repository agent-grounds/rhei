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
            assessment.unsafe_path(format!("{} is a symlink", display_path(&runtime)));
            return Ok(());
        }
        Ok(_) => {}
    }
    for target in scoped_runtime_targets(&runtime, id, machine) {
        match target {
            ScopedTarget::Exact(path) => classify_owned_path(root, &path, id, assessment),
            ScopedTarget::Prefixed(named) => {
                let entries = match fs::read_dir(&named.dir) {
                    Ok(entries) => entries,
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(err) => {
                        assessment
                            .history
                            .push(("unreadable".into(), format!("{}: {err}", display_path(&named.dir))));
                        continue;
                    }
                };
                let prefix = named.prefix();
                for entry in entries.flatten() {
                    if !entry.file_name().to_string_lossy().starts_with(&prefix) {
                        continue;
                    }
                    // Ambiguous where another ticket's longer id could stand at the id.
                    // §FS-rhei-reset.2.1
                    if !named.owns(&entry.path(), others.iter().map(String::as_str)) {
                        assessment.unsafe_path(format!(
                            "{} may belong to another ticket",
                            relative_to(root, &entry.path())
                        ));
                        continue;
                    }
                    classify_owned_path(root, &entry.path(), id, assessment);
                }
            }
            // A declared path with another placeholder, matched in full: the path is the
            // ticket's only where no other ticket's longer id could stand at the id.
            // §FS-rhei-reset.2.1
            ScopedTarget::Pattern(pattern) => {
                let paths = match pattern.entries() {
                    Ok(paths) => paths,
                    Err((dir, err)) => {
                        assessment
                            .history
                            .push(("unreadable".into(), format!("{}: {err}", display_path(&dir))));
                        continue;
                    }
                };
                for path in paths {
                    if !pattern.owns(&path, others.iter().map(String::as_str)) {
                        assessment.unsafe_path(format!(
                            "{} may belong to another ticket",
                            relative_to(root, &path)
                        ));
                        continue;
                    }
                    classify_owned_path(root, &path, id, assessment);
                }
            }
        }
    }
    Ok(())
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
        assessment.unsafe_path(format!("{shown} is a symlink"));
        return;
    }
    if !path_stays_under(root, path) {
        assessment.unsafe_path(format!("{shown} escapes the execution root"));
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
