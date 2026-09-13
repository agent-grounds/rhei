use anyhow::{Context, Result};
use clap::{error::ErrorKind, Args, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::engine::{
    ArgValueCompleter, CompletionCandidate, PathCompleter, ValueCompleter,
};
use clap_complete::env::{
    Bash as CompletionBash, Elvish as CompletionElvish, EnvCompleter, Fish as CompletionFish,
    Powershell as CompletionPowerShell, Zsh as CompletionZsh,
};
use clap_complete::CompleteEnv;
use fs2::FileExt;
use indexmap::IndexMap;
use miette::{miette, Report, Result as MietteResult};
use minijinja::{Environment as MiniJinjaEnvironment, UndefinedBehavior};
#[cfg(unix)]
use nix::sys::signal::{self, Signal};
#[cfg(unix)]
use nix::unistd::Pid;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use regex::Regex;
use rhei_core::ast::{Metadata, TaskId};
use rhei_core::callback::{CallbackContext, CallbackExecutor, ShellCallbackExecutor};
use rhei_core::workspace;
use rhei_validator::{
    parse_execution_target, AgentConfig, CustomAgentProfile, ExecutionTarget, McpServerProfile,
    SkillProfile, StateMcpEntry, StateMcpEntryObject, StateSkillEntry,
};
use serde::Deserialize;
use serde_yaml::{Mapping as YamlMapping, Value as YamlValue};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::ffi::OsStr;
use std::fs;
use std::io::{BufRead, BufReader, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Command-line driver for the Rhei agent runtime.
#[derive(Parser, Debug)]
#[command(
    name = "rhei",
    author,
    version,
    about = "Run governed agent workflows from Markdown plans",
    long_about = None,
    arg_required_else_help = true,
    help_template = "\
{about}

Usage: {usage}

Inspection:
  validate    Validate a markdown plan against the configured states
  render      Render a markdown plan into a selected output format
  states      Print the states and allowed transitions for the configured state machine
  list        List tasks in a plan with optional filters
  schema      Print or list published accounting JSON Schemas
  viz         Render a self-contained HTML flow visualization of a plan or workspace

Templates:
  templates   List available templates
  instantiate Instantiate a template into a concrete plan or workspace

Execution:
  transition  Atomically transition a task from one state to another (compare-and-swap)
  run         Execute a plan by advancing tasks through the state machine in dependency order
  attach      Connect a live surface to a run this shell did not start
  runs        List the runs that are live on this machine
  stop        Ask a run to stop, entering the same interruption path as Ctrl+C
  intervene   Send a message to a running agent's stdin during a live run
  cost        Inspect run token and cost accounting artifacts
  summary     Print a compact Markdown run summary for a pull request body
  snapshot    Inspect, prune, or continue from session snapshots
  next        Transition the next ready task to the next state
  complete    Complete a task: transition to terminal state, write ledger/result,\n              link it from the task, and remove the assignee
  release     Drop a ticket's assignee so abandoned work can be claimed again
  reset       Return every task to the state it was authored in; for workspaces,\n              also remove runtime output

Authoring:
  new         Create a rhei under Panta, or a ticket inside one with --under

Setup:
  init            Set up a Panta project in a gitignored `panta/` folder (or in place with --here)
  install-skills  Install rhei skills into AI coding agent configuration directories
  completions     Generate shell completion scripts

Info:
  version     Print versions for the CLI and related crates
  help        Print this message or the help of the given subcommand(s)

Options:
{options}"
)]
struct Cli {
    // The flag stays accepted before the subcommand for compatibility, but it
    // is no longer `global`: propagating it advertised `--state-machine` in the
    // help of seven commands that never read it. Consuming subcommands declare
    // their own copy below, and it wins when both are given.
    #[arg(
        long,
        value_name = "PATH",
        help = "Path to a states YAML file (uses built-in default when omitted)",
        add = ArgValueCompleter::new(complete_yaml_path)
    )]
    state_machine: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

fn cli_command() -> clap::Command {
    Cli::command()
}

/// Supported CLI subcommands.
#[derive(Subcommand, Debug)]
enum Commands {
    /// Set up a Panta project in a gitignored `panta/` folder (or in place
    /// with --here)
    Init {
        /// Host directory (default: current directory; created when missing).
        /// The project is created at <DIR>/panta/, or <DIR> itself with --here
        #[arg(value_name = "DIR")]
        dir: Option<PathBuf>,
        /// Make DIR itself the project — the mode for adopting a directory
        /// of existing, versioned plans
        #[arg(long)]
        here: bool,
        /// Project title (default: derived from the host directory name)
        #[arg(long, value_name = "TITLE")]
        title: Option<String>,
        /// Skip the AGENTS.md agent-discovery note
        #[arg(long)]
        no_agents: bool,
        /// Re-initialize an existing project: overwrite the manifest,
        /// update companion files in place
        #[arg(long)]
        force: bool,
    },
    /// Create a rhei under Panta, or a ticket inside one with --under
    ///
    /// With no --under, writes a new rhei next to index.panta.md. With
    /// --under, writes a ticket into the named rhei (`--under auth`) or under
    /// the named ticket (`--under auth.3`).
    // §FS-rhei-new.1: the flag groups below are the spec's own §1.1/§1.2/§1.3.
    #[command(after_help = NEW_COMMAND_EXAMPLES)]
    New {
        #[command(flatten)]
        options: NewOptions,
    },
    /// Validate a markdown plan against the configured states
    Validate {
        /// Path to a states YAML file (uses built-in default when omitted)
        #[arg(long, value_name = "PATH", add = ArgValueCompleter::new(complete_yaml_path))]
        state_machine: Option<PathBuf>,
        /// Re-run validation when the plan or states file changes
        #[arg(long)]
        watch: bool,
        /// Path to the markdown plan file (.rhei.md); omitted, the nearest
        /// enclosing project, workspace, or lone plan is used
        #[arg(value_name = "RHEI_PLAN", add = ArgValueCompleter::new(complete_rhei_plan_path))]
        input: Option<PathBuf>,
    },
    /// Render a markdown plan into a selected output format
    Render {
        /// Path to a states YAML file (uses built-in default when omitted)
        #[arg(long, value_name = "PATH", add = ArgValueCompleter::new(complete_yaml_path))]
        state_machine: Option<PathBuf>,
        /// Path to the markdown plan file (.rhei.md); omitted, the nearest
        /// enclosing project, workspace, or lone plan is used
        #[arg(value_name = "RHEI_PLAN", add = ArgValueCompleter::new(complete_rhei_plan_path))]
        input: Option<PathBuf>,
        /// Output format
        // Spelling the choices into the value name puts them in the
        // missing-argument error too, not only in `--help`.
        #[arg(long, value_enum, value_name = "json|github|progress")]
        format: RenderFormat,
        /// Pretty-print JSON output
        #[arg(long)]
        pretty: bool,
        /// Disable ANSI color in progress output
        #[arg(long)]
        no_color: bool,
        /// Omit metadata in GitHub markdown output
        #[arg(long)]
        no_metadata: bool,
        /// Omit subtask content in GitHub markdown output
        #[arg(long)]
        no_content: bool,
    },
    /// Print the states and allowed transitions for the configured state machine
    States {
        /// Path to a states YAML file (uses built-in default when omitted)
        #[arg(long, value_name = "PATH", add = ArgValueCompleter::new(complete_yaml_path))]
        state_machine: Option<PathBuf>,
        /// Path to the markdown plan file (.rhei.md); omitted, the nearest
        /// enclosing project, workspace, or lone plan is used
        #[arg(value_name = "RHEI_PLAN", add = ArgValueCompleter::new(complete_rhei_plan_path))]
        input: Option<PathBuf>,
        /// Narrow to the named rhei (repeatable; one id per flag). A rhei id
        /// is its file stem or directory name; default is the whole project
        #[arg(long = "rhei", value_name = "RHEI_ID", add = ArgValueCompleter::new(complete_rhei_id))]
        rhei: Vec<String>,
        /// Emit the state machine as JSON instead of plain text
        #[arg(long)]
        json: bool,
    },
    /// List tasks in a plan with optional filters
    List {
        /// Path to a states YAML file (uses built-in default when omitted)
        #[arg(long, value_name = "PATH", add = ArgValueCompleter::new(complete_yaml_path))]
        state_machine: Option<PathBuf>,
        /// Path to the markdown plan file (.rhei.md); omitted, the nearest
        /// enclosing project, workspace, or lone plan is used
        #[arg(value_name = "RHEI_PLAN", add = ArgValueCompleter::new(complete_rhei_plan_path))]
        input: Option<PathBuf>,
        /// Narrow to the named rhei (repeatable; one id per flag). A rhei id
        /// is its file stem or directory name; default is the whole project
        #[arg(long = "rhei", value_name = "RHEI_ID", add = ArgValueCompleter::new(complete_rhei_id))]
        rhei: Vec<String>,
        /// Filter by state (repeatable; comma-separated list also accepted)
        #[arg(
            long,
            value_name = "STATE",
            value_delimiter = ',',
            add = ArgValueCompleter::new(complete_comma_state_name)
        )]
        state: Vec<String>,
        /// Filter by assignee value (exact match)
        #[arg(
            long,
            value_name = "ASSIGNEE",
            conflicts_with = "no_assignee",
            add = ArgValueCompleter::new(complete_assignee)
        )]
        assignee: Option<String>,
        /// Only tasks with no assignee
        #[arg(long, conflicts_with = "assignee")]
        no_assignee: bool,
        /// Filter by node kind (e.g. task, bug, spec)
        #[arg(long, value_name = "KIND", add = ArgValueCompleter::new(complete_node_kind))]
        kind: Option<String>,
        /// Only tasks that list <TASK_ID> in their **Prior:** dependencies
        #[arg(long, value_name = "TASK_ID", add = ArgValueCompleter::new(complete_task_id))]
        has_prior: Option<String>,
        /// Only direct children of <TASK_ID>
        #[arg(
            long,
            value_name = "TASK_ID",
            conflicts_with = "root",
            add = ArgValueCompleter::new(complete_task_id)
        )]
        parent: Option<String>,
        /// Only top-level tasks (no parent)
        #[arg(long, conflicts_with = "parent")]
        root: bool,
        /// Substring match against task title and content (case-insensitive)
        #[arg(long, value_name = "TEXT")]
        contains: Option<String>,
        /// Only tasks whose state is terminal in the resolved state machine
        #[arg(long, conflicts_with = "non_terminal")]
        terminal: bool,
        /// Only tasks whose state is non-terminal
        #[arg(long, conflicts_with = "terminal")]
        non_terminal: bool,
        /// Only tasks in the ready set — required inputs and poll deadlines
        /// included; `rhei next` narrows it further by assignee and initial state
        #[arg(long, conflicts_with = "blocked")]
        ready: bool,
        /// Only non-terminal tasks that are not ready (the complement of --ready)
        #[arg(long, conflicts_with = "ready")]
        blocked: bool,
        /// Maximum number of tasks to print (0 means no limit)
        #[arg(long, default_value_t = 0, add = ArgValueCompleter::new(complete_limit))]
        limit: usize,
        /// Emit output as JSON for machine consumption
        #[arg(long)]
        json: bool,
    },
    /// Atomically transition a task from one state to another (compare-and-swap)
    Transition {
        /// Path to a states YAML file (uses built-in default when omitted)
        #[arg(long, value_name = "PATH", add = ArgValueCompleter::new(complete_yaml_path))]
        state_machine: Option<PathBuf>,
        /// Ticket id to transition (`auth.1`, `3`), or path to the markdown
        /// plan file (.rhei.md). An id-shaped argument that names no existing
        /// path is the ticket, with the plan inferred from the working
        /// directory; omitted, the nearest enclosing project, workspace, or
        /// lone plan is used
        #[arg(value_name = "TICKET_OR_PLAN", add = ArgValueCompleter::new(complete_rhei_plan_path))]
        input: Option<PathBuf>,
        /// Task identifier (number or name); alternative to naming the ticket
        /// positionally
        #[arg(long, add = ArgValueCompleter::new(complete_task_id))]
        task: Option<String>,
        /// Expected current state of the task
        #[arg(long, add = ArgValueCompleter::new(complete_transition_from_state))]
        from: String,
        /// Target state to transition to
        #[arg(long, add = ArgValueCompleter::new(complete_transition_to_state))]
        to: String,
        /// Result message appended to `runtime/results/<task-id>.md`. Required
        /// when `--to` is a final state and the ticket has no result yet
        #[arg(long)]
        result: Option<String>,
        /// The supervisor issuing this move. Must name the moving task's
        /// nearest in-scope supervising ancestor, and suppresses the checkpoint
        /// the move would otherwise deliver to it. Grants no authority: a held
        /// descendant is directly movable with or without this flag
        // Kept to one paragraph: clap renders only the first as the option's
        // short help, and the flag is documented, never hidden.
        // §FS-rhei-transition-cmd.2
        #[arg(long, value_name = "TASK_ID")]
        supervisor: Option<String>,
        /// Skip execution of on_leave/on_enter callbacks
        #[arg(long)]
        no_callbacks: bool,
    },
    /// Execute a plan by advancing tasks through the state machine in dependency order
    Run {
        /// Path to a states YAML file (uses built-in default when omitted)
        #[arg(long, value_name = "PATH", add = ArgValueCompleter::new(complete_yaml_path))]
        state_machine: Option<PathBuf>,
        /// Path to the markdown plan file (.rhei.md); omitted, the nearest
        /// enclosing project, workspace, or lone plan is used
        #[arg(value_name = "RHEI_PLAN", add = ArgValueCompleter::new(complete_rhei_plan_path))]
        input: Option<PathBuf>,
        #[command(flatten)]
        standalone: StandaloneExecutionFlags,
        #[command(flatten)]
        agent: AgentExecutionFlags,
        #[command(flatten)]
        program: ProgramExecutionFlags,
        #[command(flatten)]
        snapshot: SnapshotExecutionFlags,
    },
    /// Connect a live surface to a run this shell did not start
    ///
    /// Follows the run's event log and per-task agent logs, and reaches its
    /// agents through the same control endpoints the browser dashboard uses.
    /// Ctrl+C and `q` detach; the run is stopped with `rhei stop`.
    Attach {
        /// Run id, id prefix, or plan/workspace path; omitted, the run of the
        /// enclosing project or workspace
        #[arg(value_name = "RUN", add = ArgValueCompleter::new(complete_run_reference))]
        run: Option<String>,
        /// Stream the run's JSON event records to stdout instead of opening a surface
        #[arg(long)]
        json: bool,
        /// With --json, resume after this sequence number instead of replaying the run
        #[arg(long, value_name = "SEQ", default_value_t = 0)]
        since: u64,
        /// Wait for the run to end and exit with the run's own exit code
        #[arg(long)]
        wait: bool,
    },
    /// List the runs that are live on this machine
    Runs {
        /// Emit output as JSON for machine consumption
        #[arg(long)]
        json: bool,
        /// Also list the ended runs the registry still holds
        #[arg(long)]
        all: bool,
        /// Only runs that started at or after this time; implies --all
        #[arg(long, value_name = "TIME")]
        since: Option<String>,
        /// Only runs that started before this time; implies --all
        #[arg(long, value_name = "TIME")]
        until: Option<String>,
    },
    /// Ask a run to stop, entering the same interruption path as Ctrl+C
    Stop {
        /// Run id, id prefix, or plan/workspace path; omitted, the run of the
        /// enclosing project or workspace
        #[arg(value_name = "RUN", add = ArgValueCompleter::new(complete_run_reference))]
        run: Option<String>,
        /// Ask twice: signal, wait out a short grace, then signal again so
        /// in-flight work is killed without its grace
        #[arg(long)]
        kill: bool,
        /// Wait until the run has actually gone, then report its exit status
        #[arg(long)]
        wait: bool,
    },
    /// Send a message to a running agent's stdin during a live run
    ///
    /// Headless sibling of the dashboard's intervene composer: it discovers the
    /// live run's loopback URL from runtime/dashboard.json and delivers the
    /// message to the agent's stdin. Only reaches agents whose profile keeps
    /// stdin open (intervene_stdin); it never transitions or edits the plan.
    Intervene {
        /// Plan file or workspace root of the live run (locates runtime/dashboard.json)
        #[arg(long, value_name = "RHEI_PLAN", default_value = ".", add = ArgValueCompleter::new(complete_rhei_plan_path))]
        plan: PathBuf,
        /// Id of the running task whose agent should receive the message
        #[arg(long, value_name = "ID", add = ArgValueCompleter::new(complete_task_id))]
        task: String,
        /// Worker slot to disambiguate when a task fans out to several agents
        #[arg(long, value_name = "N")]
        slot: Option<u16>,
        /// The message delivered to the agent's stdin
        #[arg(long, short, value_name = "TEXT")]
        message: String,
    },
    /// Inspect run token and cost accounting artifacts
    Cost {
        /// Path to the markdown plan file (.rhei.md) or workspace directory
        #[arg(value_name = "RHEI_PLAN_OR_WORKSPACE", add = ArgValueCompleter::new(complete_rhei_plan_path))]
        input: Option<PathBuf>,
        /// Narrow to the named rhei (repeatable; one id per flag). A rhei id
        /// is its file stem or directory name; default is the whole project
        #[arg(long = "rhei", value_name = "RHEI_ID", add = ArgValueCompleter::new(complete_rhei_id))]
        rhei: Vec<String>,
        /// Show direct and subtree accounting for one task id
        #[arg(long, value_name = "ID", add = ArgValueCompleter::new(complete_task_id))]
        task: Option<String>,
        /// Emit output as JSON for machine consumption
        #[arg(long)]
        json: bool,
        /// Group totals in text/JSON output
        #[arg(long, value_enum, default_value = "node")]
        by: CostGroup,
        /// Total only one run's invocations; `unattributed` selects the records
        /// that name no run
        #[arg(long, value_name = "ID")]
        run: Option<String>,
        /// Only invocations that started at or after this time (RFC 3339
        /// instant, UTC date, or a duration before now such as 7d)
        #[arg(long, value_name = "TIME")]
        since: Option<String>,
        /// Only invocations that started before this time, in the same
        /// vocabulary as --since
        #[arg(long, value_name = "TIME")]
        until: Option<String>,
    },
    /// Print or list published accounting JSON Schemas
    Schema {
        /// Full accounting schema id to print; omitted, lists published ids
        #[arg(value_name = "SCHEMA_ID", conflicts_with = "list")]
        name: Option<String>,
        /// List every published accounting schema id
        #[arg(long)]
        list: bool,
    },
    /// Print a compact Markdown run summary for a pull request body
    // §FS-rhei-summary.1: the positional resolves exactly as `rhei cost`'s does.
    Summary {
        /// Path to the markdown plan file (.rhei.md) or workspace directory
        #[arg(value_name = "RHEI_PLAN_OR_WORKSPACE", add = ArgValueCompleter::new(complete_rhei_plan_path))]
        input: Option<PathBuf>,
        /// Narrow to the named rhei (repeatable; one id per flag). A rhei id
        /// is its file stem or directory name; default is the whole project
        #[arg(long = "rhei", value_name = "RHEI_ID", add = ArgValueCompleter::new(complete_rhei_id))]
        rhei: Vec<String>,
        /// Wrap the summary in a collapsed <details> block
        #[arg(long)]
        details: bool,
    },
    /// Render a self-contained HTML flow visualization of a plan or workspace
    Viz {
        /// Path to a states YAML file (uses built-in default when omitted)
        #[arg(long, value_name = "PATH", add = ArgValueCompleter::new(complete_yaml_path))]
        state_machine: Option<PathBuf>,
        /// Path to the markdown plan file (.rhei.md) or a workspace directory;
        /// omitted, the nearest enclosing project, workspace, or lone plan is used
        #[arg(value_name = "RHEI_PLAN_OR_WORKSPACE", add = ArgValueCompleter::new(complete_rhei_plan_path))]
        input: Option<PathBuf>,
        /// Write the HTML here (default: runtime/<input>.html, or runtime/rhei-viz.html for a workspace)
        #[arg(long, short, value_name = "FILE")]
        output: Option<PathBuf>,
        /// Open the rendered file in the default browser
        #[arg(long)]
        open: bool,
    },
    /// Inspect, prune, or continue from session snapshots
    Snapshot {
        /// Path to a states YAML file (uses built-in default when omitted)
        #[arg(long, value_name = "PATH", add = ArgValueCompleter::new(complete_yaml_path))]
        state_machine: Option<PathBuf>,
        #[command(subcommand)]
        command: SnapshotCommand,
    },
    /// List available templates, or show one template in detail
    Templates {
        /// Template name (or path to a template directory) to show in detail;
        /// omitted, all discovered templates are listed
        #[arg(value_name = "TEMPLATE", add = ArgValueCompleter::new(templates::complete_template_reference))]
        template: Option<String>,
        /// Emit the template list as JSON instead of plain text
        #[arg(long)]
        json: bool,
        /// Filter by discovery source: project, user, builtin, or all
        #[arg(
            long,
            default_value = "all",
            value_name = "SOURCE",
            add = ArgValueCompleter::new(complete_template_source)
        )]
        source: String,
    },
    /// Instantiate a template into a concrete plan or workspace
    Instantiate {
        /// Template name or path to a template directory
        #[arg(
            value_name = "TEMPLATE",
            add = ArgValueCompleter::new(templates::complete_template_reference)
        )]
        template: Option<String>,
        /// Set an input value (repeatable)
        #[arg(
            long = "set",
            value_name = "KEY=VALUE",
            add = ArgValueCompleter::new(templates::complete_template_set_value)
        )]
        set_values: Vec<String>,
        /// Set an input value from file contents (repeatable)
        #[arg(
            long = "set-file",
            value_name = "KEY=PATH",
            add = ArgValueCompleter::new(templates::complete_template_set_file)
        )]
        set_files: Vec<String>,
        /// Load input values from a YAML or JSON file (repeatable)
        #[arg(long, value_name = "FILE", add = ArgValueCompleter::new(complete_values_path))]
        values: Vec<PathBuf>,
        /// Output directory (default: <project>/<template-name>/ inside a Panta
        /// project, else ./<template-name>/, where <template-name> is the
        /// directory basename of the resolved template)
        // §FS-rhei-templates.6.1 writes this help line out; the default is §FS-rhei-templates.6.2.
        #[arg(long, value_name = "PATH", add = ArgValueCompleter::new(complete_any_path))]
        output: Option<PathBuf>,
        /// Instantiate and immediately begin execution
        ///
        /// Pass `rhei run` options after `--`, for example:
        /// `rhei instantiate my-template --execute -- --parallel 4`.
        #[arg(long)]
        execute: bool,
        /// Show what would be generated without writing files
        #[arg(long)]
        dry_run: bool,
        /// Keep the output directory on validation failure
        #[arg(long)]
        keep_on_error: bool,
        /// Print the template input schema and exit
        #[arg(long)]
        list_inputs: bool,
        /// Positional input values or KEY=VALUE assignments
        #[arg(
            value_name = "INPUT",
            num_args = 0..,
            add = ArgValueCompleter::new(templates::complete_template_input_arg)
        )]
        input_args: Vec<String>,
    },
    /// Transition the next ready task to the next state
    ///
    /// Finds the first task whose prerequisites are satisfied, transitions it
    /// forward one step, and prints the task details with state-machine
    /// instructions so an agent knows exactly what to do.
    Next {
        /// Path to a states YAML file (uses built-in default when omitted)
        #[arg(long, value_name = "PATH", add = ArgValueCompleter::new(complete_yaml_path))]
        state_machine: Option<PathBuf>,
        /// Path to the markdown plan file (.rhei.md); omitted, the nearest
        /// enclosing project, workspace, or lone plan is used
        #[arg(value_name = "RHEI_PLAN", add = ArgValueCompleter::new(complete_rhei_plan_path))]
        input: Option<PathBuf>,
        /// Target a specific task instead of auto-selecting
        #[arg(long, add = ArgValueCompleter::new(complete_task_id))]
        task: Option<String>,
        /// Narrow to the named rhei (repeatable; one id per flag). A rhei id
        /// is its file stem or directory name; default is the whole project
        #[arg(long = "rhei", value_name = "RHEI_ID", add = ArgValueCompleter::new(complete_rhei_id))]
        rhei: Vec<String>,
        /// Emit output as JSON for machine consumption
        #[arg(long)]
        json: bool,
        /// Skip execution of on_leave/on_enter callbacks
        #[arg(long)]
        no_callbacks: bool,
        /// Read-only: print the next claimable task without claiming or
        /// advancing state. Does not write `**Assignee:**` or acquire a lock.
        #[arg(long)]
        peek: bool,
    },
    /// Complete a task: transition to terminal state, write the state ledger and
    /// result artifact, link it from the task, and remove the assignee.
    Complete {
        /// Path to a states YAML file (uses built-in default when omitted)
        #[arg(long, value_name = "PATH", add = ArgValueCompleter::new(complete_yaml_path))]
        state_machine: Option<PathBuf>,
        /// Ticket id to complete (`auth.1`, `3`), or path to the markdown plan
        /// file (.rhei.md). An id-shaped argument that names no existing path
        /// is the ticket, with the plan inferred from the working directory;
        /// omitted, the nearest enclosing project, workspace, or lone plan is
        /// used
        #[arg(value_name = "TICKET_OR_PLAN", add = ArgValueCompleter::new(complete_rhei_plan_path))]
        input: Option<PathBuf>,
        /// Task identifier (number or name); alternative to naming the ticket
        /// positionally
        #[arg(long, add = ArgValueCompleter::new(complete_task_id))]
        task: Option<String>,
        /// Result message written to `runtime/results/<task-id>.md`
        #[arg(long)]
        result: String,
        /// Skip execution of on_leave/on_enter callbacks
        #[arg(long)]
        no_callbacks: bool,
    },
    /// Release a claim: drop `**Assignee:**` from a ticket so it can be picked
    /// up again, leaving its state and artifacts untouched
    Release {
        /// Path to a states YAML file (uses built-in default when omitted)
        #[arg(long, value_name = "PATH", add = ArgValueCompleter::new(complete_yaml_path))]
        state_machine: Option<PathBuf>,
        /// Ticket id to release (`auth.1`, `3`), or path to the markdown plan
        /// file (.rhei.md). An id-shaped argument that names no existing path
        /// is the ticket, with the plan inferred from the working directory;
        /// omitted, the nearest enclosing project, workspace, or lone plan is
        /// used
        #[arg(value_name = "TICKET_OR_PLAN", add = ArgValueCompleter::new(complete_rhei_plan_path))]
        input: Option<PathBuf>,
        /// Ticket to release; alternative to naming it positionally. Omit with
        /// --all to release every claimed ticket
        #[arg(long, add = ArgValueCompleter::new(complete_task_id))]
        task: Option<String>,
        /// Release every claimed non-terminal ticket in scope
        #[arg(long)]
        all: bool,
        /// Narrow to the named rhei (repeatable; one id per flag). A rhei id is
        /// its file stem or directory name; default is the whole project
        #[arg(long = "rhei", value_name = "RHEI_ID", add = ArgValueCompleter::new(complete_rhei_id))]
        rhei: Vec<String>,
        /// Report what would be released without changing anything
        #[arg(long)]
        dry_run: bool,
    },
    /// Return every ticket in a rhei or project to the state it was authored
    /// in and remove runtime output
    Reset {
        /// Path to a states YAML file (uses built-in default when omitted)
        #[arg(long, value_name = "PATH", add = ArgValueCompleter::new(complete_yaml_path))]
        state_machine: Option<PathBuf>,
        /// Path to the markdown plan file (.rhei.md), workspace directory, or
        /// project. Reset destroys runtime state, so its target is never
        /// inferred
        #[arg(value_name = "RHEI_PLAN", add = ArgValueCompleter::new(complete_rhei_plan_path))]
        input: Option<PathBuf>,
        /// Narrow to the named rhei (repeatable; one id per flag). A rhei id
        /// is its file stem or directory name; default is the whole project
        #[arg(long = "rhei", value_name = "RHEI_ID", add = ArgValueCompleter::new(complete_rhei_id))]
        rhei: Vec<String>,
        /// Report what would be reset and removed without changing anything
        #[arg(long)]
        dry_run: bool,
        /// Skip the interactive confirmation
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// Print versions for the CLI and related crates
    Version,
    /// Install rhei skills into AI coding agent configuration directories
    InstallSkills {
        /// Target agent (default: all)
        #[arg(long, value_enum, default_value_t = Agent::All)]
        agent: Agent,
        /// Install into the current project directory instead of global user config
        #[arg(long)]
        local: bool,
        /// Symlink skill files instead of copying
        #[arg(long)]
        link: bool,
        /// Remove previously installed skills
        #[arg(long)]
        uninstall: bool,
        /// Print what would be done without changing anything
        #[arg(long)]
        dry_run: bool,
        /// Comma-separated list of skills to install
        #[arg(
            long,
            value_delimiter = ',',
            // Every embedded skill, kept in step with the binary's own set by
            // `default_skills_covers_every_builtin`. §FS-rhei-install-skills.2
            default_value = "rhei-plan-writer,rhei-plan-worker,rhei-state-machine-writer,rhei-template-writer",
            add = ArgValueCompleter::new(complete_skill_name)
        )]
        skills: Vec<String>,
    },
    /// Generate shell completion scripts
    Completions {
        /// Shell to generate completions for (detected from $SHELL when omitted)
        #[arg(value_enum)]
        shell: Option<CompletionShell>,
        /// Write completions to the shell's default completion location
        #[arg(long)]
        install: bool,
        /// Install into the current user's shell configuration directories
        #[arg(long, conflicts_with = "system")]
        user: bool,
        /// Install into system-wide completion directories
        #[arg(long, conflicts_with = "user")]
        system: bool,
        /// Write completions to an explicit path
        #[arg(long, value_name = "PATH", add = ArgValueCompleter::new(complete_any_path))]
        output: Option<PathBuf>,
        /// Print the destination path without writing files
        #[arg(long)]
        dry_run: bool,
    },
}
