// `rhei gate`: its subcommands and their arguments, and the one dispatch that
// resolves a ticket or a root before handing each to its command.

// §FS-rhei-gate-cmd.1

/// `rhei gate` subcommands. §FS-rhei-gate-cmd.1
#[derive(Subcommand, Debug)]
enum GateCommand {
    /// Print the brief of a ticket that waits at a gate
    // §FS-rhei-gate-cmd.2
    Show {
        /// Ticket id (`agora.1`), or the plan, workspace or project when
        /// `--task` names the ticket
        #[arg(value_name = "TICKET_OR_PLAN", add = ArgValueCompleter::new(complete_rhei_plan_path))]
        input: Option<PathBuf>,
        /// Task identifier; alternative to naming the ticket positionally
        #[arg(long, add = ArgValueCompleter::new(complete_task_id))]
        task: Option<String>,
        #[arg(long, value_name = "PATH", help = STATE_MACHINE_HELP, add = ArgValueCompleter::new(complete_yaml_path))]
        state_machine: Option<PathBuf>,
        /// Emit the brief as one JSON object (`rhei gate schema show`)
        #[arg(long)]
        json: bool,
    },
    /// Answer the brief of a ticket at a gate and move it where the answer leads
    // §FS-rhei-gate-cmd.3
    Answer {
        /// Ticket id (`agora.1`), or the plan, workspace or project when
        /// `--task` names the ticket
        #[arg(value_name = "TICKET_OR_PLAN", add = ArgValueCompleter::new(complete_rhei_plan_path))]
        input: Option<PathBuf>,
        /// Task identifier; alternative to naming the ticket positionally
        #[arg(long, add = ArgValueCompleter::new(complete_task_id))]
        task: Option<String>,
        #[arg(long, value_name = "PATH", help = STATE_MACHINE_HELP, add = ArgValueCompleter::new(complete_yaml_path))]
        state_machine: Option<PathBuf>,
        /// The gate state the ticket is expected to be in (compare-and-swap)
        #[arg(long, value_name = "STATE")]
        from: String,
        /// The answer document (`rhei gate schema answer`), or `-` for stdin
        #[arg(long, value_name = "FILE|-")]
        answers: String,
        /// Run every check and write nothing
        #[arg(long)]
        dry_run: bool,
        /// Emit the result as one JSON object (`rhei gate schema result`)
        #[arg(long)]
        json: bool,
    },
    /// List every ticket that waits at a gate
    // §FS-rhei-gate-cmd.4
    List {
        /// Path to a plan, workspace, or Panta project; omitted, discover it
        #[arg(value_name = "ROOT", add = ArgValueCompleter::new(complete_rhei_plan_path))]
        input: Option<PathBuf>,
        #[arg(long, value_name = "PATH", help = STATE_MACHINE_HELP, add = ArgValueCompleter::new(complete_yaml_path))]
        state_machine: Option<PathBuf>,
        /// Emit the list as one JSON object (`rhei gate schema list`)
        #[arg(long)]
        json: bool,
    },
    /// Print the JSON Schema of a gate document
    // §FS-rhei-gate-cmd.5
    Schema {
        /// `request`, `answer`, `show`, `result` or `list`; omitted, all five
        #[arg(value_name = "NAME")]
        name: Option<String>,
    },
}

impl GateCommand {
    /// Whether the subcommand prints JSON, so its failures do too. §FS-rhei-errors.5
    fn wants_json(&self) -> bool {
        match self {
            Self::Show { json, .. } | Self::Answer { json, .. } | Self::List { json, .. } => *json,
            Self::Schema { .. } => false,
        }
    }

    /// The plan the subcommand was pointed at, and the machine, for the
    /// commands a diagnostic suggests. §FS-rhei-errors.1.2.1
    fn help_target(&self) -> (Option<PathBuf>, Option<&PathBuf>, bool) {
        match self {
            Self::Show { input, task, state_machine, .. }
            | Self::Answer { input, task, state_machine, .. } => (
                split_ticket_target(input.clone(), task.clone()).ok().and_then(|(plan, _)| plan),
                state_machine.as_ref(),
                true,
            ),
            Self::List { input, state_machine, .. } => (input.clone(), state_machine.as_ref(), true),
            Self::Schema { .. } => (None, None, false),
        }
    }

    /// The ticket a subcommand names, which may pick its rhei among loose
    /// plans. §FS-rhei-panta.6.7
    fn ticket_pick(&self) -> Option<TicketPick> {
        let (input, task, name) = match self {
            Self::Show { input, task, .. } => (input, task, "gate show"),
            Self::Answer { input, task, .. } => (input, task, "gate answer"),
            Self::List { .. } | Self::Schema { .. } => return None,
        };
        let (_, ticket) = split_ticket_target(input.clone(), task.clone()).ok()?;
        Some(TicketPick::ticket(name, &ticket?))
    }
}

/// Split a `rhei gate` subcommand's positional, where the ticket is mandatory
/// and an id-shaped word is the ticket, never a missing path. §FS-rhei-gate-cmd.1.1
fn split_gate_ticket_target(
    input: Option<PathBuf>,
    task: Option<String>,
    verb: &str,
) -> MietteResult<(Option<PathBuf>, String)> {
    match split_ticket_target(input, task)? {
        (plan, Some(task)) => Ok((plan, task)),
        (Some(plan), None) => Err(miette!(
            help = ticket_id_required_help(),
            "'{}' is a plan path; name the ticket too: `rhei {verb} <ticket-id>` \
             (or `rhei {verb} {} --task <ticket-id>`)",
            plan.display(),
            help_target_word_or(&plan),
        )),
        (None, None) => Err(miette!(
            help = ticket_id_required_help(),
            "name the ticket: `rhei {verb} <ticket-id>` (or `--task <ticket-id>`)"
        )),
    }
}

/// Resolve what a `rhei gate` subcommand names and run it. §FS-rhei-gate-cmd.1.1
fn gate_command(command: GateCommand, before_subcommand: Option<PathBuf>) -> MietteResult<()> {
    match command {
        GateCommand::Show { input, task, state_machine, json } => {
            let (input, task) = split_gate_ticket_target(input, task, "gate show")?;
            let target = resolve_ticket_plan_target(input, Some(&TicketPick::ticket("gate show", &task)))?;
            let machine = state_machine.or(before_subcommand);
            gate_show_command(target.path(), &target.scope_with(&[]), machine.as_deref(), &task, json)
        }
        GateCommand::Answer { input, task, state_machine, from, answers, dry_run, json } => {
            let (input, task) = split_gate_ticket_target(input, task, "gate answer")?;
            let target = resolve_ticket_plan_target(input, Some(&TicketPick::ticket("gate answer", &task)))?;
            let machine = state_machine.or(before_subcommand);
            gate_answer_command(&GateAnswerRequest {
                input: target.path(),
                rhei_scope: &target.scope_with(&[]),
                state_machine_path: machine.as_deref(),
                task: &task,
                from: &from,
                answers: &answers,
                dry_run,
                json,
            })
        }
        GateCommand::List { input, state_machine, json } => {
            let target = resolve_plan_target(input)?;
            let machine = state_machine.or(before_subcommand);
            gate_list_command(target.path(), &target.scope_with(&[]), machine.as_deref(), json)
        }
        GateCommand::Schema { name } => gate_schema_command(name.as_deref()),
    }
}
