// What target a help prints, answered in one place so that its line runs as
// printed.
//
// Its own part because the answer is about the invocation, not about any one
// message. Most helps are built far from where the target was known, and the
// `input` a command works on is already widened to the project a member loads
// through, so a help reads this rather than being handed a path.

// §AR-source-file-size.3 §FS-rhei-errors.1.2.1

/// What a help prints where there is no target at all: discovery found nothing,
/// and the refusal stands. §FS-rhei-errors.1.2.1
const NO_TARGET: &str = "<plan>";

/// The invocation a help on this thread is printed for. §FS-rhei-errors.1.2.1
struct HelpTarget {
    /// The command's own target, as typed.
    typed: Option<PathBuf>,
    /// Whether the command takes a plan target at all. One that does not, such as
    /// `rhei instantiate`, has only the paths it works on, and a help about one of
    /// those names that path rather than whatever the directory holds.
    takes_target: bool,
    /// The `--state-machine` it was given, on either side of the subcommand.
    machine: Option<PathBuf>,
    /// Whether stderr is a detached run's `runtime/run.log`, which is read later
    /// and from anywhere, so a relative path would mean nothing there.
    detached: bool,
    /// What an omitted target discovers. Looked for once, and only by a help that
    /// prints it, so a command that succeeds never walks the tree for it.
    discovered: std::sync::OnceLock<Option<PathBuf>>,
}

impl HelpTarget {
    fn new(typed: Option<PathBuf>, takes_target: bool, machine: Option<PathBuf>) -> Self {
        Self {
            typed,
            takes_target,
            machine,
            detached: is_headless_child(),
            discovered: std::sync::OnceLock::new(),
        }
    }

    /// A target the command resolved for itself, such as the rhei `--into` named.
    fn naming(target: &Path) -> Self {
        Self::new(Some(target.to_path_buf()), true, None)
    }

    /// Read before the command runs, so that what refuses first still knows the
    /// target: a broken settings file is read long before a plan resolves.
    /// `split_ticket_target` is the split the ticket commands themselves run, and
    /// its refusal is theirs to report, so here it only means nothing was typed.
    fn of_command(command: &Commands, before_subcommand: Option<&Path>) -> Self {
        let plan_of = |input: &Option<PathBuf>, task: &Option<String>| {
            split_ticket_target(input.clone(), task.clone()).ok().and_then(|(plan, _)| plan)
        };
        let (typed, machine, takes_target) = match command {
            Commands::Validate { input, state_machine, .. }
            | Commands::Run { input, state_machine, .. }
            | Commands::Render { input, state_machine, .. }
            | Commands::States { input, state_machine, .. }
            | Commands::List { input, state_machine, .. }
            | Commands::Viz { input, state_machine, .. }
            | Commands::Next { input, state_machine, .. }
            | Commands::Reset { input, state_machine, .. }
            | Commands::Seal { options: SealOptions { input, state_machine, .. } } => {
                (input.clone(), state_machine.as_ref(), true)
            }
            Commands::Retry { input, task, state_machine }
            | Commands::Transition { input, task, state_machine, .. }
            | Commands::Complete { input, task, state_machine, .. }
            | Commands::Release { input, task, state_machine, .. }
            | Commands::Remove { input, task, state_machine, .. } => {
                (plan_of(input, task), state_machine.as_ref(), true)
            }
            Commands::Show { input, task, .. } => (plan_of(input, task), None, true),
            Commands::Migrate { command: MigrateCommand::ExportPriors { input, .. } }
            | Commands::Roster { input, .. }
            | Commands::Cost { input, .. } => (input.clone(), None, true),
            Commands::Summary { options } => (options.input.clone(), None, true),
            Commands::Archive { options } => (options.input.clone(), None, true),
            // An archive is not a target another command could be pointed at.
            Commands::Verify { options } => {
                (options.input.clone().filter(|input| !is_archive_file(input)), None, true)
            }
            Commands::Note { .. } => (None, None, true),
            Commands::Report { input, .. } | Commands::Intervene { plan: input, .. } => {
                (Some(input.clone()), None, true)
            }
            Commands::Snapshot { command, state_machine } => {
                (Some(command.plan().to_path_buf()), state_machine.as_ref(), true)
            }
            Commands::Init { dir, .. } => (dir.clone(), None, true),
            Commands::New { options } => (options.project.clone(), None, true),
            Commands::Budget { command } => (command.input().cloned(), None, true),
            Commands::Schema { .. }
            | Commands::Templates { .. }
            | Commands::Instantiate { .. }
            | Commands::Attach { .. }
            | Commands::Runs { .. }
            | Commands::Stop { .. }
            | Commands::Recover { .. }
            | Commands::Version
            | Commands::InstallSkills { .. }
            | Commands::Completions { .. } => (None, None, false),
        };
        let machine = machine.cloned().or_else(|| before_subcommand.map(Path::to_path_buf));
        Self::new(typed, takes_target, machine)
    }

    /// The target as typed; otherwise what the command discovers, or for one with
    /// no target of its own, only the path the help is about: what the directory
    /// holds is another plan's answer. §FS-rhei-errors.1.2.1
    fn target(&self, fallback: Option<&Path>) -> Option<PathBuf> {
        if let Some(typed) = &self.typed {
            return Some(typed.clone());
        }
        let fallback = fallback.map(Path::to_path_buf);
        if self.takes_target {
            self.discovered().or(fallback)
        } else {
            fallback
        }
    }

    fn discovered(&self) -> Option<PathBuf> {
        self.discovered.get_or_init(discover_help_target_silently).clone()
    }

    /// `path` as one shell word. A detached run prints it absolute, and not resolved
    /// through a symlink, because a symlinked workspace's name is its rhei id.
    fn spelled(&self, path: &Path) -> String {
        if self.detached {
            shell_word(&offered_path(path))
        } else {
            shell_word(path)
        }
    }

    fn machine_argument(&self) -> Option<String> {
        let machine = self.machine.as_deref()?;
        Some(if self.detached {
            state_machine_argument(&offered_path(machine))
        } else {
            state_machine_argument(machine)
        })
    }
}

/// Discovery on a path that is already failing: it may print nothing and fail
/// nothing, because the refusal being reported is the one that matters.
///
/// `resolve_plan_path` asks the root-access layer why a directory could not be
/// listed, and that layer can warn on stderr, so such a directory stops here. A
/// failure inside it is only a `Report` nobody prints. §FS-rhei-errors.1.2.1
fn discover_help_target_silently() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    workspace::discover_rhei_entries(&cwd).ok()?;
    resolve_plan_path(None).ok()
}

thread_local! {
    static HELP_TARGET: std::cell::RefCell<Option<Arc<HelpTarget>>> =
        const { std::cell::RefCell::new(None) };
}

/// Put back what the thread held when a command's target goes out of scope, which
/// is also what keeps one test's target from another's.
struct HelpTargetScope {
    previous: Option<Arc<HelpTarget>>,
}

impl Drop for HelpTargetScope {
    fn drop(&mut self) {
        let _ = HELP_TARGET.try_with(|current| current.replace(self.previous.take()));
    }
}

/// Answer "what target?" with `target` on this thread for the scope's lifetime.
/// §FS-rhei-errors.1.2.1
#[must_use]
fn enter_help_target(target: HelpTarget) -> HelpTargetScope {
    let previous = HELP_TARGET.with(|current| current.replace(Some(Arc::new(target))));
    HelpTargetScope { previous }
}

/// What a worker thread is to [`inherit_help_target`] when it is spawned.
fn current_help_target() -> Option<Arc<HelpTarget>> {
    HELP_TARGET.try_with(|current| current.borrow().clone()).ok().flatten()
}

/// Adopt the calling command's target on a worker thread, whose thread-local
/// starts empty, so what the worker reports still names it. §FS-rhei-errors.1.2.1
fn inherit_help_target(target: Option<Arc<HelpTarget>>) {
    HELP_TARGET.with(|current| current.replace(target));
}

fn shell_word(path: &Path) -> String {
    shell_quote(&path.display().to_string())
}

/// The one spelling of a machine passed on: last on the line, after the space a
/// flag takes. §FS-rhei-errors.1.2.1 §FS-rhei-reset.5.1
fn state_machine_argument(path: &Path) -> String {
    format!(" --state-machine {}", shell_word(path))
}

/// With no command in scope there is nothing to discover from, which keeps a unit
/// test, or a thread nothing carried the target to, to what it was handed.
fn target_word(fallback: Option<&Path>) -> String {
    let holder = current_help_target();
    let spelled = match &holder {
        Some(target) => target.target(fallback).map(|path| target.spelled(&path)),
        None => fallback.map(shell_word),
    };
    spelled.unwrap_or_else(|| NO_TARGET.to_owned())
}

fn command_line(verb: &str, rest: &str, fallback: Option<&Path>) -> String {
    format!("{}{}", machineless_line(verb, rest, fallback), help_machine_argument())
}

fn machineless_line(verb: &str, rest: &str, fallback: Option<&Path>) -> String {
    let word = target_word(fallback);
    let rest = if rest.is_empty() { String::new() } else { format!(" {rest}") };
    format!("rhei {verb} {word}{rest}")
}

/// The ` --state-machine <word>` the command was given, or nothing, for a line
/// built somewhere else: a seal's remedy prints its own target. §FS-rhei-errors.1.2.1
fn help_machine_argument() -> String {
    current_help_target().and_then(|target| target.machine_argument()).unwrap_or_default()
}

/// The command's target as one shell word, for a command line that spells the rest
/// itself: one that takes no `--state-machine`, or passes it some other way.
/// §FS-rhei-errors.1.2.1
fn help_target_word() -> String {
    target_word(None)
}

/// [`help_target_word`] where the caller holds the path it would otherwise print,
/// for a command with no target of its own. §FS-rhei-errors.1.2.1
fn help_target_word_or(fallback: &Path) -> String {
    target_word(Some(fallback))
}

/// `rhei <verb>` of the command's target, under the machine it was given.
/// §FS-rhei-errors.1.2.1
fn help_command(verb: &str) -> String {
    command_line(verb, "", None)
}

/// [`help_command`] where the caller holds the path it would otherwise print.
fn help_command_or(verb: &str, fallback: &Path) -> String {
    command_line(verb, "", Some(fallback))
}

/// [`help_command`] with further arguments between the target and the machine.
fn help_command_with(verb: &str, rest: &str) -> String {
    command_line(verb, rest, None)
}

/// [`help_command_with`] where the caller holds the path it would otherwise print.
fn help_command_with_or(verb: &str, rest: &str, fallback: &Path) -> String {
    command_line(verb, rest, Some(fallback))
}

/// The command's target as one shell word where it has one, and `None` where it has
/// none, for a line that runs bare then.
fn target_word_if_any() -> Option<String> {
    current_help_target().and_then(|holder| holder.target(None).map(|path| holder.spelled(&path)))
}

/// `rhei <verb>` of the command's target where it has one, and bare where it has
/// none, for a verb that runs with no target at all. A placeholder there would ask
/// for what the line does not need. §FS-rhei-errors.1.2.1
fn help_command_or_bare(verb: &str) -> String {
    let word = target_word_if_any().map(|word| format!(" {word}")).unwrap_or_default();
    format!("rhei {verb}{word}{}", help_machine_argument())
}

/// `rhei <verb> <subcommand>` of the command's target, after `flag` where the verb
/// takes it there, for `rhei budget` and `rhei snapshot`. No `--state-machine` is
/// passed on: `rhei budget` takes none, and `rhei snapshot list` reads the same store
/// with or without one. Each discovers its own target, so where the command has none
/// the line stays bare rather than gain a placeholder. §FS-rhei-errors.1.2.1
fn help_subcommand_or_bare(verb: &str, flag: &str, rest: &str) -> String {
    let mut line = format!("rhei {verb}");
    if let Some(word) = target_word_if_any() {
        for part in [flag, word.as_str()].into_iter().filter(|part| !part.is_empty()) {
            line.push(' ');
            line.push_str(part);
        }
    }
    if !rest.is_empty() {
        line.push(' ');
        line.push_str(rest);
    }
    line
}

/// [`help_command_with_or`] for a verb that refuses `--state-machine`, as `rhei
/// show`, `rhei summary` and `rhei cost` do. §FS-rhei-errors.1.2.1
fn help_command_without_machine(verb: &str, rest: &str, fallback: Option<&Path>) -> String {
    machineless_line(verb, rest, fallback)
}

/// Whether a diagnostic's text prints a command naming a target, which a wrap would
/// cut into two pasted commands, or into a quoted path with a line break in it.
/// §FS-rhei-errors.1.2.1 §FS-rhei-errors.2
///
/// This reads what the text says rather than asking who built it, because a detached
/// run's refusal reaches its launcher as text. The first word after the verb that is
/// not a flag is what tells a target from a ticket id, and may follow a flag, as it
/// does on `rhei intervene --plan`. A verb with a subcommand, such as `rhei budget
/// init`, is read as one, and only for the subcommands it has. A false positive only
/// leaves prose unwrapped, for the terminal to soft-wrap.
fn names_help_target(text: &str) -> bool {
    let typed = current_help_target()
        .and_then(|target| target.typed.as_deref().map(|typed| target.spelled(typed)));
    text.match_indices("rhei ").any(|(at, prefix)| {
        let starts_a_word = text[..at]
            .chars()
            .next_back()
            .is_none_or(|before| before.is_whitespace() || matches!(before, '`' | '('));
        if !starts_a_word {
            return false;
        }
        let mut words = text[at + prefix.len()..].split_ascii_whitespace().peekable();
        let Some(verb) = words.next() else {
            return false;
        };
        let subcommands = SUBCOMMANDS.iter().find(|(name, _)| *name == verb).map(|(_, of)| *of);
        if words.peek().is_some_and(|word| subcommands.is_some_and(|of| of.contains(word))) {
            words.next();
        }
        let Some(word) = words.find(|word| !word.starts_with("--")) else {
            return false;
        };
        verb.chars().all(|ch| ch.is_ascii_lowercase() || ch == '-')
            && (typed.as_deref() == Some(word) || is_path_word(word, typed.as_deref()))
    })
}

/// The verbs whose next word is a subcommand rather than a target, with those
/// subcommands. §FS-rhei-errors.1.2.1
const SUBCOMMANDS: &[(&str, &[&str])] =
    &[("budget", &["init", "show", "adjust", "forget"]), ("snapshot", &["list", "show", "gc", "continue"])];

fn is_path_word(word: &str, typed: Option<&str>) -> bool {
    let word = word.trim_end_matches([',', ';', ':', ')', '`', '.']);
    typed == Some(word)
        || word.contains(['/', '\\'])
        || word.ends_with(".md")
        || word.ends_with(".yaml")
        || word.ends_with(".yml")
        || word.starts_with(['\'', '"'])
}

impl BudgetCommand {
    /// The target a budget subcommand was pointed at; `None` where it discovers one.
    fn input(&self) -> Option<&PathBuf> {
        match self {
            Self::Init { input, .. }
            | Self::Show { input, .. }
            | Self::Adjust { input, .. }
            | Self::Forget { input, .. } => input.as_ref(),
        }
    }
}

impl SnapshotCommand {
    /// The plan a snapshot command was pointed at, `.` where it was not.
    fn plan(&self) -> &Path {
        match self {
            Self::List { plan, .. }
            | Self::Show { plan, .. }
            | Self::Gc { plan, .. }
            | Self::Continue { plan, .. } => plan,
        }
    }
}
