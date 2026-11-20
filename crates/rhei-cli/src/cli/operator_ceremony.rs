include!("operator_handoff.rs");

/// Reject contradictory flags before any recovery preflight. §FS-rhei-transition-cmd.6
fn validate_force_options(reason: Option<&str>, supervisor: Option<&str>, no_callbacks: bool) -> MietteResult<()> {
    // §FS-rhei-errors.6
    if supervisor.is_some() { return Err(miette!(help = "remove --supervisor for the human operator recovery, or remove --force to use supervisor authority.", "--force cannot be combined with --supervisor")); }
    // §FS-rhei-errors.6
    if no_callbacks { return Err(miette!(help = "remove --no-callbacks; forced recovery already runs no callbacks.", "a forced recovery runs no callbacks; drop --no-callbacks")); }
    if reason.is_none_or(|reason| reason.trim().is_empty()) {
        // §FS-rhei-errors.6
        return Err(miette!(help = "a human operator must supply --reason with a fresh, non-empty explanation of this correction.", "--force requires a fresh non-empty --reason"));
    }
    Ok(())
}

/// No environment/configuration grant or test bypass participates in authority.
/// §FS-rhei-recover.1 §FS-rhei-errors.1.2
fn confirm_operator_hop(handoff: &OperatorHandoff) -> MietteResult<String> {
    use std::io::IsTerminal;
    if !std::io::stdin().is_terminal() {
        return Err(handoff.refusal(None, false));
    }
    let expected = &handoff.expected;
    // Query the operating system's account utility, never an author-supplied environment name.
    // §FS-rhei-transition-cmd.6.1
    let account = std::process::Command::new("whoami").output()
        // §FS-rhei-errors.6
        .map_err(|err| miette!(help = OS_ACCOUNT_HELP, "cannot identify invoking OS account: {err}"))?;
    let account_name = String::from_utf8(account.stdout)
        // §FS-rhei-errors.6
        .map_err(|err| miette!(help = OS_ACCOUNT_HELP, "cannot decode invoking OS account: {err}"))?;
    if !account.status.success() || account_name.trim().is_empty() {
        // §FS-rhei-errors.6
        return Err(miette!(help = OS_ACCOUNT_HELP, "cannot identify invoking OS account"));
    }
    eprintln!("Operator {}: type {expected}", account_name.trim());
    // §FS-rhei-errors.6
    std::io::stderr().flush().map_err(|err| miette!(help = "restore a writable standard error stream on the operator terminal before requesting fresh confirmation.", "cannot print operator confirmation: {err}"))?;
    let mut response = String::new();
    let read = std::io::stdin().read_line(&mut response)
        // §FS-rhei-errors.6
        .map_err(|err| miette!(help = "restore readable standard input on the operator terminal before requesting fresh confirmation.", "cannot read operator confirmation: {err}"))?;
    if read == 0 || response.trim_end_matches(['\r', '\n']) != expected.as_str() {
        // §FS-rhei-errors.6
        return Err(miette!(help = "if this correction is intended, the human operator must run it again at an interactive terminal and type the displayed hop exactly.", "operator confirmation did not match the exact hop; no changes made"));
    }
    Ok(account_name.trim().to_string())
}

/// Isolated callers share production acquisition and safe contention reporting. §FS-rhei-recover.3
#[cfg(test)]
fn operator_run_locks(roots: &[PathBuf], owner: &str) -> MietteResult<Vec<HeldRunLock>> {
    operator_run_locks_with_handoff(roots, owner, None)
}

/// Run locks precede root guards; the typing race uses the same complete handoff.
/// §FS-rhei-recover.3 §FS-rhei-transition-cmd.6
fn operator_run_locks_with_handoff(roots: &[PathBuf], owner: &str,
    handoff: Option<&OperatorHandoff>) -> MietteResult<Vec<HeldRunLock>> {
    let mut held = Vec::new();
    for root in roots {
        let Some(mut lock) = try_acquire_run_lock(root)? else {
            // §FS-rhei-errors.6
            return Err(match handoff {
                Some(handoff) => handoff.refusal(Some(root), true),
                None => {
                    let blocker = operator_blocker(root, None);
                    miette!(help = blocker.guidance, "{}", blocker.summary)
                }
            });
        };
        let body = serde_json::json!({"operator": owner, "pid": std::process::id(), "operation": "forced-recovery"});
        lock.file.rewind().and_then(|()| lock.file.set_len(0))
            .and_then(|()| lock.file.write_all(body.to_string().as_bytes()))
            .and_then(|()| lock.file.flush())
            .map_err(|err| file_io_report(&root.join(".rhei/run.lock"), "cannot record operator run-lock owner", err))?;
        held.push(lock);
    }
    Ok(held)
}
