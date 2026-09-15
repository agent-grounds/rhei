//! Required artifacts are checked at real visits and execution identities.
//! §FS-rhei-plan-language.3.13 §FS-rhei-validate.4

use super::*;
use std::fs;
use std::path::Path;

fn required_settings(root: &Path, agent: &Path) {
    fs::create_dir_all(root.join(".agent-grounds/rhei")).unwrap();
    fs::write(root.join(".agent-grounds/rhei/settings.json"), format!(r#"{{
      "agents": {{"mock": {{"command": {}, "stdin_prompt": true, "timeout": "10s", "modes": {{"safe": []}}}}}},
      "models": {{
        "model-a": {{"provider": "mock", "model": "model-a", "default_agent": "mock"}},
        "model-b": {{"provider": "mock", "model": "model-b", "default_agent": "mock"}}
      }}
    }}"#, fixture_command(agent))).unwrap();
}

fn required_agent(dir: &Path) -> std::path::PathBuf {
    write_python_agent(
        dir,
        "capture.py",
        r#"write(pathlib.Path(env('RHEI_ROOT')) / 'runtime' / 'captured.md', agent_prompt())
result('Required artifact control completed.\n')
"#,
    )
}

/// The second visit and future permitted visits are both validated. Actual run
/// dispatch and next cannot accept the excluded required file either.
/// §FS-rhei-plan-language.3.13 §FS-rhei-next.3.1
#[test]
fn exclusions_required_inputs_resolve_all_visits_and_directory_overlap() {
    for state in ["review-1", "review-2"] {
        for (path, excluded, blocked) in [
            ("runtime/brief-{visit_count}.md", "runtime/brief-2.md", true),
            ("runtime/briefs/{visit_count}.md", "runtime/briefs/", true),
            ("runtime/brief-{visit_count}.md", "runtime/brief-3.md", false),
            ("runtime/briefs/{visit_count}.md", "runtime/briefs-copy/", false),
        ] {
            let plan = format!("# Rhei: Required visits\n\n## Tasks\n\n### Task 1: Review\n**State:** {state}\n**Excludes:** artifact={excluded}\n");
            let (dir, plan_path, machine) = setup_single_file("exclusions-required-visits", &plan);
            fs::write(&machine, format!("name: required-visits\nversion: 1\nstates:\n  review:\n    initial: true\n    visits: 2\n    agent: mock\n    inputs:\n      - {{ name: brief, path: '{path}' }}\n  completed: {{ final: true }}\ntransitions: [{{ from: review, to: completed }}]\n")).unwrap();
            let agent = required_agent(&dir);
            required_settings(&dir, &agent);
            for visit in [1, 2] {
                let file = dir.join(path.replace("{visit_count}", &visit.to_string()));
                fs::create_dir_all(file.parent().unwrap()).unwrap();
                fs::write(file, "required brief").unwrap();
            }
            for (command, args) in [
                ("validate", vec![]),
                ("next", vec!["--peek"]),
                ("run", vec!["--no-tui", "--no-callbacks", "--parallel", "2"]),
            ] {
                let output = run_cli(command, &plan_path, &machine, &args);
                if blocked {
                    assert!(!output.status.success(), "{command} accepted {excluded}");
                    assert!(output.stderr.contains("required input 'brief'"), "{}", output.stderr);
                    assert!(!dir.join("runtime/captured.md").exists());
                } else {
                    assert_success(&output);
                }
            }
        }
    }
}

/// Target overrides, model overrides, mode/provider/name placeholders, and
/// every configured fan-out target participate in required-input validation.
/// §FS-rhei-plan-language.3.13
#[test]
fn exclusions_required_inputs_resolve_task_and_fanout_identities() {
    for (selection, task_override, path, resolved) in [
        (
            "target: mock:mock:model-a",
            "**Target:** mock[safe]:mock:model-b\n",
            "runtime/{target.slug}/{model.provider}/{model.name}/{agent}/{agent.mode}/brief.md",
            "runtime/mock-safe-mock-model-b/mock/model-b/mock/safe/brief.md",
        ),
        (
            "agent: mock\n    model: model-a",
            "**Model:** model-b\n",
            "runtime/{model}/brief.md",
            "runtime/model-b/brief.md",
        ),
        (
            "all_targets: ['mock:mock:model-a', 'mock:mock:model-b']",
            "",
            "runtime/{target.slug}/brief.md",
            "runtime/mock-mock-model-b/brief.md",
        ),
    ] {
        for (exclusion, blocked) in [
            (resolved.to_string(), true),
            (resolved.rsplit_once('/').unwrap().0.to_string() + "/", true),
            ("runtime/non-conflicting/".to_string(), false),
        ] {
            let plan = format!("# Rhei: Required identity\n\n## Tasks\n\n### Task 1: Review\n**State:** review\n**Excludes:** artifact={exclusion}\n{task_override}");
            let (dir, plan_path, machine) =
                setup_single_file("exclusions-required-identity", &plan);
            fs::write(&machine, format!("name: required-identity\nversion: 1\nmodels: [model-a, model-b]\nstates:\n  review:\n    initial: true\n    {selection}\n    inputs:\n      - {{ name: brief, path: '{path}' }}\n  completed: {{ final: true }}\ntransitions: [{{ from: review, to: completed }}]\n")).unwrap();
            let agent = required_agent(&dir);
            required_settings(&dir, &agent);
            let output = run_cli("validate", &plan_path, &machine, &[]);
            if blocked {
                assert!(!output.status.success(), "accepted {exclusion}");
                assert!(output.stderr.contains("required input 'brief'"), "{}", output.stderr);
                assert!(output.stderr.contains(resolved), "{}", output.stderr);
            } else {
                assert_success(&output);
            }
        }
    }
}

/// Source visit and source target (not the successor's) determine handoff
/// paths, and the allowed control proves the same resolution composes content.
/// §FS-rhei-plan-language.3.13 §FS-rhei-states.3.2
#[test]
fn exclusions_required_handoffs_use_source_transition_identity_and_visit() {
    for (excluded, blocked) in [
        ("runtime/handoff/mock-mock-model-a/2.md", true),
        ("runtime/handoff/mock-mock-model-a/", true),
        ("runtime/handoff/mock-mock-model-b/", false),
    ] {
        let plan = format!("# Rhei: Source handoff\n\n---\nmetadata:\n  tasks:\n    '1':\n      stateVisits:\n        implement: 2\n---\n\n## Tasks\n\n### Task 1: Review\n**State:** review\n**Excludes:** artifact={excluded}\n");
        let (dir, plan_path, machine) = setup_single_file("exclusions-required-handoff", &plan);
        fs::write(
            &machine,
            r#"name: required-handoff-context
version: 1
states:
  implement:
    initial: true
    visits: 2
    target: mock:mock:model-a
    outputs:
      - { name: brief, kind: handoff, path: 'runtime/handoff/{target.slug}/{visit_count}.md' }
  review:
    target: mock:mock:model-b
    handoff:
      inherit: [{ from: transition.previous, required: true }]
  completed: { final: true }
transitions:
  - { from: implement, to: review }
  - { from: review, to: completed }
"#,
        )
        .unwrap();
        let agent = required_agent(&dir);
        required_settings(&dir, &agent);
        fs::create_dir_all(dir.join("runtime/handoff/mock-mock-model-a")).unwrap();
        fs::write(dir.join("runtime/handoff/mock-mock-model-a/2.md"), "SOURCE-VISIT-TWO-HANDOFF")
            .unwrap();
        fs::write(dir.join("runtime/state-transitions.log"), "plan.1 implement-2@review\n")
            .unwrap();
        let validate = run_cli("validate", &plan_path, &machine, &[]);
        let run = run_cli("run", &plan_path, &machine, &["--no-tui", "--no-callbacks"]);
        if blocked {
            for output in [validate, run] {
                assert!(!output.status.success());
                assert!(output.stderr.contains("required handoff 'brief'"), "{}", output.stderr);
            }
            assert!(!dir.join("runtime/captured.md").exists());
        } else {
            assert_success(&validate);
            assert_success(&run);
            assert!(fs::read_to_string(dir.join("runtime/captured.md"))
                .unwrap()
                .contains("SOURCE-VISIT-TWO-HANDOFF"));
        }
    }
}

/// Unlimited counted loops still reject future visits and existing aliases.
/// §FS-rhei-plan-language.3.13
#[cfg(unix)]
#[test]
fn exclusions_required_inputs_resolve_unbounded_visits_and_aliases() {
    for (excluded, blocked) in [
        ("runtime/brief-17.md", true),
        ("runtime/private.md", true),
        ("runtime/unrelated.md", false),
    ] {
        let plan = format!("# Rhei: Counted loop\n\n## Tasks\n\n### Task 1: Review\n**State:** review\n**Excludes:** artifact={excluded}\n");
        let (dir, plan_path, machine) = setup_single_file("exclusions-unbounded-visits", &plan);
        fs::write(
            &machine,
            r#"name: unbounded-input
version: 1
states:
  review:
    initial: true
    inputs: [{ name: brief, path: 'runtime/brief-{visit_count}.md' }]
  completed: { final: true }
transitions:
  - { from: review, to: review }
  - { from: review, to: completed }
"#,
        )
        .unwrap();
        fs::create_dir_all(dir.join("runtime")).unwrap();
        fs::write(dir.join("runtime/private.md"), "required via alias").unwrap();
        std::os::unix::fs::symlink(dir.join("runtime/private.md"), dir.join("runtime/brief-17.md"))
            .unwrap();
        let output = run_cli("validate", &plan_path, &machine, &[]);
        if blocked {
            assert!(!output.status.success());
            assert!(output.stderr.contains("required input 'brief'"), "{}", output.stderr);
            assert!(output.stderr.contains("visit 17"), "{}", output.stderr);
        } else {
            assert_success(&output);
        }
    }
}

/// A failed attempt changes a required input's alias before the retry; the
/// fresh policy must stop the second process from receiving a denied input.
/// §FS-rhei-agents.5.2.1 §FS-rhei-plan-language.3.13
#[cfg(unix)]
#[test]
fn exclusions_retry_rechecks_required_input_alias_after_first_attempt() {
    let plan = "# Rhei: Retry alias\n\n## Tasks\n\n### Task 1: Review\n**State:** review\n**Excludes:** artifact=runtime/private.md\n";
    let (dir, plan_path, machine) = setup_single_file("exclusions-retry-required", plan);
    fs::write(
        &machine,
        r#"name: retry-input
version: 1
states:
  review:
    initial: true
    agent: mock
    attempts: 2
    inputs: [{ name: brief, path: runtime/brief.md }]
  completed: { final: true }
transitions: [{ from: review, to: completed }]
"#,
    )
    .unwrap();
    fs::create_dir_all(dir.join("runtime")).unwrap();
    fs::write(dir.join("runtime/brief.md"), "allowed before first attempt").unwrap();
    fs::write(dir.join("runtime/private.md"), "excluded").unwrap();
    let agent = write_python_agent(
        &dir,
        "change-alias.py",
        r#"root = pathlib.Path(env('RHEI_ROOT'))
count = root / 'runtime' / 'spawn-count.txt'
write(count, count.read_text() + 'spawn\n' if count.exists() else 'spawn\n')
brief = root / 'runtime' / 'brief.md'
brief.unlink()
brief.symlink_to(root / 'runtime' / 'private.md')
sys.exit(9)
"#,
    );
    required_settings(&dir, &agent);
    let first = run_cli("run", &plan_path, &machine, &["--no-tui", "--no-callbacks"]);
    assert!(!first.status.success());
    assert_eq!(fs::read_to_string(dir.join("runtime/spawn-count.txt")).unwrap(), "spawn\n");
    let retry = run_cli("run", &plan_path, &machine, &["--no-tui", "--no-callbacks"]);
    assert!(!retry.status.success());
    assert!(retry.stderr.contains("required input 'brief'"), "{}", retry.stderr);
    assert_eq!(fs::read_to_string(dir.join("runtime/spawn-count.txt")).unwrap(), "spawn\n");
}
