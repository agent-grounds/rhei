//! Authored state effort is validated and inspectable without changing
//! execution identity. §FS-rhei-states.1.2 §FS-rhei-states.1.3
//! §FS-rhei-states-cmd.4 §FS-rhei-states-cmd.5

use super::effort_support::*;
use super::*;

fn inspection_machine(effort: Option<&str>) -> String {
    let field = effort.map(|value| format!("    effort: {value}\n")).unwrap_or_default();
    format!(
        r#"name: effort-inspection
version: 1
states:
  work:
    description: Inspect effort
    target: codex[yolo]:openai:gpt-5.6-sol
{field}  completed:
    final: true
transitions:
  - from: work
    to: completed
"#
    )
}

#[test]
fn effort_json_inspection_retains_the_authored_value() {
    let dir = unique_temp_dir("effort-json-inspection");
    let (plan, machine) = write_case(&dir, PLAN, &inspection_machine(Some("low")));

    let result = run_cli("states", &plan, &machine, &["--json"]);
    assert_success(&result);
    let json: serde_json::Value =
        serde_json::from_str(&result.stdout).expect("states output is JSON");
    let work = json["states"]
        .as_array()
        .expect("state array")
        .iter()
        .find(|state| state["name"] == "work")
        .expect("work state");

    assert_eq!(work["effort"], "low", "authored effort must survive states --json");
}

#[test]
fn effort_text_inspection_prints_the_authored_value() {
    let dir = unique_temp_dir("effort-text-inspection");
    let (plan, machine) = write_case(&dir, PLAN, &inspection_machine(Some("high")));

    let result = run_cli("states", &plan, &machine, &[]);
    assert_success(&result);
    assert!(
        result.stdout.contains("Effort: high"),
        "text inspection must expose authored effort; got:\n{}",
        result.stdout
    );
}

#[test]
fn effort_omission_preserves_inspection_shapes() {
    let dir = unique_temp_dir("effort-omitted-inspection");
    let (plan, machine) = write_case(&dir, PLAN, &inspection_machine(None));

    let text = run_cli("states", &plan, &machine, &[]);
    assert_success(&text);
    assert!(!text.stdout.contains("Effort:"), "omission adds no empty text line");

    let json = run_cli("states", &plan, &machine, &["--json"]);
    assert_success(&json);
    let json: serde_json::Value = serde_json::from_str(&json.stdout).expect("states JSON");
    for state in json["states"].as_array().expect("states array") {
        assert!(state.get("effort").is_none(), "omission adds no JSON member: {state}");
    }
}

#[test]
fn effort_machine_loading_accepts_exactly_the_canonical_strings() {
    for value in ["off", "minimal", "low", "medium", "high", "xhigh", "max"] {
        let dir = unique_temp_dir(&format!("effort-valid-{value}"));
        let (plan, machine) = write_case(&dir, PLAN, &inspection_machine(Some(value)));
        let result = run_cli("states", &plan, &machine, &[]);
        assert!(
            result.status.success(),
            "canonical effort {value:?} must load; stdout:\n{}\nstderr:\n{}",
            result.stdout,
            result.stderr
        );
    }
}

#[test]
fn effort_machine_loading_rejects_invalid_values_types_and_non_agent_states() {
    let cases = [
        ("unknown value", "    target: codex:openai:model\n    effort: turbo\n"),
        ("non-string value", "    target: codex:openai:model\n    effort: 7\n"),
        ("program state", "    effort: low\n    program: echo fixture\n"),
        ("gating state", "    effort: low\n    gating: true\n"),
    ];
    let mut accepted = Vec::new();
    for (label, body) in cases {
        let dir = unique_temp_dir(&format!("effort-invalid-{}", label.replace(' ', "-")));
        let machine_text = one_state_machine(body);
        let (plan, machine) = write_case(&dir, PLAN, &machine_text);
        let result = run_cli("validate", &plan, &machine, &[]);
        if result.status.success() {
            accepted.push(label);
        }
    }
    assert!(accepted.is_empty(), "invalid effort cases unexpectedly loaded: {accepted:?}");
}

#[test]
fn effort_profile_schema_rejects_malformed_values_args_and_conflicts() {
    let malformed = [
        ("unknown canonical key", r#"{"values":{"turbo":"turbo"},"args":["--effort","{value}"]}"#),
        ("missing placeholder", r#"{"values":{"low":"low"},"args":["--effort","low"]}"#),
        ("two placeholders", r#"{"values":{"low":"low"},"args":["{value}","{value}"]}"#),
        (
            "bad conflict",
            r#"{"values":{"low":"low"},"args":["--effort","{value}"],"conflicts":[["--effort"]]}"#,
        ),
    ];
    let mut accepted = Vec::new();
    for (label, effort) in malformed {
        let dir = unique_temp_dir(&format!("effort-profile-{}", label.replace(' ', "-")));
        write_settings(
            &dir,
            &format!(
                r#"{{"agents":{{"fixture":{{"command":{},"stdin_prompt":true,"effort":{effort}}}}}}}"#,
                effort_command(&[])
            ),
        );
        let machine_text = one_state_machine("    agent: fixture\n    effort: low\n");
        let (plan, machine) = write_case(&dir, PLAN, &machine_text);
        let result = run_cli("validate", &plan, &machine, &[]);
        if result.status.success() {
            accepted.push(label);
        }
    }
    assert!(accepted.is_empty(), "malformed effort mappings were accepted: {accepted:?}");
}

#[test]
fn effort_supporting_profile_rejects_an_unrepresentable_value() {
    let dir = unique_temp_dir("effort-unrepresentable");
    write_settings(
        &dir,
        &format!(
            r#"{{"agents":{{"fixture":{{"command":{},"stdin_prompt":true,"effort":{{"values":{{"low":"native-low"}},"args":["--effort","{{value}}"]}}}}}}}}"#,
            effort_command(&[])
        ),
    );
    let machine_text = one_state_machine("    agent: fixture\n    effort: high\n");
    let (plan, machine) = write_case(&dir, PLAN, &machine_text);

    let result = run_cli("validate", &plan, &machine, &[]);
    assert!(!result.status.success(), "a supporting profile must reject a value it cannot encode");
    assert!(
        result.stderr.contains("high") && result.stderr.contains("fixture"),
        "the error must identify value and profile; got:\n{}",
        result.stderr
    );
}

#[test]
fn effort_does_not_change_target_locked_override_validation() {
    let dir = unique_temp_dir("effort-target-locked");
    let plan_text = r#"# Rhei: Locked effort

## Tasks

### Task 1: Locked identity
**State:** work
**Target:** codex:openai:other
"#;
    let machine_text = one_state_machine(
        "    target: codex:openai:model\n    target_locked: true\n    effort: low\n",
    );
    let (plan, machine) = write_case(&dir, plan_text, &machine_text);

    let result = run_cli("validate", &plan, &machine, &[]);
    assert!(!result.status.success(), "target lock still rejects identity overrides");
    assert!(result.stderr.contains("target_locked"), "wrong refusal:\n{}", result.stderr);
}

#[test]
fn effort_does_not_relax_the_fanout_task_override_ban() {
    let dir = unique_temp_dir("effort-fanout-override-ban");
    let plan_text = r#"# Rhei: Fanout effort override

## Tasks

### Task 1: Conflicting override
**State:** work
**Target:** codex:openai:other
"#;
    let machine_text = one_state_machine(
        "    all_targets: [codex:openai:one, codex:openai:two]\n    effort: low\n",
    );
    let (plan, machine) = write_case(&dir, plan_text, &machine_text);

    let result = run_cli("validate", &plan, &machine, &[]);
    assert!(!result.status.success(), "fanout still rejects a task target override");
    assert!(
        result.stderr.contains("fanout") || result.stderr.contains("all_targets"),
        "wrong refusal:\n{}",
        result.stderr
    );
}
