// Where a ticket's account lives on disk, and the obligation a terminal edge
// carries: the ticket-level result file, the per-invocation fragments a
// fanned-out state writes instead, the merge that folds them in, and the check
// that refuses a `final: true` entry with nothing written.
//
// Its own part because the shared transition path only *asks* whether the
// obligation is met; deciding which file answers for which invocation is a
// question about artifact layout, not about applying a transition.

// §AR-source-file-size.3 §FS-rhei-states.3.3 §FS-rhei-complete.3

/// The ticket's result file under the owning rhei's execution root.
/// §FS-rhei-complete.3
fn result_file_path(artifact_root: &Path, task_id: &str) -> PathBuf {
    artifact_root.join("runtime").join("results").join(format!("{task_id}.md"))
}

/// Which invocation a result path belongs to: the state that invocation worked,
/// that state's visit number, and its fan-out identity.
///
/// The three travel together because all three key the path, and because the
/// side that *tells* a worker where to write and the side that later *checks*
/// and merges must derive it identically. `identity: None` means a state that
/// runs one invocation, which writes the ticket's result file directly.
// §FS-rhei-states.3.3
#[derive(Clone, Copy)]
struct ResultInvocation<'a> {
    state: &'a str,
    visit_count: u64,
    identity: Option<&'a str>,
}

impl<'a> ResultInvocation<'a> {
    /// The ticket-level file: the state runs once, so nothing keys a fragment.
    fn whole_task() -> Self {
        Self { state: "", visit_count: 1, identity: None }
    }
}

/// Where one invocation writes its account, relative to the artifact root.
///
/// A single-invocation state writes the ticket's result file itself. A
/// fanned-out state gives every invocation its own fragment, keyed the way the
/// rest of that invocation's artifacts are keyed — state, visit, identity —
/// because one shared path would let the last writer erase its siblings and the
/// first writer satisfy the obligation on everyone's behalf, and a path keyed by
/// identity alone would let a fragment from an earlier fanned-out state, or an
/// earlier visit of this one, stand in as this invocation's answer.
// §FS-rhei-states.3.3
fn result_relative_path(task_id: &str, invocation: ResultInvocation<'_>) -> String {
    match invocation.identity {
        Some(identity) => format!(
            "runtime/results/{task_id}/{}/{}/{identity}.md",
            invocation.state, invocation.visit_count
        ),
        None => format!("runtime/results/{task_id}.md"),
    }
}

/// [`result_relative_path`] resolved against the owning rhei's execution root.
// §FS-rhei-states.3.3
fn invocation_result_file_path(
    artifact_root: &Path,
    task_id: &str,
    invocation: ResultInvocation<'_>,
) -> PathBuf {
    artifact_root.join(result_relative_path(task_id, invocation))
}

/// The per-invocation key a fanned-out state's result fragments are filed
/// under: the target slug for `all_targets`, the model id for `all_models`.
///
/// `None` for every state that runs one invocation — those keep writing the
/// ticket's result file directly, so nothing changes for the common case. A
/// `program:` state is one of them however many targets it declares: `rhei run`
/// spawns a program once per ticket, so demanding a fragment per target would
/// ask for files nothing can write.
// §FS-rhei-states.3.3 §FS-rhei-transitions.4.2 §FS-rhei-programs.2
fn fanout_result_identity(
    state_def: Option<&rhei_validator::StateDef>,
    target: Option<&ExecutionTarget>,
    model: Option<&str>,
) -> Option<String> {
    let state_def = state_def?;
    if state_def.program.is_some() {
        return None;
    }
    if !state_def.all_targets.is_empty() {
        return target.map(|target| target.slug());
    }
    if !state_def.all_models.is_empty() {
        return model.map(slugify_target_value);
    }
    None
}

/// Every result fragment a fanned-out state's invocations were told to write,
/// in declared invocation order, paired with the identity that keyed it.
// §FS-rhei-states.3.3
fn fanout_result_fragments(
    artifact_root: &Path,
    task_id: &str,
    state_name: &str,
    visit_count: u64,
    state_def: &rhei_validator::StateDef,
    invocations: &[ResolvedAgent],
) -> Vec<(String, PathBuf)> {
    invocations
        .iter()
        .filter_map(|resolved| {
            let identity = fanout_result_identity(
                Some(state_def),
                resolved.target.as_ref(),
                resolved.model.as_deref(),
            )?;
            let path = invocation_result_file_path(
                artifact_root,
                task_id,
                ResultInvocation {
                    state: state_name,
                    visit_count,
                    identity: Some(&identity),
                },
            );
            Some((identity, path))
        })
        .collect()
}

/// Fold a fanned-out state's per-invocation fragments into the ticket's one
/// result file, one attributed `## Result` entry each, in declared invocation
/// order.
///
/// Called by `rhei run` once every declared invocation has satisfied its own
/// completion condition and before the transition is applied, so the shared path
/// sees a single non-empty result carrying every worker's account instead of
/// whichever invocation happened to write last. The heading carries the identity
/// and no arrow, so the result-file history reader (which keys on `<from> →
/// <to>` headings) still reads the file the way it always did.
///
/// **Idempotent.** The merged block is a deterministic function of the
/// fragments, so a block the result file already carries is not appended again:
/// a move refused after the merge (a target `inputs:` artifact missing, say)
/// leaves the merge on disk and the next attempt over the same fragments adds
/// nothing. Fragments that changed since are a different block and do append —
/// entries accumulate, exactly as any other carried message does, so a result
/// the ticket collected on an earlier hop is kept.
///
/// Every declared invocation must have written its fragment. This is the same
/// rule declared `outputs:` follow — those are checked across every invocation
/// identity on the shared path (`ensure_state_outputs_exist_for_transition`).
/// Under `rhei run` it is a backstop rather than the ordinary path: a silent
/// invocation fails its own completion condition first
/// (`task_has_pending_agent_invocations`), so nothing gets this far.
// §FS-rhei-states.3.3 §FS-rhei-agents.3.2 §FS-rhei-complete.3.2
fn merge_fanout_result_fragments(
    artifact_root: &Path,
    task_id: &str,
    state_name: &str,
    visit_count: u64,
    state_def: &rhei_validator::StateDef,
    invocations: &[ResolvedAgent],
) -> MietteResult<bool> {
    let fragments =
        fanout_result_fragments(artifact_root, task_id, state_name, visit_count, state_def, invocations);
    if fragments.is_empty() {
        return Ok(false);
    }
    let mut merged = String::new();
    let mut missing: Vec<String> = Vec::new();
    for (identity, path) in fragments {
        let content = fs::read_to_string(&path).unwrap_or_default();
        if content.trim().is_empty() {
            missing.push(format!("{identity} ({})", path.display()));
            continue;
        }
        // A fragment that already opens with its own `## Result` heading is
        // re-titled rather than nested: the merged file must read as one list
        // of entries, not as a heading inside a heading.
        let trimmed = content.trim();
        let body = trimmed.strip_prefix("## Result").map(str::trim_start).unwrap_or(trimmed);
        merged.push_str(&format!("## Result \u{2014} {identity}\n\n{body}\n\n"));
    }
    if !missing.is_empty() {
        return Err(miette!(
            help = format!(
                "each invocation of a fanned-out state writes its own result, and the ticket's \
                 result is the merge of them. Rerun to let the missing invocation(s) write, or \
                 write the file(s) named above."
            ),
            "Task {} cannot finish from '{}': {} of its fan-out invocation(s) wrote no result.\n\
             Missing: {}",
            task_id,
            state_name,
            missing.len(),
            missing.join(", ")
        ));
    }
    let destination = result_file_path(artifact_root, task_id);
    let mut existing = fs::read_to_string(&destination).unwrap_or_default();
    // Already merged: the same fragments produce the same block, and this runs
    // again on every retry of a move that was refused after it. §FS-rhei-states.3.3
    if existing.contains(&merged) {
        return Ok(true);
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| file_io_report(parent, "failed to create runtime/results", err))?;
    }
    if !existing.trim().is_empty() && !existing.ends_with('\n') {
        existing.push('\n');
    }
    let combined =
        if existing.trim().is_empty() { merged } else { format!("{existing}{merged}") };
    fs::write(&destination, combined)
        .map_err(|err| file_io_report(&destination, "failed to merge fanout results", err))?;
    Ok(true)
}

/// The ticket's result file as a program subprocess must see it: absolute.
///
/// A subprocess runs from the checkout root, which is routinely not the Rhei
/// artifact root, so a root that was itself given relative to `rhei run`'s own
/// working directory would resolve somewhere else entirely in the child.
// §FS-rhei-programs.2
fn absolute_result_file_path(artifact_root: &Path, task_id: &str) -> PathBuf {
    let path = result_file_path(artifact_root, task_id);
    std::path::absolute(&path).unwrap_or(path)
}

/// Whether a file exists and holds something other than whitespace.
///
/// Whitespace-only counts as absent, on the same reading state handoffs use: an
/// existence-only contract would otherwise let an empty file stand in for an
/// answer.
// §FS-rhei-states.3.3 §FS-rhei-states.3.2
fn file_has_content(path: &Path) -> bool {
    fs::read_to_string(path).map(|content| !content.trim().is_empty()).unwrap_or(false)
}

/// Whether the ticket already has a result worth the name.
// §FS-rhei-states.3.3
fn task_result_is_present(artifact_root: &Path, task_id: &str) -> bool {
    file_has_content(&result_file_path(artifact_root, task_id))
}

/// Whether any fan-out invocation of this ticket has left a fragment behind.
///
/// Only used to explain a refusal: the fragments are real accounts of real work,
/// and an operator who sees the ticket-level path named as empty should be told
/// where the rest of the story is.
// §FS-rhei-states.3.3
fn fanout_result_fragments_exist(artifact_root: &Path, task_id: &str) -> bool {
    let root = artifact_root.join("runtime").join("results").join(task_id);
    fs::read_dir(&root).map(|mut entries| entries.next().is_some()).unwrap_or(false)
}

/// Reject an edge into a `final: true` state when nothing says why the ticket
/// ended there.
///
/// The terminal result is an artifact contract of the target state that no
/// machine declares and none can opt out of. `outputs:` cannot express it —
/// those are checked when a state is *left*, and a terminal state is never left
/// — so it is enforced here, on the edge in, at the same point the target
/// state's `inputs:` are enforced and against the same effective target, so a
/// callback `nextState` redirect cannot smuggle a terminal entry past it.
///
/// It is satisfied by an existing non-empty result file or by a message the
/// caller carried through the move; the message is appended once the move
/// succeeds.
// §FS-rhei-states.3.3 §FS-rhei-transition-cmd.3.2
fn ensure_terminal_result_available(
    machine: &rhei_validator::StateMachine,
    artifact_root: &Path,
    qualified_id: &str,
    from: &str,
    to: &str,
    carried_message: Option<&str>,
    plan_path: &Path,
) -> MietteResult<()> {
    if !machine.states.get(to).map(|def| def.terminal).unwrap_or(false) {
        return Ok(());
    }
    if carried_message.is_some_and(|message| !message.trim().is_empty()) {
        return Ok(());
    }
    if task_result_is_present(artifact_root, qualified_id) {
        return Ok(());
    }
    let relative = format!("runtime/results/{qualified_id}.md");
    // A ticket that fanned out has its workers' accounts on disk as fragments;
    // only `rhei run` folds them in, so say so rather than let the operator read
    // the empty ticket-level path as lost work. §FS-rhei-states.3.3
    let fragments = if fanout_result_fragments_exist(artifact_root, qualified_id) {
        format!(
            " This ticket has fan-out result fragments under runtime/results/{qualified_id}/; \
             `rhei run` merges those into {relative} when it takes the edge, and a manual \
             finish carries its own --result."
        )
    } else {
        String::new()
    };
    // Name the file that was checked and the flag that carries the message:
    // "write a result" is the answer, but the user still has to know where.
    // §FS-rhei-errors.2 §FS-rhei-errors.1.2.1
    Err(miette!(
        help = format!(
            "a final state records why the ticket ended there. Pass it on the move: {} ({} for \
             the everyday finish), or write {relative} before the move.{fragments}",
            help_command_with_or(
                "transition",
                &format!("--task {qualified_id} --from {from} --to {to} --result \"<what happened>\""),
                plan_path
            ),
            help_command_with_or(
                "complete",
                &format!("--task {qualified_id} --result \"<what happened>\""),
                plan_path
            )
        ),
        "Task {} cannot enter terminal state '{}' without a result.\n\
         Expected a non-empty result file at: {}",
        qualified_id,
        to,
        result_file_path(artifact_root, qualified_id).display()
    ))
}
