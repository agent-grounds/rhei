// A ticket picks its rhei among loose plans: where the omitted-target walk meets
// several loose rheis and no `index.panta.md`, the ticket the command was given
// settles it by its leading segment, or the walk refuses and says why.
//
// Its own part because it is one decision with its own refusals, and the walk
// it extends already sits in a file past the size cap.

// §FS-rhei-panta.6.7 §AR-source-file-size.3

/// The ticket an invocation offers the omitted-target walk, and how the
/// command was given it, so a refusal offers the forms that command takes.
/// §FS-rhei-panta.6.7
#[derive(Clone, Debug)]
struct TicketPick {
    by: TicketPickBy,
    ticket: String,
}

/// Which of the inputs that consult the ticket this is. Nothing else picks: id
/// filters, `--rhei` and ticketless commands never build one. §FS-rhei-panta.6.7
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TicketPickBy {
    /// A single-ticket command's positional or `--task`. §FS-rhei-usage.2.2.1
    Ticket(&'static str),
    /// `rhei next --task`. §FS-rhei-next.2.1
    NextTask,
    /// `rhei note`'s writing task, `--task` else `RHEI_TASK_ID`. §FS-rhei-note.1
    NoteTask,
    /// `rhei new --under`. §FS-rhei-new.1.3
    Under,
}

impl TicketPick {
    fn ticket(command: &'static str, ticket: &str) -> Self {
        Self { by: TicketPickBy::Ticket(command), ticket: ticket.to_string() }
    }

    fn of(by: TicketPickBy, ticket: &str) -> Self {
        Self { by, ticket: ticket.to_string() }
    }

    /// The rhei id the ticket names, when its shape may name one: a dotted id
    /// whose leading segment is a name, as a `**Prior:**` is qualified, or
    /// `--under`'s single rhei id. Only compared, never prepended.
    // §FS-rhei-panta.6.7 §AR-rhei-panta.3
    fn rhei_prefix(&self) -> Option<&str> {
        let id = parse_task_id(&self.ticket);
        let eligible = workspace::is_cross_rhei_reference(&id)
            || (self.by == TicketPickBy::Under && id.as_named().is_some());
        eligible.then(|| self.ticket.split('.').next().unwrap_or(&self.ticket))
    }

    /// The ticket with its rhei segment stripped when it is eligible, else whole:
    /// what the plan form names once the plan itself is given.
    fn local(&self) -> &str {
        match self.rhei_prefix() {
            Some(_) => self.ticket.split_once('.').map_or("", |(_, rest)| rest),
            None => &self.ticket,
        }
    }

    /// The ticket as it would be named by the rhei `id`.
    fn qualified_by(&self, id: &str) -> String {
        match self.local() {
            "" => id.to_string(),
            local => format!("{id}.{local}"),
        }
    }

    /// The command that names the ticket by the rhei `id`. §FS-rhei-panta.6.7
    fn qualified_form(&self, id: &str) -> String {
        let ticket = self.qualified_by(id);
        match self.by {
            TicketPickBy::Ticket(command) => format!("rhei {command} {ticket}"),
            TicketPickBy::NextTask => format!("rhei next --task {ticket}"),
            TicketPickBy::NoteTask => format!("rhei note --task {ticket} <fact>"),
            TicketPickBy::Under => format!("rhei new <title> --under {ticket}"),
        }
    }

    /// The command that names the plan instead, where the command takes one:
    /// `rhei note` resolves from the directory alone. §FS-rhei-panta.6.7
    fn plan_form(&self, plan: &str, id: &str) -> Option<String> {
        let local = self.local();
        match self.by {
            TicketPickBy::Ticket(command) => Some(format!("rhei {command} {plan} --task {local}")),
            TicketPickBy::NextTask => Some(format!("rhei next {plan} --task {local}")),
            TicketPickBy::NoteTask => None,
            TicketPickBy::Under => {
                let under = if local.is_empty() { id } else { local };
                Some(format!("rhei new <title> --project {plan} --under {under}"))
            }
        }
    }
}

/// The id loading would give a loose entry, or `None` when it has none — an
/// invalid stem, or the reserved basin — which makes it a non-match rather
/// than an error that aborts the pick. §FS-rhei-panta.6.7
fn loose_rhei_id(entry: &Path) -> Option<String> {
    workspace::rhei_id_for_path(entry).ok()
}

/// The loose entries whose id is `prefix`. §FS-rhei-panta.6.7
fn loose_rheis_with_id<'a>(entries: &'a [PathBuf], prefix: &str) -> Vec<&'a PathBuf> {
    entries.iter().filter(|entry| loose_rhei_id(entry).as_deref() == Some(prefix)).collect()
}

fn entry_name(entry: &Path) -> String {
    entry.file_name().map_or_else(
        || entry.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// Settle several loose rheis in `dir`: the one the ticket's prefix names, or a
/// refusal that says why none was picked. Only the invocation directory is
/// asked, so an unmatched prefix never climbs. §FS-rhei-panta.6.7
fn pick_loose_rhei(
    dir: &Path,
    entries: Vec<PathBuf>,
    ticket: Option<&TicketPick>,
) -> MietteResult<PathBuf> {
    let Some((pick, prefix)) = ticket.and_then(|pick| Some((pick, pick.rhei_prefix()?))) else {
        return Err(several_loose_rheis_refusal(dir, &entries, ticket));
    };
    let matching = loose_rheis_with_id(&entries, prefix);
    match matching.as_slice() {
        [one] => Ok((*one).clone()),
        [] => Err(unmatched_prefix_refusal(dir, &entries, pick, prefix)),
        shared => Err(shared_prefix_refusal(dir, shared, pick, prefix)),
    }
}

/// The first entry with a usable id, for the example forms a refusal offers.
fn example_entry(entries: &[PathBuf]) -> Option<(String, String)> {
    entries.iter().find_map(|entry| Some((entry_name(entry), loose_rhei_id(entry)?)))
}

/// The ambiguity refusal of §FS-rhei-panta.6, word for word; a ticket command's
/// help adds that the ticket may be named by its rhei. §FS-rhei-panta.6.7
fn several_loose_rheis_refusal(
    dir: &Path,
    entries: &[PathBuf],
    ticket: Option<&TicketPick>,
) -> miette::Report {
    let names: Vec<String> = entries.iter().map(|entry| entry_name(entry)).collect();
    let help = match ticket {
        None => "run `rhei init --here` to make the directory a project, or pass one of the \
                 rheis above."
            .to_string(),
        Some(pick) => {
            let id = example_entry(entries).map_or_else(|| "<rhei>".to_string(), |(_, id)| id);
            format!(
                "run `rhei init --here` to make the directory a project, pass one of the rheis \
                 above, or name the ticket by its rhei: `{}`.",
                pick.qualified_form(&id)
            )
        }
    };
    miette!(
        help = help,
        "{} holds {} rheis ({}) but no `index.panta.md`, so there is no single plan to pick. \
         Pass one explicitly, or run `rhei init` to make the directory a project (writes \
         index.panta.md)",
        dir.display(),
        names.len(),
        names.join(", ")
    )
}

/// No loose rhei has the prefix as its id. Says only that: `review.1` may be a
/// subtask of a ticket `review`, so the prefix is never called a rhei.
/// §FS-rhei-panta.6.7
fn unmatched_prefix_refusal(
    dir: &Path,
    entries: &[PathBuf],
    pick: &TicketPick,
    prefix: &str,
) -> miette::Report {
    let listed = entries
        .iter()
        .map(|entry| match loose_rhei_id(entry) {
            Some(id) => format!("  {} (id `{id}`)", entry_name(entry)),
            None => format!("  {} (no usable rhei id)", entry_name(entry)),
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut forms = Vec::new();
    if let Some((plan, id)) = example_entry(entries) {
        forms.push(format!("name the ticket by one of those rheis (`{}`)", pick.qualified_form(&id)));
        if let Some(plan_form) = pick.plan_form(&plan, &id) {
            forms.push(format!("pass the plan (`{plan_form}`)"));
        }
    }
    forms.push("run `rhei init --here` to make the directory a project".to_string());
    miette!(
        help = format!("{}.", join_alternatives(&forms)),
        "{} holds {} rheis but no `index.panta.md`, and no rhei here has the id `{prefix}` \
         that `{}` begins with:\n{listed}",
        dir.display(),
        entries.len(),
        pick.ticket
    )
}

/// Two loose entries share the prefix as their id, so it picks neither.
/// §FS-rhei-panta.6.7
fn shared_prefix_refusal(
    dir: &Path,
    shared: &[&PathBuf],
    pick: &TicketPick,
    prefix: &str,
) -> miette::Report {
    let names: Vec<String> = shared.iter().map(|entry| entry_name(entry)).collect();
    let count = match names.len() {
        2 => "two".to_string(),
        n => n.to_string(),
    };
    let help = match pick.plan_form(&names[0], prefix) {
        Some(plan_form) => format!(
            "pass the plan you mean (`{plan_form}`), or rename one of them so each rhei id is \
             unique."
        ),
        None => "rename one of them so each rhei id is unique.".to_string(),
    };
    miette!(
        help = help,
        "the id `{prefix}` names {count} rheis in {} ({}), so `{}` picks none of them",
        dir.display(),
        names.join(", "),
        pick.ticket
    )
}

/// `a`, `a or b`, `a, b, or c`.
fn join_alternatives(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [first, second] => format!("{first} or {second}"),
        [init @ .., last] => format!("{}, or {last}", init.join(", ")),
    }
}

/// The ticket an invocation offers the walk, read off the parsed command before
/// it runs, so a help printed for it discovers what the command itself would.
/// §FS-rhei-panta.6.7 §FS-rhei-errors.1.2.1
fn ticket_pick_of(command: &Commands) -> Option<TicketPick> {
    let ticket_of = |name: &'static str, input: &Option<PathBuf>, task: &Option<String>| {
        let (_, ticket) = split_ticket_target(input.clone(), task.clone()).ok()?;
        Some(TicketPick::ticket(name, &ticket?))
    };
    match command {
        Commands::Show { input, task, .. } => ticket_of("show", input, task),
        Commands::Retry { input, task, .. } => ticket_of("retry", input, task),
        Commands::Transition { input, task, .. } => ticket_of("transition", input, task),
        Commands::Complete { input, task, .. } => ticket_of("complete", input, task),
        Commands::Release { input, task, .. } => ticket_of("release", input, task),
        Commands::Remove { input, task, .. } => ticket_of("remove", input, task),
        Commands::Next { task, .. } => {
            task.as_deref().map(|task| TicketPick::of(TicketPickBy::NextTask, task))
        }
        Commands::Note { options } => note_ticket_pick(options),
        Commands::New { options } => new_ticket_pick(options),
        _ => None,
    }
}

/// `rhei note`'s writing task, read without refusing: a note with none is
/// refused by the verb itself, after its plan resolves. §FS-rhei-note.1
fn note_ticket_pick(options: &NoteOptions) -> Option<TicketPick> {
    let flag = options.task.as_deref().map(str::trim).filter(|id| !id.is_empty());
    let task = flag.map(str::to_string).or_else(|| {
        inherited_rhei_var("RHEI_TASK_ID").map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
    })?;
    Some(TicketPick::of(TicketPickBy::NoteTask, &task))
}

/// `rhei new --under`, which chooses among loose plans only with `--project`
/// omitted; a named project is resolved before the ticket is asked.
/// §FS-rhei-new.1.3
fn new_ticket_pick(options: &NewOptions) -> Option<TicketPick> {
    options.under.as_deref().map(|under| TicketPick::of(TicketPickBy::Under, under))
}
