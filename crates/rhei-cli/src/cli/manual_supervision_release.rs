// Explicit CLI release authority; engine release semantics stay shared and unchanged.
// §FS-rhei-transition-cmd.2 §FS-rhei-supervision.3.1

/// Refuse a release flag on any edge other than an ordinary supervising self-loop,
/// before locks, callbacks, results or accounting. §FS-rhei-transition-cmd.2
fn release_flag_preflight(
    release: bool,
    force: bool,
    machine: &rhei_validator::StateMachine,
    from: &str,
    to: &str,
) -> MietteResult<()> {
    if release && (force || from != to || execute_on_of(machine, from).is_none()) {
        return Err(diagnostic!("--release-subtree is only valid on a supervising self-loop and cannot be combined with --force"));
    }
    Ok(())
}

/// Read pending events under the transition's metadata lock. Refusal is a pure
/// preflight; it must precede any write or callback. §FS-rhei-transition-cmd.2
fn manual_release_preflight(
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
    key: &TaskId,
    task: &str,
    from: &str,
    to: &str,
    release: bool,
) -> MietteResult<usize> {
    release_flag_preflight(release, false, machine, from, to)?;
    if from != to || execute_on_of(machine, from).is_none() {
        return Ok(0);
    }
    let checkpoints = supervision_checkpoints(metadata, key);
    let count = checkpoints.len();
    if count > 0 && !release {
        let prefix = task.strip_suffix(&key.to_string()).unwrap_or("");
        let events = checkpoints
            .iter()
            .map(|event| format!("Task {prefix}{} {} -> {}", event.task, event.from, event.to))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(diagnostic!("refusing to release Task {task}: the self-loop '{from}' -> '{to}' releases the subtree and drops {count} unread checkpoint{} ({events}). To give the halted visit more attempts with the subtree still held: rhei retry {task}. To release anyway: rhei transition {task} --from {from} --to {to} --release-subtree", if count == 1 { "" } else { "s" }));
    }
    Ok(count)
}

/// The actual command `next` hands a manual supervisor. §FS-rhei-supervision.3.4
fn manual_supervisor_release_command(
    input: &Path,
    state_machine: Option<&Path>,
    task: &str,
    state: &str,
) -> String {
    format!(
        "rhei{} transition {} --task {} --from {} --to {} --release-subtree",
        state_machine_flag(state_machine),
        shell_quote(&input.display().to_string()),
        task,
        shell_quote(state),
        shell_quote(state)
    )
}
