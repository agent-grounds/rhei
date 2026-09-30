// `rhei note`'s flags, beside the command they belong to rather than in the
// declaration file every other verb shares.
//
// Its own part for the reason `new_options.rs` is: the declaration file is at
// its size bound, and a verb's flag set is the one thing about it that reads
// better next to what consumes it.

// §AR-source-file-size.3 §FS-rhei-note.1

/// The three exclusive forms of one slot, plus who is spending it.
///
/// The group is `required` and `multiple(false)`, so "exactly one of the text,
/// `--restate` and `--strike`" is clap's own usage error with clap's own exit
/// code rather than a check inside the command. §FS-rhei-note.1 §FS-rhei-note.5
#[derive(Args, Debug)]
#[command(group(
    clap::ArgGroup::new("note_form").required(true).multiple(false)
        .args(["text", "restate", "strike"])
))]
struct NoteOptions {
    /// The fact to leave for later tickets, at most 3 lines
    #[arg(value_name = "TEXT")]
    text: Option<String>,
    /// Make that task's live entry the newest, instead of leaving one of your own
    #[arg(long, value_name = "TASK_ID", add = ArgValueCompleter::new(complete_task_id))]
    restate: Option<String>,
    /// Take that task's live entry out of composition, instead of leaving one
    #[arg(long, value_name = "TASK_ID", add = ArgValueCompleter::new(complete_task_id))]
    strike: Option<String>,
    /// Name the writing task; defaults to `RHEI_TASK_ID`, which `rhei run` exports
    #[arg(long, value_name = "ID", add = ArgValueCompleter::new(complete_task_id))]
    task: Option<String>,
}
