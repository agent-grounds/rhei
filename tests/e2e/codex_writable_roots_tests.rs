//! A Codex launch under `workspace-write` is handed every root its work needs,
//! so it can write its artifacts, Rhei's own state, and a commit in the
//! checkout it runs in, without `danger-full-access` (agent-grounds/rhei#509).
//!
//! The plan and the checkout live in separate directories, and the plan's path
//! carries a space and a non-ASCII letter, so a root that was split, quoted or
//! re-spelled on its way to the agent is a root the shim does not see.
//! Workspace plans live beneath their fixture checkout, or explicitly select an owned
//! linked worktree, so ancestor Git discovery cannot select another repository.
//! §FS-rhei-agents.2.2.1 §REQ-test-isolation.4 §FS-rhei-agents.1.1.2

use std::fs;
use std::path::Path;

use super::codex_writable_roots_support::*;
use super::templates_tests::run_raw;
use super::*;

/// A `family: codex` wrapper and a plan beneath its owned checkout: the plan,
/// the checkout, its git dir and Rhei's state directory are all distinct roots.
/// §FS-rhei-agents.2.2.1
#[test]
fn a_codex_workspace_launch_writes_its_plan_state_and_commit_through_its_roots() {
    let dir = unique_temp_dir("codex-roots-plain");
    let Some(checkout) = git_repository(&dir.join("checkout")) else { return };
    let plan = checkout.join("plans dir ü").join("plan");
    write_plan_at(&plan, "Codex roots", ONE_TASK);
    let home = dir.join("home");
    let log = dir.join("shim.jsonl");
    let shim = write_roots_shim(&dir, &log, &home.join("state"), &[]);
    write_settings_at(
        &plan,
        serde_json::json!({ "cdx": cdx_profile(&shim, serde_json::json!({})) }),
    );
    let machine = write_fixture_file(&dir, "states.yaml", ROOTS_MACHINE);

    let run = run_plan(&home, &checkout, &machine, &plan);
    let records = shim_records(&log);
    let record = records.last().unwrap_or_else(|| {
        panic!("the fake Codex never ran\nstdout:\n{}\nstderr:\n{}", run.stdout, run.stderr)
    });
    let state_dir = home.join("state").join("rhei");
    assert_wrote_through_roots(
        record,
        &[&plan, &checkout, &checkout.join(".git"), &state_dir],
        &run,
    );
    let roots: Vec<String> = serde_json::from_value(record["roots"].clone()).unwrap();
    assert!(
        roots.iter().any(|root| root.contains("plans dir ü")),
        "the plan root's spelling must survive, space and ü included: {roots:?}"
    );
    assert!(git_ok(&checkout, &["log", "-1", "--format=%s"]).contains("roots probe"));
    assert!(fs::read_to_string(plan.join("tasks/01.md")).unwrap().contains("**State:** completed"));
}

/// In a linked worktree the checkout's git dir and the common dir differ, and a
/// commit there writes both. §FS-rhei-agents.2.2 §FS-rhei-agents.4
#[test]
fn a_codex_workspace_launch_commits_in_a_linked_worktree() {
    let dir = unique_temp_dir("codex-roots-worktree");
    let Some(main) = git_repository(&dir.join("main")) else { return };
    let worktree = dir.join("wt");
    git_ok(&main, &["worktree", "add", "-q", "-b", "roots-wt", &worktree.display().to_string()]);
    let plan = dir.join("plans dir ü").join("plan");
    write_plan_at(&plan, "Codex roots", ONE_TASK);
    let home = dir.join("home");
    let log = dir.join("shim.jsonl");
    let shim = write_roots_shim(&dir, &log, &home.join("state"), &[]);
    write_settings_at(
        &plan,
        serde_json::json!({ "cdx": cdx_profile(&shim, serde_json::json!({})) }),
    );
    let machine = write_fixture_file(&dir, "states.yaml", ROOTS_MACHINE);
    let task_id =
        render_json(&plan, &machine)["tasks"][0]["id"]["path"].as_str().unwrap().to_string();
    let refs = plan.join("runtime/worktree-refs");
    fs::create_dir_all(&refs).unwrap();
    let path = serde_json::to_string(&worktree.display().to_string()).unwrap();
    fs::write(refs.join(format!("{task_id}.yaml")), format!("path: {path}\n")).unwrap();

    let run = run_plan(&home, &dir, &machine, &plan);
    let records = shim_records(&log);
    let record = records.last().unwrap_or_else(|| {
        panic!("the fake Codex never ran\nstdout:\n{}\nstderr:\n{}", run.stdout, run.stderr)
    });
    let state_dir = home.join("state").join("rhei");
    let git_dir = main.join(".git").join("worktrees").join("wt");
    let common = main.join(".git");
    assert_wrote_through_roots(record, &[&plan, &worktree, &git_dir, &common, &state_dir], &run);
    assert!(git_ok(&worktree, &["log", "-1", "--format=%s"]).contains("roots probe"));
}

/// Settings hoisted to a Panta project still give the wrapper its roots, and
/// the project directory is one of them beside the member rhei.
/// §FS-rhei-agents.2.2.1 §FS-rhei-templates.4
#[test]
fn a_codex_workspace_launch_from_a_panta_member_grants_the_project_too() {
    let dir = unique_temp_dir("codex-roots-panta");
    let Some(checkout) = git_repository(&dir.join("checkout")) else { return };
    let project = checkout.join("plans dir ü").join("project");
    let member = project.join("alpha");
    write_plan_at(&member, "alpha", ONE_TASK);
    fs::write(project.join("index.panta.md"), "# Panta: Codex roots\n").unwrap();
    let home = dir.join("home");
    let log = dir.join("shim.jsonl");
    let shim = write_roots_shim(&dir, &log, &home.join("state"), &[]);
    write_settings_at(
        &project,
        serde_json::json!({ "cdx": cdx_profile(&shim, serde_json::json!({})) }),
    );
    let machine = write_fixture_file(&dir, "states.yaml", ROOTS_MACHINE);

    let run = run_plan(&home, &checkout, &machine, &member);
    let records = shim_records(&log);
    let record = records.last().unwrap_or_else(|| {
        panic!("the fake Codex never ran\nstdout:\n{}\nstderr:\n{}", run.stdout, run.stderr)
    });
    let state_dir = home.join("state").join("rhei");
    let expected = [&member, &project, &checkout, &checkout.join(".git"), &state_dir];
    assert_wrote_through_roots(record, &expected.map(|path| path.as_path()), &run);
}

/// A wrapper that writes its own `modes` replaces them whole but still inherits
/// `writable_roots`, and a grant it writes into a mode passes through as
/// written. §FS-rhei-agents.1.1.2 §FS-rhei-agents.2.2.1
#[test]
fn a_wrapper_that_writes_its_own_modes_still_inherits_its_roots() {
    let dir = unique_temp_dir("codex-roots-own-modes");
    let Some(checkout) = git_repository(&dir.join("checkout")) else { return };
    let cache = dir.join("cache ü");
    fs::create_dir_all(&cache).unwrap();
    let plan = checkout.join("plans dir ü").join("plan");
    write_plan_at(&plan, "Codex roots", ONE_TASK);
    let home = dir.join("home");
    let log = dir.join("shim.jsonl");
    let shim = write_roots_shim(&dir, &log, &home.join("state"), &[&cache]);
    let mut mode: Vec<String> = super::template_sandbox_sweep_support::WORKSPACE_FLAGS
        .iter()
        .map(|flag| (*flag).to_string())
        .collect();
    mode.extend(["--add-dir".to_string(), cache.display().to_string()]);
    let modes = serde_json::json!({ "modes": { "workspace": mode } });
    write_settings_at(&plan, serde_json::json!({ "cdx": cdx_profile(&shim, modes) }));
    let machine = write_fixture_file(&dir, "states.yaml", ROOTS_MACHINE);

    let run = run_plan(&home, &checkout, &machine, &plan);
    let records = shim_records(&log);
    let record = records.last().unwrap_or_else(|| {
        panic!("the fake Codex never ran\nstdout:\n{}\nstderr:\n{}", run.stdout, run.stderr)
    });
    let state_dir = home.join("state").join("rhei");
    assert_wrote_through_roots(
        record,
        &[&plan, &checkout, &checkout.join(".git"), &state_dir, &cache],
        &run,
    );
    let argv: Vec<String> = serde_json::from_value(record["argv"].clone()).unwrap();
    let cache = cache.display().to_string();
    assert_eq!(argv.iter().filter(|arg| **arg == cache).count(), 1, "{argv:?}");
}

/// `changeset-review` composes `codex` from `code-review` and `fix-codex` from
/// `fix`. Launched through the selectors and modes it ships, with only the
/// binary swapped, each writes through its roots. §FS-rhei-templates.4.1
/// §FS-rhei-agents.2.2.1
#[test]
fn changeset_review_composed_profiles_write_through_their_roots() {
    let dir = unique_temp_dir("codex-roots-changeset");
    let Some(checkout) = git_repository(&dir.join("checkout")) else { return };
    let source = repo_root().join("crates/rhei-cli/templates/changeset-review");
    let laid = run_raw(
        &["instantiate", &source.display().to_string(), "change_ref=HEAD", "--output", "laid"],
        &dir,
    );
    assert_success(&laid);
    let laid = dir.join("laid");
    let settings_text = fs::read_to_string(laid.join(".agent-grounds/rhei/settings.json")).unwrap();
    let settings: serde_json::Value = serde_json::from_str(&settings_text).unwrap();
    // YAML keeps the file's key order, so this is the first mode `fix-codex` declares.
    let ordered: serde_yaml::Value = serde_yaml::from_str(&settings_text).unwrap();
    let fix_mode = ordered["agents"]["fix-codex"]["modes"]
        .as_mapping()
        .and_then(|modes| modes.keys().next())
        .and_then(|key| key.as_str())
        .expect("the composed fix-codex declares a mode")
        .to_string();
    let selectors = [
        ("review", composed_selector(&laid, "review", "codex[")),
        ("final-fix", composed_selector(&laid, "final-fix", "")),
        ("fix-codex", format!("fix-codex[{fix_mode}]:openai:gpt-5.5")),
    ];

    let plan = checkout.join("plans dir ü").join("plan");
    write_plan_at(&plan, "Composed roots", "### Task 1: Review and fix\n**State:** review\n");
    let home = dir.join("home");
    let log = dir.join("shim.jsonl");
    let shim = write_roots_shim(&dir, &log, &home.join("state"), &[]);
    let command: serde_json::Value = serde_json::from_str(&fixture_command(&shim)).unwrap();
    let mut agents = settings["agents"].clone();
    for agent in ["codex", "fix-codex"] {
        assert!(agents[agent].is_object(), "changeset-review composes {agent}: {settings:#}");
        agents[agent]["command"] = command.clone();
        agents[agent]["timeout"] = serde_json::json!("60s");
    }
    write_settings_at(&plan, agents);
    let mut machine = String::from("name: composed-roots\nversion: 1\nstates:\n");
    let mut transitions = String::from("transitions:\n");
    for (index, (state, selector)) in selectors.iter().enumerate() {
        let initial = if index == 0 { "    initial: true\n" } else { "" };
        machine.push_str(&format!(
            "  {state}:\n{initial}    target: {selector:?}\n    agent_timeout: 60s\n    outputs:\n      \
             - name: {state}\n        path: runtime/exports/{{task_id}}/{state}.md\n"
        ));
        let next = selectors.get(index + 1).map_or("completed", |(next, _)| next);
        transitions.push_str(&format!("  - from: {state}\n    to: {next}\n"));
    }
    machine.push_str("  completed:\n    final: true\n");
    machine.push_str(&transitions);
    let machine = write_fixture_file(&dir, "states.yaml", &machine);

    let run = run_plan(&home, &checkout, &machine, &plan);
    let records = shim_records(&log);
    let state_dir = home.join("state").join("rhei");
    let expected = [&plan, &checkout, &checkout.join(".git"), &state_dir];
    for (state, selector) in &selectors {
        let record = records.iter().find(|record| record["state"] == *state).unwrap_or_else(|| {
            panic!(
                "the fake Codex never ran for {state} ({selector}); records={records:#?}\nstdout:\n{}\nstderr:\n{}",
                run.stdout, run.stderr
            )
        });
        assert_wrote_through_roots(record, &expected.map(|path| path.as_path()), &run);
    }
}

/// The first selector a laid `changeset-review` state launches whose agent
/// starts with `prefix`.
fn composed_selector(laid: &Path, state: &str, prefix: &str) -> String {
    let text = fs::read_to_string(laid.join("states.yaml")).unwrap();
    let doc: serde_yaml::Value = serde_yaml::from_str(&text).unwrap();
    let state = &doc["states"][state];
    let mut selectors: Vec<String> =
        state["target"].as_str().map(str::to_string).into_iter().collect();
    if let Some(all) = state["all_targets"].as_sequence() {
        selectors.extend(all.iter().filter_map(|t| t.as_str().map(str::to_string)));
    }
    selectors
        .into_iter()
        .find(|selector| selector.starts_with(prefix))
        .unwrap_or_else(|| panic!("no {prefix} selector on the laid {state:?}"))
}

/// A state naming a `family: codex` wrapper and no mode still runs `yolo`, the
/// built-in's first mode, so `workspace` changes no existing plan. A guard.
/// §FS-rhei-agents.2 §FS-rhei-agents.1.4.1
#[test]
fn a_codex_launch_with_no_mode_still_runs_yolo() {
    let dir = unique_temp_dir("codex-roots-first-mode");
    let plan = dir.join("plan");
    write_plan_at(&plan, "Codex first mode", ONE_TASK);
    let home = dir.join("home");
    let log = dir.join("shim.jsonl");
    let shim = write_roots_shim(&dir, &log, &home.join("state"), &[]);
    write_settings_at(
        &plan,
        serde_json::json!({ "cdx": cdx_profile(&shim, serde_json::json!({})) }),
    );
    let machine = ROOTS_MACHINE.replace("target: cdx[workspace]:openai:fixture", "agent: cdx");
    let machine = write_fixture_file(&dir, "states.yaml", &machine);

    let run = run_plan(&home, &dir, &machine, &plan);
    let records = shim_records(&log);
    let record = records.last().unwrap_or_else(|| {
        panic!("the fake Codex never ran\nstdout:\n{}\nstderr:\n{}", run.stdout, run.stderr)
    });
    let argv: Vec<String> = serde_json::from_value(record["argv"].clone()).unwrap();
    let yolo = [
        "--sandbox",
        "danger-full-access",
        "--skip-git-repo-check",
        "-c",
        "approval_policy=\"never\"",
    ];
    assert!(argv.windows(yolo.len()).any(|window| window == yolo), "{argv:?}");
}

/// The shim keeps a root's `.git` read-only unless that exact path is granted,
/// so a launch that drops the git-dir grant cannot pass by accident. A guard
/// on the fixture itself. §FS-rhei-agents.2.2
#[test]
fn the_fake_codex_refuses_a_commit_without_the_git_dir_grant() {
    let dir = unique_temp_dir("codex-roots-shim-check");
    let Some(checkout) = git_repository(&dir.join("checkout")) else { return };
    let log = dir.join("shim.jsonl");
    let state = dir.join("state");
    let shim = write_roots_shim(&dir, &log, &state, &[]);
    let probe = |extra: &[String]| {
        let mut args = vec!["--add-dir".to_string(), checkout.display().to_string()];
        args.extend(extra.iter().cloned());
        args.extend(["--sandbox", "workspace-write", "--"].map(str::to_string));
        let mut command = std::process::Command::new(python_command());
        // The shim's own git must act on the fixture, never on an inherited
        // repository. §REQ-test-isolation.3
        super::git_env::scrub_git_repository_env(&mut command);
        let status = command
            .arg(&shim)
            .args(&args)
            .current_dir(&checkout)
            .env_remove("RHEI_ATTEMPT")
            .status()
            .expect("run the shim");
        assert!(status.success());
        shim_records(&log).pop().expect("the shim logged")
    };
    let without = probe(&[]);
    assert_eq!(without["committed"], false, "{without:#}");
    let git_dir = checkout.join(".git").display().to_string();
    let with = probe(&["--add-dir".to_string(), git_dir]);
    assert_eq!(with["committed"], true, "{with:#}");
}
