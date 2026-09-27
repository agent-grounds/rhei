use std::fs;

use super::*;

#[test]
fn next_auto_discovers_sibling_state_machine_from_states_declaration() {
    let plan = r#"# Rhei: Auto-discovered Machine
**States:** custom-review

## Tasks

### Task 1: Review API surface
**State:** draft
Inspect public interfaces.
"#;
    let machine = r#"name: custom-review
version: 1
states:
  draft:
    initial: true
    description: Planned
  review:
    description: Review in progress
    instructions: Follow the custom review workflow.
  completed:
    final: true
    description: Done
transitions:
  - from: draft
    to: review
  - from: review
    to: completed
"#;

    let dir = unique_temp_dir("next-auto-states");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli_without_machine("next", &plan_path, &["--no-callbacks", "--json"]);
    assert_success(&result);

    let json: serde_json::Value = serde_json::from_str(&result.stdout).expect("next JSON");
    assert_eq!(json["task_id"], "plan.1");
    assert_eq!(json["state"], "review");
    assert_eq!(json["from_state"], "draft");
    assert!(
        json["instructions"]
            .as_str()
            .expect("instructions string")
            .contains("Follow the custom review workflow."),
        "expected custom machine instructions; got:\n{}",
        result.stdout
    );
}

#[test]
fn next_omitted_states_uses_builtin_even_with_sibling_states_yaml() {
    let plan = r#"# Rhei: Builtin Default

## Tasks

### Task 1: Use builtin machine
**State:** pending
"#;
    let sibling_machine = r#"name: rhei
version: 1
states:
  draft:
    initial: true
    description: Planned
  review:
    description: Custom review
    instructions: This sibling machine should be ignored.
  completed:
    final: true
    description: Done
transitions:
  - from: draft
    to: review
  - from: review
    to: completed
"#;

    let dir = unique_temp_dir("next-omitted-states-built-in");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    write_fixture_file(&dir, "states.yaml", sibling_machine);

    let result = run_cli_without_machine("next", &plan_path, &["--no-callbacks", "--json"]);
    assert_success(&result);

    let json: serde_json::Value = serde_json::from_str(&result.stdout).expect("next JSON");
    assert_eq!(json["task_id"], "plan.1");
    assert_eq!(json["state"], "pending");
    assert_eq!(json["from_state"], "pending");
    assert_eq!(json["instructions"], "Do the task.");
    let content = fs::read_to_string(&plan_path).expect("read claimed plan");
    assert!(
        content.contains("**State:** pending\n**Assignee:** manual"),
        "expected durable manual claim; got:\n{}",
        content
    );

    let second = run_cli_without_machine("next", &plan_path, &["--no-callbacks", "--json"]);
    assert!(!second.status.success(), "second next should not reclaim manually assigned task");
    assert!(
        second.stderr.contains("Task plan.1 (pending, assignee manual)"),
        "expected assigned task diagnostic; got:\n{}",
        second.stderr
    );
}

/// Drive a plan to completion using `next` (to claim from initial state)
/// followed by `complete` (to finish the task). This simulates the agent
/// workflow: orchestrator calls `next`, agent does work, agent calls `complete`.
fn drive_to_completion_via_next(plan_path: &std::path::Path, machine_path: &std::path::Path) {
    loop {
        let next_result = run_cli("next", plan_path, machine_path, &["--no-callbacks", "--json"]);
        if !next_result.status.success() {
            break;
        }

        let json: serde_json::Value = serde_json::from_str(&next_result.stdout).expect("next JSON");
        let task_id = json["task_id"].as_str().expect("task_id field");

        let complete_result = run_cli(
            "complete",
            plan_path,
            machine_path,
            &["--task", task_id, "--result", "done", "--no-callbacks"],
        );
        assert_success(&complete_result);
    }
}

#[test]
fn next_single_file_repeated_to_completion() {
    let (_dir, plan_path, machine_path) = setup_single_file("next-repeat", LINEAR_PLAN);

    drive_to_completion_via_next(&plan_path, &machine_path);

    assert_all_tasks_in_state(&plan_path, &machine_path, "completed");

    // Verify next now fails.
    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks"]);
    assert!(!result.status.success(), "next should fail when all tasks are completed");
    assert!(
        result.stderr.contains("Plan complete. All 3 task(s) are in terminal states."),
        "expected plan-complete diagnostic; got:\n{}",
        result.stderr
    );
}

#[test]
fn next_single_file_json_output() {
    let plan = r#"# Rhei: JSON Output Test

## Tasks

### Task 1: Setup environment
**State:** draft
Configure the build system.
"#;

    let (_dir, plan_path, machine_path) = setup_single_file("next-json", plan);

    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--json"]);
    assert_success(&result);

    let json: serde_json::Value =
        serde_json::from_str(&result.stdout).expect("stdout should be valid JSON");

    assert_eq!(json["task_id"], "plan.1");
    assert_eq!(json["title"], "Setup environment");
    assert_eq!(json["from_state"], "draft");
    assert_eq!(json["state"], "pending");
    assert!(json["personality"].is_null());
    assert!(json["instructions"].is_string());
    assert!(json["content"].is_string());
    assert!(json["children"].is_array());
}

#[test]
fn next_prints_state_personality_in_text_and_json_when_configured() {
    let plan = r#"# Rhei: Personality Output Test

## Tasks

### Task 1: Teach concurrency
**State:** draft
Explain lock-free tradeoffs.
"#;
    let machine = r#"name: professor-demo
version: 1
states:
  draft:
    initial: true
    description: Planning
    instructions: Analyze first.
  pending:
    description: Ready
    personality: You are an MIT professor.
    instructions: Teach clearly.
  done:
    description: Done
    final: true
transitions:
  - from: draft
    to: pending
  - from: pending
    to: done
"#;

    let dir = unique_temp_dir("next-personality");
    let text_plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let json_plan_path = write_fixture_file(&dir, "plan-json.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let text_result =
        run_cli("next", &text_plan_path, &machine_path, &["--no-callbacks", "--task", "1"]);
    assert_success(&text_result);
    assert!(
        text_result.stdout.contains("Personality: You are an MIT professor."),
        "expected personality in text output; got:\n{}",
        text_result.stdout
    );
    assert!(
        text_result.stdout.contains("## Task plan.1: Teach concurrency"),
        "expected task heading in text output; got:\n{}",
        text_result.stdout
    );

    let json_result = run_cli(
        "next",
        &json_plan_path,
        &machine_path,
        &["--no-callbacks", "--task", "1", "--json"],
    );
    assert_success(&json_result);
    let json: serde_json::Value = serde_json::from_str(&json_result.stdout).expect("parse JSON");
    assert_eq!(json["personality"], "You are an MIT professor.");
    assert_eq!(json["task_id"], "plan-json.1");
}

#[test]
fn next_resolves_runtime_template_variables_in_instructions() {
    let plan = r#"# Rhei: Template Resolution

## Tasks

### Task 1: Review cache layer
**State:** draft
Check the implementation carefully.
"#;
    let machine = r#"name: template-demo
version: 1
states:
  draft:
    initial: true
    description: Planned
  review:
    description: Review work
    visits: 2
    instructions: |
      Review pass {visit_count} of {visits} for Task {task_id}: {task_title}.
      Plan: {plan_title}
      Write findings to {output.findings.path}.
    outputs:
      - name: findings
        path: runtime/findings/{task_id}-{visit_count}.md
  done:
    description: Done
    final: true
transitions:
  - from: draft
    to: review
  - from: review
    to: done
"#;

    let dir = unique_temp_dir("next-template-vars");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--task", "1"]);
    assert_success(&result);
    assert!(
        result.stdout.contains("Review pass 1 of 2 for Task plan.1: Review cache layer."),
        "expected visit/task placeholders to resolve; got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("Plan: Template Resolution"),
        "expected plan_title placeholder to resolve; got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("runtime/findings/plan.1-1.md"),
        "expected output artifact path placeholder to resolve; got:\n{}",
        result.stdout
    );
}

#[test]
fn next_does_not_auto_transition_runnable_initial_states() {
    let plan = r#"# Rhei: Runnable Initial State

## Tasks

### Task coordinate: Coordinate review
**State:** split
Review the change and write the manifest.
"#;
    let machine = r#"name: runnable-initial
version: 1
models:
  - codex
states:
  split:
    initial: true
    description: Coordinator
    model: codex
    instructions: |
      Write the overview to `{output.overview.path}`.
    outputs:
      - name: overview
        path: runtime/manifests/coordinate.md
  completed:
    final: true
    description: Done
transitions:
  - from: split
    to: completed
"#;

    let dir = unique_temp_dir("next-runnable-initial");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--json"]);
    assert_success(&result);

    let json: serde_json::Value = serde_json::from_str(&result.stdout).expect("next JSON");
    assert_eq!(json["task_id"], "plan.coordinate");
    assert_eq!(json["from_state"], "split");
    assert_eq!(json["state"], "split");
    assert!(
        json["instructions"]
            .as_str()
            .expect("instructions string")
            .contains("runtime/manifests/coordinate.md"),
        "expected runnable-state instructions; got:\n{}",
        result.stdout
    );
    assert_task_state(&plan_path, &machine_path, "coordinate", "split");
}

#[test]
fn next_writes_codex_assignee_and_complete_removes_it() {
    let plan = r#"# Rhei: Codex Claim

## Tasks

### Task 1: Implement claim
**State:** draft
"#;
    let machine = r#"name: codex-claim
version: 1
states:
  draft:
    initial: true
    description: Planned
  pending:
    description: Ready
    agent: codex
    instructions: Implement the task.
  completed:
    final: true
    description: Done
transitions:
  - from: draft
    to: pending
  - from: pending
    to: completed
"#;

    let dir = unique_temp_dir("next-codex-assignee");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--json"]);
    assert_success(&result);
    let json: serde_json::Value = serde_json::from_str(&result.stdout).expect("next JSON");
    assert_eq!(json["task_id"], "plan.1");
    assert_eq!(json["agent"], "codex");

    let claimed = fs::read_to_string(&plan_path).expect("read claimed plan");
    assert!(
        claimed.contains("**State:** pending\n**Assignee:** codex"),
        "expected codex assignee after state; got:\n{claimed}"
    );

    let duplicate = run_cli("next", &plan_path, &machine_path, &["--no-callbacks"]);
    assert!(!duplicate.status.success(), "assigned task should not be claimable again");
    assert_stderr_contains(&duplicate, "currently in progress");
    assert_stderr_contains(&duplicate, "Task plan.1 (pending, assignee codex)");

    let complete = run_cli(
        "complete",
        &plan_path,
        &machine_path,
        &["--task", "1", "--result", "done", "--no-callbacks"],
    );
    assert_success(&complete);
    let completed = fs::read_to_string(&plan_path).expect("read completed plan");
    assert!(
        !completed.contains("**Assignee:**"),
        "complete should remove assignee; got:\n{completed}"
    );
}

#[test]
fn next_task_rejects_already_assigned_task() {
    let plan = r#"# Rhei: Assigned Target

## Tasks

### Task 1: Claimed
**State:** draft
**Assignee:** codex
"#;

    let (_dir, plan_path, machine_path) = setup_single_file("next-assigned-target", plan);

    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--task", "1"]);
    assert!(!result.status.success(), "assigned task should be rejected");
    assert!(
        result.stderr.contains("Task plan.1 is already assigned to codex"),
        "expected assigned-task error; got:\n{}",
        result.stderr
    );
    let unchanged = fs::read_to_string(&plan_path).expect("read plan");
    assert!(unchanged.contains("**State:** draft\n**Assignee:** codex"));
}

#[test]
fn next_peek_does_not_write_assignee() {
    let plan = r#"# Rhei: Peek Claim

## Tasks

### Task 1: Inspect
**State:** draft
"#;
    let machine = r#"name: peek-claim
version: 1
states:
  draft:
    initial: true
    description: Runnable
    agent: codex
    instructions: Inspect only.
  completed:
    final: true
    description: Done
transitions:
  - from: draft
    to: completed
"#;

    let dir = unique_temp_dir("next-peek-assignee");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--peek"]);
    assert_success(&result);
    let content = fs::read_to_string(&plan_path).expect("read plan");
    assert!(!content.contains("**Assignee:**"), "peek must not write assignee; got:\n{content}");
    assert_task_state(&plan_path, &machine_path, "1", "draft");
}

#[test]
fn next_no_claimable_mid_workflow_lists_transition_commands() {
    let plan = r#"# Rhei: Mid Workflow

## Tasks

### Task 1: Already pending
**State:** pending
"#;

    let (_dir, plan_path, machine_path) = setup_single_file("next-mid-workflow", plan);

    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--peek"]);
    let stderr = &result.stderr;
    assert!(!result.status.success(), "mid-workflow task should require explicit transition");
    assert!(
        stderr.contains("Task plan.1 is mid-workflow in state 'pending'"),
        "expected mid-workflow diagnostic; got:\n{}",
        result.stderr
    );
    assert!(
        stderr.contains("Available transitions:"),
        "expected transition command list; got:\n{}",
        result.stderr
    );
    assert!(
        stderr.contains("--task plan.1 --from=pending --to=completed"),
        "expected concrete completed transition command; got:\n{}",
        result.stderr
    );
    assert!(
        stderr.contains("--task plan.1 --from=pending --to=cancelled"),
        "expected concrete cancelled transition command; got:\n{}",
        result.stderr
    );
}

#[test]
fn next_no_claimable_mid_workflow_quotes_custom_transition_command() {
    let plan = r#"# Rhei: Mid Workflow Custom Machine

## Tasks

### Task 1: Already in progress
**State:** `in progress`
"#;
    let machine = r#"name: custom-space-machine
version: 1
states:
  "in progress":
    description: Already underway
  "done now":
    final: true
    description: Done
transitions:
  - from: "in progress"
    to: "done now"
"#;

    let dir = unique_temp_dir("next-mid-workflow-custom-machine");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "custom states.yaml", machine);

    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--peek"]);
    let stderr = &result.stderr;
    assert!(!result.status.success(), "mid-workflow task should require explicit transition");
    assert!(
        stderr.contains("Task plan.1 is mid-workflow in state 'in progress'"),
        "expected mid-workflow diagnostic; got:\n{}",
        result.stderr
    );
    assert!(
        stderr.contains(&format!(
            "rhei --state-machine={} transition",
            shell_quote(&machine_path.display().to_string())
        )),
        "expected suggested command to include quoted custom state-machine path; got:\n{}",
        result.stderr
    );
    assert!(
        stderr.contains(&format!(
            "--from={} --to={}",
            shell_quote("in progress"),
            shell_quote("done now")
        )),
        "expected suggested command to quote state names; got:\n{}",
        result.stderr
    );
}

#[test]
fn next_no_claimable_mid_workflow_only_lists_applicable_transitions() {
    let plan = r#"# Rhei: Mid Workflow Conditional

## Tasks

### Task 1: Fix review findings
**State:** fix
"#;
    let machine = r#"name: conditional-mid-workflow
version: 1
states:
  draft:
    initial: true
    description: Draft
  fix:
    description: Fix findings
    visits: 2
  completed:
    final: true
    description: Done
transitions:
  - from: draft
    to: fix
  - from: fix
    to: fix
    condition: visitCount < visits
  - from: fix
    to: completed
    condition: visitCount >= visits
"#;

    let dir = unique_temp_dir("next-mid-workflow-conditional");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--peek"]);
    let stderr = &result.stderr;
    assert!(!result.status.success(), "mid-workflow task should require explicit transition");
    assert!(
        stderr.contains("--task plan.1 --from=fix --to=fix"),
        "expected applicable self-loop transition command; got:\n{}",
        result.stderr
    );
    assert!(
        !stderr.contains("--to=completed"),
        "blocked completed transition should not be suggested; got:\n{}",
        result.stderr
    );
}

#[test]
fn next_no_claimable_mid_workflow_uses_equals_for_hyphen_leading_states() {
    let plan = r#"# Rhei: Mid Workflow Hyphen States

## Tasks

### Task 1: Already fixing
**State:** --fix
"#;
    let machine = r#"name: hyphen-state-machine
version: 1
states:
  "--fix":
    description: Fixing
  "--done":
    final: true
    description: Done
transitions:
  - from: "--fix"
    to: "--done"
"#;

    let dir = unique_temp_dir("next-mid-workflow-hyphen-state");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--peek"]);
    let stderr = &result.stderr;
    assert!(!result.status.success(), "mid-workflow task should require explicit transition");
    assert!(
        stderr.contains("--from=--fix --to=--done"),
        "expected equals-form option values for leading-hyphen states; got:\n{}",
        result.stderr
    );
}

#[test]
fn next_no_claimable_gating_state_reports_human_action() {
    let plan = r#"# Rhei: Human Gate

## Tasks

### Task 1: Approve rollout
**State:** human-review
"#;
    let machine = r#"name: human-gate
version: 1
states:
  draft:
    initial: true
    description: Draft
  human-review:
    description: Human approval
    gating: true
  completed:
    final: true
    description: Done
transitions:
  - from: draft
    to: human-review
  - from: human-review
    to: completed
"#;

    let dir = unique_temp_dir("next-human-gate");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks"]);
    assert!(!result.status.success(), "human-gated task should not be claimable");
    assert_stderr_contains(
        &result,
        "Blocked: 1 task(s) waiting on human action: Task plan.1 (human-review).",
    );
}

#[test]
fn next_workspace_writes_assignee_to_task_file() {
    let index = "# Rhei: Workspace Codex Claim\n";
    let machine = r#"name: workspace-codex-claim
version: 1
states:
  draft:
    initial: true
    description: Planned
  pending:
    description: Ready
    agent: codex
    instructions: Implement the task.
  completed:
    final: true
    description: Done
transitions:
  - from: draft
    to: pending
  - from: pending
    to: completed
"#;
    let dir = unique_temp_dir("next-ws-codex-assignee");
    let ws = dir.join("workspace");
    let tasks_dir = ws.join("tasks");
    fs::create_dir_all(&tasks_dir).expect("create workspace dirs");
    fs::write(ws.join("index.rhei.md"), index).expect("write index");
    let task_file = tasks_dir.join("one.md");
    fs::write(&task_file, "### Task 1: Claim me\n**State:** draft\n").expect("write task");
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli("next", &ws, &machine_path, &["--no-callbacks"]);
    assert_success(&result);

    let task_content = fs::read_to_string(&task_file).expect("read task file");
    assert!(
        task_content.contains("**State:** pending\n**Assignee:** codex"),
        "expected assignee in task file; got:\n{task_content}"
    );
    let index_content = fs::read_to_string(ws.join("index.rhei.md")).expect("read index");
    assert!(
        !index_content.contains("**Assignee:**"),
        "workspace index must not receive task assignee; got:\n{index_content}"
    );
}

#[test]
fn complete_custom_node_removes_assignee_and_links_result() {
    let plan = r#"# Rhei: Custom Completion
---
structure:
  nodeKinds: [task, bug]
---

## Tasks

### Bug cache-key: Fix cache
**State:** pending
**Assignee:** codex
"#;
    let machine = r#"name: custom-completion
version: 1
states:
  pending:
    description: Ready
  completed:
    final: true
    description: Done
transitions:
  - from: pending
    to: completed
"#;

    let dir = unique_temp_dir("complete-custom-node");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let complete = run_cli(
        "complete",
        &plan_path,
        &machine_path,
        &["--task", "cache-key", "--result", "done", "--no-callbacks"],
    );
    assert_success(&complete);
    let content = fs::read_to_string(&plan_path).expect("read plan");
    assert!(
        content.contains("### Bug cache-key: Fix cache\n**State:** completed"),
        "custom node should complete; got:\n{content}"
    );
    assert!(
        !content.contains("**Assignee:**"),
        "custom node assignee should be removed; got:\n{content}"
    );
    assert!(
        content.contains("> **Result:** [plan.cache-key](runtime/results/plan.cache-key.md)"),
        "custom node result link should be inserted; got:\n{content}"
    );
}

#[test]
fn next_task_can_claim_child_task_with_assignee() {
    let plan = r#"# Rhei: Child Codex Claim

## Tasks

### Task 1: Parent
**State:** draft

#### Task 1.1: Child
**State:** draft
"#;
    let machine = r#"name: child-codex-claim
version: 1
states:
  draft:
    initial: true
    description: Planned
  pending:
    description: Ready
    agent: codex
    instructions: Implement the child task.
  completed:
    final: true
    description: Done
transitions:
  - from: draft
    to: pending
  - from: pending
    to: completed
"#;

    let dir = unique_temp_dir("next-child-codex-assignee");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--task", "1.1"]);
    assert_success(&result);
    let content = fs::read_to_string(&plan_path).expect("read plan");
    assert!(
        content.contains("### Task 1: Parent\n**State:** draft"),
        "parent state should be unchanged; got:\n{content}"
    );
    assert!(
        content.contains("#### Task 1.1: Child\n**State:** pending\n**Assignee:** codex"),
        "child task should be claimed; got:\n{content}"
    );

    let complete = run_cli(
        "complete",
        &plan_path,
        &machine_path,
        &["--no-callbacks", "--task", "1.1", "--result", "done"],
    );
    assert_success(&complete);
    let content = fs::read_to_string(&plan_path).expect("read completed plan");
    assert!(
        content.contains("#### Task 1.1: Child\n**State:** completed"),
        "child task should complete; got:\n{content}"
    );
    assert!(
        !content.contains("**Assignee:** codex"),
        "child assignee should be removed; got:\n{content}"
    );
    assert!(
        content.contains("> **Result:** [plan.1.1](runtime/results/plan.1.1.md)"),
        "child result link should be inserted; got:\n{content}"
    );
}

#[test]
fn next_respects_dependency_order() {
    let (_dir, plan_path, machine_path) = setup_single_file("next-deps", LINEAR_PLAN);

    // First next: should advance Task 1 (draft -> pending) since it has no deps.
    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--json"]);
    assert_success(&result);

    let json: serde_json::Value = serde_json::from_str(&result.stdout).expect("parse JSON");
    assert_eq!(json["task_id"], "plan.1", "first next should pick Task 1");
    assert_task_state(&plan_path, &machine_path, "1", "pending");
    assert_task_state(&plan_path, &machine_path, "2", "draft");
    assert_task_state(&plan_path, &machine_path, "3", "draft");

    // Second next: Task 1 is already claimed and Task 2 is still blocked.
    // Auto-pick mode should not collide with already-claimed work.
    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--json"]);
    assert!(!result.status.success(), "no new task should be claimable");
    assert!(
        result.stderr.contains("No tasks can be auto-claimed")
            || result.stderr.contains("no tasks are ready")
            || result.stderr.contains("currently in progress"),
        "expected no-claimable diagnostic; got:\n{}",
        result.stderr
    );

    // Complete Task 1 so Task 2 becomes ready.
    let r = run_cli(
        "complete",
        &plan_path,
        &machine_path,
        &["--task", "1", "--result", "done", "--no-callbacks"],
    );
    assert_success(&r);
    assert_task_state(&plan_path, &machine_path, "1", "completed");

    // Now next should pick Task 2.
    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--json"]);
    assert_success(&result);
    let json: serde_json::Value = serde_json::from_str(&result.stdout).expect("parse JSON");
    assert_eq!(json["task_id"], "plan.2", "after completing Task 1, next picks Task 2");
}

#[test]
fn next_workspace_repeated_to_completion() {
    let (_dir, ws, machine_path) = create_workspace(
        "next-ws-repeat",
        "# Rhei: Workspace Next\n",
        &[
            ("a.md", "### Task 1: First\n**State:** draft\n"),
            ("b.md", "### Task 2: Second\n**State:** draft\n**Prior:** Task 1\n"),
            ("c.md", "### Task 3: Third\n**State:** draft\n**Prior:** Task 2\n"),
        ],
    );

    drive_to_completion_via_next(&ws, &machine_path);

    assert_all_tasks_in_state(&ws, &machine_path, "completed");

    // Verify next fails now.
    let result = run_cli("next", &ws, &machine_path, &["--no-callbacks"]);
    assert!(!result.status.success());
}

#[test]
fn next_with_task_flag_targets_specific() {
    let (_dir, plan_path, machine_path) = setup_single_file("next-task-flag", INDEPENDENT_PLAN);

    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--task", "2"]);
    assert_success(&result);

    // Task 2 should be advanced, Tasks 1 and 3 remain draft.
    assert_task_state(&plan_path, &machine_path, "2", "pending");
    assert_task_state(&plan_path, &machine_path, "1", "draft");
    assert_task_state(&plan_path, &machine_path, "3", "draft");
}

/// Selection reaches the children first: the parent is a task too, but it is
/// not workable while its own subtree is open.
// §FS-rhei-next.3
#[test]
fn next_auto_claims_a_child_before_its_parent() {
    let (_dir, plan_path, machine_path) = setup_single_file("next-children", SUBTASK_PLAN);

    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--json"]);
    assert_success(&result);

    let json: serde_json::Value = serde_json::from_str(&result.stdout).expect("parse JSON");
    assert_eq!(json["task_id"], "plan.1.1");
    let children = json["children"].as_array().expect("children should be array");
    assert!(children.is_empty(), "claimed leaf task should not expose child tasks");
    let content = fs::read_to_string(&plan_path).expect("read plan");
    assert!(content.contains("### Task 1: Parent task\n**State:** draft"));
    assert!(content.contains("#### Task 1.1: First subtask\n**State:** pending"));
}

pub const PARENT_WITH_ONE_OPEN_CHILD: &str = r#"# Rhei: Parent Eligibility

## Tasks

### Task 1: Parent task
**State:** draft

#### Task 1.1: Finished subtask
**State:** completed

#### Task 1.2: Open subtask
**State:** draft

### Task 2: Dependent
**State:** draft
**Prior:** Task 1
"#;

pub const PARENT_WITH_TERMINAL_SUBTREE: &str = r#"# Rhei: Parent Eligibility

## Tasks

### Task 1: Parent task
**State:** draft

#### Task 1.1: Finished subtask
**State:** completed

#### Task 1.2: Also finished
**State:** completed

### Task 2: Dependent
**State:** draft
**Prior:** Task 1
"#;

/// A parent whose subtree is terminal is the next thing selection hands out —
/// mid-plan, without waiting for unrelated branches, and without any cascade
/// stamping it on its children's behalf.
// §FS-rhei-next.3
#[test]
fn next_auto_claims_a_parent_once_its_subtree_is_terminal() {
    let (_dir, plan_path, machine_path) =
        setup_single_file("next-parent-claimable", PARENT_WITH_TERMINAL_SUBTREE);

    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--json"]);
    assert_success(&result);

    let json: serde_json::Value = serde_json::from_str(&result.stdout).expect("parse JSON");
    assert_eq!(json["task_id"], "plan.1", "the parent is the next claimable ticket");
    // Claiming a non-runnable initial state advances it the same way it does
    // for a leaf; the dependent stays blocked until the parent is finished.
    assert_task_state(&plan_path, &machine_path, "1", "pending");
    assert_task_state(&plan_path, &machine_path, "2", "draft");
    let content = fs::read_to_string(&plan_path).expect("read plan");
    assert!(
        content.contains("### Task 1: Parent task\n**State:** pending\n**Assignee:**"),
        "the parent should carry the claim; got:\n{content}"
    );
}

/// `--task` on a parent is refused only while its subtree is open, and the
/// refusal names the open descendants and the ticket that is claimable now
/// instead of asserting a cascade that does not exist.
// §FS-rhei-next.3.4
#[test]
fn next_task_refuses_a_parent_while_a_descendant_is_open() {
    let (_dir, plan_path, machine_path) =
        setup_single_file("next-parent-open-child", PARENT_WITH_ONE_OPEN_CHILD);

    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--task", "1"]);
    assert!(
        !result.status.success(),
        "claiming a parent with an open descendant must fail\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert_stderr_contains(
        &result,
        "Task plan.1 cannot be claimed while 1 descendant task(s) are still open.",
    );
    assert_stderr_contains(&result, "Open descendants: Task plan.1.2 (draft)");
    assert_stderr_contains(&result, "claim what is ready instead");
    assert_stderr_contains(&result, "--task plan.1.2");
    assert!(
        !result.stderr.contains("advances when its children"),
        "the refusal must not claim a cascade exists; got:\n{}",
        result.stderr
    );

    let content = fs::read_to_string(&plan_path).expect("read plan");
    assert!(
        !content.contains("**Assignee:**"),
        "a refused claim must not write an assignee; got:\n{content}"
    );
}

/// The same `--task` invocation succeeds once the subtree is terminal: the
/// refusal is about open descendants, not about being a parent.
// §FS-rhei-next.3.4
#[test]
fn next_task_claims_a_parent_once_its_subtree_is_terminal() {
    let (_dir, plan_path, machine_path) =
        setup_single_file("next-parent-target", PARENT_WITH_TERMINAL_SUBTREE);

    let result =
        run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--task", "1", "--json"]);
    assert_success(&result);

    let json: serde_json::Value = serde_json::from_str(&result.stdout).expect("parse JSON");
    assert_eq!(json["task_id"], "plan.1");
    assert_task_state(&plan_path, &machine_path, "1", "pending");
}

/// The whole loop from the defect report: children finish, the parent becomes
/// claimable and completes on its own, and only then does the dependent that
/// listed it as `**Prior:**` unblock.
// §FS-rhei-next.3 §FS-rhei-plan-language.3
#[test]
fn next_unblocks_a_dependent_only_after_the_parent_itself_completes() {
    let (_dir, plan_path, machine_path) =
        setup_single_file("next-parent-dependent", PARENT_WITH_TERMINAL_SUBTREE);

    // The parent is claimed and worked like any ticket.
    assert_success(&run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--task", "1"]));
    // Its children being done did not finish it; the dependent is still blocked.
    let blocked = run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--task", "2"]);
    assert!(
        !blocked.status.success(),
        "the dependent must stay blocked while the parent is unfinished\nstdout:\n{}\nstderr:\n{}",
        blocked.stdout,
        blocked.stderr
    );
    assert_stderr_contains(&blocked, "Task plan.2 is blocked by incomplete prerequisites");
    assert_stderr_contains(&blocked, "waiting on Task plan.1 (pending)");

    assert_success(&run_cli(
        "complete",
        &plan_path,
        &machine_path,
        &["--no-callbacks", "--task", "1", "--result", "integrated"],
    ));

    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--json"]);
    assert_success(&result);
    let json: serde_json::Value = serde_json::from_str(&result.stdout).expect("parse JSON");
    assert_eq!(json["task_id"], "plan.2", "the dependent unblocks once the parent is terminal");
}

#[test]
fn next_fails_when_all_completed() {
    let plan = r#"# Rhei: All Done

## Tasks

### Task 1: Done
**State:** completed
"#;

    let (_dir, plan_path, machine_path) = setup_single_file("next-done", plan);

    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks"]);
    assert!(!result.status.success(), "should fail when nothing to advance");
    assert!(
        result.stderr.contains("Plan complete. All 1 task(s) are in terminal states."),
        "expected plan-complete diagnostic; got:\n{}",
        result.stderr
    );
}

#[test]
fn next_does_not_allow_cancelled_prerequisite_to_unblock_dependents() {
    let plan = r#"# Rhei: Cancelled Dependency

## Tasks

### Task 1: Abandoned
**State:** cancelled

### Task 2: Still blocked
**State:** draft
**Prior:** Task 1
"#;

    let (_dir, plan_path, machine_path) = setup_single_file("next-cancelled-dep", plan);

    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks"]);
    assert!(!result.status.success(), "cancelled prerequisite should keep Task 2 blocked");
    assert_stderr_contains(&result, "Task plan.2 waiting on Task plan.1 (cancelled)");
    assert_stderr_contains(&result, "blocked");
    assert_stderr_contains(&result, "incomplete prerequisites");

    let targeted = run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--task", "2"]);
    assert!(!targeted.status.success(), "targeted next should still respect blocked prerequisites");
    assert_stderr_contains(&targeted, "blocked by incomplete prerequisites");
    assert_stderr_contains(&targeted, "waiting on Task plan.1");
    assert_stderr_contains(&targeted, "(cancelled)");
}

#[test]
fn complete_fails_when_only_cancelled_terminal_is_available() {
    let plan = r#"# Rhei: Cancel Is Not Complete

## Tasks

### Task 1: Work item
**State:** active
"#;
    let machine = r#"name: cancelled-only
version: 1
states:
  active:
    description: Working
  cancelled:
    description: Abandoned
    final: true
transitions:
  - from: active
    to: cancelled
"#;

    let dir = unique_temp_dir("complete-no-cancel");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli(
        "complete",
        &plan_path,
        &machine_path,
        &["--task", "1", "--result", "done", "--no-callbacks"],
    );
    assert!(!result.status.success(), "complete should fail instead of cancelling");
    assert!(
        result.stderr.contains("no transition to a terminal state available"),
        "expected missing-completion-target error; got:\n{}",
        result.stderr
    );
    assert_task_state(&plan_path, &machine_path, "1", "active");
    assert!(
        !dir.join("runtime/results/plan.1.md").exists(),
        "result file should not be written on failure"
    );
}

#[test]
fn next_fails_with_explicit_error_when_current_state_input_artifact_is_missing() {
    let plan = r#"# Rhei: Missing Current Input

## Tasks

### Task 1: Apply findings
**State:** fix
"#;
    let machine = r#"name: missing-current-input
version: 1
states:
  draft:
    description: Planned
    initial: true
  fix:
    description: Needs findings
    inputs:
      - name: findings
        path: runtime/findings/{task_id}.md
  completed:
    description: Done
    final: true
transitions:
  - from: draft
    to: fix
  - from: fix
    to: completed
"#;

    let dir = unique_temp_dir("next-missing-input");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli("next", &plan_path, &machine_path, &["--no-callbacks", "--task", "1"]);
    assert!(!result.status.success(), "next should fail when current-state input is missing");
    assert!(
        result.stderr.contains("Task plan.1 cannot be claimed in state fix."),
        "expected explicit claim failure; got:\n{}",
        result.stderr
    );
    assert!(
        result
            .stderr
            .contains("Missing required input artifact: findings (runtime/findings/plan.1.md)"),
        "expected missing artifact detail; got:\n{}",
        result.stderr
    );
    assert_task_state(&plan_path, &machine_path, "1", "fix");
}

#[test]
fn complete_fails_when_required_output_artifact_is_missing() {
    let plan = r#"# Rhei: Missing Completion Output

## Tasks

### Task 1: Review item
**State:** review
"#;
    let machine = r#"name: missing-output
version: 1
states:
  review:
    description: Must produce findings before leaving
    outputs:
      - name: findings
        path: runtime/findings/{task_id}.md
  completed:
    description: Done
    final: true
transitions:
  - from: review
    to: completed
"#;

    let dir = unique_temp_dir("complete-missing-output");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", machine);

    let result = run_cli(
        "complete",
        &plan_path,
        &machine_path,
        &["--task", "1", "--result", "done", "--no-callbacks"],
    );
    assert!(!result.status.success(), "complete should fail when required output is missing");
    // §FS-rhei-panta.6: diagnostics and artifact paths use the qualified id.
    assert_stderr_contains(&result, "Task plan.1 cannot leave state review.");
    assert_stderr_contains(
        &result,
        "Missing required output artifact: findings (runtime/findings/plan.1.md)",
    );
    assert_task_state(&plan_path, &machine_path, "1", "review");
    assert!(
        !dir.join("runtime/results/plan.1.md").exists(),
        "result file should not be written on failure"
    );
}

/// The ready-set refusal has to name a command that actually shows the body it
/// is refusing to show. `rhei list` prints none, which §FS-rhei-errors.1.2 calls
/// the wrong help line, and `rhei show` is the verb that does.
/// §FS-rhei-next.4 §FS-rhei-show
#[test]
fn next_refusal_help_names_the_verb_that_prints_the_body() {
    let plan = r#"# Rhei: Refusal Help

## Tasks

### Task 1: The claimable one
**State:** draft

### Task 2: The finished one
**State:** completed

Body nobody can reach through a claim.
"#;

    let dir = unique_temp_dir("next-refusal-help-show");
    let plan_path = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine_path = write_fixture_file(&dir, "states.yaml", STATE_MACHINE);

    let result = run_cli("next", &plan_path, &machine_path, &["--task", "plan.2", "--peek"]);
    assert!(!result.status.success(), "a terminal ticket is still refused:\n{}", result.stdout);

    let said = &result.stderr;
    assert!(
        said.contains("Task plan.2 is not ready to be claimed in state 'completed'"),
        "the gate itself is unchanged; got:\n{said}"
    );
    assert!(
        said.contains("rhei show plan.2"),
        "the help should name the verb that prints this task's body; got:\n{said}"
    );
    assert!(
        !said.contains("inspect the task and current ready work with: rhei list"),
        "the old help line sent the reader to the one place the body is not; got:\n{said}"
    );
}
