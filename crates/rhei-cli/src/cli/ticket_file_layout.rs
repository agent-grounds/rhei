/// How a plan's top-level tickets sit in its plan files, as `rhei run
/// --parallel N` judges them.
///
/// The one place the rule is computed: the run decides its fallback from it,
/// and `rhei validate` warns from the same answer, so the warning can never
/// name a layout the run would schedule in parallel, or miss one it would not.
/// Only top-level tickets count toward a file: a ticket and its subtasks are
/// one unit, so descendants never add to a file's count. A single-file plan
/// maps its tickets to the plan file itself, so two of them are `AllInOne`.
/// §FS-rhei-run.2.5 §FS-rhei-validate.4.5
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TicketFileLayout<'a> {
    /// Every top-level ticket lives in this one file and there are at least
    /// two of them: `rhei run` falls back to sequential execution.
    AllInOne(&'a Path),
    /// The tickets span several files, but this one holds two or more of them.
    Shared(&'a Path),
    /// No file holds two top-level tickets.
    Separate,
}

// §FS-rhei-run.2.5
fn ticket_file_layout(loaded: &LoadedPlan) -> TicketFileLayout<'_> {
    let mut counts: BTreeMap<&Path, usize> = BTreeMap::new();
    for task in &loaded.rhei.tasks {
        if let Some(path) = loaded.task_sources.get(&task.id.to_string()) {
            *counts.entry(path.as_path()).or_default() += 1;
        }
    }
    match counts.iter().find(|(_, count)| **count > 1) {
        Some((path, _)) if counts.len() == 1 => TicketFileLayout::AllInOne(path),
        Some((path, _)) => TicketFileLayout::Shared(path),
        None => TicketFileLayout::Separate,
    }
}

/// One warning per `concurrent: true` state that the plan's file layout
/// serializes: when every top-level ticket is in one file, `rhei run` drops
/// `--parallel` to 1, so a state holding two or more tasks — counted by each
/// task's current `**State:**`, in the machine governing it — runs them one by
/// one although its author asked for them to overlap. The file is named
/// relative to the plan root. §FS-rhei-validate.4.5
fn serialized_concurrent_state_warnings(
    input: &Path,
    loaded: &LoadedPlan,
    machines: &rhei_validator::MachineSet,
) -> Vec<String> {
    let TicketFileLayout::AllInOne(file) = ticket_file_layout(loaded) else {
        return Vec::new();
    };
    let mut held: IndexMap<String, usize> = IndexMap::new();
    for task in flatten_tasks(&loaded.rhei) {
        let machine = machines.for_task(&task.id);
        let state = normalized_state_name(&task.state, machine);
        if machine.states.get(&state).is_some_and(|def| def.concurrent) {
            *held.entry(state).or_default() += 1;
        }
    }
    // A single-file plan is its own shared file, so its root is its directory.
    let input = normalize_workspace_input(input);
    let root =
        if input.is_dir() { input.as_path() } else { input.parent().unwrap_or(Path::new("")) };
    let shown = file.strip_prefix(root).unwrap_or(file);
    held.into_iter()
        .filter(|(_, count)| *count >= 2)
        .map(|(state, _)| {
            format!(
                "state '{state}' is concurrent, but every top-level ticket is in {}, so rhei \
                 run drops --parallel to 1; give a top-level ticket a file of its own",
                shown.display()
            )
        })
        .collect()
}
