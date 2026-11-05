//! Failure-derived help, including shared wrappers. §FS-rhei-errors.1.2 §FS-rhei-errors.6

use super::*;
use std::process::Stdio;

const FINISHED: &str = "# Rhei: Project\n\n## Tasks\n\n### Task 1: Finished\n**State:** done\n";
const MACHINE: &str = "name: guidance\nversion: 1\nstates:\n  open:\n    initial: true\n    description: Work\n  done:\n    description: Finished\n    final: true\ntransitions:\n  - { from: open, to: done, description: Finish }\n";

fn fixture(plan: &str, machine: &str) -> TestDir {
    let dir = unique_temp_dir("rhei-557-guidance");
    write_fixture_file(&dir, "plan.rhei.md", plan);
    write_fixture_file(&dir, "states.yaml", machine);
    let valid = invoke(&dir, &["validate", "plan.rhei.md"], false);
    assert!(valid.status.success(), "invalid fixture: {}", valid.stderr);
    dir
}

fn invoke(dir: &Path, args: &[&str], worker: bool) -> CliRun {
    let mut command = rhei_command(dir.join("home"));
    // These are independent scratch projects, never this worker's reservation.
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("RHEI_") {
            command.env_remove(key);
        }
    }
    if worker {
        command.env("RHEI_TASK_ID", "plan.1");
    }
    command
        .env("XDG_CONFIG_HOME", dir.join("home/config"))
        .current_dir(dir)
        .stdin(Stdio::null())
        .args(args);
    CliRun::from(&command.output().expect("run scratch CLI"))
}

fn help(run: &CliRun, message: &str) -> String {
    assert_eq!(run.status.code(), Some(1), "refusal changed: {}", run.stderr);
    assert!(run.stderr.contains(message), "wrong refusal: {}", run.stderr);
    let help = run.stderr.split_once("help:").map(|(_, help)| help.trim()).unwrap_or("");
    assert!(!help.is_empty(), "missing next action: {}", run.stderr);
    help.to_owned()
}

fn requires(help: &str, words: &[&str]) {
    for word in words {
        assert!(help.to_lowercase().contains(word), "help must name {word:?}; got: {help}");
    }
}

/// Both attempts stop on nonterminal stdin; neither can answer the ceremony.
/// §FS-rhei-transition-cmd.6
fn forced_refusal(dir: &Path, task: &str, message: &str) -> String {
    let before = fs::read(dir.join("plan.rhei.md")).unwrap();
    let args = [
        "transition",
        "plan.rhei.md",
        "--task",
        task,
        "--from",
        "done",
        "--to",
        "open",
        "--force",
        "--reason",
        "repair route",
    ];
    let first = invoke(dir, &args, false);
    let again = invoke(dir, &args, false);
    assert_eq!(fs::read(dir.join("plan.rhei.md")).unwrap(), before, "refusal changed plan");
    assert_eq!(first.status.code(), again.status.code());
    assert_eq!(first.stderr, again.stderr, "same invocation must meet the same refusal");
    help(&first, message)
}

#[test]
fn diagnostic_guidance_force_requires_human_terminal() {
    let dir = fixture(FINISHED, MACHINE);
    let guidance = forced_refusal(&dir, "1", "requires an interactive terminal");
    requires(&guidance, &["human", "operator", "terminal", "confirmation", "agent"]);
}

#[test]
fn diagnostic_guidance_force_reopens_ancestor_first() {
    let plan = format!("{FINISHED}\n#### Task 1.1: Child\n**State:** done\n");
    let dir = fixture(&plan, MACHINE);
    let guidance = forced_refusal(&dir, "1.1", "terminal ancestor plan.1");
    requires(&guidance, &["operator", "reopen", "first", "plan.1"]);
    let (_, command) = guidance.split_once("rhei transition").expect("ancestor command");
    requires(command, &["plan.rhei.md", "--from done", "--to open", "--force", "--reason"]);
    let words = format!("{} ", command.split_whitespace().collect::<Vec<_>>().join(" "));
    assert!(
        words.contains("--task 1 ") || words.contains("--task plan.1 "),
        "command must target the ancestor, not its child: {command}"
    );
}

#[test]
fn diagnostic_guidance_retry_terminal_task() {
    let dir = fixture(FINISHED, MACHINE);
    let run = invoke(&dir, &["retry", "plan.rhei.md", "--task", "1"], false);
    let guidance = help(&run, "cannot retry a terminal state");
    requires(&guidance, &["reopen", "operator"]);
}

#[test]
fn diagnostic_guidance_retry_worker_handoff() {
    let dir = fixture(FINISHED, MACHINE);
    let run = invoke(&dir, &["retry", "plan.rhei.md", "--task", "1"], true);
    let guidance = help(&run, "rhei retry is refused inside a worker");
    requires(&guidance, &["operator", "outside", "worker"]);
    for bypass in ["unset", "env -u", "remove rhei_task_id", "clear rhei_task_id"] {
        assert!(!guidance.to_lowercase().contains(bypass), "boundary bypass: {guidance}");
    }
}

#[test]
fn diagnostic_guidance_reason_without_force() {
    let dir = fixture(FINISHED, MACHINE);
    let run = invoke(
        &dir,
        &[
            "transition",
            "plan.rhei.md",
            "--task",
            "1",
            "--from",
            "done",
            "--to",
            "open",
            "--reason",
            "repair route",
        ],
        false,
    );
    let guidance = help(&run, "--reason is only valid with --force");
    assert!(
        guidance.contains("remove") || guidance.contains("drop") || guidance.contains("omit"),
        "help must remove the invalid flag: {guidance}"
    );
    requires(&guidance, &["--reason"]);
    if guidance.contains("--force") {
        requires(&guidance, &["operator", "terminal"]);
    }
}

#[test]
fn diagnostic_guidance_forwarded_output_remedy() {
    let machine = MACHINE.replace("    description: Work", "    description: Work\n    outputs:\n      - name: ruling\n        path: runtime/ruling.md");
    let dir = fixture(&FINISHED.replace("**State:** done", "**State:** open"), &machine);
    let before = fs::read(dir.join("plan.rhei.md")).unwrap();
    let args = [
        "transition",
        "plan.rhei.md",
        "--task",
        "1",
        "--from",
        "open",
        "--to",
        "done",
        "--result",
        "finished",
    ];
    let ordinary = invoke(&dir, &args, false);
    let original = help(&ordinary, "Missing required output artifact: ruling");
    requires(&original, &["write", "runtime", "ruling.md"]);
    let mut forced_args = args.to_vec();
    forced_args.extend(["--force", "--reason", "repair route"]);
    let forwarded = invoke(&dir, &forced_args, false);
    assert_eq!(fs::read(dir.join("plan.rhei.md")).unwrap(), before);
    assert!(
        forwarded.stderr.contains("--force does not bypass safeguards on declared edges"),
        "wrong wrapper: {}",
        forwarded.stderr
    );
    let guidance = help(&forwarded, "Missing required output artifact: ruling");
    assert!(
        guidance.contains(&original),
        "forwarding lost useful help\noriginal: {original}\nforwarded: {guidance}"
    );
}

/// Existing cause-specific help must survive the macro-wide audit unchanged.
/// This control already passes before the repair. §FS-rhei-errors.6
#[test]
fn diagnostic_guidance_filesystem_keeps_path_and_cause() {
    let dir = fixture(FINISHED, MACHINE);
    let run = invoke(&dir, &["validate", "absent.rhei.md"], false);
    let guidance = help(&run, "absent.rhei.md");
    requires(&guidance, &["nothing exists", "spelling"]);
    assert!(!guidance.contains("bug report"), "user error treated as invariant: {guidance}");
}
