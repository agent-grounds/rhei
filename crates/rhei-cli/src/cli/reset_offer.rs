// The `rhei reset` a message offers as the way out of something: one text, built
// here, so a refusal, a halt row and a note cannot drift apart.
//
// Its own part because the text is about the invocation that prints it, its
// target and its machine, which no one message site knows on its own.

// §AR-source-file-size.3 §FS-rhei-reset.5.1

/// What a message says about how far the reset it offers reaches.
const RESET_OFFER_REACH: &str =
    "which returns every ticket in that rhei to its authored state and deletes their results and logs";

/// The `--reason` of the forced transition a finished ticket's refusal offers. It
/// is there so the command runs as printed, and the message says it is the
/// operator's to replace. §FS-rhei-reset.5.1
const FORCED_REOPEN_REASON: &str = "reopen this finished ticket";

/// Whether a diagnostic's text carries a command built here, which the renderer
/// keeps on the line it was printed on. §FS-rhei-reset.5.1
fn carries_offered_command(text: &str) -> bool {
    text.contains(RESET_OFFER_REACH) || text.contains(FORCED_REOPEN_REASON)
}

/// The target and machine of the invocation printing a message, which are what
/// the reset it offers has to name: reset never infers a target, and without the
/// machine it cannot move the ticket back. §FS-rhei-reset.5.1
///
/// Both are absolute because the message may be read from a report or a log, in
/// another directory, and neither is resolved through a symlink because a
/// symlinked workspace's name is its rhei id. §FS-rhei-panta.6
#[derive(Clone)]
struct ResetOffer {
    target: PathBuf,
    state_machine: Option<PathBuf>,
}

impl ResetOffer {
    fn new(target: &Path) -> Self {
        Self { target: offered_path(target), state_machine: None }
    }

    /// This offer under the `--state-machine` the invocation was given, if any.
    fn under_machine(self, state_machine: Option<&Path>) -> Self {
        Self { state_machine: state_machine.map(offered_path), ..self }
    }

    /// The target as one shell word, and the `--state-machine` that goes with it.
    fn quoted_target_and_machine(&self) -> (String, String) {
        let machine = self
            .state_machine
            .as_ref()
            .map(|path| format!(" --state-machine {}", shell_quote(&path.display().to_string())))
            .unwrap_or_default();
        (shell_quote(&self.target.display().to_string()), machine)
    }

    /// The command and its reach, for the rhei owning `task_id`, the first segment
    /// of its qualified id. §FS-rhei-reset.5.1
    fn for_ticket(&self, task_id: &str) -> String {
        let rhei = task_id.split('.').next().unwrap_or(task_id);
        let (target, machine) = self.quoted_target_and_machine();
        format!("`rhei reset {target} --rhei {}{machine}`, {RESET_OFFER_REACH}", shell_quote(rhei))
    }

    /// The forced transition that reopens one finished ticket and touches no other.
    /// §FS-rhei-reset.5.1 §FS-rhei-transition-cmd.6
    fn forced_reopen(&self, task_id: &str, from: &str, to: &str) -> String {
        let (target, machine) = self.quoted_target_and_machine();
        let [task_id, from, to, reason] =
            [task_id, from, to, FORCED_REOPEN_REASON].map(shell_quote);
        format!(
            "`rhei transition {target} --task {task_id} --from {from} --to {to} --force \
             --reason {reason}{machine}`"
        )
    }

    /// The help of a refusal over a finished ticket, standing in `from`. It offers
    /// the rhei-wide reset only where that reset reopens this ticket: one that leaves
    /// it finished still deletes the rhei's results and logs, so it is no way back.
    /// A pre-authored chain's children never were in the initial state, so the forced
    /// line goes there only when the reset is not offered beside it.
    /// §FS-rhei-reset.5.1 §FS-rhei-complete.4
    fn reopen_finished(
        &self,
        loaded: &LoadedPlan,
        input: &Path,
        task_id: &str,
        task: &rhei_core::ast::Task,
        machine: &rhei_validator::StateMachine,
        from: &str,
    ) -> String {
        const FINISHED: &str = "nothing to do — the task is finished.";
        let reopens = |state: &str| machine.states.get(state).is_some_and(|def| !def.terminal);
        let forced = |to: &str| {
            format!(
                "An operator at a terminal can reopen this ticket alone with {}, giving their \
                 own reason; an agent may not run that.",
                self.forced_reopen(task_id, from, to)
            )
        };

        let root = loaded.task_route(task_id, input).execution_root;
        let dest = reset_destination(ledger_first_departures(&root).get(task_id), from, machine);
        if reopens(&dest) {
            return format!(
                "{FINISHED} {} Otherwise, run {}",
                forced(&dest),
                self.for_ticket(task_id)
            );
        }

        let no_reset = format!("A reset would not reopen it: it would leave this ticket in '{dest}'");
        let initial = initial_state_for_node(machine, &task.kind, task.profile_level()).ok();
        match initial.filter(|state| reopens(state)) {
            Some(initial) => format!("{FINISHED} {} {no_reset}", forced(&initial)),
            None => format!("{FINISHED} {no_reset}"),
        }
    }
}

/// `path` as the offer prints it: absolute, and not canonical. §FS-rhei-reset.5.1
fn offered_path(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}
