// The argument group for `rhei new`: one flag per field the plan language lets
// an author write, kept beside the command that consumes them rather than in
// the shared declaration file.

// §FS-rhei-new.1

/// The two shapes of the command, shown under the flags.
///
/// Nineteen flags in one list is a wall; two worked examples say in two lines
/// which half of it any given invocation needs.
// §FS-rhei-new.1
const NEW_COMMAND_EXAMPLES: &str = "\
Examples:
  rhei new \"Authentication\"                      create a rhei
  rhei new \"Rotate keys\" --under auth --prior auth.1
                                                 create a ticket inside one";

/// Every field `rhei new` can author, in one flattened argument group.
///
/// The set is deliberately exhaustive: each metadata field the plan language
/// lets an author write on a new ticket has a flag here, so creating a ticket
/// never has to be finished by hand in an editor.
// §FS-rhei-new.1
#[derive(Args, Debug)]
struct NewOptions {
    /// Title of the rhei or ticket
    #[arg(value_name = "TITLE")]
    title: String,
    /// Project or plan to write into; omitted, the enclosing project,
    /// workspace, or lone plan is used
    #[arg(
        long,
        value_name = "RHEI_PLAN",
        add = ArgValueCompleter::new(complete_rhei_plan_path)
    )]
    project: Option<PathBuf>,
    /// Explicit id: for a rhei, the one otherwise derived from the title;
    /// for a ticket, the segment otherwise taken from the sibling numbering.
    /// A name works for a ticket too (`--id review` -> `plat.review`)
    #[arg(long, value_name = "ID")]
    id: Option<String>,
    /// Body content: the ticket's description, or the rhei's lead paragraph.
    /// Prose only — a heading, a `**Field:**` line, a bare `---`, or an
    /// unclosed ``` fence would author plan structure, so each is refused
    #[arg(long, value_name = "TEXT")]
    description: Option<String>,
    /// Read the description from a file; `-` reads standard input
    #[arg(
        long,
        value_name = "PATH",
        conflicts_with = "description",
        add = ArgValueCompleter::new(complete_any_path)
    )]
    description_file: Option<PathBuf>,
    /// Create a Directory Workspace rhei instead of a single file; an empty
    /// same-id directory or one containing an authored `states.yaml` and
    /// optional `prompt_templates/` is adopted
    #[arg(long, help_heading = "Creating a rhei")]
    dir: bool,
    /// Bind the new rhei to a state machine by name. The machine has to
    /// resolve at create time, so author it first with
    /// `/rhei-state-machine-writer`; `--keep-on-error` writes the rhei anyway
    #[arg(
        long,
        value_name = "NAME",
        help_heading = "Creating a rhei",
        add = ArgValueCompleter::new(complete_new_states_name)
    )]
    states: Option<String>,
    /// Write `structure.maxLevels` for the new rhei
    #[arg(long, value_name = "N", help_heading = "Creating a rhei")]
    max_levels: Option<u8>,
    /// Write `structure.nodeKinds` for the new rhei (repeatable;
    /// comma-separated list also accepted)
    #[arg(
        long,
        value_name = "KIND",
        value_delimiter = ',',
        help_heading = "Creating a rhei"
    )]
    node_kinds: Vec<String>,
    /// Owning rhei id (`auth`, `basin`) for a top-level ticket, or ticket id
    /// (`auth.3`) for a subtask. Omitted, a rhei is created under Panta
    #[arg(
        long,
        value_name = "PARENT",
        help_heading = "Creating a ticket",
        add = ArgValueCompleter::new(complete_new_parent)
    )]
    under: Option<String>,
    /// Heading keyword for the new ticket, checked against structure.nodeKinds
    #[arg(
        long,
        value_name = "KIND",
        help_heading = "Creating a ticket",
        add = ArgValueCompleter::new(complete_new_node_kind)
    )]
    kind: Option<String>,
    /// Starting state; defaults to the owning rhei machine's initial state
    #[arg(
        long,
        value_name = "STATE",
        help_heading = "Creating a ticket",
        add = ArgValueCompleter::new(complete_state_name)
    )]
    state: Option<String>,
    /// Prior dependency (repeatable; comma-separated list also accepted)
    #[arg(
        long,
        value_name = "ID",
        value_delimiter = ',',
        help_heading = "Creating a ticket",
        add = ArgValueCompleter::new(complete_task_id)
    )]
    prior: Vec<String>,
    /// Export this ticket publishes (repeatable; comma-separated also accepted)
    #[arg(
        long,
        value_name = "NAME",
        value_delimiter = ',',
        help_heading = "Creating a ticket"
    )]
    provides: Vec<String>,
    /// Export this ticket reads, as `<task-id>:<name>` (repeatable;
    /// comma-separated list also accepted). Selects export prompt injection,
    /// not filesystem visibility. Not a dependency: a consumer is ready
    /// before its producer runs, so order it with --prior
    // §FS-rhei-new.1.3 §FS-rhei-plan-language.3.12
    #[arg(
        long,
        value_name = "ID:NAME",
        value_delimiter = ',',
        help_heading = "Creating a ticket"
    )]
    consumes: Vec<String>,
    /// Claim the new ticket for someone. An assignee means "in progress":
    /// `rhei next` and `rhei run` skip it until `rhei release <id>`
    #[arg(long, value_name = "WHO", help_heading = "Creating a ticket")]
    assignee: Option<String>,
    /// Per-ticket model override; mutually exclusive with --target, which
    /// names a whole execution identity
    #[arg(long, value_name = "MODEL", help_heading = "Creating a ticket")]
    model: Option<String>,
    /// Per-ticket execution-identity override; mutually exclusive with
    /// --model, which the identity already carries
    #[arg(
        long,
        value_name = "TARGET",
        conflicts_with = "model",
        help_heading = "Creating a ticket"
    )]
    target: Option<String>,
    /// Preview the create: it is written, validated, and then always rolled
    /// back, so the preview reports what the real create would do
    #[arg(long)]
    dry_run: bool,
    /// Emit the created id, kind, path, and state as JSON
    #[arg(long)]
    json: bool,
    /// Keep the write when validation fails, instead of rolling it back; no
    /// effect under --dry-run, which always rolls back
    #[arg(long)]
    keep_on_error: bool,
}
