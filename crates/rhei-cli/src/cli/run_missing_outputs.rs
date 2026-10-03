// What a worker still owes when it stops: the required artifacts of the
// completion condition, resolved against the paths they will actually be read
// from, and the stall an operator reads when one of them is not there.
//
// Its own part because every execution path asks this — the single worker, the
// pool, and callback-only advancement next door — and none of them is where the
// answer belongs.

// §AR-source-file-size.3 §FS-rhei-agents.3.2 §FS-rhei-states.3.3

/// Emit the "agent exited 0 but ..." warning(s) after a 0-exit run that did
/// not advance the task. When required outputs are missing, the warning
/// includes the missing names.
// §FS-rhei-agents.3.2.1: Missing-output warning contents.
#[allow(clippy::too_many_arguments)]
fn emit_exit_zero_warnings(
    workspace_root: &Path,
    artifact_root: &Path,
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
    task: &rhei_core::ast::Task,
    task_id_str: &str,
    state_name: &str,
    selected_to: Option<&str>,
    // Carried from the spawn that just finished: only it knows whether the
    // visit has an attempt left. §FS-rhei-agents.3.2.1
    outlook: RetryOutlook,
    sink: &Arc<dyn rhei_tui::EventSink>,
) {
    let missing = collect_missing_required_outputs(
        workspace_root,
        artifact_root,
        machine,
        metadata,
        task,
        state_name,
        selected_to,
    );
    if missing.is_empty() {
        sink.emit(rhei_tui::RunEvent::Message {
            level: rhei_tui::MessageLevel::Warn,
            text: format!(
                "  warning: agent exited 0 but task {} did not advance from '{}'",
                task_id_str, state_name
            ),
        });
    } else {
        emit_missing_required_outputs_warning(
            "agent",
            task_id_str,
            state_name,
            0,
            &missing,
            outlook,
            sink,
        );
    }
}

/// The stall an operator reads, plus the same facts as data.
///
/// The run report classifies a halted ticket from the structured event; without
/// it the only record of *which* artifact was missing was this line's prose,
/// and the report fell back to "stalled in non-terminal state <s> — inspect
/// logs", which names nothing the operator can act on.
///
/// `worker` is `agent` or `program`. A program is a worker like any other and
/// stalls the same way, so it must reach the report the same way; only the noun
/// in the sentence differs.
///
/// `exit_code` is the code the worker actually exited with, not always `0`: a
/// program that took a declared route out of a non-zero exit owes the ticket's
/// result the same way, and a line that said `0` would send the operator
/// looking for an exit that never happened. §FS-rhei-programs.3.2
// §FS-rhei-agents.3.2.1 §FS-rhei-run-report.3.1
fn emit_missing_required_outputs_warning(
    worker: &str,
    task_id_str: &str,
    state_name: &str,
    exit_code: i32,
    missing: &[String],
    // What the run will actually do next, which is not decided by the missing
    // artifacts alone. §FS-rhei-agents.3.2.1
    outlook: RetryOutlook,
    sink: &Arc<dyn rhei_tui::EventSink>,
) {
    sink.emit(rhei_tui::RunEvent::Message {
        level: rhei_tui::MessageLevel::Warn,
        text: format!(
            "  warning: {} exited {} but required outputs are missing for task {} in state '{}': {}",
            worker,
            exit_code,
            task_id_str,
            state_name,
            missing.join(", ")
        ),
    });
    // The warning says *what* is missing; this says what the run is doing about
    // it — and after the last budgeted attempt what it does is nothing, so this
    // is conditioned on the budget. §FS-rhei-agents.3.2.1
    sink.emit(rhei_tui::RunEvent::Message {
        level: rhei_tui::MessageLevel::Warn,
        text: outlook.halt_line(task_id_str, state_name, missing),
    });
    sink.emit(rhei_tui::RunEvent::TaskOutputsMissing {
        task: task_id_str.to_string(),
        state: state_name.to_string(),
        entries: missing.to_vec(),
    });
}

/// Does this path still carry a `{...}` template — a reference to a variable
/// outside the namespace, which artifact resolution leaves verbatim by design?
///
/// Asked here for every surface that names an unwritten artifact, because a
/// template is not a path that was checked and none of them may present one as
/// though it were.
// §FS-rhei-agents.3.2.1 §FS-rhei-memory.4.4
fn is_unresolved_template(shown: &str) -> bool {
    shown.contains('{')
}

/// Render one missing required output as `name (path)`, flagging a path that
/// still carries a `{...}` template.
// §FS-rhei-agents.3.2.1: Missing-output warning names the resolved path.
fn format_missing_required_output(name: &str, relative: &str) -> String {
    if is_unresolved_template(relative) {
        format!("{name} ({relative}, unresolved template)")
    } else {
        format!("{name} ({relative})")
    }
}

/// One required artifact of a completion condition that is not on disk.
///
/// Kept as data rather than as a rendered line because the two surfaces that
/// read this list spell a path by different rules: the run log names a declared
/// output relative to the artifact root and the result absolutely, while a
/// prompt spells every path it carries by one rule (§FS-rhei-agents.4.1), so one
/// bare relative path beside absolute neighbours is worse there than none. The
/// name, the path whose absence put it in the list, and the log's own spelling
/// of that path are the whole of what either surface needs.
// §FS-rhei-agents.3.2.1 §FS-rhei-memory.4.4
struct MissingRequiredOutput {
    /// The declared artifact's name, or `result` for the ticket's result.
    name: String,
    /// The path `.exists()` was asked about, as it was asked — one `join` of
    /// the authored relative onto the root, never re-derived from it. Re-joining
    /// component by component writes the platform's separator and hands a prompt
    /// a path nobody typed, which §FS-rhei-agents.4.1 forbids by name.
    // §FS-rhei-agents.4.1
    path: PathBuf,
    /// Whether the *authored* spelling of that path still carries a `{...}`
    /// template, which artifact resolution leaves verbatim by design.
    ///
    /// Answered here, where the authored relative is still in hand, rather than
    /// by any reader of the rendered string: a root that happens to contain a
    /// brace is not a template, and a path that was checked may not be presented
    /// as though it were one. §FS-rhei-memory.4.4
    unresolved_template: bool,
    /// That path as the missing-output warning spells it.
    warning_spelling: String,
}

impl MissingRequiredOutput {
    /// This entry as the missing-output warning and the halt line print it, marked
    /// from the flag the retry prompt reads, never from the root-bearing spelling.
    // §FS-rhei-agents.3.2.1 §FS-rhei-memory.4.4
    fn warning_entry(&self) -> String {
        let marker = if self.unresolved_template { ", unresolved template" } else { "" };
        format!("{} ({}{marker})", self.name, self.warning_spelling)
    }
}

/// The transition `rhei run` would select for `task` from its current state.
///
/// Used only to decide whether the completion condition includes the terminal
/// result. A selection error is not this check's business — the auto-advance
/// path reports it — so it reads as "no edge selected".
// §FS-rhei-run.3
fn selected_forward_transition(
    rhei: &rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
    task: &rhei_core::ast::Task,
) -> Option<String> {
    selected_forward_transition_from(rhei, machine, task, task.state.as_str())
}

/// The ticket's terminal result, rendered as the missing required output it is.
///
/// A `final: true` state requires a non-empty `runtime/results/<task-id>.md` on
/// the edge into it, and under `orchestrator` authority the subprocess is the
/// worker that knows why the ticket is finishing — it was shown the path in its
/// prompt. A zero exit that selects a terminal edge
/// with nothing written therefore fails the completion condition and is
/// reported and routed exactly like any other missing required output, under
/// the artifact name `result`.
///
/// `invocation` names the invocation being judged, so a fanned-out state is
/// judged per invocation exactly as its declared `outputs:` are: one worker's
/// fragment never excuses a sibling that wrote nothing, and a fragment from an
/// earlier fanned-out state or an earlier visit never excuses this one.
///
/// The path is rendered **absolute**. Declared outputs render relative to the
/// workspace root, but in a Panta project the result lives under the owning
/// rhei's root, and a relative path resolved against the wrong root is one an
/// operator cannot paste.
// §FS-rhei-states.3.3 §FS-rhei-agents.3.2 §FS-rhei-agents.3.2.1 §FS-rhei-run.3
fn missing_terminal_result_entry(
    result_root: &Path,
    machine: &rhei_validator::StateMachine,
    task: &rhei_core::ast::Task,
    selected_to: Option<&str>,
    invocation: ResultInvocation<'_>,
) -> Option<MissingRequiredOutput> {
    if !is_terminal_state(selected_to?, machine) {
        return None;
    }
    let task_id = task.id.to_string();
    // The result lives under the owning rhei's execution root, which is where
    // the transition path will look for it. §FS-rhei-panta.6.2
    let path = invocation_result_file_path(result_root, &task_id, invocation);
    if file_has_content(&path) {
        return None;
    }
    let shown = std::path::absolute(&path).unwrap_or_else(|_| path.clone());
    Some(MissingRequiredOutput {
        name: "result".to_string(),
        path,
        // Asked of the authored relative path, never the root: a fan-out
        // fragment's path carries the state name verbatim. §FS-rhei-agents.3.2.1
        unresolved_template: is_unresolved_template(&result_relative_path(&task_id, invocation)),
        warning_spelling: shown.display().to_string(),
    })
}

/// [`missing_terminal_result_entry`] as the warning prints it.
// §FS-rhei-agents.3.2.1
fn missing_terminal_result_output(
    result_root: &Path,
    machine: &rhei_validator::StateMachine,
    task: &rhei_core::ast::Task,
    selected_to: Option<&str>,
    invocation: ResultInvocation<'_>,
) -> Option<String> {
    missing_terminal_result_entry(result_root, machine, task, selected_to, invocation)
        .map(|entry| entry.warning_entry())
}

/// The required artifacts a program's exit leaves unwritten, chosen by what the
/// exit was.
///
/// A zero exit is judged on the whole completion condition: the state's
/// declared `outputs:` and, on an edge into a `final: true` state, the ticket's
/// result. A **declared route** — a non-zero exit that fired an exact
/// `exit_code:` edge — is judged on the ticket's result alone: the engine writes
/// no result of its own for it, so the program owes one, while the state's
/// declared `outputs:` stay skipped on a non-zero exit exactly as before. Any
/// other non-zero exit carries the engine's own account and is judged on
/// nothing.
///
/// A `program:` state never fans out, however many targets it names, so the
/// result is judged as the whole task's. §FS-rhei-programs.2
// §FS-rhei-run.3 §FS-rhei-programs.3.2 §FS-rhei-states.3.3
#[allow(clippy::too_many_arguments)]
fn missing_program_exit_outputs(
    workspace_root: &Path,
    artifact_root: &Path,
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
    task: &rhei_core::ast::Task,
    state_name: &str,
    route: &ProgramExitRoute,
    exit_code: i32,
) -> Vec<String> {
    if route.to == state_name {
        return Vec::new();
    }
    if exit_code == 0 {
        return collect_missing_required_outputs(
            workspace_root,
            artifact_root,
            machine,
            metadata,
            task,
            state_name,
            Some(route.to.as_str()),
        );
    }
    if !route.matched.is_declared_route() {
        return Vec::new();
    }
    missing_terminal_result_output(
        artifact_root,
        machine,
        task,
        Some(route.to.as_str()),
        ResultInvocation::whole_task(),
    )
    .into_iter()
    .collect()
}

/// Walk all resolved invocations for this state and collect the union of
/// required output artifacts that do not exist on disk, each rendered as
/// `name (resolved/path)` so the warning points at the file that was checked.
///
/// `selected_to` is the transition the exit would take; when it lands on a
/// `final: true` state the ticket's terminal result joins the list — once per
/// fan-out identity, because that is how many result fragments the state was
/// asked for.
///
/// `workspace_root` is only for re-resolving invocation settings; declared
/// `outputs:` and the terminal result resolve against `artifact_root`, the
/// owning rhei's execution root — the two must stay distinct rather than
/// merged into one same-typed root. §FS-rhei-agents.3.2 condition (2)
// §FS-rhei-agents.3.2.1 §FS-rhei-states.3.3: the warning names resolved paths.
#[allow(clippy::too_many_arguments)]
fn collect_missing_required_outputs(
    workspace_root: &Path,
    artifact_root: &Path,
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
    task: &rhei_core::ast::Task,
    state_name: &str,
    selected_to: Option<&str>,
) -> Vec<String> {
    let Some(state_def) = machine.states.get(state_name) else {
        return missing_terminal_result_output(
            artifact_root,
            machine,
            task,
            selected_to,
            ResultInvocation::whole_task(),
        )
        .into_iter()
        .collect();
    };
    // Walked even with no declared `outputs:`: fragments are per invocation, so
    // the union needs the invocation list. A `program:` state never fans out,
    // however many targets it names. §FS-rhei-programs.2
    let fans_out = state_def.program.is_none()
        && (!state_def.all_targets.is_empty() || !state_def.all_models.is_empty());
    if state_def.outputs.is_empty() && !fans_out {
        return missing_terminal_result_output(
            artifact_root,
            machine,
            task,
            selected_to,
            ResultInvocation::whole_task(),
        )
        .into_iter()
        .collect();
    }
    // This warning path cannot return a settings error after the run has
    // already spawned. Validation loads settings earlier and reports real
    // runtime configuration failures before execution starts.
    let settings = load_merged_settings(workspace_root)
        .unwrap_or_else(|_| RheiSettings { agents: built_in_agents(), ..Default::default() });
    let invocations =
        resolve_agent_invocations(machine, state_name, &settings, &default_run_options())
            .unwrap_or_default();
    let mut missing: Vec<String> = Vec::new();
    let mut seen = HashSet::new();
    let visit = render_visit_count(metadata, &task.id, state_name, task.state.as_str(), machine);
    let visit_count = Some(visit);
    let contexts = transition_contexts_for_state(state_def, &invocations);
    let mut terminal_results: Vec<String> = Vec::new();
    for (target, model, model_provider, model_name, agent, agent_mode) in contexts {
        for artifact in &state_def.outputs {
            let (relative, path) = resolve_artifact_path(
                artifact_root,
                artifact,
                &task.id.to_string(),
                state_name,
                visit_count,
                target,
                model,
                model_provider,
                model_name,
                agent,
                agent_mode,
            );
            if path.exists() {
                continue;
            }
            // Dedup on the resolved path, not the name: a fanned-out state
            // resolves one artifact name to a distinct path per target, and
            // each missing path is worth naming.
            let entry = format_missing_required_output(&artifact.name, &relative);
            if seen.insert(entry.clone()) {
                missing.push(entry);
            }
        }
        let identity = fanout_result_identity(Some(state_def), target, model);
        if let Some(entry) = missing_terminal_result_output(
            artifact_root,
            machine,
            task,
            selected_to,
            ResultInvocation {
                state: state_name,
                visit_count: visit,
                identity: identity.as_deref(),
            },
        ) {
            if !terminal_results.contains(&entry) {
                terminal_results.push(entry);
            }
        }
    }
    missing.extend(terminal_results);
    missing
}

/// What one invocation of `state_name` still owes: its declared `outputs:` in
/// declaration order, each judged by whether the path it resolves to exists,
/// then the ticket's result where the edge this exit takes is terminal and the
/// result has no content.
///
/// Takes the invocation as its six identity fields rather than as a resolution,
/// so a caller that never resolved an agent — prompt composition, which is
/// handed the identity it is composing for — asks this question here instead of
/// walking `outputs:` a second time and getting a second chance to name a
/// different file. The invocation is already chosen either way, so there is no
/// settings re-load, unlike [`collect_missing_required_outputs`], and one root
/// suffices.
///
/// `visit` is passed rather than derived, because a caller that has already
/// resolved this invocation's artifacts against a visit count must not be
/// answered against another one: that is how `## Result` and the retry
/// paragraph would come to name two files. §FS-rhei-memory.4.4
// §FS-rhei-agents.3.2 condition (2) §FS-rhei-states.3.3
#[allow(clippy::too_many_arguments)]
fn missing_required_outputs_for_invocation(
    artifact_root: &Path,
    machine: &rhei_validator::StateMachine,
    task: &rhei_core::ast::Task,
    state_name: &str,
    visit: u64,
    selected_to: Option<&str>,
    invocation: TransitionInvocationContext<'_>,
) -> Vec<MissingRequiredOutput> {
    let (target, model, model_provider, model_name, agent, agent_mode) = invocation;
    let terminal_result = missing_terminal_result_entry(
        artifact_root,
        machine,
        task,
        selected_to,
        ResultInvocation {
            state: state_name,
            visit_count: visit,
            identity: fanout_result_identity(machine.states.get(state_name), target, model)
                .as_deref(),
        },
    );
    let Some(state_def) = machine.states.get(state_name) else {
        return terminal_result.into_iter().collect();
    };

    let mut missing = Vec::new();
    for artifact in &state_def.outputs {
        let (relative, path) = resolve_artifact_path(
            artifact_root,
            artifact,
            &task.id.to_string(),
            state_name,
            Some(visit),
            target,
            model,
            model_provider,
            model_name,
            agent,
            agent_mode,
        );
        if !path.exists() {
            missing.push(MissingRequiredOutput {
                name: artifact.name.clone(),
                path,
                unresolved_template: is_unresolved_template(&relative),
                warning_spelling: relative,
            });
        }
    }
    missing.extend(terminal_result);
    missing
}

/// [`missing_required_outputs_for_invocation`] as the warning prints it, for
/// the exit paths that hold the resolution and report the stall.
// §FS-rhei-agents.3.2 condition (2)
#[allow(clippy::too_many_arguments)]
fn collect_missing_required_outputs_for_resolved_invocation(
    artifact_root: &Path,
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
    task: &rhei_core::ast::Task,
    state_name: &str,
    selected_to: Option<&str>,
    resolved: &ResolvedAgent,
) -> Vec<String> {
    let visit = render_visit_count(metadata, &task.id, state_name, task.state.as_str(), machine);
    missing_required_outputs_for_invocation(
        artifact_root,
        machine,
        task,
        state_name,
        visit,
        selected_to,
        invocation_context(resolved),
    )
    .iter()
    .map(MissingRequiredOutput::warning_entry)
    .collect()
}
