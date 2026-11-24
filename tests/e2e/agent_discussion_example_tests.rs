use std::fs;

use super::*;

/// The committed mock must satisfy both terminal entries and preserve the
/// discussion loop and downstream gate. §FS-rhei-states.3.3.1
#[test]
fn example_agent_discussion_runs_with_committed_mock_settings() {
    let dir = unique_temp_dir("rhei-767-agent-discussion");
    let workspace = dir.join("agent-discussion");
    copy_dir_recursive(&repo_root().join("examples/agent-discussion"), &workspace);
    let machine_path = workspace.join("discussion-states.yaml");
    let home = dir.join("home");
    let command = || {
        let mut cmd = rhei_command(&home);
        cmd.current_dir(&dir)
            .env("XDG_CONFIG_HOME", home.join("config"))
            .env("XDG_DATA_HOME", home.join("data"))
            .env("XDG_CACHE_HOME", home.join("cache"))
            .arg("--state-machine")
            .arg(&machine_path);
        cmd
    };
    // rhei_command removes inherited RHEI_* (including live discussion mode
    // and nested run identity); keep the example's settings untouched.
    let output = command()
        .arg("run")
        .arg(&workspace)
        .arg("--no-tui")
        .output()
        .expect("run the committed example");
    let result = CliRun::from(&output);
    assert_success(&result);

    let rendered = command()
        .arg("render")
        .arg(&workspace)
        .args(["--format", "json"])
        .output()
        .expect("render the completed example");
    assert_success(&CliRun::from(&rendered));
    let json: serde_json::Value = serde_json::from_slice(&rendered.stdout).expect("render JSON");
    let tasks = json["tasks"].as_array().expect("tasks array");
    assert_eq!(tasks.len(), 2);
    for (id, state) in [
        ("agent-discussion.discussion-seed", "converged"),
        ("agent-discussion.apply-decision", "completed"),
    ] {
        let task =
            tasks.iter().find(|task| task["id"]["path"] == id).expect("expected example task");
        assert_eq!(task["state"], state, "{id}: exact terminal state");
        let result = fs::read_to_string(workspace.join(format!("runtime/results/{id}.md")))
            .expect("each terminal task writes its own result");
        assert!(!result.trim().is_empty(), "{id}: nonempty terminal result");
    }

    let discussion = workspace.join("runtime/discussion");
    for round in 1..=2 {
        let digest = fs::read_to_string(discussion.join(format!("digest/round-{round}.md")))
            .expect("judge writes each round's digest");
        for participant in ["claude", "codex", "gemini", "cursor"] {
            let position =
                fs::read_to_string(discussion.join(format!("round-{round}/{participant}.md")))
                    .expect("each participant writes in both rounds");
            assert!(!position.trim().is_empty());
            assert!(digest.contains(position.trim()), "digest includes {participant}'s position");
        }
    }
    let decision = fs::read_to_string(discussion.join("decision.md")).expect("converged decision");
    assert!(decision.contains("**Converged:** round 2"));
    let applied =
        fs::read_to_string(discussion.join("applied.md")).expect("downstream application");
    assert!(applied.contains("Applied: D-merge-policy"));
    assert!(applied.contains("runtime/discussion/decision.md"));

    let log = fs::read_to_string(workspace.join("runtime/logs/discussion.log"))
        .expect("discussion callback log");
    let first = log.find("round 1 inconclusive -> another round").expect("first round loops");
    let second =
        log.find("round 2 converged -> decision recorded").expect("second round converges");
    let apply = log.find("applied the converged decision").expect("downstream task runs");
    assert!(first < second && second < apply, "discussion gates downstream application:\n{log}");
    assert_eq!(log.matches("wrote round 1 position").count(), 4);
    assert_eq!(log.matches("wrote round 2 position").count(), 4);
    assert_eq!(log.matches("applied the converged decision").count(), 1);
}
