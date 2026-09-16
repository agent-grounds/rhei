// The `help =` lines Rhei's errors carry: one function per situation, each
// naming the next command to run rather than describing the failure again.
//
// Its own part because it is a catalogue — no logic, no callers of each other —
// while the guidance next door builds reports and computes suggestions.

// §AR-source-file-size.3 §FS-rhei-errors.2

/// Help for a state machine that declares something Rhei cannot execute.
fn state_machine_help() -> &'static str {
    "fix the state definition in the active states.yaml. Inspect the machine \
     rhei resolved with: rhei states"
}

/// Help for a settings file that is missing or malformed.
///
/// No plan root is in scope here, so the project file cannot be resolved and
/// both of its names are given rather than one: a project on the deprecated
/// home sent to the new path would shadow the file it reads.
/// §FS-rhei-agents.1.1
fn settings_help() -> &'static str {
    "settings merge from ~/.config/rhei/settings.json then the project's \
     .agent-grounds/rhei/settings.json, or the deprecated .agents/rhei/settings.json \
     where that is the file the project has. Check them with: rhei validate <plan>"
}

/// Help for a plan whose markdown does not carry what a command needs.
fn plan_authoring_help() -> &'static str {
    "check the plan's task metadata (**State:**, **Prior:**, **Assignee:**), then re-run: \
     rhei validate <plan>"
}

/// Help for a rhei whose id cannot be derived from the path naming it.
///
/// The plan is not the problem here — it parsed — so the reader must be sent to
/// the path they typed rather than to their task metadata. It serves three
/// failures — a path that names nothing, a name that is not a valid id, and the
/// reserved `basin` — so it states the rule and offers both remedies: pointing
/// a directory named `not.an.id` at its absolute path names a fix that cannot
/// work, and the message above it already prescribes the rename. §FS-rhei-panta.6
fn rhei_identity_help() -> &'static str {
    "a rhei's id comes from the path naming it: the directory name of a Directory \
     Workspace, or the `<id>` of `<id>.rhei.md`. That name must be a valid \
     single-segment id and not the reserved `basin`. Rename it, or name the rhei \
     by a path that carries such a name."
}

/// Help for the snapshot store.
fn snapshot_help() -> &'static str {
    "inspect the snapshot store with: rhei snapshot list"
}

/// Help for a template bundle whose `template.yaml` is wrong.
///
/// The reader here is the template author, not the person instantiating, so the
/// remedy is the manifest plus the validation command that re-checks it.
fn template_manifest_help() -> &'static str {
    "fix template.yaml in the template directory, then re-check the bundle with: \
     rhei instantiate <template> --dry-run"
}

/// Help for a stale or malformed per-task git worktree reference.
fn worktree_ref_help() -> &'static str {
    "a task worktree reference is written by the state that created the worktree. Delete the \
     stale file under runtime/worktree-refs/ and re-run that state."
}

/// Help for a broken internal invariant: an error the user cannot cause must
/// not invent a remedy, so it asks for a report. §FS-rhei-errors.1.2
fn internal_error_help() -> &'static str {
    "this is a bug in rhei, not a problem with your input. Please report it with \
     the command you ran and this message."
}

// The helps below are the shared vocabulary for the recurring failure
// categories: functions, not inline literals, so improving one remedy improves
// every site that reaches it. §FS-rhei-errors.1.2

/// Help for the atomic temp-file dance every plan edit goes through.
fn temp_write_help() -> &'static str {
    "rhei writes plan edits through a temp file in the same directory. Check that \
     directory is writable and has free space."
}

/// Help for the process working directory disappearing under the command.
fn cwd_help() -> &'static str {
    "re-run from a directory that still exists."
}

/// Help for a task id that is not in the plan.
fn task_id_help() -> &'static str {
    "list the task ids in this plan with: rhei list <plan>"
}

/// Help for a task another actor advanced while this command was deciding.
fn task_moved_help() -> &'static str {
    "someone moved the task since you looked. Re-read its current state with: \
     rhei list <plan>"
}

/// Help for an unnamed state.
fn unknown_state_help() -> &'static str {
    "pick a state the machine declares. List them with: rhei states"
}

/// Help for an artifact path that leaves the workspace.
fn artifact_path_help() -> &'static str {
    "artifact paths are workspace-relative. Remove the leading '/' or the '..' \
     segments from this artifact's `path` in the state machine."
}

/// Help for `runtime/` as a whole not being writable.
fn runtime_dir_help() -> &'static str {
    "rhei records results and transitions under runtime/. Check that the workspace \
     directory is writable."
}

/// Help for the per-task result files.
fn runtime_results_help() -> &'static str {
    "rhei records results under runtime/results/. Check that directory is writable."
}

/// Help for the transition log.
fn transition_log_help() -> &'static str {
    "rhei appends to runtime/state-transitions.log. Check that directory is writable."
}

/// Help for the log file a program state writes to.
fn program_log_help() -> &'static str {
    "program output is logged under runtime/logs/. Check that directory is writable."
}

/// Help for a program state whose command failed.
fn program_state_failed_help() -> &'static str {
    "the program state failed. Its log is under runtime/logs/; fix the cause, then re-run."
}

/// Help for the log file an agent invocation writes to.
fn agent_log_help() -> &'static str {
    "agent output is logged under runtime/logs/. Check that directory is writable."
}

/// Help for an agent command that would not start or behaved unexpectedly.
fn agent_command_help() -> &'static str {
    "check the agent's command and flags in settings.json: rhei validate <plan>"
}

/// Help for a run that ended on a failing agent pass.
fn run_report_help() -> &'static str {
    "inspect the run with the report it printed, fix the cause, and re-run: rhei run <plan>"
}

/// Help for a run with nothing left it is allowed to pick up.
fn nothing_claimable_help() -> &'static str {
    "every remaining task is blocked, gated, or assigned. See what is left with: \
     rhei list <plan>"
}

/// Help for a transition callback declared by the state machine.
fn callback_command_help() -> &'static str {
    "the callback command is declared in the state machine. Fix the command or the \
     state it redirects to, then retry the transition."
}

/// Help for `pollAttempts`-style operands used outside a poll state.
fn poll_operand_help() -> &'static str {
    "pollAttempts and pollMaxAttempts exist only inside a state that declares \
     `poll:`. Use a different operand, or make the state a poll state."
}

/// Help for a condition that asks about a subtree the caller could not supply.
// §FS-rhei-supervision.4.1
fn open_descendants_operand_help() -> &'static str {
    "openDescendants counts the transitioning task's non-terminal descendants, \
     so it is only defined where the task tree is in hand. Inspect the plan \
     with: rhei list <plan> --non-terminal"
}

/// Help for a plan whose `**States:**` name disagrees with the states file.
fn states_declaration_help() -> &'static str {
    "the plan's `**States:**` declaration must match the name inside the states \
     file. Rename one of them, or point --state-machine at the matching file."
}

/// Help for a `**States:**` naming a machine nothing declares.
///
/// The declaration is authored before the machine exists as often as after, so
/// "write the machine" is a real answer here and not just a diagnosis, and the
/// spec already names who writes one.
// §AR-rhei-panta.4 §FS-rhei-new.6
fn missing_state_machine_help() -> &'static str {
    "rename the `**States:**` declaration to one of the names above, point \
     --state-machine at the file that declares it, or run `/rhei-state-machine-writer` to \
     author the machine that is missing."
}

/// Why a rhei that loaded holds no tickets, named in the layout it uses.
///
/// The basin has no authored index and no `tasks/` directory — its task files
/// sit directly in `basin/` — so the sentence that fits a domain rhei is wrong
/// advice for the one rhei every project has.
// §FS-rhei-plan-language.1.2 §AR-rhei-panta.1
fn empty_rhei_help(id: &str) -> String {
    if id == workspace::BASIN_RHEI_ID {
        return format!(
            "rhei '{id}' holds no tickets: the basin takes its tickets from the non-hidden \
             `*.md` files directly inside `basin/`. Capture one with \
             `rhei new \"<title>\" --under basin`"
        );
    }
    format!(
        "rhei '{id}' holds no tickets: a workspace rhei takes its tickets from non-hidden \
         `tasks/**/*.md` files, a single-file rhei from its `## Tasks` section"
    )
}

/// Help for a duration that is not `<number><unit>`.
fn duration_format_help() -> &'static str {
    "durations are a number plus a unit: 7d, 4h, 30m, 10s."
}

/// Help for the git worktree rhei needs to read.
fn git_worktree_help() -> &'static str {
    "rhei needs a readable git worktree here. Check `git status` runs in this directory."
}

/// Help for `--watch` failing to acquire an OS watch handle.
fn watch_help() -> &'static str {
    "--watch needs an OS file-watch handle. Re-run without --watch, or raise the \
     inotify limits."
}

/// Help for `rhei viz` pointed somewhere it found nothing to render.
fn viz_path_help() -> &'static str {
    "check the path and re-run: rhei viz <plan-or-directory>"
}

/// Help for an intervention with no dashboard listening.
fn dashboard_required_help() -> &'static str {
    "the dashboard must be running to receive an intervention: rhei run <plan> --dashboard"
}

/// Help for a snapshot reference that does not parse.
fn snapshot_reference_help() -> &'static str {
    "a reference is <task>:<name>[:<state>][@<visit>][:<target>][/g<N>]. Copy one \
     from: rhei snapshot list"
}

/// Help for a snapshot generation whose stored bytes do not read back.
fn snapshot_corrupt_help() -> &'static str {
    "this cached snapshot is corrupt. Delete its generation directory and re-record \
     it: rhei snapshot gc --orphaned"
}

/// Help for the redactor hook that runs over recorded snapshots.
fn snapshot_redactor_help() -> &'static str {
    "the redactor is the command in `snapshot.redact` in settings.json. Check it \
     exists, reads stdin, and writes stdout."
}

/// Help for an agent that can neither record nor resume a native session.
fn session_capture_resume_help() -> &'static str {
    "this agent profile cannot capture or resume a native session. Configure \
     `agents.<id>.session` in settings.json, or continue with an agent that supports it."
}

/// Help for an agent that cannot record a native session.
fn session_capture_help() -> &'static str {
    "this agent profile cannot capture a native session. Configure \
     `agents.<id>.session` in settings.json, or drop snapshot emission for this state."
}

/// Help for a snapshot that cannot satisfy the state's `snapshot.inherit`.
fn snapshot_inherit_help() -> &'static str {
    "the override does not satisfy the state's snapshot.inherit contract. Pick a \
     snapshot that does — list them with: rhei snapshot list — or relax \
     snapshot.inherit in the state machine."
}

/// Help for a snapshot this agent cannot resume.
fn snapshot_resume_help() -> &'static str {
    "that snapshot cannot be resumed by this agent. Pick another with: rhei snapshot \
     list, or run the state without --from-snapshot."
}

/// Help for a `--from-snapshot` value the run does not offer.
fn snapshot_candidates_help() -> &'static str {
    "the candidates above are the snapshot.inherit invocations this run offers. Pass \
     one of them, or drop --from-snapshot."
}

/// Help for a snapshot lookup that needs a fully-qualified target.
fn snapshot_key_help() -> &'static str {
    "snapshots are keyed by agent, provider, and model. Use a full \
     <agent>:<provider>:<model> selector for this state."
}

/// Help for a snapshot with no recorded target.
fn snapshot_target_help() -> &'static str {
    "a snapshot records the target it ran under. Re-create the snapshot, or pass an \
     explicit target."
}

/// Help for more than one cached generation matching an inherit rule.
fn snapshot_ambiguous_help() -> &'static str {
    "more than one cached generation matches. Narrow it with snapshot.inherit.select \
     in the state machine, or prune with: rhei snapshot gc"
}

/// Help for unpacking something embedded in the binary into scratch space.
fn embedded_extraction_help() -> &'static str {
    "built-in skills and templates are unpacked into a temp directory. Check that \
     $TMPDIR exists, is writable, and has free space."
}

/// Help for a `rhei init` conflict, which every message already names a flag for.
fn init_conflict_help() -> &'static str {
    "inspect what is already here with: rhei list, then re-run init with the flag \
     named above."
}

/// Help for a command that needs a ticket id it was not given.
fn ticket_id_required_help() -> &'static str {
    "ticket ids are the bold `Task <id>` values in the plan. List them with: rhei list <plan>"
}

/// Help for a `--rhei` scope that excludes what the command was asked to touch.
fn rhei_scope_help() -> &'static str {
    "drop --rhei to search the whole project, or name the rhei that owns the ticket. \
     List the rheis with: rhei list"
}

/// Help for a `rhei cost --task` naming a ticket outside the accounting roots
/// the reading covers. The scope may come from `--rhei` or from the member the
/// positional named, so both ways out are spelled. §FS-rhei-panta.6.5
fn cost_task_scope_help() -> &'static str {
    "point the command at the project, with no --rhei, to read every accounting root, \
     or narrow to the rhei that owns the ticket. List the rheis with: rhei list"
}

/// Help for `--local` used where no project root could be found.
fn local_install_help() -> &'static str {
    "--local writes into the current project. Run it inside a git repository or a \
     Panta project, or install for your user with --user."
}

/// Help for reading or writing session reports under runtime/reports/.
// §FS-rhei-session-reports.1
fn session_report_help() -> &'static str {
    "session reports are derived views rendered under runtime/reports/ from the \
     logs in runtime/logs/. Check both directories are readable and writable; \
     regenerating with `rhei report` is always safe."
}

/// Help for a declared metric whose value could not be resolved or recorded.
// §FS-rhei-metrics.1 §FS-rhei-metrics.3
fn metric_value_help() -> &'static str {
    "a declared metric's value comes from the measurement artifacts its \
     states.yaml declaration names (artifact/value_artifact/pointer/program). \
     Check the declaration against what the measure state actually wrote; \
     iteration records live under runtime/metrics/."
}
