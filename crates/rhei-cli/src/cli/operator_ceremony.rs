/// Reject contradictory flags before any recovery preflight. §FS-rhei-transition-cmd.6
fn validate_force_options(reason: Option<&str>, supervisor: Option<&str>, no_callbacks: bool) -> MietteResult<()> {
    if supervisor.is_some() { return Err(diagnostic!("--force cannot be combined with --supervisor")); }
    if no_callbacks { return Err(diagnostic!("a forced recovery runs no callbacks; drop --no-callbacks")); }
    if reason.is_none_or(|reason| reason.trim().is_empty()) {
        return Err(diagnostic!("--force requires a fresh non-empty --reason"));
    }
    Ok(())
}

/// No environment/configuration grant or test bypass participates in authority. §FS-rhei-recover.1
fn confirm_operator_hop(expected: &str, operation: &str) -> MietteResult<String> {
    use std::io::IsTerminal;
    if !std::io::stdin().is_terminal() {
        return Err(diagnostic!("{operation} requires an interactive terminal and fresh typed confirmation"));
    }
    // Query the operating system's account utility, never an author-supplied environment name.
    // §FS-rhei-transition-cmd.6.1
    let account = std::process::Command::new("whoami").output()
        .map_err(|err| diagnostic!("cannot identify invoking OS account: {err}"))?;
    let account_name = String::from_utf8(account.stdout)
        .map_err(|err| diagnostic!("cannot decode invoking OS account: {err}"))?;
    if !account.status.success() || account_name.trim().is_empty() {
        return Err(diagnostic!("cannot identify invoking OS account"));
    }
    eprintln!("Operator {}: type {expected}", account_name.trim());
    std::io::stderr().flush().map_err(|err| diagnostic!("cannot print operator confirmation: {err}"))?;
    let mut response = String::new();
    let read = std::io::stdin().read_line(&mut response)
        .map_err(|err| diagnostic!("cannot read operator confirmation: {err}"))?;
    if read == 0 || response.trim_end_matches(['\r', '\n']) != expected {
        return Err(diagnostic!("operator confirmation did not match the exact hop; no changes made"));
    }
    Ok(account_name.trim().to_string())
}

/// Run locks precede root guards; contention reports the current recorded owner. §FS-rhei-recover.3
fn operator_run_locks(roots: &[PathBuf], owner: &str) -> MietteResult<Vec<HeldRunLock>> {
    let mut held = Vec::new();
    for root in roots {
        let Some(mut lock) = try_acquire_run_lock(root)? else {
            let path = root.join(".rhei/run.lock");
            let recorded = fs::read_to_string(&path).unwrap_or_else(|err| format!("owner record unreadable: {err}"));
            return Err(diagnostic!("execution root {} has a held run lock; recorded owner: {}", root.display(), recorded.trim()));
        };
        let body = serde_json::json!({"operator": owner, "pid": std::process::id(), "operation": "forced-recovery"});
        lock.file.rewind().and_then(|()| lock.file.set_len(0))
            .and_then(|()| lock.file.write_all(body.to_string().as_bytes()))
            .and_then(|()| lock.file.flush())
            .map_err(|err| diagnostic!("cannot record operator run-lock owner: {err}"))?;
        held.push(lock);
    }
    Ok(held)
}
