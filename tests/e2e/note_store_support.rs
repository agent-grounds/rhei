// The fixture every `rhei note` scenario is read against: the four-rhei shape
// the reproducer for agent-grounds/rhei#325 builds, with `auth.2` finished
// before the run starts and its result file carrying a fact below its first
// line.
//
// Its own part because three test files read it — the composed block, the verb,
// and what a reset does to the store — and because the shape is the argument:
// `auth.3` and `reporting.1` are the two tickets that lose the fact today,
// `billing.1` and `mentor.1.2` are the two that already keep it and must go on
// keeping it unchanged.

// §FS-rhei-note §FS-rhei-memory.2

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::*;

/// One visit per ticket and nothing else to read: the machine the reproducer
/// used, so what the e2e asserts and what the reproducer showed are the same run.
pub const NOTE_MACHINE: &str = r#"name: note-store-e2e
version: 1
states:
  pending:
    initial: true
    description: Ready for work
    agent: mock
    agent_timeout: 60s
    instructions: Do the work for Task {task_id}.
  completed:
    description: Done
    final: true
  cancelled:
    description: Dropped
    final: true
transitions:
  - { from: pending, to: completed, description: Work done }
  - { from: "*", to: cancelled, description: Dropped }
"#;

/// The fact `auth.2` paid for, below the first line of its result file — so the
/// 120-character Plan History slice cannot carry it (§FS-rhei-memory.4.3).
pub const TRAP: &str = "Three concurrent cargo builds under /tmp fill the root disk; build under \
     ~/ag/tmp with --target-dir.";

/// The first line of `auth.2`'s result: what Plan History does carry, and what
/// reads like a finished report.
pub const SLICE: &str =
    "Token refresh implemented behind the auth.refresh flag; 12 new tests, cargo test green.";

/// A pre-written store, in file order: `auth.1` first, `auth.2` second, so
/// `auth.2` is the newest and renders first (§FS-rhei-memory.4.2).
pub fn store_contents() -> String {
    format!(
        "- [auth.1] The session API contract is the api-contract export; do not restate it in \
         results.\n- [auth.2] {TRAP}\n"
    )
}

/// The block §FS-rhei-memory.3.1 puts inside `## Position` for this store.
pub fn expected_block() -> String {
    format!(
        "### Project Notes\n\nFacts earlier tickets left for whoever came next, newest first.\n\n\
         - [auth.2] {TRAP}\n\
         - [auth.1] The session API contract is the api-contract export; do not restate it in \
         results.\n"
    )
}

/// The agent every ticket runs: it records the prompt it was handed under the
/// **project** execution root, which it finds by walking up to the manifest.
const CAPTURE_AGENT: &str = r#"root = pathlib.Path(env('RHEI_ROOT'))
project = root
while not (project / 'index.panta.md').exists() and project != project.parent:
    project = project.parent
task = env('RHEI_TASK_ID')

prompt = ''
args = sys.argv[1:]
while args:
    if args.pop(0) == '--prompt' and args:
        prompt = args.pop(0)
write(project / 'prompts' / (task + '.md'), prompt)

result('## Result\n\nTask {} finished.\n'.format(task))
"#;

/// Build the four-rhei project: `auth` (three tickets, two finished),
/// `billing` (a declared prior on `auth.2`), `mentor` (a decomposed subtree),
/// and `reporting` (no edge on `auth.2` at all).
///
/// `store` writes `runtime/notes.md` at the project execution root before the
/// run, exactly as the fixture already pre-writes `auth.2`'s result file for a
/// ticket that finished before the run started.
pub fn note_fixture(prefix: &str, store: Option<&str>) -> (TestDir, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let root = dir.to_path_buf();
    for sub in
        ["auth/tasks", "auth/runtime/results", "billing/tasks", "mentor/tasks", "reporting/tasks"]
    {
        fs::create_dir_all(root.join(sub)).expect("fixture directories");
    }

    write_fixture_file(
        &root,
        "index.panta.md",
        "# Panta: Knowledge\n\n## House Rules\n\nRun the gate before shipping.\n",
    );
    write_fixture_file(&root, "states.yaml", NOTE_MACHINE);

    let script = write_python_agent(&root, "mock-agent.py", CAPTURE_AGENT);
    let settings_dir = root.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("settings directory");
    let command = fixture_command(&script);
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "mock", "agent_timeout": "60s" }},
  "agents": {{ "mock": {{ "command": {command}, "prompt_flag": "--prompt", "timeout": "60s" }} }}
}}"#
        ),
    )
    .expect("write settings");

    write_rhei(&root, "auth", "Auth", "Keep the auth surface stable.");
    write_fixture_file(
        &root.join("auth/tasks"),
        "01-design.md",
        "### Task 1: Design the refresh flow\n**State:** completed\n\nWrite down how a refresh \
         token is minted.\n",
    );
    write_fixture_file(
        &root.join("auth/tasks"),
        "02-implement.md",
        "### Task 2: Implement token refresh\n**State:** completed\n\nBuild the refresh path \
         behind a flag.\n",
    );
    write_fixture_file(
        &root.join("auth/tasks"),
        "03-harden.md",
        "### Task 3: Harden the refresh path\n**State:** pending\n\nThe same rhei as Task 2, with \
         no declared prior on it.\n",
    );
    write_fixture_file(
        &root.join("auth/runtime/results"),
        "auth.1.md",
        "## Result\n\nRefresh flow designed; the contract is in docs/auth/refresh.md.\n",
    );
    write_fixture_file(
        &root.join("auth/runtime/results"),
        "auth.2.md",
        &format!("## Result\n\n{SLICE}\n\nTrap: {TRAP}\n"),
    );

    write_rhei(&root, "billing", "Billing", "Money paths are idempotent.");
    write_fixture_file(
        &root.join("billing/tasks"),
        "01-charge.md",
        "### Task 1: Charge on refreshed sessions\n**State:** pending\n**Prior:** Task auth.2\n\n\
         Bill against a session the auth work refreshed.\n",
    );

    fs::write(
        root.join("mentor/index.rhei.md"),
        "# Rhei: Mentor\n\n---\nstructure:\n  maxLevels: 3\n---\n\n## Ground Rules\n\nCarry what \
         you learn.\n",
    )
    .expect("write mentor index");
    write_fixture_file(
        &root.join("mentor/tasks"),
        "01-parent.md",
        "### Task 1: Carry the fact down a subtree\n**State:** pending\n\nA paragraph this \
         task appended to its own body, which \"Leaving a trail\" already permits.\n\n\
         #### Task 1.1: First child\n**State:** completed\n\n\
         #### Task 1.2: Second child\n**State:** pending\n",
    );

    write_rhei(&root, "reporting", "Reporting", "Reports are read-only.");
    write_fixture_file(
        &root.join("reporting/tasks"),
        "01-rollup.md",
        "### Task 1: Nightly rollup\n**State:** pending\n\nA third rhei with no prior edge on \
         auth.2 at all.\n",
    );

    if let Some(store) = store {
        fs::create_dir_all(root.join("runtime")).expect("project runtime");
        write_fixture_file(&root.join("runtime"), "notes.md", store);
    }

    (dir, root)
}

fn write_rhei(root: &Path, id: &str, title: &str, rule: &str) {
    fs::write(
        root.join(id).join("index.rhei.md"),
        format!("# Rhei: {title}\n\n## Ground Rules\n\n{rule}\n"),
    )
    .expect("write rhei index");
}

/// A command with the calling process's own run identity taken off it.
///
/// This suite is itself run from inside a `rhei run` on this project, and a
/// nested run refuses an ancestor budget reservation it cannot see. Clearing
/// the run's variables is what makes the fixture answer for itself rather than
/// for whatever started the test.
// §FS-rhei-budgets.5
pub fn isolated_command(root: &Path) -> Command {
    let mut cmd = rhei_command(root.join(".home"));
    for name in [
        "RHEI_BUDGET_PARENT_RESERVATION",
        "RHEI_AGENT",
        "RHEI_MODEL",
        "RHEI_TARGET",
        "RHEI_TARGET_SLUG",
        "RHEI_STATE",
        "RHEI_TASK_ID",
        "RHEI_VISIT_COUNT",
        "RHEI_ATTEMPT",
        "RHEI_CHECKOUT_ROOT",
        "RHEI_ROOT",
        "RHEI_RESULT_PATH",
        "RHEI_STATE_MACHINE_PATH",
        "RHEI_AGENT_MODE",
        "RHEI_MCP_SERVERS",
        "RHEI_SKILLS",
        "RHEI_MODEL_PROVIDER",
        "RHEI_MODEL_NAME",
        "RHEI_ACCOUNTING_USAGE_PATH",
        "RHEI_ACCOUNTING_USAGE_SCHEMA",
    ] {
        cmd.env_remove(name);
    }
    cmd
}

/// Run the whole project once, and return what the run printed.
pub fn run_note_fixture(root: &Path) -> CliRun {
    let mut cmd = isolated_command(root);
    cmd.current_dir(root);
    cmd.args(["--state-machine", "states.yaml", "run", "."]);
    cmd.args(["--no-callbacks", "--no-tui", "--continue-on-error"]);
    let output = cmd.output().expect("rhei run should run");
    CliRun::from(&output)
}

/// The prompt one ticket was handed, as the capture agent recorded it.
pub fn note_prompt(root: &Path, task: &str) -> String {
    let path = root.join("prompts").join(format!("{task}.md"));
    fs::read_to_string(&path).unwrap_or_else(|err| panic!("read prompt {}: {err}", path.display()))
}

/// A prompt with everything that varies between machines taken out: the
/// fixture root becomes `{ROOT}`, and separators become `/` so one baseline
/// serves Windows too.
///
/// One location, up to three spellings in one prompt, and they nest — so the
/// set has to be complete and the order longest first (§REQ-cross-platform.5).
/// macOS hands out `/var/folders/…` and canonicalizes it to
/// `/private/var/folders/…`; substituting the shorter one first rewrites the
/// inside of the longer one and leaves `/private{ROOT}/…` behind. Windows puts
/// `TEMP` under the 8.3 short name, while the run spells its own working
/// directory as the long name — which is what `fs::canonicalize` returns once
/// its `\\?\` verbatim prefix is off, and which neither of the other two
/// spellings covers.
pub fn normalize_prompt(root: &Path, prompt: &str) -> String {
    let slashed = |path: &Path| path.display().to_string().replace('\\', "/");
    let canonical = slashed(&fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf()));
    let bare = canonical.strip_prefix("//?/").unwrap_or(&canonical).to_string();

    let mut spellings = vec![slashed(root), bare, canonical];
    spellings.sort_by_key(|spelling| std::cmp::Reverse(spelling.len()));
    spellings.dedup();

    let mut out = prompt.replace('\\', "/");
    for spelling in spellings {
        out = out.replace(&spelling, "{ROOT}");
    }
    out
}

/// A committed prompt, captured at `315860109b` and read back for comparison.
///
/// With `RHEI_NOTE_BASELINE_UPDATE` set the capture is rewritten instead of
/// compared, which is how the file under `fixtures/` is produced. It is a
/// tripwire, not a source of truth: a legitimate change to prompt composition
/// is expected to update it in the same commit that makes the change.
pub fn baseline_prompt(task: &str, normalized: &str) -> String {
    let path = repo_root().join("tests/e2e/fixtures/note-store").join(format!("{task}.prompt.md"));
    if std::env::var_os("RHEI_NOTE_BASELINE_UPDATE").is_some() {
        fs::create_dir_all(path.parent().expect("fixture parent")).expect("fixture directory");
        fs::write(&path, normalized).expect("write baseline");
    }
    fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

/// The `### Project Notes` block cut out of a prompt, with what is left.
///
/// The block runs from its heading to the blank line before the next `##` or
/// `###` heading, which is the only shape §FS-rhei-memory.3.1 gives it.
pub fn without_project_notes(prompt: &str) -> (String, Option<String>) {
    let Some(start) = prompt.find("### Project Notes\n") else {
        return (prompt.to_string(), None);
    };
    let rest = &prompt[start + "### Project Notes\n".len()..];
    let end = rest
        .match_indices('\n')
        .map(|(at, _)| at + 1)
        .find(|at| rest[*at..].starts_with("## ") || rest[*at..].starts_with("### "))
        .map_or(prompt.len(), |at| start + "### Project Notes\n".len() + at);
    let block = prompt[start..end].to_string();
    let mut kept = String::with_capacity(prompt.len());
    kept.push_str(&prompt[..start]);
    kept.push_str(&prompt[end..]);
    (kept, Some(block))
}

/// `rhei note`, run from a directory inside the project, as an agent runs it.
pub fn run_note(root: &Path, cwd: &Path, task: Option<&str>, args: &[&str]) -> CliRun {
    let mut cmd: Command = isolated_command(root);
    cmd.current_dir(cwd);
    cmd.arg("note");
    for arg in args {
        cmd.arg(arg);
    }
    if let Some(task) = task {
        cmd.env("RHEI_TASK_ID", task);
    }
    let output = cmd.output().expect("rhei note should run");
    CliRun::from(&output)
}

/// The one line beginning `prefix` cut out of a prompt, with what is left.
pub fn without_line_starting(prompt: &str, prefix: &str) -> (String, Option<String>) {
    let Some(line) = prompt.lines().find(|line| line.starts_with(prefix)) else {
        return (prompt.to_string(), None);
    };
    let owned = format!("{line}\n");
    (prompt.replacen(&owned, "", 1), Some(owned))
}
