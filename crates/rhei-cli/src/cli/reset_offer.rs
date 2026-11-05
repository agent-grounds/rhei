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

    /// The command and its reach, for the rhei owning `task_id`, the first segment
    /// of its qualified id. §FS-rhei-reset.5.1
    fn for_ticket(&self, task_id: &str) -> String {
        let rhei = task_id.split('.').next().unwrap_or(task_id);
        let target = shell_quote(&self.target.display().to_string());
        let machine = self
            .state_machine
            .as_ref()
            .map(|path| format!(" --state-machine {}", shell_quote(&path.display().to_string())))
            .unwrap_or_default();
        format!("`rhei reset {target} --rhei {}{machine}`, {RESET_OFFER_REACH}", shell_quote(rhei))
    }
}

/// `path` as the offer prints it: absolute, and not canonical. §FS-rhei-reset.5.1
fn offered_path(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}
