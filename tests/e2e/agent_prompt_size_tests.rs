//! What happens to a prompt too large for a command line. agent-grounds/rhei#179:
//! a supervisor brief past the platform's limit aborted the spawn with
//! `Argument list too long`, and the help sent the user to `PATH` for a binary
//! that was plainly there.
//!
//! The first two tests drive a registered agent, and pin that the stdin
//! transport survives the size and that the agents still bounded by it say so.
//! The third reaches a built-in: agent-grounds/rhei#659, a state on the built-in
//! `pi` whose prompt was past the limit. It puts a compiled stand-in on `PATH` as
//! `pi` — compiled, because §REQ-cross-platform.4 keeps a shell script out of a
//! fixture — and leaves `HOME` empty, so the built-in profile is what spawns it.

use std::ffi::OsString;
use std::fs;

use super::*;

/// Larger than every supported platform's limit: per §FS-rhei-errors.7.1 Linux
/// rejects a single argument of 131072 bytes or more, Windows a command line above
/// 32767 characters, and macOS a total above roughly one megabyte. One size
/// clears all three, so neither test is vacuous on any of them.
const OVERSIZED_BODY_BYTES: usize = 2 * 1024 * 1024;

/// Present in every line of the task body, so "the prompt reached `argv`" is a
/// substring search rather than a length comparison.
const MARKER: &str = "OVERSIZED-PROMPT-MARKER";

const MACHINE: &str = r#"name: oversized-prompt
version: 1
states:
  work:
    initial: true
    description: Carry the brief
    instructions: |
      Read the brief and finish.
  completed:
    final: true
    description: Done
transitions:
  - from: work
    to: completed
"#;

fn oversized_plan() -> String {
    let line = format!("{MARKER} filler that makes this task body larger than a command line.\n");
    let body = line.repeat(OVERSIZED_BODY_BYTES / line.len() + 1);
    let heading = "# Rhei: Oversized Prompt\n\n## Tasks\n\n";
    format!("{heading}### Task 1: Carry a very large brief\n**State:** work\n\n{body}")
}

/// A fixture that reports how the prompt reached it. The prelude's
/// `agent_prompt()` already resolves either transport, so what this adds is the
/// argument vector it was spawned with.
fn write_recording_agent(dir: &std::path::Path, record: &std::path::Path) -> std::path::PathBuf {
    let preamble = format!(
        "import json\nRECORD = {}\nMARKER = {}\n",
        serde_json::to_string(&record.display().to_string()).expect("record path json"),
        serde_json::to_string(MARKER).expect("marker json"),
    );
    write_python_agent(
        dir,
        "recording-agent.py",
        &format!(
            "{preamble}{}",
            r#"
prompt = agent_prompt()
write(pathlib.Path(RECORD), json.dumps({
    'prompt_bytes': len(prompt.encode('utf-8')),
    'prompt_in_argv': any(MARKER in arg for arg in sys.argv[1:]),
    'argv': sys.argv[1:],
}))
result('## Result\n\nThe brief arrived whole.\n')
"#
        ),
    )
}

fn write_settings(root: &std::path::Path, agent_id: &str, entry: &str) {
    let settings_dir = root.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings directory");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "{agent_id}", "agent_timeout": "60s" }},
  "agents": {{ "{agent_id}": {entry} }}
}}"#
        ),
    )
    .expect("write settings");
}

/// The guard, not the proof: this transport already works today, and the point
/// of the ticket's fix is to put `claude-code` on it. What it holds is that
/// prompt size stops being a spawn concern once the prompt travels on stdin —
/// no cap applies, and the brief arrives whole rather than truncated.
// §FS-rhei-agents.2.2 §FS-rhei-agents.1.1.2
#[test]
fn a_brief_past_every_platform_limit_reaches_a_stdin_agent_whole() {
    let dir = unique_temp_dir("oversized-prompt-stdin");
    let record = dir.join("delivery.json");
    let agent = write_recording_agent(&dir, &record);
    write_settings(
        &dir,
        "stdin-agent",
        &format!(
            r#"{{ "command": {}, "stdin_prompt": true, "timeout": "60s" }}"#,
            fixture_command(&agent)
        ),
    );

    let plan_path = write_fixture_file(&dir, "plan.rhei.md", &oversized_plan());
    let machine_path = write_fixture_file(&dir, "states.yaml", MACHINE);

    let run = run_cli("run", &plan_path, &machine_path, &["--no-tui"]);
    assert_success(&run);

    let delivered: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&record).expect("delivery record"))
            .expect("delivery record json");
    assert_eq!(
        delivered["prompt_in_argv"],
        serde_json::Value::Bool(false),
        "a stdin transport puts no part of the prompt on the command line: {delivered}"
    );
    let delivered_bytes = delivered["prompt_bytes"].as_u64().expect("prompt_bytes") as usize;
    assert!(
        delivered_bytes >= OVERSIZED_BODY_BYTES,
        "the whole brief must arrive: {delivered_bytes} bytes of at least {OVERSIZED_BODY_BYTES}"
    );
    assert_task_state(&plan_path, &machine_path, "1", "completed");
}

/// The failure the ticket is about, seen from the outside. An agent that
/// carries its prompt in `argv` still cannot take a brief this size — that is
/// the platform's rule, not rhei's — but the failure now says the size is why,
/// instead of sending the reader to `PATH` for a binary that ran a moment ago.
// §FS-rhei-errors.7 §FS-rhei-errors.1.2
#[test]
fn an_argv_agent_past_the_limit_blames_the_prompt_size_not_the_path() {
    let dir = unique_temp_dir("oversized-prompt-argv");
    let record = dir.join("delivery.json");
    let agent = write_recording_agent(&dir, &record);
    write_settings(
        &dir,
        "argv-agent",
        &format!(
            r#"{{ "command": {}, "prompt_flag": "--prompt", "timeout": "60s" }}"#,
            fixture_command(&agent)
        ),
    );

    let plan_path = write_fixture_file(&dir, "plan.rhei.md", &oversized_plan());
    let machine_path = write_fixture_file(&dir, "states.yaml", MACHINE);

    let run = run_cli("run", &plan_path, &machine_path, &["--no-tui"]);
    assert!(!run.status.success(), "the spawn cannot succeed:\n{}", run.stdout);

    // Miette wraps both the message and the help, so every claim below is made
    // against the diagnostic with its line breaks collapsed.
    let said = run.stderr.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        !said.contains("Check it exists on PATH"),
        "the binary is present; the PATH remedy is the wrong next action:\n{}",
        run.stderr
    );
    assert!(
        said.contains("stdin"),
        "the remedy is prompt delivery, so it names stdin:\n{}",
        run.stderr
    );

    let sizes: Vec<usize> = said
        .split(" bytes")
        .filter_map(|before| before.rsplit(' ').next())
        .filter_map(|word| word.parse().ok())
        .collect();
    assert!(
        sizes.iter().any(|size| *size >= OVERSIZED_BODY_BYTES),
        "the failure states the composed prompt's size; found {sizes:?} in:\n{}",
        run.stderr
    );
}

/// One state on the built-in `pi`, with no settings to replace it.
const PI_MACHINE: &str = r#"name: oversized-prompt-pi
version: 1
states:
  work:
    initial: true
    description: Carry the brief
    target: pi:provider:model
    agent_timeout: 60s
    instructions: |
      Read the brief and finish.
  completed:
    final: true
    description: Done
transitions:
  - from: work
    to: completed
"#;

/// Linux refuses any single argument of this many bytes at `execve`
/// (§FS-rhei-errors.7.1), the cap agent-grounds/rhei#659 ran into.
const LINUX_ARGUMENT_CAP: u64 = 131_072;

/// agent-grounds/rhei#659: the built-in `pi` passed the whole prompt as the value
/// of `-p`, so a brief past the platform's cap never started pi; and the one
/// stdin route a settings entry offered ended in a `--` that pi refuses. The
/// stand-in refuses `--` exactly as pi does, so a fix that moves the prompt to
/// stdin but keeps the separator fails here too.
// §FS-rhei-agents.2.2.2 §FS-rhei-agents.2
#[test]
fn a_brief_past_every_platform_limit_starts_the_builtin_pi_on_stdin() {
    let dir = unique_temp_dir("oversized-prompt-builtin-pi");
    let bin = dir.join("bin");
    fs::create_dir_all(&bin).expect("create stand-in directory");
    fs::copy(
        env!("CARGO_BIN_EXE_pi-stdin-fixture"),
        bin.join(format!("pi{}", std::env::consts::EXE_SUFFIX)),
    )
    .expect("copy the compiled pi stand-in");

    let plan_path = write_fixture_file(&dir, "plan.rhei.md", &oversized_plan());
    let machine_path = write_fixture_file(&dir, "states.yaml", PI_MACHINE);

    let home = dir.join("home");
    let mut paths = vec![bin];
    paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
    let path: OsString = std::env::join_paths(paths).expect("join stand-in PATH");
    let mut cmd = rhei_command(&home);
    cmd.env("PATH", path)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .arg("--state-machine")
        .arg(&machine_path)
        .arg("run")
        .arg(&plan_path)
        .args(["--no-tui", "--no-callbacks"]);
    let run = CliRun::from(&cmd.output().expect("run the built-in pi"));
    assert_success(&run);

    let record = dir.join("runtime/pi-stdin/delivery.json");
    let delivered: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&record).expect("delivery record"))
            .expect("delivery record json");
    let argv = delivered["argv"].as_array().expect("argv");
    assert!(
        !argv.iter().any(|arg| arg.as_str().is_some_and(|arg| arg.contains(MARKER))),
        "no argument carries the prompt: {argv:?}"
    );
    let largest = delivered["largest_arg_bytes"].as_u64().expect("largest_arg_bytes");
    assert!(
        largest < LINUX_ARGUMENT_CAP,
        "no argument reaches the per-argument cap: {largest} bytes"
    );
    let arrived = delivered["stdin_bytes"].as_u64().expect("stdin_bytes") as usize;
    assert!(
        arrived >= OVERSIZED_BODY_BYTES,
        "the whole brief arrives on stdin: {arrived} bytes of at least {OVERSIZED_BODY_BYTES}"
    );
    assert_task_state(&plan_path, &machine_path, "1", "completed");
}
