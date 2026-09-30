// `rhei note` — the one writer of the project note store, and the only channel
// that carries a fact sideways into the future.
//
// Its own part because writing the store is the whole command: it resolves no
// state machine, moves no ticket, and touches no plan. What it does own is the
// slot, the line bound, the duplicate refusal and the lock — the four things an
// entry written around the verb would hold none of.

// §FS-rhei-note

/// Which of the slot's three uses this invocation is spending it on.
// §FS-rhei-note.2
enum NoteForm {
    Record(String),
    Restate(String),
    Strike(String),
}

/// Text with every run of whitespace collapsed, for the duplicate rule: an
/// entry rewrapped across three lines is the same sentence, and the match is
/// exact after that and nothing fuzzier. §FS-rhei-note.4
fn note_normalized(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A usage error in clap's own voice, so the exit code is the `2` every other
/// malformed invocation of this CLI produces. §FS-rhei-note.5
fn note_usage_error(message: String) -> ! {
    let mut command = cli_command();
    command.error(ErrorKind::MissingRequiredArgument, message).exit()
}

/// The task whose slot is being spent: `--task` first, then `RHEI_TASK_ID`,
/// which `rhei run` exports to every agent it spawns. With neither, the verb
/// cannot know whose entry this is, and that is usage rather than a refusal.
// §FS-rhei-note.1 §FS-rhei-note.5
fn note_writing_task(options: &NoteOptions) -> String {
    if let Some(task) = options.task.as_deref().map(str::trim).filter(|id| !id.is_empty()) {
        return task.to_string();
    }
    match std::env::var("RHEI_TASK_ID") {
        Ok(task) if !task.trim().is_empty() => task.trim().to_string(),
        _ => note_usage_error(
            "no writing task: `rhei note` spends one task's slot, so name it with \
             `--task <ticket-id>` or run under `rhei run`, which exports RHEI_TASK_ID."
                .to_string(),
        ),
    }
}

/// Which form the parsed flags carry. The group declared on [`NoteOptions`] has
/// already refused none of them and more than one of them.
// §FS-rhei-note.1
fn note_form(options: &NoteOptions) -> NoteForm {
    if let Some(target) = options.restate.as_deref() {
        return NoteForm::Restate(target.to_string());
    }
    if let Some(target) = options.strike.as_deref() {
        return NoteForm::Strike(target.to_string());
    }
    NoteForm::Record(options.text.clone().unwrap_or_default())
}

/// A task id argument as the store spells ids: qualified, when the plan can say
/// so. A `--restate` or `--strike` naming a task this plan does not hold is
/// **not** an error — the entry may have been struck, or may arrive later — so
/// an unresolvable target is carried through as written.
// §FS-rhei-note.4 §FS-rhei-panta.6.1
fn note_qualified_id(loaded: &LoadedPlan, raw: &str, scope: &RheiScope) -> String {
    resolve_cli_task_id(loaded, raw, scope).unwrap_or_else(|_| raw.to_string())
}

/// The standing context the writing task's own prompt already carries verbatim:
/// its `### Rhei Context` and the project's `### Project Context`.
///
/// The refusal points at exactly what composition pastes, so the store cannot
/// spend a slot repeating what the plan writer already said.
// §FS-rhei-note.4 §FS-rhei-memory.3.1
fn note_writers_standing_context(
    loaded: &LoadedPlan,
    input: &Path,
    project_root: &Path,
    task_id: &str,
) -> String {
    let memory = prompt_memory(loaded, input, &project_root.join("runtime"), BTreeSet::new());
    let rhei_id = find_task_by_id_str(&loaded.rhei.tasks, task_id).and_then(rhei_id_of);
    format!(
        "{}\n{}",
        scoped_content_sections(&memory, rhei_id.as_deref()),
        scoped_content_sections(&memory, None)
    )
}

/// The entry's text, bounded. Refused rather than truncated: an agent can retry
/// in the same breath, and a silently halved fact is worse than no fact.
// §FS-rhei-note.4
fn note_bounded_text(raw: &str) -> MietteResult<String> {
    let text = raw.trim_end().to_string();
    if text.trim().is_empty() {
        return Err(miette!(
            help = "pass the fact itself: rhei note \"<what the next ticket would rediscover>\".",
            "an entry has no text"
        ));
    }
    let lines = text.lines().count();
    if lines > memory_caps::NOTE_ENTRY_LINES {
        return Err(miette!(
            help = "shorten it, or leave the detail in your result file and note where it is.",
            "an entry is bounded at {} lines and this one has {lines}; nothing was written",
            memory_caps::NOTE_ENTRY_LINES
        ));
    }
    Ok(text)
}

/// Refuse a fact the writing task's own prompt already carries. Checked before
/// the store is opened, so a refusal leaves no file behind. §FS-rhei-note.4
fn refuse_duplicate_of_standing_context(text: &str, context: &str) -> MietteResult<()> {
    let normalized = note_normalized(text);
    if note_normalized(context).contains(&normalized) {
        return Err(miette!(
            help = "every prompt already carries the rhei and project context; \
                    note what is not in it.",
            "that text is already in this task's own standing context; nothing was written"
        ));
    }
    Ok(())
}

/// Refuse a fact a live entry already carries, so the store cannot say the same
/// sentence twice at every later reader's expense. Exact after normalization:
/// anything fuzzier is selection by similarity. §FS-rhei-note.4
fn refuse_duplicate_of_live_entry(text: &str, live: &[NoteEntry]) -> MietteResult<()> {
    let normalized = note_normalized(text);
    if let Some(entry) = live.iter().find(|entry| note_normalized(&entry.text) == normalized) {
        return Err(miette!(
            help = "endorse it instead, which costs you the same slot: \
                    rhei note --restate <task-id>.",
            "{} already left that entry; nothing was written",
            entry.task
        ));
    }
    Ok(())
}

/// What a successful call says: the record, and what it costs every later
/// prompt. §FS-rhei-note.5
fn report_note_written(record: &NoteRecord) {
    match record {
        NoteRecord::Left { task, text } => {
            let lines = text.lines().count().max(1);
            let noun = if lines == 1 { "line" } else { "lines" };
            println!(
                "noted as {task} \u{2014} {lines} {noun}, composed into later prompts from now on"
            );
        }
        NoteRecord::Restates { target, .. } => println!(
            "restated {target} \u{2014} now the newest entry, and this task's slot is spent"
        ),
        NoteRecord::Strikes { target, .. } => println!(
            "struck {target} \u{2014} out of composition; its bytes stay in runtime/notes.md"
        ),
    }
}

/// Append one record under the store's lock, after the one refusal that needs
/// the store's own bytes to decide.
///
/// The read happens inside the hold, so the live entries a duplicate is judged
/// against are the ones the record is appended after. A refusal restores the
/// file, which removes it again when this call is what created it.
// §FS-rhei-note.3.3 §FS-rhei-note.4
fn append_note_record(
    project_root: &Path,
    record: &NoteRecord,
    duplicate_check: Option<&str>,
) -> MietteResult<()> {
    let mut journal = LockedRuntimeJournal::open(project_root, NOTE_STORE_JOURNAL)?;
    let store = journal.path().to_path_buf();
    let outcome = (|| {
        if let Some(text) = duplicate_check {
            // Readers take no lock, so `read_to_string` is how every reader of
            // the store sees it; the hold is what keeps a second writer out.
            let contents = fs::read_to_string(journal.path()).unwrap_or_default();
            refuse_duplicate_of_live_entry(text, &fold_note_store(&contents))?;
        }
        // One whole item, trailing newline included, so a partial append is the
        // only thing `restore` ever has to take back.
        let markdown = record.to_markdown();
        let file = journal.handle();
        file.write_all(markdown.as_bytes())
            .map_err(|err| file_io_report(&store, "failed to write the note store", err))?;
        file.flush()
            .map_err(|err| file_io_report(&store, "failed to flush the note store", err))
    })();
    if outcome.is_err() {
        journal.restore()?;
    }
    outcome
}

/// Execute the `note` subcommand: spend the writing task's one slot.
///
/// The plan is loaded leniently, for the reason `rhei show`'s is: leaving a
/// note is what an agent reaches for *while* something else is broken, and the
/// store is not plan markdown, so an unrelated rhei that will not load costs a
/// writer in a healthy one nothing but a warning. It does not excuse the
/// writing task itself: resolution is `rhei complete`'s, per §FS-rhei-note.1,
/// and a task the plan cannot place is refused, because a record carries its
/// writer's id as its provenance and an entry nothing can be traced to would
/// stand in front of every later prompt.
// §FS-rhei-note.1 §FS-rhei-note.2 §FS-rhei-note.4
fn note_command(input: &Path, rhei_scope: &[String], options: &NoteOptions) -> MietteResult<()> {
    let input_buf = normalize_workspace_input(input);
    let input = input_buf.as_path();
    let project_root = execution_workspace_root(input);
    let loaded = load_plan_leniently(input)?;
    for skipped in &loaded.unloadable {
        eprintln!("warning: {skipped}");
    }
    let scope = resolve_rhei_scope(&loaded, rhei_scope)?;
    // §FS-rhei-errors.1.3: an id the plan cannot place is a near miss, reported
    // by the same resolver every other ticket-taking command reports one with.
    let task = resolve_cli_task_id(&loaded, &note_writing_task(options), &scope)?;

    let (record, duplicate_check) = match note_form(options) {
        NoteForm::Record(raw) => {
            let text = note_bounded_text(&raw)?;
            refuse_duplicate_of_standing_context(
                &text,
                &note_writers_standing_context(&loaded, input, &project_root, &task),
            )?;
            (NoteRecord::Left { task, text: text.clone() }, Some(text))
        }
        // Restatement is the one exception to the duplicate rule, and it is one
        // precisely because it costs the restater its own slot. §FS-rhei-note.4
        NoteForm::Restate(target) => (
            NoteRecord::Restates { task, target: note_qualified_id(&loaded, &target, &scope) },
            None,
        ),
        NoteForm::Strike(target) => (
            NoteRecord::Strikes { task, target: note_qualified_id(&loaded, &target, &scope) },
            None,
        ),
    };
    append_note_record(&project_root, &record, duplicate_check.as_deref())?;
    report_note_written(&record);
    Ok(())
}
