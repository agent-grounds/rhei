//! Prior-edge session continuation and per-task inheritance controls.
//! §FS-rhei-snapshots.4 §FS-rhei-plan-language.3.13

use std::fs;
use std::path::{Path, PathBuf};

use super::snapshot_tests::{write_fake_snapshot_agent, write_fake_snapshot_settings};
use super::*;

pub(super) fn setup_flow(prefix: &str, plan: &str, machine: &str) -> (TestDir, PathBuf, PathBuf) {
    let dir = unique_temp_dir(prefix);
    let agent = write_fake_snapshot_agent(&dir);
    write_fake_snapshot_settings(&dir, &agent);
    let plan = write_fixture_file(&dir, "plan.rhei.md", plan);
    let machine = write_fixture_file(&dir, "states.yaml", machine);
    (dir, plan, machine)
}

pub(super) fn state_rule_machine(rule: &str) -> String {
    format!(
        r#"name: snapshot-prior
version: 1
states:
  source:
    initial: true
    description: Produce the named session
    target: fake:acme:model-a
    snapshot:
      emit:
        name: implementation
        on: always
  consume:
    description: Continue the declared predecessor
    target: fake:acme:model-a
{rule}  completed:
    description: Done
    final: true
transitions:
  - from: source
    to: completed
  - from: consume
    to: completed
"#
    )
}

pub(super) fn required_prior_rule() -> &'static str {
    r#"    snapshot:
      inherit:
        name: implementation
        from: prior
        required: true
        select:
          state: source
          target: same
"#
}

pub(super) fn read_agent_log(dir: &Path) -> String {
    fs::read_to_string(dir.join("runtime/fake-agent.log")).expect("fake agent log")
}

pub(super) fn assert_consumer_resumed_source(log: &str, consumer: &str, source: &str) {
    let resume = format!("task={consumer} state=consume target=fake-acme-model-a resume={source}-source-fake-acme-model-a");
    assert!(log.contains(&resume), "consumer did not resume the declared source:\n{log}");
    let parent = format!(
        "\"snapshot_name\":\"implementation\",\"target_slug\":\"fake-acme-model-a\",\"task_id\":\"{source}\",\"visit\":1"
    );
    assert!(log.contains(&parent), "consumer did not receive the exact source parent_ref:\n{log}");
}

#[test]
fn snapshot_prior_state_rule_resumes_the_declared_named_predecessor() {
    let plan = r#"# Rhei: Prior State Rule

## Tasks

### Task source: Source
**State:** source

### Task consumer: Consumer
**State:** consume
**Prior:** Task source
"#;
    let (dir, plan, machine) =
        setup_flow("snapshot-prior-state-rule", plan, &state_rule_machine(required_prior_rule()));

    let run = run_cli("run", &plan, &machine, &["--no-tui"]);
    assert_success(&run);
    assert_consumer_resumed_source(&read_agent_log(&dir), "plan.consumer", "plan.source");
}

#[test]
fn snapshot_prior_task_rule_works_without_a_state_inheritance_rule() {
    let plan = r#"# Rhei: Prior Task Rule

## Tasks

### Task source: Source
**State:** source

### Task consumer: Consumer
**State:** consume
**Prior:** Task source
**Inherits:** implementation from prior
"#;
    let (dir, plan, machine) =
        setup_flow("snapshot-prior-task-rule", plan, &state_rule_machine(""));

    let run = run_cli("run", &plan, &machine, &["--no-tui"]);
    assert_success(&run);
    assert_consumer_resumed_source(&read_agent_log(&dir), "plan.consumer", "plan.source");
}

#[test]
fn snapshot_prior_task_rule_overlays_only_name_and_axis() {
    let plan = r#"# Rhei: Prior Task Overlay

## Tasks

### Task source: Source
**State:** source

### Task consumer: Consumer
**State:** consume
**Prior:** Task source
**Inherits:** implementation from prior
"#;
    let inherited_fields = r#"    snapshot:
      inherit:
        name: wrong-name
        from: self
        compat: native
        required: true
        select:
          state: source
          target: same
"#;
    let (dir, plan, machine) =
        setup_flow("snapshot-prior-task-overlay", plan, &state_rule_machine(inherited_fields));

    let run = run_cli("run", &plan, &machine, &["--no-tui"]);
    assert_success(&run);
    assert_consumer_resumed_source(&read_agent_log(&dir), "plan.consumer", "plan.source");
}

#[test]
fn snapshot_prior_none_forces_a_cold_task_despite_the_state_rule() {
    let plan = r#"# Rhei: Prior Opt Out

## Tasks

### Task source: Source
**State:** source

### Task consumer: Independent consumer
**State:** consume
**Prior:** Task source
**Inherits:** none
"#;
    let (dir, plan, machine) =
        setup_flow("snapshot-prior-none", plan, &state_rule_machine(required_prior_rule()));

    let run = run_cli("run", &plan, &machine, &["--no-tui"]);
    assert_success(&run);
    let log = read_agent_log(&dir);
    assert!(
        log.contains("task=plan.consumer state=consume target=fake-acme-model-a resume= parent="),
        "none must produce an observable cold invocation:\n{log}"
    );
    assert!(
        dir.join(".rhei/cache/snapshots/plan.source/implementation").is_dir(),
        "none must not disable the predecessor's emission"
    );
}

#[test]
fn snapshot_prior_none_applies_to_every_autonomous_state_of_the_task() {
    let plan = r#"# Rhei: Prior Opt Out Scope

## Tasks

### Task source: Source
**State:** source

### Task consumer: Independent consumer
**State:** consume-a
**Prior:** Task source
**Inherits:** none
"#;
    let machine = r#"name: snapshot-prior-none-scope
version: 1
states:
  source:
    initial: true
    description: Produce
    target: fake:acme:model-a
    snapshot:
      emit: { name: implementation, on: always }
  consume-a:
    description: First autonomous state
    target: fake:acme:model-a
    snapshot:
      inherit: { name: implementation, from: prior, required: true }
  consume-b:
    description: Second autonomous state
    target: fake:acme:model-a
    snapshot:
      inherit: { name: implementation, from: prior, required: true }
  completed:
    description: Done
    final: true
transitions:
  - from: source
    to: completed
  - from: consume-a
    to: consume-b
  - from: consume-b
    to: completed
"#;
    let (dir, plan, machine) = setup_flow("snapshot-prior-none-scope", plan, machine);

    let run = run_cli("run", &plan, &machine, &["--no-tui"]);
    assert_success(&run);
    let log = read_agent_log(&dir);
    for state in ["consume-a", "consume-b"] {
        let cold =
            format!("task=plan.consumer state={state} target=fake-acme-model-a resume= parent=");
        assert!(log.contains(&cold), "none did not keep {state} cold:\n{log}");
    }
}

#[test]
fn snapshot_prior_task_metadata_is_reread_before_each_spawn() {
    let dir = unique_temp_dir("snapshot-prior-reread");
    let agent = write_python_agent(
        &dir,
        "snapshot-prior-reread.py",
        r#"session_dir = ''
resume_value = ''
args = sys.argv[1:]
while args:
    flag = args.pop(0)
    if flag == '--session-dir':
        session_dir = args.pop(0) if args else ''
    elif flag == '--resume':
        resume_value = args.pop(0) if args else ''
    elif flag in ('--prompt', '--model') and args:
        args.pop(0)

runtime_root = pathlib.Path(env('RHEI_ROOT', '.')) / 'runtime'
append(
    runtime_root / 'fake-agent.log',
    'task={} state={} resume={} parent={}\n'.format(
        env('RHEI_TASK_ID'), env('RHEI_STATE'), resume_value,
        env('RHEI_SNAPSHOT_PARENT_REF'),
    ),
)
if env('RHEI_STATE') == 'consume-a':
    plan = pathlib.Path(env('RHEI_PLAN_PATH'))
    write(plan, plan.read_text(encoding='utf-8').replace(
        '**Inherits:** implementation from prior', '**Inherits:** none'
    ))

result('## Result\n\nFake agent finished.\n')
if session_dir:
    session_id = '{}-{}-{}'.format(
        env('RHEI_TASK_ID'), env('RHEI_STATE'), env('RHEI_TARGET_SLUG', 'target')
    )
    write(pathlib.Path(session_dir) / (session_id + '.jsonl'), '{"role":"assistant"}\n')
"#,
    );
    write_fake_snapshot_settings(&dir, &agent);
    let plan = write_fixture_file(
        &dir,
        "plan.rhei.md",
        r#"# Rhei: Prior Re-read

## Tasks

### Task source: Source
**State:** source

### Task consumer: Consumer
**State:** consume-a
**Prior:** Task source
**Inherits:** implementation from prior
"#,
    );
    let machine = write_fixture_file(
        &dir,
        "states.yaml",
        r#"name: snapshot-prior-reread
version: 1
states:
  source:
    initial: true
    description: Produce
    target: fake:acme:model-a
    snapshot:
      emit: { name: implementation, on: always }
  consume-a:
    description: Warm and opt out
    target: fake:acme:model-a
  consume-b:
    description: Observe re-read opt-out
    target: fake:acme:model-a
  completed:
    description: Done
    final: true
transitions:
  - from: source
    to: completed
  - from: consume-a
    to: consume-b
  - from: consume-b
    to: completed
"#,
    );

    let run = run_cli("run", &plan, &machine, &["--no-tui"]);
    assert_success(&run);
    let log = read_agent_log(&dir);
    assert!(
        log.contains(
            "task=plan.consumer state=consume-a resume=plan.source-source-fake-acme-model-a"
        ),
        "first consumer state should inherit:\n{log}"
    );
    assert!(
        log.contains("task=plan.consumer state=consume-b resume= parent="),
        "second consumer state should see the authored opt-out:\n{log}"
    );
}

#[test]
fn snapshot_prior_parallel_preload_uses_each_tasks_own_declared_edge() {
    let plan = r#"# Rhei: Prior Parallel

## Tasks

### Task source-a: Source A
**State:** source

### Task source-b: Source B
**State:** source

### Task consumer-a: Consumer A
**State:** consume
**Prior:** Task source-a

### Task consumer-b: Consumer B
**State:** consume
**Prior:** Task source-b
"#;
    let (dir, plan, machine) =
        setup_flow("snapshot-prior-parallel", plan, &state_rule_machine(required_prior_rule()));

    let run = run_cli("run", &plan, &machine, &["--no-tui", "--parallel", "2"]);
    assert_success(&run);
    let log = read_agent_log(&dir);
    assert_consumer_resumed_source(&log, "plan.consumer-a", "plan.source-a");
    assert_consumer_resumed_source(&log, "plan.consumer-b", "plan.source-b");
}

#[test]
fn snapshot_prior_ambiguity_keeps_sources_with_unequal_visit_histories() {
    let plan = r#"# Rhei: Prior Ambiguity

## Tasks

### Task source-a: Source A twice
**State:** source-a

### Task source-b: Source B once
**State:** source-b

### Task consumer: Consumer
**State:** consume
**Prior:** Task source-a, Task source-b
"#;
    let machine = format!(
        r#"name: snapshot-prior-ambiguity
version: 1
states:
  source-a:
    initial: true
    description: Emit twice
    visits: 2
    target: fake:acme:model-a
    snapshot:
      emit: {{ name: implementation, on: always }}
  source-b:
    description: Emit once
    target: fake:acme:model-a
    snapshot:
      emit: {{ name: implementation, on: always }}
  consume:
    description: Must not choose globally newest
    target: fake:acme:model-a
{}  completed:
    description: Done
    final: true
transitions:
  - from: source-a
    to: source-a
    condition: visitCount < visits
  - from: source-a
    to: completed
    condition: visitCount >= visits
  - from: source-b
    to: completed
  - from: consume
    to: completed
"#,
        required_prior_rule()
    );
    let (_dir, plan, machine) = setup_flow("snapshot-prior-ambiguity", plan, &machine);

    let run = run_cli("run", &plan, &machine, &["--no-tui"]);
    assert!(!run.status.success(), "ambiguous lineage must fail");
    let output = format!("{}{}", run.stdout, run.stderr);
    for needle in [
        "ambiguous-lineage",
        "plan.source-a",
        "plan.source-b",
        "implementation:source-a@2",
        "implementation:source-b@1",
    ] {
        assert!(output.contains(needle), "missing {needle:?} from ambiguity:\n{output}");
    }
}

#[test]
fn snapshot_prior_excludes_an_undeclared_emitter_and_runs_optional_lookup_cold() {
    let plan = r#"# Rhei: Prior Scope

## Tasks

### Task declared: Declared predecessor
**State:** declared

### Task stray: Undeclared emitter
**State:** stray

### Task consumer: Consumer
**State:** consume
**Prior:** Task declared
"#;
    let machine = r#"name: snapshot-prior-scope
version: 1
states:
  declared:
    initial: true
    description: Emits a different name
    target: fake:acme:model-a
    snapshot:
      emit: { name: other, on: always }
  stray:
    description: Must remain unreachable
    target: fake:acme:model-a
    snapshot:
      emit: { name: implementation, on: always }
  consume:
    description: Optional lookup
    target: fake:acme:model-a
    snapshot:
      inherit:
        name: implementation
        from: prior
        required: false
  completed:
    description: Done
    final: true
transitions:
  - from: declared
    to: completed
  - from: stray
    to: completed
  - from: consume
    to: completed
"#;
    let (dir, plan, machine) = setup_flow("snapshot-prior-scope", plan, machine);

    let run = run_cli("run", &plan, &machine, &["--no-tui"]);
    assert_success(&run);
    let log = read_agent_log(&dir);
    assert!(
        log.contains("task=plan.consumer state=consume target=fake-acme-model-a resume= parent="),
        "undeclared emitter must not be inherited:\n{log}"
    );
}

#[test]
fn snapshot_prior_effective_task_target_is_resolved_before_same_selector() {
    let plan = r#"# Rhei: Prior Effective Target

## Tasks

### Task source: Source
**State:** source

### Task consumer: Consumer
**State:** consume
**Prior:** Task source
**Target:** fake:acme:model-a
"#;
    let machine = state_rule_machine(required_prior_rule()).replace(
        "  consume:\n    description: Continue the declared predecessor\n    target: fake:acme:model-a",
        "  consume:\n    description: Continue the declared predecessor\n    target: fake:acme:model-b",
    );
    let (dir, plan, machine) = setup_flow("snapshot-prior-effective-target", plan, &machine);

    let run = run_cli("run", &plan, &machine, &["--no-tui"]);
    assert_success(&run);
    assert_consumer_resumed_source(&read_agent_log(&dir), "plan.consumer", "plan.source");
}
