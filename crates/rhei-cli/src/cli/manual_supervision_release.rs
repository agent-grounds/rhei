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
        // §FS-rhei-errors.6
        return Err(miette!(help = "remove --release-subtree, or use it only on a supervising self-loop without --force.", "--release-subtree is only valid on a supervising self-loop and cannot be combined with --force"));
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
        // The message names the target its help does. §FS-rhei-errors.6 §FS-rhei-errors.1.2.1
        let retry = help_command_with("retry", &format!("--task {task}"));
        let release_anyway = help_command_with(
            "transition",
            &format!("--task {task} --from {} --to {} --release-subtree", shell_quote(from), shell_quote(to)),
        );
        return Err(miette!(help = format!("inspect the pending checkpoints first. An operator outside the worker can grant attempts with {retry}, or explicitly release with {release_anyway}."), "refusing to release Task {task}: the self-loop '{from}' -> '{to}' releases the subtree and drops {count} unread checkpoint{} ({events}). To give the halted visit more attempts with the subtree still held: {retry}. To release anyway: {release_anyway}", if count == 1 { "" } else { "s" }));
    }
    Ok(count)
}

/// The actual command `next` hands a manual supervisor, naming the target `next` was
/// typed with rather than the project `input` widened to. §FS-rhei-supervision.3.4
/// §FS-rhei-errors.1.2.1
fn manual_supervisor_release_command(
    input: &Path,
    state_machine: Option<&Path>,
    task: &str,
    state: &str,
) -> String {
    format!(
        "rhei transition {} --task {} --from {} --to {} --release-subtree{}",
        help_target_word_or(input),
        task,
        shell_quote(state),
        shell_quote(state),
        state_machine.map(state_machine_argument).unwrap_or_default()
    )
}
