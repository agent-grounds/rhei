// The `rhei reset` a message offers as the way out of something: one text, built
// here, so a refusal, a halt row and a note cannot drift apart.
//
// Its own part because the text is about the invocation that prints it, its
// target and its machine, which no one message site knows on its own.

// §AR-source-file-size.3 §FS-rhei-reset.5.1

/// What a message says about how far the reset it offers reaches. The renderer
/// reads it too, to keep the command on the line it was printed on.
const RESET_OFFER_REACH: &str =
    "which returns every ticket in that rhei to its authored state and deletes their results and logs";

/// The `--reason` of the forced transition a finished ticket's refusal offers.
const FORCED_REOPEN_REASON: &str = "reopen this finished ticket";

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

    /// The help of a refusal over a finished ticket: the operator's one-ticket way
    /// back when there is a state to go back to, then the rhei-wide reset.
    /// §FS-rhei-reset.5.1 §FS-rhei-complete.4
    fn reopen_finished(&self, task_id: &str, from: &str, to: Option<&str>) -> String {
        let reset = self.for_ticket(task_id);
        match to {
            Some(to) => format!(
                "nothing to do — the task is finished. An operator at a terminal can reopen this \
                 ticket alone with {}; an agent may not run that. Otherwise, run {reset}",
                self.forced_reopen(task_id, from, to)
            ),
            None => format!("nothing to do — the task is finished. To reopen it, run {reset}"),
        }
    }
}

/// `path` as the offer prints it: absolute, and not canonical. §FS-rhei-reset.5.1
fn offered_path(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

/// The state a finished ticket reopens into, so the forced transition and the reset
/// offered after it agree: what the ledger says it started in, else its profile's
/// initial state. A pre-authored chain's children never were in the initial state,
/// so that alone would send them somewhere a reset would not. §FS-rhei-reset.5.1
fn reopen_state(
    loaded: &LoadedPlan,
    input: &Path,
    task_id: &str,
    task: &rhei_core::ast::Task,
    machine: &rhei_validator::StateMachine,
    from: &str,
) -> Option<String> {
    let usable = |state: &String| {
        state != from && machine.states.get(state).is_some_and(|def| !def.terminal)
    };
    let route = loaded.task_route(task_id, input);
    ledger_first_departures(&route.execution_root).remove(task_id).filter(usable).or_else(|| {
        initial_state_for_node(machine, &task.kind, task.profile_level()).ok().filter(usable)
    })
}
