// `## Previous Visits`: what already happened to *this* task — the trail it
// took through the machine, every verdict recorded against it, and where the
// last transcript is.
//
// Its own part because this is the one section composed from a single task's
// own runtime record rather than from the graph around it.

// §AR-source-file-size.3 §FS-rhei-memory.3.3 §FS-rhei-memory.4.4

/// The states this task has been through, ending in the visit being composed.
///
/// The ledger records each hop as `from@to`, so the trail is the first hop's
/// source followed by every destination. The visit that is starting has no
/// ledger line yet: when the trail already ends in the state being entered —
/// the engine wrote the line that moved the task here — that last state is
/// annotated rather than repeated, which is what `pending → review → review`
/// used to say about a task that had been through `review` exactly once.
// §FS-rhei-memory.3.3 §FS-rhei-memory.4.4
fn render_visit_trail(
    render_context: &RuntimeTemplateContext<'_>,
    ledger: &[(String, String, String)],
) -> String {
    let task_id = render_context.task.id.to_string();
    let mut steps: Vec<String> = Vec::new();
    for (entry_task, from, to) in ledger {
        if entry_task != &task_id {
            continue;
        }
        // A ledger line carrying a `-<visit>` suffix names the same state as
        // the plain one; leaving it raw would both spell a state two ways and
        // defeat the annotate-in-place rule below. §FS-rhei-memory.3.1
        if steps.is_empty() {
            steps.push(normalized_state_name(from, render_context.machine));
        }
        steps.push(normalized_state_name(to, render_context.machine));
    }
    let visit = render_visit_count(
        render_context.metadata,
        &render_context.task.id,
        render_context.state_name,
        render_context.current_state_raw,
        render_context.machine,
    );
    let here = format!("{} (this visit, visit {visit})", render_context.state_name);
    match steps.last_mut() {
        // A self-loop leaves `fix` twice in the ledger, and both belong: the
        // second is the visit before this one, not this one.
        Some(last) if last.as_str() == render_context.state_name => *last = here,
        _ => steps.push(here),
    }
    format!("Trail for this task: {}.\n", steps.join(" \u{2192} "))
}

/// The log file of the previous visit of this same state, when it is on disk.
///
/// The path is enough: a transcript is not worth its tokens, but an agent
/// retrying a state that stalled has to be able to find out how.
// §FS-rhei-memory.3.3 §FS-rhei-memory.4.4 §FS-rhei-agents.8.1
fn render_previous_log(render_context: &RuntimeTemplateContext<'_>) -> String {
    let Some(memory) = render_context.memory else { return String::new() };
    let visit = render_visit_count(
        render_context.metadata,
        &render_context.task.id,
        render_context.state_name,
        render_context.current_state_raw,
        render_context.machine,
    );
    if visit <= 1 {
        return String::new();
    }
    // The previous visit's *last* attempt: where it was retried, that is the
    // one that ran, and the earlier ones are kept beside it. Keyed on
    // `visit_count − 1`, so an uncounted re-entry names none. §FS-rhei-agents.8.1
    let Some(path) = latest_agent_log_path(
        &memory.runtime_dir,
        &render_context.task.id.to_string(),
        render_context.state_name,
        agent_log_suffix(render_context.target, render_context.model, Some(visit - 1)).as_deref(),
    ) else {
        return String::new();
    };
    format!("\nPrevious log: `{}`\n", memory_path(render_context, &path))
}

/// What this retry still owes: every required artifact of *this* invocation's
/// completion condition that is not on disk as the prompt is composed, under the
/// names and in the order the missing-output warning gives them, and nothing at
/// all where nothing is unmet.
///
/// The list is the collector's, not a second walk over `outputs:` here: the
/// warning, the halt line, the run report and this clause are four readings of
/// one question, and the one a paid attempt reads is the one that may not
/// disagree. Two answers the collector cannot supply from a prompt come from the
/// prompt's own side of the same questions — the terminal edge, because no
/// transition has been selected yet, and the visit count, so that the result
/// named here and the result `## Result` names are one path.
///
/// Paths go through `memory_path`, which every other path in this section
/// already uses, including the transcript in this same sentence, so the clause
/// and that transcript cannot read against different bases. What it is handed is
/// the path that was checked — a single join of the relative spelling its author
/// wrote, which `memory_path` answers textually and therefore gives back
/// unchanged, so the `/` the author typed survives to the prompt on every
/// platform and the `result` entry is the string `## Result` already shows.
/// §FS-rhei-agents.4.1
///
/// Whether a path still carries a `{...}` template is the collector's answer
/// about the authored spelling, not a question asked of the rendered string: an
/// artifact root that happens to contain a brace would otherwise make every
/// entry claim to be a template the warning never called one.
// §FS-rhei-memory.4.4 §FS-rhei-agents.3.2.1
fn render_owed_clause(render_context: &RuntimeTemplateContext<'_>, visit: u64) -> String {
    let owed = missing_required_outputs_for_invocation(
        // The root `## Result` and `## Artifacts` resolve against, so the clause
        // cannot name a path they spell another way. §FS-rhei-panta.6.2
        render_context.workspace_root,
        render_context.machine,
        render_context.task,
        render_context.state_name,
        visit,
        terminal_state_reachable_from(render_context.machine, render_context.state_name),
        (
            render_context.target,
            render_context.model,
            render_context.model_provider,
            render_context.model_name,
            render_context.agent,
            render_context.agent_mode,
        ),
    );
    if owed.is_empty() {
        return String::new();
    }
    let entries: Vec<String> = owed
        .iter()
        .map(|entry| {
            let shown = memory_path(render_context, &entry.path);
            if entry.unresolved_template {
                format!("{} (`{shown}`, unresolved template)", entry.name)
            } else {
                format!("{} (`{shown}`)", entry.name)
            }
        })
        .collect();
    format!(" It did not write what this visit still owes: {}.", entries.join(", "))
}

/// What this visit already tried, when it has already tried something.
///
/// A re-spawn used to receive the prompt of the attempt it was recovering from,
/// byte for byte: same `RHEI_VISIT_COUNT`, no attempt number, and no
/// `Previous log:` line, because that line keys off the *previous visit* and a
/// stalled ticket never left this one. So attempt two did what attempt one did
/// and left the same thing unwritten. This paragraph is the difference: it says
/// that this is a retry, which attempt it is, how the last one ended, and which
/// files that attempt was obliged to write and did not.
///
/// Rendered only when the record belongs to *this* visit. A record from an
/// earlier stay in the state is not a retry, and telling a fresh entry that it
/// is one is the same untruth in the other direction.
// §FS-rhei-memory.3.3 §FS-rhei-memory.4.4 §FS-rhei-agents.3.2.1
fn render_retry_notice(render_context: &RuntimeTemplateContext<'_>, task_root: &Path) -> String {
    let Some(memory) = render_context.memory else { return String::new() };
    let visit = render_visit_count(
        render_context.metadata,
        &render_context.task.id,
        render_context.state_name,
        render_context.current_state_raw,
        render_context.machine,
    );
    let task_id = render_context.task.id.to_string();
    // Found by this invocation's own name, entry number included, or a retry
    // inside entry 2 would read entry 1's record and lose this paragraph. A
    // ledger the spawn will refuse on reads as entry 1 here. §FS-rhei-memory.3.3
    let number = resolve_log_number(
        render_context.machine,
        render_context.state_name,
        visit,
        task_root,
        &memory.runtime_dir,
        &task_id,
    )
    .unwrap_or(LogNumber::Entry(1));
    let plan = plan_spawn_attempt(
        &memory.runtime_dir,
        task_root,
        &task_id,
        render_context.state_name,
        agent_log_identity(render_context.target, render_context.model).as_deref(),
        number,
    );
    let Some(previous) = plan.previous.as_ref() else { return String::new() };
    let owed = render_owed_clause(render_context, visit);
    // Unmet only where the owed clause names what is unmet. §FS-rhei-memory.4.4
    let unmet = if previous.exited_zero() && !owed.is_empty() {
        " without meeting this state's completion condition"
    } else {
        ""
    };
    // The revert's own sentence ends the paragraph, after the transcript. §FS-rhei-memory.4.4
    let reverted = previous.reverted.as_ref().map_or(String::new(), |edit| {
        format!(
            " Its edit broke the plan at `{}` ({}), so the run reverted this task's text to what it \
             was before that attempt; what it wrote is in `{}`.",
            edit.location,
            edit.message,
            memory_path(render_context, &edit.text)
        )
    });
    format!(
        "\nRetrying this visit: attempt {}. The previous attempt {}{unmet}.{owed} Its transcript \
         is `{}`.{reverted}\n",
        plan.attempt,
        previous.ending_sentence(),
        memory_path(render_context, &previous.log)
    )
}

/// Every verdict recorded against this task so far, pasted whole.
///
/// This is where a worker's `--result` message and the engine's own failure
/// entries both land, so a retry can read why the last attempt did not stand.
// §FS-rhei-memory.3.3 §FS-rhei-memory.4.4 §FS-rhei-agents.3.2.1
fn render_result_entries(
    render_context: &RuntimeTemplateContext<'_>,
    body: &str,
) -> String {
    let (kept, truncated) = tail_lines(body, memory_caps::RESULT_LINES);
    // The file the body came from — the legacy link's, when a pre-qualification
    // plan is what carries this ticket's account. §FS-rhei-memory.4.4
    let path = resolved_result_path(render_context, &render_context.task.id).unwrap_or_else(|| {
        task_result_path(
            export_root_for_task(render_context, &render_context.task.id),
            &render_context.task.id,
        )
    });
    let overflow = if truncated {
        format!(
            "\u{2026} earlier entries omitted; read {}\n\n",
            memory_path(render_context, &path)
        )
    } else {
        String::new()
    };
    format!("\nResult entries so far:\n\n{overflow}{}\n", fenced_markdown(&kept))
}

/// `## Previous Visits` — omitted on a task's first invocation, which has no
/// ledger line and no result file, and therefore nothing to say.
// §FS-rhei-memory.3.3 §FS-rhei-memory.4.4
fn render_previous_visits(render_context: &RuntimeTemplateContext<'_>) -> MietteResult<String> {
    let Some(memory) = render_context.memory else { return Ok(String::new()) };
    let Some(rhei_id) = owning_rhei_id(render_context) else { return Ok(String::new()) };
    let root = memory
        .rhei_roots
        .get(&rhei_id)
        .map(PathBuf::as_path)
        .unwrap_or(render_context.workspace_root);
    let ledger = read_ledger(root)?;
    let task_id = render_context.task.id.to_string();
    let has_trail = ledger.iter().any(|(entry_task, _, _)| entry_task == &task_id);
    let result = read_task_result(render_context, &render_context.task.id)?;
    // A ticket retried on its first visit has neither a ledger line nor a
    // result, and it is exactly the invocation that most needs to be told it is
    // a retry. §FS-rhei-memory.4.4
    let retry = render_retry_notice(render_context, root);
    if !has_trail && result.is_none() && retry.is_empty() {
        return Ok(String::new());
    }
    let mut out = String::from("\n## Previous Visits\n\n");
    out.push_str(&render_visit_trail(render_context, &ledger));
    if let Some(body) = result {
        out.push_str(&render_result_entries(render_context, &body));
    }
    out.push_str(&render_previous_log(render_context));
    out.push_str(&retry);
    Ok(out)
}
