//! The built-in `codex` profile gains a `workspace` mode and the writable-root
//! setting, `yolo` keeps its meaning, and the setting is validated like
//! `deny_read` (agent-grounds/rhei#509). §FS-rhei-agents.2 §FS-rhei-agents.1.1.2

use super::roster_support::{
    roster_json, run_roster, write_plan, write_settings, CURRENT_SETTINGS,
};
use super::template_sandbox_sweep_support::WORKSPACE_FLAGS;
use super::*;

const TASK: &str = "### Task 1: Work\n**State:** working\n";

const MACHINE: &str = "name: roots\nversion: 1\nstates:\n  working:\n    initial: true\n    \
                       agent: cdx\n    agent_timeout: 60s\n  completed:\n    final: true\n\
                       transitions:\n  - from: working\n    to: completed\n";

/// `yolo` unchanged beside `workspace`, `writable_roots` on `codex` and on no
/// other built-in. The roster sorts mode names, so the order is pinned by the
/// launch guard in `codex_writable_roots_tests`. §FS-rhei-agents.2
#[test]
fn roster_shows_codex_workspace_mode_and_writable_roots_beside_unchanged_yolo() {
    let dir = unique_temp_dir("codex-workspace-roster");
    let home = dir.join("home");
    write_plan(&dir, "index.rhei.md", "# Rhei: Built-in roster\n");
    let json = roster_json(&run_roster(&home, &dir, None, true));
    let codex = &json["agents"]["codex"];
    assert_eq!(
        codex["modes"],
        serde_json::json!({
            "yolo": ["--sandbox", "danger-full-access", "--skip-git-repo-check", "-c", "approval_policy=\"never\""],
            "workspace": WORKSPACE_FLAGS,
        }),
        "{codex:#}"
    );
    assert_eq!(codex["writable_roots"], serde_json::json!({"path_flag": "--add-dir"}), "{codex:#}");
    let agents = json["agents"].as_object().expect("roster agents");
    for (id, profile) in agents.iter().filter(|(id, _)| id.as_str() != "codex") {
        assert!(
            profile.get("writable_roots").is_none(),
            "built-in {id} declares writable_roots: {profile:#}"
        );
    }
}

fn validate_with_profile(name: &str, profile: serde_json::Value) -> CliRun {
    let dir = unique_temp_dir(name);
    let plan = dir.join("plan");
    std::fs::create_dir_all(plan.join("tasks")).unwrap();
    write_plan(&plan, "index.rhei.md", "# Rhei: Writable roots\n");
    write_plan(&plan, "tasks/01.md", TASK);
    write_plan(&plan, "states.yaml", MACHINE);
    write_settings(
        &plan,
        CURRENT_SETTINGS,
        &serde_json::json!({"agents": {"cdx": profile}}).to_string(),
    );
    let output = rhei_command(dir.join("home"))
        .current_dir(&dir)
        .arg("validate")
        .arg(&plan)
        .output()
        .unwrap();
    CliRun::from(&output)
}

/// An empty `writable_roots.path_flag` is refused, as an empty `deny_read`
/// one is. §FS-rhei-agents.1.1.2
#[test]
fn an_empty_writable_roots_path_flag_is_refused() {
    let run = validate_with_profile(
        "codex-workspace-empty-flag",
        serde_json::json!({"family": "codex", "writable_roots": {"path_flag": ""}}),
    );
    assert!(!run.status.success(), "stdout:\n{}\nstderr:\n{}", run.stdout, run.stderr);
    assert!(run.stderr.contains("writable_roots.path_flag"), "{}", run.stderr);
}

/// `writable_roots` is a field of a profile, so writing it draws no
/// unknown-key warning. §FS-rhei-agents.1.1.2
#[test]
fn writable_roots_is_a_known_profile_field() {
    let run = validate_with_profile(
        "codex-workspace-known-field",
        serde_json::json!({"family": "codex", "writable_roots": {"path_flag": "--add-dir"}}),
    );
    assert_success(&run);
    assert!(
        !run.stderr.contains("not a field of an agent profile"),
        "stdout:\n{}\nstderr:\n{}",
        run.stdout,
        run.stderr
    );
}
