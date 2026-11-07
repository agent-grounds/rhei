//! The effort fixture answers version probes without writing under an outer
//! run's identity, while real tasks retain their effort arguments and results.
//! §FS-rhei-evidence.7.1 §FS-rhei-agents.2.2

use std::fs;
use std::process::{Command, Stdio};

use super::effort_support::*;
use super::evidence_support::one_start;
use super::{assert_success, rhei_command, unique_temp_dir, CliRun};

fn remove_run_identity(cmd: &mut Command) {
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("RHEI_") {
            cmd.env_remove(name);
        }
    }
}

fn assert_probe_isolated(agent: &str, executable: &str, expected: &[&str]) {
    let dir = unique_temp_dir(&format!("effort-version-{agent}"));
    let outer = dir.join("outer-checkout");
    fs::create_dir(&outer).expect("create empty outer sentinel");
    let bin = dir.join("bin");
    fs::create_dir(&bin).expect("create shim directory");
    fs::copy(fixture_binary(), bin.join(format!("{executable}{}", std::env::consts::EXE_SUFFIX)))
        .expect("copy compiled fixture shim");
    let mut paths = vec![bin];
    paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
    let path = std::env::join_paths(paths).expect("join shim PATH");
    let machine =
        one_state_machine(&format!("    target: {agent}:provider:model\n    effort: high\n"));
    // Mentioning the flag inside a real prompt must not turn the task into a probe.
    let plan_text = PLAN.replace("Exercise effort", "Exercise effort with --version in the prompt");
    let (plan, machine) = write_case(&dir, &plan_text, &machine);
    let mut cmd = rhei_command(dir.join("home"));
    remove_run_identity(&mut cmd);
    cmd.env("PATH", path)
        .env("RHEI_CHECKOUT_ROOT", &outer)
        .env("RHEI_STATE", "specify")
        .env("RHEI_MODEL", "outer-model")
        .env("RHEI_AGENT", "outer-agent")
        .arg("--state-machine")
        .arg(&machine)
        .arg("run")
        .arg(&plan)
        .args(["--no-tui", "--no-callbacks"]);
    let output = cmd.output().expect("run built-in effort shim");
    assert_success(&CliRun::from(&output));
    let args = record_args(&dir, "work", "model");
    assert!(
        args.windows(expected.len()).any(|span| span == expected),
        "{agent} missing task effort {expected:?}: {args:?}"
    );
    assert_eq!(
        fs::read_to_string(dir.join("runtime/results/plan.1.md")).expect("read real task result"),
        "Fixture agent completed.\n",
        "{agent} must execute its real task before isolation is judged"
    );
    let entries: Vec<_> = fs::read_dir(&outer)
        .expect("read outer sentinel")
        .map(|entry| entry.expect("sentinel entry").file_name())
        .collect();
    let record = record_path(&outer, "specify", "outer-model");
    let observed = fs::read_to_string(&record).unwrap_or_default();
    assert!(
        entries.is_empty(),
        "{agent} version probe wrote under outer identity: {entries:?}\n\
         runtime/effort-argv/specify-outer-model.txt:\n{observed}"
    );
    let start = one_start(&dir.join("runtime/evidence"), "agent");
    assert!(
        start["agent_cli_version"].as_str().is_some_and(|line| !line.trim().is_empty()),
        "{agent} must answer the actual built-in version probe: {start:#}"
    );
    assert!(start["agent_cli_version_note"].is_null(), "{start:#}");
}

#[test]
fn effort_version_claude_probe_leaves_outer_root_empty() {
    assert_probe_isolated("claude-code", "claude", &["--effort", "high"]);
}

#[test]
fn effort_version_codex_probe_leaves_outer_root_empty() {
    assert_probe_isolated("codex", "codex", &["-c", "model_reasoning_effort=\"high\""]);
}

#[test]
fn effort_version_kilo_probe_leaves_outer_root_empty() {
    assert_probe_isolated("kilocode", "kilo", &["--variant", "high"]);
}

#[test]
fn effort_version_pi_probe_leaves_outer_root_empty() {
    assert_probe_isolated("pi", "pi", &["--thinking", "high"]);
}

#[test]
fn effort_version_queries_succeed_without_run_identity() {
    let dir = unique_temp_dir("effort-version-no-identity");
    let mut failures = Vec::new();
    for args in [&["--version"][..], &["exec", "--version"][..]] {
        let mut versions = Vec::new();
        for _ in 0..2 {
            let mut cmd = Command::new(fixture_binary());
            remove_run_identity(&mut cmd);
            let output =
                cmd.args(args).current_dir(&dir).stdin(Stdio::null()).output().expect("probe");
            let diagnostic = super::stderr(&output);
            let version = String::from_utf8(output.stdout).expect("version UTF-8");
            if !output.status.success() || version.trim().is_empty() {
                failures.push(format!(
                    "{args:?}: status={}, stdout={version:?}, stderr={diagnostic}",
                    output.status
                ));
            }
            versions.push(version);
        }
        assert_eq!(versions[0], versions[1], "version query must be deterministic");
    }
    assert_eq!(fs::read_dir(&dir).expect("probe directory").count(), 0, "probe wrote artifacts");
    assert!(failures.is_empty(), "version queries failed:\n{}", failures.join("\n"));
}
