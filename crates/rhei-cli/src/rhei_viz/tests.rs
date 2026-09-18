// Visualization model regressions. §FS-rhei-viz
use super::*;
use crate::rhei_validator::StateMachine;
use rhei_core::parse;

fn builtin() -> StateMachine {
    StateMachine::builtin_default()
}

#[test]
fn flat_tasks_carry_depth_and_parent() {
    let rhei = parse(
            "# Rhei: Deep\n**States:** rhei\n---\nstructure:\n  maxLevels: 4\n  nodeKinds: [task, bug]\n---\n\n## Tasks\n\n### Task api: Build API\n**State:** pending\n\n#### Bug api.cache: Cache issue\n**State:** in-progress\n",
        )
        .expect("parse");
    let model = build(&rhei, &builtin());
    assert_eq!(model.tasks.len(), 2);
    assert_eq!(model.tasks[0].id, "api");
    assert_eq!(model.tasks[0].depth, 0);
    assert_eq!(model.tasks[0].parent, None);
    assert_eq!(model.tasks[1].id, "api.cache");
    assert_eq!(model.tasks[1].depth, 1);
    assert_eq!(model.tasks[1].parent.as_deref(), Some("api"));
}

/// The two polls the ticket contrasts, classified: the one waiting on a
/// person reads as a pause, the one watching CI stays active — and the
/// label crosses the wire so the browser and the terminal can agree.
// §FS-rhei-viz.1.1 §FS-rhei-states.2.5
#[test]
fn a_poll_waiting_on_a_person_is_a_pause_not_active_work() {
    let machine = StateMachine::from_yaml_str(
        r#"
name: approvals
version: 1.0
states:
  plan-approval:
    description: Wait for the author
    program: "./check-reply.sh"
    poll:
      interval: 10m
      max_attempts: 60
      waiting_on: author
  ci-watch:
    description: Watch CI
    program: "./check-ci.sh"
    poll:
      interval: 2m
      max_attempts: 30
  completed:
    final: true
transitions:
  - from: plan-approval
    to: plan-approval
  - from: plan-approval
    to: ci-watch
  - from: ci-watch
    to: ci-watch
  - from: ci-watch
    to: completed
"#,
    )
    .expect("states load");

    assert_eq!(category(&machine, "plan-approval"), Category::Gate);
    assert_eq!(category(&machine, "ci-watch"), Category::Active);

    let wire = flatten_machine(&machine);
    let state =
        |name: &str| wire.states.iter().find(|s| s.name == name).expect("state present").clone();
    assert_eq!(state("plan-approval").waiting_on.as_deref(), Some("author"));
    assert_eq!(state("ci-watch").waiting_on, None);
}

#[test]
fn task_visit_and_unambiguous_template_context_are_exposed() {
    let machine = StateMachine::from_yaml_str(
        r#"
name: custom
version: 1.0
states:
  review:
    visits: 3
    target: codex:openai:gpt-5
    instructions: "Review {task_id} in {state}-{visit_count} using {target.slug}"
    outputs:
      - name: notes
        path: runtime/reviews/{task_id}-{state}-{visit_count}-{target.slug}.md
  completed:
    final: true
transitions:
  - from: review
    to: completed
"#,
    )
    .expect("states load");
    let rhei = parse(
        "# Rhei: Visits\n**States:** custom\n\n## Tasks\n\n### Task 1: A\n**State:** review-2\n",
    )
    .expect("parse");

    let model = build(&rhei, &machine);
    assert_eq!(model.tasks[0].state, "review");
    assert_eq!(model.tasks[0].visit_count, Some(2));
    let review = model.machine.states.iter().find(|s| s.name == "review").unwrap();
    assert_eq!(review.template_context.target.as_deref(), Some("codex:openai:gpt-5"));
    assert_eq!(review.template_context.target_slug.as_deref(), Some("codex-openai-gpt-5"));
    assert_eq!(review.template_context.model.as_deref(), Some("gpt-5"));
    assert_eq!(review.template_context.model_provider.as_deref(), Some("openai"));
}

#[test]
fn multi_target_fanout_contexts_are_exposed_without_guessing() {
    let machine = StateMachine::from_yaml_str(
        r#"
name: custom
version: 1.0
states:
  product-run:
    all_targets:
      - claude-code[yolo]:anthropic:claude-opus-4-7
      - codex[xhigh]:openai:gpt-5.5
    instructions: "Write {output.notes.path} for {target}"
    outputs:
      - name: notes
        path: runtime/{target.slug}/{task_id}.md
  completed:
    final: true
transitions:
  - from: product-run
    to: completed
"#,
    )
    .expect("states load");
    let rhei = parse(
            "# Rhei: Fanout\n**States:** custom\n\n## Tasks\n\n### Task pm: Evaluate\n**State:** product-run\n",
        )
        .expect("parse");

    let model = build(&rhei, &machine);
    let product = model.machine.states.iter().find(|s| s.name == "product-run").unwrap();
    assert_eq!(product.template_context.target, None);
    assert_eq!(product.template_context.target_slug, None);
    assert_eq!(product.template_contexts.len(), 2);
    assert_eq!(
        product.template_contexts[0].target.as_deref(),
        Some("claude-code[yolo]:anthropic:claude-opus-4-7")
    );
    assert_eq!(
        product.template_contexts[0].target_slug.as_deref(),
        Some("claude-code-yolo-anthropic-claude-opus-4-7")
    );
    assert_eq!(
        product.template_contexts[1].target_slug.as_deref(),
        Some("codex-xhigh-openai-gpt-5.5")
    );
}

#[test]
fn plan_state_pending_when_only_pending_roots() {
    let rhei = parse(
            "# Rhei: P\n**States:** rhei\n\n## Tasks\n\n### Task 1: A\n**State:** pending\n\n### Task 2: B\n**State:** pending\n",
        )
        .expect("parse");
    let model = build(&rhei, &builtin());
    assert_eq!(model.plan_state.as_deref(), Some("pending"));
}

#[test]
fn plan_state_active_when_a_root_is_active_like() {
    let rhei = parse(
            "# Rhei: A\n**States:** rhei\n\n## Tasks\n\n### Task 1: A\n**State:** in-progress\n\n### Task 2: B\n**State:** pending\n",
        )
        .expect("parse");
    let model = build(&rhei, &builtin());
    assert_eq!(model.plan_state.as_deref(), Some("active"));
}

#[test]
fn plan_state_completed_and_archived() {
    let completed =
        parse("# Rhei: C\n**States:** rhei\n\n## Tasks\n\n### Task 1: A\n**State:** completed\n")
            .expect("parse");
    assert_eq!(build(&completed, &builtin()).plan_state.as_deref(), Some("completed"));

    let archived = parse(
            "# Rhei: C\n**States:** archival\n\n## Tasks\n\n### Task 1: A\n**State:** completed\n\n### Task 2: B\n**State:** archived\n",
        )
        .expect("parse");
    let machine = StateMachine::from_yaml_str(
        r#"
name: archival
version: 1
states:
  pending: { description: "ready" }
  completed: { description: "done", final: true }
  archived: { description: "retired", final: true }
transitions:
  - from: pending
    to: completed
profiles:
  default: { initial: pending, allowed: [pending, completed, archived] }
node_policy:
  root: default
  default: default
"#,
    )
    .expect("machine");
    assert_eq!(build(&archived, &machine).plan_state.as_deref(), Some("archived"));
}

#[test]
fn machine_flattening_marks_wildcard_and_initial() {
    let machine = builtin();
    let flat = flatten_machine(&machine);
    // The built-in `default-rhei` profile enters at `pending`, so the
    // profile-initial union must mark `pending` initial.
    let pending = flat.states.iter().find(|s| s.name == "pending").expect("pending state");
    assert!(pending.initial, "pending is the built-in profile's initial state");
    let completed = flat.states.iter().find(|s| s.name == "completed").expect("completed");
    assert!(completed.terminal);
    assert!(completed.transitions.is_empty(), "terminal states get no wildcard exits");
}
