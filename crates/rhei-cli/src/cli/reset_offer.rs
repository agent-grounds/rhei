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
    /// the rhei-wide reset only where that reset reopens this ticket and is not refused:
    /// one that leaves it finished still deletes the rhei's results and logs, and one
    /// that is refused changes nothing, so neither is a way back.
    /// The forced line is this ticket's own only where no ancestor is finished; under
    /// a finished one the forced path would refuse it, so the help starts at the
    /// topmost finished ancestor, the only one whose line gets past that refusal. What
    /// the help ends with is always about this ticket.
    /// §FS-rhei-reset.5.1 §FS-rhei-complete.4 §FS-rhei-transition-cmd.6
    fn reopen_finished(
        &self,
        loaded: &LoadedPlan,
        input: &Path,
        task_id: &str,
        task: &rhei_core::ast::Task,
        machines: &rhei_validator::MachineSet,
        from: &str,
    ) -> String {
        const FINISHED: &str = "nothing to do — the task is finished.";

        let machine = machines.for_task(&task.id);
        let root = loaded.task_route(task_id, input).execution_root;
        let departures = ledger_first_departures(&root);
        let dest = reset_destination(departures.get(task_id), from, machine);
        let tail = if !state_reopens(machine, &dest) {
            format!("A reset would not reopen it: it would leave this ticket in '{dest}'")
        } else {
            // Asked of the reset offered, the whole rhei's, by the computation it refuses with.
            // §FS-rhei-reset.5.1 §FS-rhei-reset.2.3
            let rhei = task_id.split('.').next().unwrap_or(task_id);
            let refused = errors_the_offered_reset_introduces(loaded, input, rhei, machines);
            if refused.is_empty() {
                format!("Otherwise, run {}", self.for_ticket(task_id))
            } else {
                format!(
                    "A reset of its rhei would be refused: {RESET_LEAVES_AN_INVALID_PLAN}, \
                     because {}",
                    refused.join("; ")
                )
            }
        };

        let Some(to) = reopen_state(machine, task, &dest) else {
            return format!("{FINISHED} {tail}");
        };
        let terminal = terminal_ancestors(&loaded.rhei.tasks, &task.id, machine);
        let Some(ancestor) = terminal.last() else {
            return format!(
                "{FINISHED} An operator at a terminal can reopen this ticket alone with {}, \
                 giving their own reason; an agent may not run that. {tail}",
                self.forced_reopen(task_id, from, &to)
            );
        };

        let ancestor_id = ancestor.id.to_string();
        let ancestor_from = normalized_state_name(&ancestor.state, machine);
        let ancestor_dest = reset_destination(departures.get(&ancestor_id), &ancestor_from, machine);
        match reopen_state(machine, ancestor, &ancestor_dest) {
            Some(ancestor_to) => format!(
                "{FINISHED} Its ancestor {ancestor_id} is finished too and has to be reopened \
                 first, then each finished ticket below it down to this one: an operator at a \
                 terminal starts with {}, giving their own reason; an agent may not run that. \
                 {tail}",
                self.forced_reopen(&ancestor_id, &ancestor_from, &ancestor_to)
            ),
            None => format!(
                "{FINISHED} Its ancestor {ancestor_id} is finished too and has to be reopened \
                 first, and neither a reset nor its profile's initial state would do that. {tail}"
            ),
        }
    }
}

/// Whether `state` is one the machine declares as not terminal. §FS-rhei-reset.5.1
fn state_reopens(machine: &rhei_validator::StateMachine, state: &str) -> bool {
    machine.states.get(state).is_some_and(|def| !def.terminal)
}

/// The state a finished task reopens into, given `dest`, where a reset would leave it:
/// that state when it is not terminal, otherwise the `initial` of the task's own profile
/// when that is not terminal, otherwise none. One rule for the ticket and the ancestor
/// the help starts at. A pre-authored chain's children never were in the initial state,
/// so a task only goes there when the reset would not reopen it. §FS-rhei-reset.5.1
fn reopen_state(
    machine: &rhei_validator::StateMachine,
    task: &rhei_core::ast::Task,
    dest: &str,
) -> Option<String> {
    if state_reopens(machine, dest) {
        return Some(dest.to_string());
    }
    initial_state_for_node(machine, &task.kind, task.profile_level())
        .ok()
        .filter(|state| state_reopens(machine, state))
}

/// `path` as the offer prints it: absolute, and not canonical. §FS-rhei-reset.5.1
fn offered_path(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}
