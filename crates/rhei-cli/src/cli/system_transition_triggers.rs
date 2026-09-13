
/// Origin metadata for a state transition. Lets callers override the
/// `triggeredBy` slot on the `TransitionContext` passed to callbacks and
/// seed `transitionData` with engine-side values (e.g. the timeout
/// duration that triggered the rule).
#[derive(Debug, Default, Clone)]
struct TransitionOrigin {
    /// `rhei next` is taking ownership while it applies this edge. The shared
    /// executor then treats state, ownership, and bookkeeping as one claim.
    /// No other transition caller opts into this behavior. §FS-rhei-next.3.1
    claim: bool,
    /// Override the default `triggered_by` slot. `None` falls back to
    /// `"user"` (or `"callback"` when an on_leave redirect rerouted).
    triggered_by: Option<&'static str>,
    /// Initial `transitionData` payload. On_leave callbacks merge into this
    /// last-write-wins.
    // §FS-rhei-agents.7.5: Timeout transition data merge.
    seed_data: Option<serde_json::Value>,
    /// System failure routes leave the source state because work failed, not
    /// because the source state's success artifacts were produced.
    skip_source_outputs: bool,
    /// Result message the caller carries through the move. It satisfies the
    /// terminal-result obligation on an edge into a `final: true` state, and is
    /// appended to `runtime/results/<task-id>.md` once the move succeeds.
    ///
    /// Only a caller that *knows* the outcome sets this: the human verbs pass
    /// `--result`, and the engine passes one on the routes the engine itself
    /// ended. A canned "advanced by the engine" line would be provenance, which
    /// the ledger already carries, and not a result.
    // §FS-rhei-states.3.3 §FS-rhei-run.3
    result_message: Option<String>,
    /// The supervisor whose prompt authorized this one explicit descendant
    /// operation. Unlike worker identity, this value exists only inside the
    /// transition process and is matched against the current plan tree.
    // §FS-rhei-supervision.2.1
    supervisor: Option<TaskId>,
    /// The engine's own account of the move, recorded *only* when the effective
    /// target turns out to be `final: true` and nothing else answered for the
    /// ticket.
    ///
    /// Callback-only advancement sets it: taking the edge is an outcome the
    /// engine produced, with no subprocess in the source state that could know
    /// better, so the engine narrates exactly that. It is not a carried
    /// message, because the same sentence would be a lie on a non-terminal hop
    /// and noise beside a result a callback did write. It is carried as facts
    /// rather than as a finished sentence because one of its clauses — whether
    /// the source state's declared `outputs:` were verified — is only settled
    /// further down this very transition, by the check that the reserved
    /// `cancelled` target waives.
    // §FS-rhei-run.3 §FS-rhei-states.1.4
    terminal_result_fallback: Option<CallbackOnlyAccount>,
}

/// What the engine can say about a state it advanced out of without spawning
/// anything, held as facts until the transition settles the last of them.
// §FS-rhei-run.3 §FS-rhei-agents.8.4
#[derive(Debug, Clone)]
struct CallbackOnlyAccount {
    from: String,
    /// The worker a spawn record proves ran there earlier, already phrased.
    /// `None` when no record answers for the state, which is the only case in
    /// which the engine may say that nothing ran.
    // §FS-rhei-agents.8.4
    evidence: Option<String>,
    /// Whether the source state declares `outputs:` at all. With none there is
    /// nothing to report about them either way.
    declares_outputs: bool,
}

impl CallbackOnlyAccount {
    /// The sentence, finished at the point the outputs question has an answer.
    ///
    /// `outputs_verified` is not this function's guess: the caller passes what
    /// the transition actually did — the source-outputs check ran and passed,
    /// or it was waived for the reserved `cancelled` target. Asserting "all
    /// present" without it is how this sentence came to claim a check that the
    /// waiver had skipped.
    // §FS-rhei-run.3 §FS-rhei-states.1.4
    fn sentence(&self, outputs_verified: bool) -> String {
        let from = &self.from;
        let opening = format!(
            "`rhei run`: this task was finished by callback-only orchestration from state \
             '{from}'."
        );
        let Some(evidence) = &self.evidence else {
            return format!(
                "{opening} No agent or program ran in that state, so no worker result was \
                 recorded."
            );
        };
        let outputs = match (self.declares_outputs, outputs_verified) {
            (false, _) => "",
            (true, true) => " The state's declared outputs were checked on this edge and were \
                             all present.",
            (true, false) => " The state's declared outputs were not checked: this edge \
                              abandons the work, which waives them.",
        };
        format!(
            "{opening} No worker was spawned there on this run, but {evidence} and wrote no \
             result.{outputs} No worker result was recorded here; that worker's account of its \
             work is its log, not this file."
        )
    }
}

/// The facts `rhei run` records when it takes an edge itself rather than on a
/// worker's behalf.
///
/// It says plainly that no worker result was recorded — which is the fact a
/// reader of the file needs, and the fact the old empty result file withheld.
/// It is a fallback: a result a callback wrote wins, and on a non-terminal hop
/// nothing is written at all.
///
/// What it says *about the worker* is checked rather than assumed. "No agent
/// ran" is true of a machine with no autonomous state and of a run that was told
/// not to spawn one, but it is a lie on a state an earlier run did work in —
/// which is exactly the state a `--no-agent` run is most likely to walk out of.
/// The evidence is the spawn record, and only that: a log file is opened before
/// its subprocess starts, so a `command:` naming a binary that does not exist
/// leaves one behind for a worker that never ran, and crediting that worker
/// would be the same class of lie as denying a real one. The accounting record
/// could not serve either — it exists only for agents that resolve a provider
/// and model.
// §FS-rhei-run.3 §FS-rhei-states.3.3 §FS-rhei-agents.8.4
fn callback_only_terminal_result(
    runtime_dir: &Path,
    task_id: &str,
    from: &str,
    from_state_def: Option<&rhei_validator::StateDef>,
) -> CallbackOnlyAccount {
    CallbackOnlyAccount {
        from: from.to_string(),
        evidence: prior_worker_run(runtime_dir, task_id, from),
        declares_outputs: from_state_def.is_some_and(|def| !def.outputs.is_empty()),
    }
}

/// One sentence about the run a spawn record is evidence of: who ran, where the
/// transcript is, and how it ended.
///
/// The record says which *kind* of worker ran, so the sentence names the agent
/// or the program that was actually there rather than the agent this state
/// would resolve to today.
// §FS-rhei-agents.8.4 §FS-rhei-programs.5 §FS-rhei-run.3
fn prior_worker_run(runtime_dir: &Path, task_id: &str, state: &str) -> Option<String> {
    let record = newest_spawn_record_for_state(runtime_dir, task_id, state)?;
    let ending = match record.code {
        Some(code) => format!(", exit code {code}, after {}", record.duration),
        None => format!(", after {}", record.duration),
    };
    let who = match record.kind.as_str() {
        "program" => format!("program `{}` ran in that state earlier", record.worker),
        _ => format!("agent '{}' ran in that state earlier", record.worker),
    };
    // Absolute, for the reason a missing result's path is absolute: the result
    // file this lands in may sit under a different root than the run's logs, so
    // a relative path is one the reader cannot follow. §FS-rhei-panta.6.2
    let shown = std::path::absolute(&record.log).unwrap_or(record.log);
    Some(format!("{who} (log: {}{ending})", shown.display()))
}

/// Variant of [`execute_transition`] for `rhei run`'s callback-only
/// advancement, which carries the engine's own account of the move for the case
/// where the edge lands on a `final: true` state and nothing else answered.
#[allow(clippy::too_many_arguments)]
fn execute_callback_only_transition(
    files: TransitionFiles<'_>,
    callback_paths: &CallbackPaths,
    machine: &rhei_validator::StateMachine,
    task_id_str: &str,
    from: &str,
    to: &str,
    no_callbacks: bool,
    // Where the run keeps spawn records, which is the evidence the engine's own
    // account of the source state is built from. §FS-rhei-agents.8.4
    runtime_dir: &Path,
) -> MietteResult<String> {
    let account = callback_only_terminal_result(
        runtime_dir,
        files.artifact_id,
        from,
        machine.states.get(from),
    );
    execute_transition_with_origin(
        files,
        callback_paths,
        machine,
        task_id_str,
        from,
        to,
        no_callbacks,
        TransitionOrigin {
            terminal_result_fallback: Some(account),
            ..TransitionOrigin::default()
        },
        None,
    )
}

/// Variant of [`execute_transition`] that fires the rule with a system-set
/// origin — currently used by the timeout watchdog to label the transition
/// as `triggeredBy: 'system'` and to seed `transitionData.timeout`.
// §FS-rhei-agents.7.5: System timeout transition origin.
#[allow(clippy::too_many_arguments)]
fn execute_system_timeout_transition(
    files: TransitionFiles<'_>,
    callback_paths: &CallbackPaths,
    machine: &rhei_validator::StateMachine,
    task_id_str: &str,
    from: &str,
    to: &str,
    timeout_label: &str,
    no_callbacks: bool,
) -> MietteResult<String> {
    let mut data = serde_json::Map::new();
    data.insert("timeout".to_string(), serde_json::Value::String(timeout_label.to_string()));
    // §FS-rhei-run.3: the engine ended this work, so the engine says why.
    let message = if timeout_label.is_empty() {
        format!("`rhei run`: the subprocess timed out in state '{from}'.")
    } else {
        format!("`rhei run`: the subprocess timed out after {timeout_label} in state '{from}'.")
    };
    execute_transition_with_origin(
        files,
        callback_paths,
        machine,
        task_id_str,
        from,
        to,
        no_callbacks,
        TransitionOrigin {
            claim: false,
            triggered_by: Some("system"),
            seed_data: Some(serde_json::Value::Object(data)),
            skip_source_outputs: true,
            result_message: Some(message),
            supervisor: None,
            terminal_result_fallback: None,
        },
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn execute_system_tooling_transition(
    files: TransitionFiles<'_>,
    callback_paths: &CallbackPaths,
    machine: &rhei_validator::StateMachine,
    task_id_str: &str,
    from: &str,
    to: &str,
    kind: ToolingKind,
    unavailable: &[String],
    no_callbacks: bool,
) -> MietteResult<String> {
    let mut data = serde_json::Map::new();
    data.insert(
        "unavailable".to_string(),
        serde_json::Value::Array(
            unavailable.iter().cloned().map(serde_json::Value::String).collect(),
        ),
    );
    data.insert("kind".to_string(), serde_json::Value::String(kind.as_str().to_string()));
    // §FS-rhei-run.3: the engine ended this work, so the engine says why.
    let message = format!(
        "`rhei run`: required {} unavailable in state '{}': {}.",
        kind.as_str(),
        from,
        unavailable.join(", ")
    );
    execute_transition_with_origin(
        files,
        callback_paths,
        machine,
        task_id_str,
        from,
        to,
        no_callbacks,
        TransitionOrigin {
            claim: false,
            triggered_by: Some("system"),
            seed_data: Some(serde_json::Value::Object(data)),
            skip_source_outputs: true,
            result_message: Some(message),
            supervisor: None,
            terminal_result_fallback: None,
        },
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn execute_system_program_exit_transition(
    files: TransitionFiles<'_>,
    callback_paths: &CallbackPaths,
    machine: &rhei_validator::StateMachine,
    task_id_str: &str,
    from: &str,
    to: &str,
    exit_code: i32,
    matched: ExitCodeMatch,
    no_callbacks: bool,
) -> MietteResult<String> {
    let mut data = serde_json::Map::new();
    data.insert("exitCode".to_string(), serde_json::Value::from(exit_code));
    // A zero exit reports success and a declared route names an edge: either
    // way the worker spoke, and only an exit the engine ended the work on is
    // the engine's to explain. §FS-rhei-run.3 §FS-rhei-programs.3.2
    let engine_owns_the_outcome = exit_code != 0 && !matched.is_declared_route();
    let message = engine_owns_the_outcome
        .then(|| format!("`rhei run`: the subprocess exited {exit_code} in state '{from}'."));
    execute_transition_with_origin(
        files,
        callback_paths,
        machine,
        task_id_str,
        from,
        to,
        no_callbacks,
        TransitionOrigin {
            claim: false,
            triggered_by: Some("system"),
            seed_data: Some(serde_json::Value::Object(data)),
            // Deliberately still the bare exit test: the source state's declared
            // `outputs:` are checked on a zero exit and skipped on a non-zero
            // one, a declared route as much as any other. §FS-rhei-run.3
            skip_source_outputs: exit_code != 0,
            result_message: message,
            supervisor: None,
            terminal_result_fallback: None,
        },
        None,
    )
}
