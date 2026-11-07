// Shared helpers that give every user-facing CLI error a next action.
// §FS-rhei-errors

// They exist so the "what do I run next" half of a diagnostic is built the same
// way everywhere: quoted for paste, echoing the arguments the user already
// typed, and pointing at a command that reveals whatever they were missing.

// The renderer that keeps a printed command intact lives in `cli_dispatch.rs`
// as `install_diagnostic_handler`: breaking only at spaces leaves a long token
// whole for the terminal to soft-wrap. §FS-rhei-errors.2

/// Quote a shell word so a printed command survives paste into an interactive
/// shell — whichever shell this platform gives the user. §FS-rhei-errors.2
fn shell_quote(value: &str) -> String {
    rhei_core::platform::shell_quote(value)
}

/// Render a `KEY=VALUE` CLI argument with the value quoted when it needs it.
///
/// Quoting only the right-hand side keeps the key readable, which matters when
/// the point of the suggestion is to teach the input's name.
fn shell_assignment(key: &str, value: &str) -> String {
    format!("{key}={}", shell_quote(value))
}

/// Quote one CLI argument, keeping a leading `KEY=` readable.
///
/// `KEY=VALUE` arguments are the ones users retype by hand, so the key stays
/// bare and only the value is quoted; everything else is quoted whole.
fn shell_arg(value: &str) -> String {
    if let Some((key, rhs)) = value.split_once('=') {
        let is_identifier = !key.is_empty()
            && key.starts_with(|ch: char| ch.is_ascii_alphabetic())
            && key.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-');
        if is_identifier {
            return shell_assignment(key, rhs);
        }
    }
    shell_quote(value)
}

/// Join already-separated arguments into a runnable, paste-safe command line.
fn shell_command<I, S>(parts: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    parts.into_iter().map(|part| shell_arg(part.as_ref())).collect::<Vec<_>>().join(" ")
}

/// The closest candidate to `input`, or `None` when nothing is close enough to
/// be worth suggesting. §FS-rhei-errors.1.3
fn nearest_match<I, S>(input: &str, candidates: I) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let input = input.trim();
    if input.is_empty() {
        return None;
    }

    let lowered = input.to_ascii_lowercase();
    let closest = candidates
        .into_iter()
        .map(|candidate| {
            let candidate = candidate.as_ref().to_string();
            let distance = levenshtein_distance(&lowered, &candidate.to_ascii_lowercase());
            (candidate, distance)
        })
        .min_by(|(left_name, left), (right_name, right)| {
            left.cmp(right).then_with(|| left_name.cmp(right_name))
        })?;

    let (name, distance) = closest;
    let threshold = std::cmp::max(2, input.chars().count() / 3);
    (distance <= threshold).then_some(name)
}

/// How many candidates a fallback list prints in full. Past this the list stops
/// being something a user reads and starts being something they scroll past, so
/// the caller's "list them all with: …" command takes over. §FS-rhei-errors.1.3
const MAX_LISTED_CANDIDATES: usize = 8;

/// A `did you mean` clause for an unknown name, falling back to listing the
/// valid names when none is a near miss. §FS-rhei-errors.1.3
fn did_you_mean(input: &str, known: &[String]) -> Option<String> {
    // `known` is listed in the caller's order, so callers pass the order a user
    // would expect to read (declaration order, not hash order). Duplicates are
    // dropped here rather than at every call site: registries are assembled by
    // merging built-ins with user settings, and a name that survives in both
    // must still be offered once.
    let mut seen = HashSet::new();
    let known = known.iter().filter(|name| seen.insert(name.as_str())).collect::<Vec<_>>();
    if known.is_empty() {
        return None;
    }
    if let Some(name) = nearest_match(input, known.iter().map(|name| name.as_str())) {
        return Some(format!("Did you mean '{name}'?"));
    }
    if known.len() > MAX_LISTED_CANDIDATES {
        let head = known[..MAX_LISTED_CANDIDATES]
            .iter()
            .map(|name| name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Some(format!(
            "Valid values include: {head} (and {} more).",
            known.len() - MAX_LISTED_CANDIDATES
        ));
    }
    Some(format!(
        "Valid values: {}.",
        known.iter().map(|name| name.as_str()).collect::<Vec<_>>().join(", ")
    ))
}

fn levenshtein_distance(left: &str, right: &str) -> usize {
    if left == right {
        return 0;
    }
    if left.is_empty() {
        return right.chars().count();
    }
    if right.is_empty() {
        return left.chars().count();
    }

    let right_chars = right.chars().collect::<Vec<_>>();
    let mut previous = (0..=right_chars.len()).collect::<Vec<_>>();
    let mut current = vec![0; right_chars.len() + 1];

    for (left_index, left_char) in left.chars().enumerate() {
        current[0] = left_index + 1;
        for (right_index, right_char) in right_chars.iter().enumerate() {
            let insertion = current[right_index] + 1;
            let deletion = previous[right_index + 1] + 1;
            let substitution = previous[right_index] + usize::from(left_char != *right_char);
            current[right_index + 1] = insertion.min(deletion).min(substitution);
        }
        std::mem::swap(&mut previous, &mut current);
    }

    previous[right_chars.len()]
}

/// Help text for a filesystem failure, derived from the error kind so the user
/// is told which of the three usual causes they hit. §FS-rhei-errors.6
fn io_error_help(path: &Path, kind: std::io::ErrorKind) -> String {
    let parent = path.parent().filter(|parent| !parent.as_os_str().is_empty());
    match kind {
        // The two NotFound cases need different remedies: a typo in a name whose
        // directory exists is a `ls` away, while a missing directory has to be
        // created before anything can be written into it.
        std::io::ErrorKind::NotFound => match parent {
            Some(parent) if parent.is_dir() => format!(
                "nothing exists at that path. Check the spelling: ls {}",
                shell_quote(&parent.display().to_string())
            ),
            Some(parent) => format!(
                "nothing exists at that path, and neither does its directory. Create it with: \
                 mkdir -p {}",
                shell_quote(&parent.display().to_string())
            ),
            None => "nothing exists at that path. Check the spelling.".to_string(),
        },
        std::io::ErrorKind::PermissionDenied => format!(
            "the current user cannot access that path. Inspect it with: ls -ld {}",
            shell_quote(&path.display().to_string())
        ),
        std::io::ErrorKind::AlreadyExists => format!(
            "that path already exists. Remove it or pick another: ls -ld {}",
            shell_quote(&path.display().to_string())
        ),
        _ => match parent {
            Some(parent) => format!(
                "check that '{}' exists, is writable, and has free space.",
                parent.display()
            ),
            None => "check that the path exists and is writable.".to_string(),
        },
    }
}

/// Turn a state machine load failure into a diagnostic that says what to fix
/// and where. §FS-rhei-errors.1.2 §FS-rhei-errors.1.2.2
fn state_machine_load_report(path: &Path, err: rhei_validator::StateMachineLoadError) -> Report {
    let quoted = shell_quote(&path.display().to_string());
    match err {
        rhei_validator::StateMachineLoadError::Io(err) => {
            file_io_report(path, "failed to read state machine", err)
        }
        rhei_validator::StateMachineLoadError::Yaml(err) => {
            // serde_yaml's message leaves out a mark at the very start, which is where a
            // top-level duplicate key puts it, so there is no position above. §FS-rhei-errors.1.2.2
            let printed = err.location().is_some_and(|at| (at.line(), at.column()) != (1, 1));
            let at = if printed { " at the position above" } else { "" };
            miette!(
                help = format!(
                    "'{}' is not valid YAML. Fix the syntax{at}, then re-check it with: \
                     rhei states --state-machine {quoted}",
                    path.display()
                ),
                "failed to parse state machine '{}': {err}",
                path.display()
            )
        }
        rhei_validator::StateMachineLoadError::Shape { error, fault } => miette!(
            help = format!(
                "{}, then re-check it with: rhei states --state-machine {quoted}",
                state_machine_shape_help(path, fault)
            ),
            "failed to parse state machine '{}': {error}",
            path.display()
        ),
        rhei_validator::StateMachineLoadError::Invalid(message) => miette!(
            help = format!(
                "edit '{}' so the state definition above is valid, then re-check it with: \
                 rhei states --state-machine {quoted}",
                path.display()
            ),
            "invalid state machine '{}': {message}",
            path.display()
        ),
    }
}

/// The project root `--local` writes into, or the diagnostic for its absence.
///
/// One call, rather than ten copies of the message. §FS-rhei-errors.1.2
fn require_project_root(project_root: Option<&Path>) -> MietteResult<&Path> {
    project_root.ok_or_else(|| {
        miette!(help = local_install_help(), "--local requires a project root")
    })
}

/// Where a registry entry is declared: the key it is written under, and the
/// two files it may be written in. Every refusal of a name the settings
/// registry would have carried says this, so a listing of what exists is never
/// the whole answer. §FS-rhei-errors.1.4
///
/// It is an instruction rather than a report, so it still reads correctly when
/// the registry came entirely from the built-ins and neither file exists yet.
///
/// `project_settings` is the file the merge resolved, not the path rhei writes:
/// a project still on the deprecated home that followed this advice to the new
/// path would create a file shadowing the registry that refused the name.
/// §FS-rhei-agents.1.1
fn settings_entry_location(key: &str, project_settings: &str) -> String {
    format!("`{key}` in {project_settings} or ~/.config/rhei/settings.json")
}

/// Help for an agent id not in the merged registry. `known` is already seeded
/// with the built-ins, so this never names them twice. §FS-rhei-errors.1.3
///
/// This is the wording every other registry refusal shares, so it is built
/// from the same clause rather than beside it. §FS-rhei-errors.1.4
fn unknown_agent_help(id: &str, known: &[String], project_settings: &str) -> String {
    let hint = did_you_mean(id, known).map(|hint| format!("{hint} ")).unwrap_or_default();
    format!("{hint}Define it under {}.", settings_entry_location("agents.<id>", project_settings))
}

/// Help for a mode the resolved agent does not declare: the listing says which
/// modes exist, the clause where to write the one that does not.
/// §FS-rhei-errors.1.4
///
/// Spawn time refuses a mode at two sites, so the whole help is built here and
/// the category keeps one wording. §FS-rhei-errors.6 A modeless agent is still
/// offered both ways out — the brackets are usually the mistake, declaring the
/// mode the other. §FS-rhei-errors.1.2
fn unknown_mode_help(id: &str, mode: &str, known: &[String], project_settings: &str) -> String {
    let hint = did_you_mean(mode, known).unwrap_or_else(|| {
        format!("agent '{id}' declares no modes; drop the brackets from the selector.")
    });
    let location = settings_entry_location(&format!("agents.{id}.modes"), project_settings);
    format!("{hint} Declare it under {location}.")
}

/// Help for a `--agent` value that is really a selector. Without it the user is
/// told to define `agents.my-agent[nope]:some-model`. §FS-rhei-errors.1.2
fn agent_flag_selector_help(value: &str, known: &[String]) -> Option<String> {
    if !value.contains(':') && !value.contains('[') {
        return None;
    }
    let target = parse_execution_target(value).ok()?;
    let mut parts = vec!["--agent".to_string(), target.agent.clone()];
    if let Some(mode) = target.mode {
        parts.push("--agent-mode".to_string());
        parts.push(mode);
    }
    parts.push("--model".to_string());
    parts.push(target.model);
    let mut help = format!(
        "--agent takes a bare agent id; the mode and model have their own flags. \
         Write it as: {}",
        shell_command(&parts)
    );
    // Surface both problems at once when the id inside the selector is itself a
    // typo, rather than making the user discover it on the next attempt.
    if !known.iter().any(|name| name == &target.agent) {
        if let Some(name) = nearest_match(&target.agent, known) {
            // Its own line: a question mark running straight off the end of a
            // command reads as part of the command.
            help = format!("{help}\nDid you mean '{name}'?");
        }
    }
    Some(help)
}

/// Help for a declared handoff whose artifact was never written with content.
fn handoff_empty_artifact_help() -> &'static str {
    "the producing state declared this handoff but wrote nothing to it. An empty file \
     does not satisfy a handoff — check that state's agent log under runtime/logs/, \
     then re-run the producing task."
}

/// What this platform refuses, and at what size.
///
/// Linux rejects any single argument above `MAX_ARG_STRLEN` whatever `ARG_MAX`
/// says; macOS has no per-argument cap and fails on the total instead; Windows
/// caps the whole command line at what `CreateProcessW` accepts.
/// §FS-rhei-errors.7.1
const COMMAND_LINE_LIMIT_BYTES: usize = if cfg!(windows) {
    32_767
} else if cfg!(target_os = "linux") {
    131_072
} else {
    1_048_576
};

/// Whether that limit is placed on one argument or on the whole line.
///
/// It decides both when the prompt is the thing over the cap and how the failure
/// states the size, so the two can never disagree. §FS-rhei-errors.7.2
const LIMIT_IS_PER_ARGUMENT: bool = cfg!(target_os = "linux");

/// `E2BIG`, which both Linux and macOS report when a command line is too large
/// for them. §FS-rhei-errors.7.1
const E2BIG: i32 = 7;

/// The command line as the operating system will count it: every argument, and
/// the space between them.
///
/// An approximation of what is actually measured, which is why it only ever
/// explains a failure that already happened. §FS-rhei-errors.7.1
fn composed_command_line_bytes(cmd: &std::process::Command) -> usize {
    cmd.get_program().len() + cmd.get_args().map(|arg| arg.len() + 1).sum::<usize>()
}

/// Whether the failure names its own cause, whatever the command line measures.
///
/// These two every platform reports distinctly, Windows included: the binary is
/// not there, or this user may not run it. §FS-rhei-errors.7.1
fn err_names_its_own_cause(err: &std::io::Error) -> bool {
    matches!(err.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied)
}

/// Whether the size of what rhei composed explains the operating system's
/// refusal to start the process.
///
/// Linux and macOS answer for themselves: both report `E2BIG`, each at its own
/// cap. Windows reports nothing distinct enough to read a size refusal from, and
/// the Rust error kind that would name it portably is newer than this
/// workspace's minimum supported Rust, so there rhei measures instead — but only
/// once the failure has declined to say what it was. The measurement is the
/// fallback for an indistinct failure, not an override of a distinct one, and it
/// explains a failure without ever causing one. §FS-rhei-errors.7.1
fn size_explains_spawn_failure(err: &std::io::Error, command_line_bytes: usize) -> bool {
    if cfg!(windows) {
        !err_names_its_own_cause(err) && command_line_bytes > COMMAND_LINE_LIMIT_BYTES
    } else {
        err.raw_os_error() == Some(E2BIG)
    }
}

/// What a failed spawn says beyond the operating system's own words.
struct SpawnFailureGuidance {
    /// Appended to the message. Empty unless the size is what explains it.
    detail: String,
    help: &'static str,
}

/// Help for a spawn that failed for any reason the command line's size does not
/// explain — a missing binary above all, which is what this reads like.
// §FS-rhei-errors.6: every failure names the next action.
fn spawn_not_started_help() -> &'static str {
    "the agent command could not start. Check it exists on PATH and is executable: rhei diag"
}

/// Help for a spawn the composed prompt's size explains.
///
/// It names the agents that already deliver on stdin and the shape of a custom
/// entry that does, rather than saying to set `stdin_prompt` on the agent that
/// failed: a settings entry for a built-in id replaces that profile wholesale
/// (§FS-rhei-agents.1.3), so that advice would paste back as a different
/// failure, which §FS-rhei-errors.1.2 rules out.
fn oversized_prompt_help() -> &'static str {
    "use an agent that delivers the prompt on stdin ('claude-code', 'codex'), or register \
     a custom agent with \"stdin_prompt\": true"
}

/// What the size of what rhei composed says about a spawn that failed.
/// §FS-rhei-errors.7.2
enum SizeVerdict {
    /// Nothing rhei measured explains the failure, or supports a number.
    Unexplained,
    /// The platform refused the line, and the prompt is not what made it long.
    LineTooLong,
    /// The prompt is what the platform refused.
    PromptTooLong,
}

/// Whether the prompt rhei composed is actually on the command line.
///
/// Read from the argument vector rather than from `stdin_prompt`, because that
/// field is not the only transport that keeps the prompt off `argv`: the Claude
/// Code stream-json arm does too, and would otherwise be told its prompt was too
/// long for a line it is not on. §FS-rhei-errors.7.2
fn prompt_reached_argv(cmd: &std::process::Command, prompt: &str) -> bool {
    cmd.get_args().any(|arg| arg == std::ffi::OsStr::new(prompt))
}

/// One question asked at a spawn failure and never before it: could this be the
/// size of what rhei composed?
///
/// Answered in two parts, because a line over the cap and a prompt over the cap
/// are different facts. The prompt has to have reached `argv` at all, and it has
/// to be what the platform is refusing — its own bytes where the cap is on one
/// argument, and what is left once it comes off the line where the cap is on the
/// total. Otherwise the size is reported without a cause. §FS-rhei-errors.7.2
fn size_verdict(
    err: &std::io::Error,
    prompt_in_argv: bool,
    prompt_bytes: usize,
    command_line_bytes: usize,
) -> SizeVerdict {
    // Nothing is claimed that rhei's own measurement does not support, so no
    // number is ever printed beside a limit larger than it. §FS-rhei-errors.7.2
    if !size_explains_spawn_failure(err, command_line_bytes)
        || command_line_bytes <= COMMAND_LINE_LIMIT_BYTES
    {
        return SizeVerdict::Unexplained;
    }
    let prompt_is_over = if LIMIT_IS_PER_ARGUMENT {
        // The cap is on the argument as the kernel stores it, terminating NUL
        // and all, so an argument of exactly the limit is already one too long.
        // §FS-rhei-errors.7.1
        prompt_bytes >= COMMAND_LINE_LIMIT_BYTES
    } else {
        command_line_bytes.saturating_sub(prompt_bytes) <= COMMAND_LINE_LIMIT_BYTES
    };
    if prompt_in_argv && prompt_is_over {
        SizeVerdict::PromptTooLong
    } else {
        SizeVerdict::LineTooLong
    }
}

/// The sentence that names an oversized prompt, in the terms this platform's
/// limit is placed on.
///
/// Where the cap is per-argument the prompt's own size is past it and says
/// everything. Where it is a total the prompt's size is below the limit, so the
/// line's size is named too and the prompt is given as its share — the number
/// and the limit then agree. §FS-rhei-errors.7.2
fn oversized_prompt_detail(prompt_bytes: usize, command_line_bytes: usize) -> String {
    if LIMIT_IS_PER_ARGUMENT {
        format!(
            "\nthe composed prompt is {prompt_bytes} bytes and this agent passes it as one \
             command-line argument, past this platform's {COMMAND_LINE_LIMIT_BYTES}-byte limit"
        )
    } else {
        format!(
            "\nthe composed command line is {command_line_bytes} bytes, {prompt_bytes} of them \
             the prompt this agent passes on it, past this platform's \
             {COMMAND_LINE_LIMIT_BYTES}-byte limit"
        )
    }
}

/// What a spawn failure says once the size question has been answered.
/// §FS-rhei-errors.7
fn spawn_failure_guidance(
    err: &std::io::Error,
    prompt_in_argv: bool,
    prompt_bytes: usize,
    command_line_bytes: usize,
) -> SpawnFailureGuidance {
    match size_verdict(err, prompt_in_argv, prompt_bytes, command_line_bytes) {
        SizeVerdict::Unexplained => {
            SpawnFailureGuidance { detail: String::new(), help: spawn_not_started_help() }
        }
        // Taking the prompt off this line would leave it just as long, so the
        // measurement is stated and the remedy stays the one it had.
        // §FS-rhei-errors.7.2
        SizeVerdict::LineTooLong => SpawnFailureGuidance {
            detail: format!(
                "\nthe composed command line is {command_line_bytes} bytes, past this \
                 platform's {COMMAND_LINE_LIMIT_BYTES}-byte limit"
            ),
            help: spawn_not_started_help(),
        },
        // The user cannot see the composed prompt, so the byte count and the
        // platform's limit are what turn "too long" into a decision about which
        // agent to run. §FS-rhei-errors.7
        SizeVerdict::PromptTooLong => SpawnFailureGuidance {
            detail: oversized_prompt_detail(prompt_bytes, command_line_bytes),
            help: oversized_prompt_help(),
        },
    }
}

/// The diagnostic for an agent process that never started. §FS-rhei-errors.7
fn spawn_failure_report(
    err: &std::io::Error,
    resolved: &ResolvedAgent,
    prompt: &str,
    cmd: &std::process::Command,
) -> miette::Report {
    let said = spawn_failure_guidance(
        err,
        prompt_reached_argv(cmd, prompt),
        prompt.len(),
        composed_command_line_bytes(cmd),
    );
    miette!(
        help = said.help,
        "failed to spawn agent '{}': {err}{}",
        resolved.agent.id(),
        said.detail
    )
}
