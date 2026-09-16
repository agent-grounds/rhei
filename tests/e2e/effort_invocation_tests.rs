//! State effort reaches each effective agent without becoming part of target
//! or mode selection. §FS-rhei-agents.1.1.2 §FS-rhei-agents.1.4.1
//! §FS-rhei-agents.2.2 §FS-rhei-plan-language.3.11

use std::ffi::OsString;
use std::fs;

use super::effort_support::*;
use super::*;

fn mapped_profile(command: &str, fixed: &str) -> String {
    format!(
        r#"{{"command":{command},"stdin_prompt":true,"model_flag":"--model","modes":{{"yolo":["--permission","write"]}},"effort":{{"values":{{"low":"native-low","high":"native-high"}},"args":["--effort","{{value}}"]}}{fixed}}}"#
    )
}

#[test]
fn effort_same_target_and_mode_translate_independently_in_final_argv() {
    let dir = unique_temp_dir("effort-same-target");
    let profile = mapped_profile(&effort_command(&["--transport", "fixed"]), "");
    write_settings(&dir, &format!(r#"{{"agents":{{"fixture":{profile}}}}}"#));
    let machine = r#"name: same-target-effort
version: 1
states:
  work:
    target: fixture[yolo]:provider:model
    effort: low
    agent_timeout: 5s
  review:
    target: fixture[yolo]:provider:model
    effort: high
    agent_timeout: 5s
  completed:
    final: true
transitions:
  - from: work
    to: review
  - from: review
    to: completed
"#;
    let (plan, machine) = write_case(&dir, PLAN, machine);

    let result = run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]);
    assert_success(&result);

    let identity = "model";
    let low = record_args(&dir, "work", identity);
    let high = record_args(&dir, "review", identity);
    assert_eq!(
        low,
        [
            "--transport",
            "fixed",
            "--permission",
            "write",
            "--effort",
            "native-low",
            "--model",
            "model",
            "--",
        ],
        "low effort must occupy the dedicated argv slot"
    );
    assert_eq!(
        high,
        [
            "--transport",
            "fixed",
            "--permission",
            "write",
            "--effort",
            "native-high",
            "--model",
            "model",
            "--",
        ],
        "only translated effort may differ between the two states"
    );
}

#[test]
fn effort_conflicts_are_removed_from_base_mode_and_model_binding_args() {
    let dir = unique_temp_dir("effort-conflicts");
    let command = effort_command(&[
        "--base-keep",
        "--effort",
        "base",
        "--thinking=base",
        "--effort-extra",
        "untouched",
    ]);
    let profile = format!(
        r#"{{"command":{command},"stdin_prompt":true,"model_flag":"--model","modes":{{"yolo":["--permission","write","--effort","mode"]}},"effort":{{"values":{{"high":"native-high"}},"args":["--effort","{{value}}"],"conflicts":[["--thinking={{value}}"]]}}}}"#
    );
    write_settings(
        &dir,
        &format!(
            r#"{{"agents":{{"fixture":{profile}}},"models":{{"model":{{"provider":"provider","model":"native-model","default_agent":"fixture","agents":{{"fixture":{{"autonomous_args":["--binding-keep","--thinking=binding"]}}}}}}}}}}"#
        ),
    );
    let machine = one_state_machine("    target: fixture[yolo]:provider:model\n    effort: high\n");
    let (plan, machine) = write_case(&dir, PLAN, &machine);

    let result = run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]);
    assert_success(&result);
    assert_eq!(
        record_args(&dir, "work", "model"),
        [
            "--base-keep",
            "--effort-extra",
            "untouched",
            "--permission",
            "write",
            "--binding-keep",
            "--effort",
            "native-high",
            "--model",
            "model",
            "--",
        ],
        "all complete effort spans are replaced while unrelated flags retain order"
    );
}

#[test]
fn effort_settings_selected_agent_translates_without_a_state_selector() {
    let dir = unique_temp_dir("effort-settings-selected-agent");
    let profile = mapped_profile(&effort_command(&["--settings-agent"]), "");
    write_settings(
        &dir,
        &format!(
            r#"{{"defaults":{{"agent":"fixture","model":"model","agent_mode":"yolo"}},"agents":{{"fixture":{profile}}},"models":{{"model":{{"provider":"provider","model":"native-model","default_agent":"fixture"}}}}}}"#
        ),
    );
    let machine = one_state_machine("    effort: low\n");
    let (plan, machine) = write_case(&dir, PLAN, &machine);

    let result = run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]);
    assert_success(&result);
    let args = record_args(&dir, "work", "model");
    assert!(
        args.windows(2).any(|pair| pair == ["--effort", "native-low"]),
        "settings-selected agent must translate effort: {args:?}"
    );
}

#[test]
fn effort_unsupported_profile_and_omission_preserve_existing_argv() {
    for (label, effort) in [("unsupported", "    effort: low\n"), ("omitted", "")] {
        let dir = unique_temp_dir(&format!("effort-argv-{label}"));
        let command = effort_command(&["--fixed", "value"]);
        write_settings(
            &dir,
            &format!(
                r#"{{"agents":{{"codex":{{"command":{command},"stdin_prompt":true,"model_flag":"--model","modes":{{"yolo":["--permission","write"]}}}}}}}}"#
            ),
        );
        let machine =
            one_state_machine(&format!("    target: codex[yolo]:provider:model\n{effort}"));
        let (plan, machine) = write_case(&dir, PLAN, &machine);
        let result = run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]);
        assert_success(&result);
        assert_eq!(
            record_args(&dir, "work", "model"),
            ["--fixed", "value", "--permission", "write", "--json", "--model", "model", "--",],
            "{label} must not filter or insert arguments; same-id override is wholesale"
        );
    }
}

#[test]
fn effort_survives_model_and_full_target_task_overrides() {
    let dir = unique_temp_dir("effort-task-overrides");
    let supporting = mapped_profile(&effort_command(&["--profile", "supporting"]), "");
    let alternate = format!(
        r#"{{"command":{},"stdin_prompt":true,"model_flag":"--model","modes":{{"review":["--permission","review"]}},"effort":{{"values":{{"low":"alt-low"}},"args":["--thinking","{{value}}"]}}}}"#,
        effort_command(&["--profile", "alternate"])
    );
    write_settings(
        &dir,
        &format!(
            r#"{{"agents":{{"supporting":{supporting},"alternate":{alternate}}},"models":{{"base":{{"provider":"provider","model":"base","default_agent":"supporting"}},"special":{{"provider":"provider","model":"special","default_agent":"supporting"}}}}}}"#
        ),
    );
    let plan_text = r#"# Rhei: Effort overrides

## Tasks

### Task model: Model override
**State:** work
**Model:** special

### Task target: Target override
**State:** work
**Target:** alternate[review]:other:new
"#;
    let machine = r#"name: effort-overrides
version: 1
models: [base, special]
states:
  work:
    target: supporting[yolo]:provider:base
    effort: low
    agent_timeout: 5s
  completed:
    final: true
transitions:
  - from: work
    to: completed
"#;
    let (plan, machine) = write_case(&dir, plan_text, machine);

    let result = run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]);
    assert_success(&result);
    let records = records_for_state(&dir, "work");
    assert_eq!(records.len(), 2, "one invocation per overridden task");
    assert!(
        records.iter().any(|args| {
            args.windows(2).any(|pair| pair == ["--effort", "native-low"])
                && args.windows(2).any(|pair| pair == ["--model", "special"])
                && args.windows(2).any(|pair| pair == ["--permission", "write"])
        }),
        "model override must retain effort and state identity: {records:?}"
    );
    assert!(
        records.iter().any(|args| {
            args.windows(2).any(|pair| pair == ["--thinking", "alt-low"])
                && args.windows(2).any(|pair| pair == ["--model", "new"])
                && args.windows(2).any(|pair| pair == ["--permission", "review"])
        }),
        "target override must remap effort through the new profile: {records:?}"
    );
}

#[test]
fn effort_all_targets_mixed_profiles_translate_or_ignore_per_member() {
    let dir = unique_temp_dir("effort-all-targets");
    let supporting = mapped_profile(&effort_command(&["--profile", "supporting"]), "");
    let unsupported = format!(
        r#"{{"command":{},"stdin_prompt":true,"model_flag":"--model"}}"#,
        effort_command(&["--profile", "unsupported"])
    );
    write_settings(
        &dir,
        &format!(r#"{{"agents":{{"supporting":{supporting},"unsupported":{unsupported}}}}}"#),
    );
    let machine = one_state_machine(
        "    all_targets: [supporting:provider:model-a, unsupported:provider:model-b]\n    effort: low\n",
    );
    let (plan, machine) = write_case(&dir, PLAN, &machine);

    let result = run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]);
    assert_success(&result);
    let records = records_for_state(&dir, "work");
    assert_eq!(records.len(), 2);
    assert_eq!(
        records.iter().filter(|args| args.iter().any(|arg| arg == "native-low")).count(),
        1,
        "only the supporting fanout member translates effort: {records:?}"
    );
    assert!(records.iter().all(|args| args.last().map(String::as_str) == Some("--")));
}

#[test]
fn effort_all_models_applies_to_every_legacy_fanout_member() {
    let dir = unique_temp_dir("effort-all-models");
    let profile = mapped_profile(&effort_command(&[]), "");
    write_settings(
        &dir,
        &format!(
            r#"{{"agents":{{"fixture":{profile}}},"models":{{"one":{{"provider":"provider","model":"one","default_agent":"fixture"}},"two":{{"provider":"provider","model":"two","default_agent":"fixture"}}}}}}"#
        ),
    );
    let machine = r#"name: effort-all-models
version: 1
models: [one, two]
states:
  work:
    agent: fixture
    agent_mode: yolo
    all_models: [one, two]
    effort: low
    agent_timeout: 5s
  completed:
    final: true
transitions:
  - from: work
    to: completed
"#;
    let (plan, machine) = write_case(&dir, PLAN, machine);

    let result = run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]);
    assert_success(&result);
    let records = records_for_state(&dir, "work");
    assert_eq!(records.len(), 2);
    assert!(
        records.iter().all(|args| args.windows(2).any(|pair| pair == ["--effort", "native-low"])),
        "each all_models member receives effort: {records:?}"
    );
}

#[test]
fn effort_fanout_unrepresentable_member_fails_before_any_member_runs() {
    let dir = unique_temp_dir("effort-fanout-unrepresentable");
    let command = effort_command(&[]);
    write_settings(
        &dir,
        &format!(
            r#"{{"agents":{{"supporting":{{"command":{command},"stdin_prompt":true,"effort":{{"values":{{"low":"low"}},"args":["--effort","{{value}}"]}}}},"unsupported":{{"command":{command},"stdin_prompt":true}}}}}}"#
        ),
    );
    let machine = one_state_machine(
        "    all_targets: [supporting:provider:model-a, unsupported:provider:model-b]\n    effort: high\n",
    );
    let (plan, machine) = write_case(&dir, PLAN, &machine);

    let result = run_cli("run", &plan, &machine, &["--no-tui", "--no-callbacks"]);
    assert!(!result.status.success(), "unrepresentable fanout effort must fail");
    assert!(
        !dir.join("runtime/effort-argv").exists(),
        "validation must finish before any fanout member spawns"
    );
}

#[test]
fn effort_builtin_profiles_expose_the_approved_value_matrix() {
    let matrix = [
        ("claude-code", &["low", "medium", "high", "xhigh", "max"][..]),
        ("codex", &["minimal", "low", "medium", "high", "xhigh"][..]),
        ("kilocode", &["minimal", "low", "high", "max"][..]),
        ("pi", &["off", "minimal", "low", "medium", "high", "xhigh"][..]),
    ];
    let canonical = ["off", "minimal", "low", "medium", "high", "xhigh", "max"];
    let mut mismatches = Vec::new();
    for (agent, supported) in matrix {
        for value in canonical {
            let dir = unique_temp_dir(&format!("effort-builtin-{agent}-{value}"));
            let machine = one_state_machine(&format!(
                "    target: {agent}:provider:model\n    effort: {value}\n"
            ));
            let (plan, machine) = write_case(&dir, PLAN, &machine);
            let result = run_cli("validate", &plan, &machine, &[]);
            if result.status.success() != supported.contains(&value) {
                mismatches.push(format!("{agent}:{value} accepted={}", result.status.success()));
            }
        }
    }
    for agent in ["gemini", "cursor"] {
        for value in canonical {
            let dir = unique_temp_dir(&format!("effort-builtin-{agent}-{value}"));
            let machine = one_state_machine(&format!(
                "    target: {agent}:provider:model\n    effort: {value}\n"
            ));
            let (plan, machine) = write_case(&dir, PLAN, &machine);
            let result = run_cli("validate", &plan, &machine, &[]);
            if !result.status.success() {
                mismatches.push(format!("unsupported {agent}:{value} was rejected"));
            }
        }
    }
    assert!(mismatches.is_empty(), "built-in effort matrix mismatch:\n{}", mismatches.join("\n"));
}

#[test]
fn effort_builtin_profiles_emit_their_native_high_arguments() {
    let cases = [
        ("claude-code", "claude", &["--effort", "high"][..]),
        ("codex", "codex", &["-c", "model_reasoning_effort=\"high\""][..]),
        ("kilocode", "kilo", &["--variant", "high"][..]),
        ("pi", "pi", &["--thinking", "high"][..]),
    ];
    let mut mismatches = Vec::new();
    for (agent, executable, expected) in cases {
        let dir = unique_temp_dir(&format!("effort-builtin-argv-{agent}"));
        let bin = dir.join("bin");
        fs::create_dir_all(&bin).expect("create shim directory");
        fs::copy(
            fixture_binary(),
            bin.join(format!("{executable}{}", std::env::consts::EXE_SUFFIX)),
        )
        .expect("copy compiled fixture shim");
        let machine =
            one_state_machine(&format!("    target: {agent}:provider:model\n    effort: high\n"));
        let (plan, machine) = write_case(&dir, PLAN, &machine);
        let mut paths = vec![bin];
        paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
        let path: OsString = std::env::join_paths(paths).expect("join shim PATH");
        let mut cmd = rhei_command(dir.join("home"));
        cmd.env("PATH", path)
            .arg("--state-machine")
            .arg(&machine)
            .arg("run")
            .arg(&plan)
            .args(["--no-tui", "--no-callbacks"]);
        let output = cmd.output().expect("run built-in shim");
        let run = CliRun::from(&output);
        if !run.status.success() {
            mismatches.push(format!("{agent} did not run: {}", run.stderr));
            continue;
        }
        let args = record_args(&dir, "work", "model");
        if !args.windows(expected.len()).any(|span| span == expected) {
            mismatches.push(format!("{agent} missing {expected:?} in {args:?}"));
            continue;
        }
        let effort_index = args.windows(expected.len()).position(|span| span == expected).unwrap();
        let accounting = match agent {
            "claude-code" => args.iter().position(|arg| arg == "--output-format"),
            "codex" => args.iter().position(|arg| arg == "--json"),
            "pi" => args.iter().position(|arg| arg == "--mode"),
            _ => None,
        };
        if accounting.is_some_and(|index| effort_index >= index) {
            mismatches.push(format!("{agent} effort follows accounting flags in {args:?}"));
        }
    }
    assert!(mismatches.is_empty(), "built-in effort argv mismatch:\n{}", mismatches.join("\n"));
}
