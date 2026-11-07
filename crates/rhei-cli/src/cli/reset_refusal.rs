// Whether a reset would leave a plan `rhei validate` refuses: asked by the reset
// itself before its preview, and by every message that offers it as a way back.
//
// Its own part because the question is about the plan the destinations add up
// to, which no one task's destination answers: it loads that plan through the
// loader every command uses, from the text the reset would write.

// §AR-source-file-size.3 §FS-rhei-reset.2.3

/// What a refused reset, and a message declining to offer one, say. One phrase,
/// so the renderer can keep both on their lines. §FS-rhei-reset.4 §FS-rhei-reset.5.1
const RESET_LEAVES_AN_INVALID_PLAN: &str = "the plan it would leave fails validation";

/// Whether a diagnostic says a reset is or would be refused, which the renderer
/// keeps unwrapped: it names each error on a line of its own, in the validator's
/// words, and a file to edit. §FS-rhei-reset.4 §FS-rhei-reset.5.1
fn says_a_reset_is_refused(text: &str) -> bool {
    [RESET_LEAVES_AN_INVALID_PLAN, RESET_CANNOT_WRITE, RESET_STOPPED_PART_WAY]
        .iter()
        .any(|phrase| text.contains(phrase))
}

/// A reset that would introduce an error validation reports, and what its refusal
/// says about it. §FS-rhei-reset.2.3 §FS-rhei-reset.4
struct ResetRefusal {
    /// Each error introduced, in the validator's words.
    errors: Vec<String>,
    /// `(task, state now, state recorded)` for each task whose recorded state the
    /// machine no longer declares, which is why it stays where it is.
    undeclared: Vec<(String, String, String)>,
    /// The way on, where one works.
    help: Vec<String>,
}

impl ResetRefusal {
    /// The refusal as §FS-rhei-reset.4 prints it, the same with or without `--dry-run`.
    fn report(&self) -> Report {
        let mut lines =
            vec![format!("Refusing to reset: {RESET_LEAVES_AN_INVALID_PLAN}, so nothing was changed.")];
        lines.extend(self.errors.iter().map(|error| format!("  {error}")));
        for (task_id, current, recorded) in &self.undeclared {
            lines.push(format!(
                "Task {task_id} started in '{recorded}', which this state machine no longer \
                 declares; a reset would leave it in '{current}'."
            ));
        }
        // §FS-rhei-reset.4: no edit and no moved task named, so this line; a help is never empty.
        let help = if self.help.is_empty() {
            "no edit to a **State:** line is known to let this reset through: fix what the \
             errors above name, then run the reset again"
                .to_string()
        } else {
            self.help.join("\n")
        };
        miette!(help = help, "{}", lines.join("\n"))
    }
}

/// The refusal for a reset of `scope` that moves the tasks `authored` says it moves,
/// or `None` when the plan it would leave introduces nothing. §FS-rhei-reset.2.3
fn reset_refusal(
    loaded: &LoadedPlan,
    input: &Path,
    scope: &RheiScope,
    machines: &rhei_validator::MachineSet,
    authored: &AuthoredStates,
) -> MietteResult<Option<ResetRefusal>> {
    let standing = rhei_validator::validate_with_machine_set(&loaded.rhei, machines).errors;
    let errors = errors_a_reset_introduces(loaded, input, scope, machines, authored, &standing)?;
    if errors.is_empty() {
        return Ok(None);
    }
    let help = refusal_help(loaded, input, scope, machines, authored, &standing, &errors);
    Ok(Some(ResetRefusal { errors, undeclared: authored.undeclared.clone(), help }))
}

/// The errors the reset of `rhei` a message offers would introduce, which is the
/// whole rhei and not one ticket's tree. A plan the reset cannot even rewrite gives
/// none here: that reset fails before its preview with nothing deleted, so offering
/// it costs no one anything. §FS-rhei-reset.5.1 §FS-rhei-reset.2.3
fn errors_the_offered_reset_introduces(
    loaded: &LoadedPlan,
    input: &Path,
    rhei: &str,
    machines: &rhei_validator::MachineSet,
) -> Vec<String> {
    let scope: RheiScope = Some(BTreeSet::from([rhei.to_string()]));
    let authored = collect_authored_states(loaded, input, &scope, machines);
    let standing = rhei_validator::validate_with_machine_set(&loaded.rhei, machines).errors;
    errors_a_reset_introduces(loaded, input, &scope, machines, &authored, &standing)
        .unwrap_or_default()
}

/// Validate the plan the reset would leave, and keep each error `standing`, the
/// plan's errors as it stands, does not already carry. Links are not checked: the
/// runtime output they point at is what a reset deletes. §FS-rhei-reset.2.3
fn errors_a_reset_introduces(
    loaded: &LoadedPlan,
    input: &Path,
    scope: &RheiScope,
    machines: &rhei_validator::MachineSet,
    authored: &AuthoredStates,
    standing: &[String],
) -> MietteResult<Vec<String>> {
    let left = plan_a_reset_leaves(loaded, input, scope, authored, HashMap::new())?;
    let errors = rhei_validator::validate_with_machine_set(&left.rhei, machines).errors;
    Ok(errors_introduced_over(standing, errors))
}

/// The plan as a reset of `scope` would leave it, loaded as every command loads a
/// plan: each file the reset rewrites read as [`reset_file_text`] would write it,
/// starting from `staged` where a file is staged there and from disk otherwise.
/// Every other file reads as it stands. §FS-rhei-reset.2.3
fn plan_a_reset_leaves(
    loaded: &LoadedPlan,
    input: &Path,
    scope: &RheiScope,
    authored: &AuthoredStates,
    mut staged: HashMap<PathBuf, String>,
) -> MietteResult<LoadedPlan> {
    let no_moves = BTreeMap::new();
    for (file, _sample_task_id) in reset_target_files(loaded, input, scope) {
        let raw = match staged.get(&file) {
            Some(text) => text.clone(),
            None => read_plan_source(&file, "failed to read plan file")?,
        };
        let text = reset_file_text(&raw, authored.by_file.get(&file).unwrap_or(&no_moves))?;
        staged.insert(file, text);
    }
    rhei_core::source::read_staged(staged, || load_plan(input))
}

/// The help of a refusal (§FS-rhei-reset.4). Where a finished task the reset leaves
/// where it stands is named by an error, and setting its line to its profile's
/// non-terminal `initial` lets the reset through, it names that edit; otherwise it
/// says where the ledger sends each named task the reset moves. An edit it cannot
/// check is one it does not name. §FS-rhei-reset.2.3
fn refusal_help(
    loaded: &LoadedPlan,
    input: &Path,
    scope: &RheiScope,
    machines: &rhei_validator::MachineSet,
    authored: &AuthoredStates,
    standing: &[String],
    errors: &[String],
) -> Vec<String> {
    let mut tasks = Vec::new();
    collect_plan_tasks(&loaded.rhei.tasks, &mut tasks);
    let named: Vec<&rhei_core::ast::Task> = tasks
        .into_iter()
        .filter(|task| {
            // The tree rule says `Task <id>` of every kind; the profile and `**Target:**`
            // rules say `<Kind> <id>`. §FS-rhei-reset.4
            let labels = [format!("Task {}", task.id), format!("{} {}", title_case_kind(&task.kind), task.id)];
            errors.iter().any(|error| labels.iter().any(|label| error_names(error, label)))
        })
        .collect();
    let moves: BTreeMap<&str, &str> =
        authored.moves.iter().map(|mv| (mv.task_id.as_str(), mv.to.as_str())).collect();

    let left_finished: Vec<&rhei_core::ast::Task> = named
        .iter()
        .copied()
        .filter(|task| !moves.contains_key(task.id.to_string().as_str()))
        .filter(|task| is_terminal_state(&task.state, machines.for_task(&task.id)))
        .collect();
    // Every such task needs an edit the help can name, or the help names none.
    let edits: Option<Vec<(&rhei_core::ast::Task, String)>> = left_finished
        .iter()
        .map(|task| {
            let machine = machines.for_task(&task.id);
            let initial = initial_state_for_node(machine, &task.kind, task.profile_level()).ok();
            initial.filter(|state| state_reopens(machine, state)).map(|state| (*task, state))
        })
        .collect();
    if let Some(edits) = edits.filter(|edits| !edits.is_empty()) {
        let through = edits_let_the_reset_through(loaded, input, scope, machines, standing, &edits);
        if through.unwrap_or(false) {
            return edits.iter().map(|(task, initial)| edit_line(loaded, input, task, initial)).collect();
        }
    }

    // §FS-rhei-reset.4: no edit to a line changes where the ledger sends a task.
    named
        .iter()
        .filter_map(|task| {
            let task_id = task.id.to_string();
            moves.get(task_id.as_str()).map(|dest| {
                format!(
                    "a reset moves Task {task_id} to '{dest}', the state the ledger records it \
                     started in, whatever its **State:** line says"
                )
            })
        })
        .collect()
}

/// Whether making every edit lets the reset through: the edited plan introduces
/// nothing over the plan as it stands, and the reset of the edited plan introduces
/// nothing over the edited plan, so the plan is valid between the two and after
/// them. Both are asked of plans loaded from the edited text. §FS-rhei-reset.2.3
fn edits_let_the_reset_through(
    loaded: &LoadedPlan,
    input: &Path,
    scope: &RheiScope,
    machines: &rhei_validator::MachineSet,
    standing: &[String],
    edits: &[(&rhei_core::ast::Task, String)],
) -> MietteResult<bool> {
    let mut by_file: BTreeMap<PathBuf, BTreeMap<String, String>> = BTreeMap::new();
    for (task, initial) in edits {
        let route = loaded.task_route(&task.id.to_string(), input);
        by_file.entry(route.task_file).or_default().insert(route.local_id, initial.clone());
    }
    let mut staged = HashMap::new();
    for (file, file_edits) in by_file {
        let raw = read_plan_source(&file, "failed to read plan file")?;
        let edited = rewrite_states_to_authored(&raw, &file_edits)?;
        let edited =
            strip_result_links_under(&edited, |id| id.is_some_and(|id| file_edits.contains_key(id)));
        staged.insert(file, edited);
    }

    let edited = rhei_core::source::read_staged(staged.clone(), || load_plan(input))?;
    let edited_errors = rhei_validator::validate_with_machine_set(&edited.rhei, machines).errors;
    if !errors_introduced_over(standing, edited_errors.clone()).is_empty() {
        return Ok(false);
    }
    let authored = collect_authored_states(&edited, input, scope, machines);
    let left = plan_a_reset_leaves(&edited, input, scope, &authored, staged)?;
    let errors = rhei_validator::validate_with_machine_set(&left.rhei, machines).errors;
    Ok(errors_introduced_over(&edited_errors, errors).is_empty())
}

/// The edit the help names for one finished task, as §FS-rhei-reset.4 prints it.
fn edit_line(
    loaded: &LoadedPlan,
    input: &Path,
    task: &rhei_core::ast::Task,
    initial: &str,
) -> String {
    let task_id = task.id.to_string();
    let file = display_path(&loaded.task_file(&task_id, input));
    // §FS-rhei-reset.2.4
    let result = if has_result_link(&task.content) { " and delete the `> **Result:**` line under it" } else { "" };
    format!(
        "a reset leaves Task {task_id} where its **State:** line in {file} puts it, because \
         nothing records a state it could restore: edit that line to '{initial}'{result}, then \
         run the reset again"
    )
}

/// Whether `error` names the task `label` (`Task plan.1`) is, and not one whose id
/// only starts with it, such as `Task plan.1.1`.
fn error_names(error: &str, label: &str) -> bool {
    let continues_id = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-';
    error.match_indices(label).any(|(at, _)| {
        let before = error[..at].chars().next_back();
        let mut after = error[at + label.len()..].chars();
        let ends = match after.next() {
            None => true,
            Some('.') => !after.next().is_some_and(continues_id),
            Some(next) => !continues_id(next),
        };
        ends && !before.is_some_and(|c| c.is_ascii_alphanumeric())
    })
}
