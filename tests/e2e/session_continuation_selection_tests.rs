use std::fs;

use super::session_continuation_support::*;
use super::*;

fn mutation_fixture(prefix: &str, visits: u32, body: &str) -> ContinuationFixture {
    let fixture = continuation_fixture(prefix, visits, 1, "supported", "", "");
    let mutator = write_python_agent(&fixture.dir, "mutate-snapshot.py", body);
    let command = fixture_command(&mutator);
    fs::write(
        &fixture.machine,
        format!(
            r#"name: session-selection
version: 1
states:
  loop:
    initial: true
    description: State-local continuation
    target: fake:acme:model-a
    visits: {visits}
    session: continue
  mutate:
    description: Change the cache between loop visits
    program:
      command: {command}
      shell: false
  completed:
    description: Done
    final: true
transitions:
  - from: loop
    to: completed
    condition: visitCount >= {visits}
  - from: loop
    to: mutate
  - from: loop
    to: loop
    condition: visitCount < 0
  - from: mutate
    to: loop
"#
        ),
    )
    .expect("write mutation machine");
    fixture
}

/// The implicit selector follows the orchestrator's `current` pointer. An
/// operator sibling generation must not replace it. §FS-rhei-snapshots.4.7
#[test]
fn continuation_uses_the_current_orchestrator_generation() {
    let fixture = mutation_fixture(
        "session-continue-current-generation",
        2,
        r#"import json, shutil
root = pathlib.Path(env('RHEI_PLAN_PATH')).parent
base = root / '.rhei/cache/snapshots/plan.1/_state/loop/1/fake-acme-model-a'
source = base / 'g1'
operator = base / 'g2'
shutil.copytree(source, operator)
manifest = json.loads((operator / 'manifest.json').read_text())
manifest['generation'] = 2
manifest['session_id'] = 'operator-generation-must-not-win'
manifest['produced_by'] = 'operator'
write(operator / 'manifest.json', json.dumps(manifest))
"#,
    );
    let run = run_continuation(&fixture, &[]);
    assert_success(&run);
    let log = continuation_log(&fixture);
    assert!(
        log.contains(
            "visit=2 attempt=1 target=fake-acme-model-a resume=plan.1-fake-acme-model-a-v1-a1"
        ) && !log.contains("resume=operator-generation-must-not-win"),
        "continuation must follow current g1, not the operator g2 sibling:\n{log}"
    );
}

/// Removing visit N-1 makes visit N cold; the selector must not search back to
/// N-2. §FS-rhei-snapshots.4.6 §FS-rhei-snapshots.4.7
#[test]
fn continuation_does_not_search_an_older_visit() {
    let fixture = mutation_fixture(
        "session-continue-no-backward-search",
        3,
        r#"import shutil
root = pathlib.Path(env('RHEI_PLAN_PATH')).parent
visit_two = root / '.rhei/cache/snapshots/plan.1/_state/loop/2'
if visit_two.exists():
    shutil.rmtree(visit_two)
"#,
    );
    let run = run_continuation(&fixture, &[]);
    assert_success(&run);
    let log = continuation_log(&fixture);
    assert!(
        log.contains(
            "visit=2 attempt=1 target=fake-acme-model-a resume=plan.1-fake-acme-model-a-v1-a1"
        ),
        "precondition: visit 2 continues visit 1:\n{log}"
    );
    let visit_three = log.lines().find(|line| line.contains("visit=3 ")).expect("visit 3");
    assert!(
        visit_three.contains("resume= parent=")
            && !visit_three.contains("plan.1-fake-acme-model-a-v1-a1"),
        "visit 3 must run cold instead of searching back to visit 1:\n{log}"
    );
    let output = format!("{}{}", run.stdout, run.stderr).to_lowercase();
    assert!(
        output.contains("running cold")
            && (output.contains("missing")
                || output.contains("no snapshot")
                || output.contains("no current snapshot")),
        "missing N-1 must produce a reasoned fallback:\n{output}"
    );
}

fn assert_mutated_source_runs_cold(prefix: &str, mutation: &str, reason: &str) {
    let fixture = mutation_fixture(prefix, 2, mutation);
    let run = run_continuation(&fixture, &[]);
    assert_success(&run);
    let log = continuation_log(&fixture);
    let visit_two = log.lines().find(|line| line.contains("visit=2 ")).expect("visit 2");
    assert!(visit_two.contains("resume= parent="), "visit 2 must run cold:\n{log}");
    let output = format!("{}{}", run.stdout, run.stderr).to_lowercase();
    assert!(
        output.contains("running cold") && output.contains(reason),
        "fallback must name {reason:?}:\n{output}"
    );
}

/// Timed-out, unreadable, and layout-incompatible exact sources all take the
/// optional cold path. §FS-rhei-snapshots.4.7 §FS-rhei-snapshots.10.1
#[test]
fn unusable_exact_sources_have_reasoned_cold_fallbacks() {
    assert_mutated_source_runs_cold(
        "session-continue-timeout",
        r#"import json
root = pathlib.Path(env('RHEI_PLAN_PATH')).parent
path = root / '.rhei/cache/snapshots/plan.1/_state/loop/1/fake-acme-model-a/g1/manifest.json'
manifest = json.loads(path.read_text())
manifest['completion'] = 'timeout'
write(path, json.dumps(manifest))
"#,
        "timeout",
    );
    assert_mutated_source_runs_cold(
        "session-continue-unreadable",
        r#"root = pathlib.Path(env('RHEI_PLAN_PATH')).parent
path = root / '.rhei/cache/snapshots/plan.1/_state/loop/1/fake-acme-model-a/g1/transcript.jsonl'
path.unlink()
"#,
        "unreadable",
    );
    assert_mutated_source_runs_cold(
        "session-continue-incompatible",
        r#"import json
root = pathlib.Path(env('RHEI_PLAN_PATH')).parent
path = root / '.rhei/cache/snapshots/plan.1/_state/loop/1/fake-acme-model-a/g1/manifest.json'
manifest = json.loads(path.read_text())
manifest['session_layout']['ext'] = 'other'
write(path, json.dumps(manifest))
"#,
        "incompatible",
    );
    assert_mutated_source_runs_cold(
        "session-continue-incompatible-agent",
        r#"import json
root = pathlib.Path(env('RHEI_PLAN_PATH')).parent
path = root / '.rhei/cache/snapshots/plan.1/_state/loop/1/fake-acme-model-a/g1/manifest.json'
manifest = json.loads(path.read_text())
manifest['target']['resolved']['agent'] = 'other-agent'
write(path, json.dumps(manifest))
"#,
        "incompatible",
    );
}

/// A broken `current` pointer and a target changed between visits both run
/// cold; neither may broaden the exact identity lookup. §FS-rhei-snapshots.4.7
#[test]
fn missing_current_and_changed_target_run_cold_without_substitution() {
    let cases = [
        (
            "session-continue-missing-current",
            r#"root = pathlib.Path(env('RHEI_PLAN_PATH')).parent
path = root / '.rhei/cache/snapshots/plan.1/_state/loop/1/fake-acme-model-a/current'
path.unlink()
"#,
            "fake-acme-model-a",
        ),
        (
            "session-continue-changed-target",
            r#"root = pathlib.Path(env('RHEI_PLAN_PATH')).parent
path = pathlib.Path(env('RHEI_PLAN_PATH'))
raw = path.read_text()
needle = '**State:** mutate\n'
if needle not in raw:
    raise RuntimeError('task is not at the target mutation boundary')
write(path, raw.replace(needle, needle + '**Target:** fake:acme:model-b\n', 1))
"#,
            "fake-acme-model-b",
        ),
    ];
    for (prefix, mutation, expected_target) in cases {
        let fixture = mutation_fixture(prefix, 2, mutation);
        let run = run_continuation(&fixture, &[]);
        assert_success(&run);
        assert!(
            fixture
                .dir
                .join(
                    ".rhei/cache/snapshots/plan.1/_state/loop/1/fake-acme-model-a/g1/manifest.json"
                )
                .exists(),
            "precondition: visit-1 target A snapshot must remain available"
        );
        let log = continuation_log(&fixture);
        let visit_two = log.lines().find(|line| line.contains("visit=2 ")).expect("visit 2");
        assert!(
            visit_two.contains(&format!("target={expected_target}"))
                && visit_two.contains("resume= parent="),
            "changed identity must run cold:\n{log}"
        );
        let output = format!("{}{}", run.stdout, run.stderr).to_lowercase();
        assert!(
            output.contains("running cold")
                && (output.contains("missing")
                    || output.contains("no snapshot")
                    || output.contains("no current snapshot")),
            "fallback must explain the absent exact source:\n{output}"
        );
    }
}

/// Fanout continuation is per target; neither target may inherit its sibling's
/// session. §FS-rhei-snapshots.4.7 §FS-rhei-snapshots.10.3
#[test]
fn fanout_continues_each_target_independently() {
    let fixture = continuation_fixture("session-continue-fanout", 2, 1, "supported", "", "");
    fs::write(
        &fixture.machine,
        r#"name: session-fanout
version: 1
states:
  loop:
    initial: true
    description: Continue every fanout target independently
    all_targets: [fake:acme:model-a, fake:acme:model-b]
    visits: 2
    session: continue
  completed:
    description: Done
    final: true
transitions:
  - from: loop
    to: completed
    condition: visitCount >= 2
  - from: loop
    to: loop
"#,
    )
    .expect("write fanout machine");
    let run = run_continuation(&fixture, &["--parallel", "2"]);
    assert_success(&run);
    let log = continuation_log(&fixture);
    for (target, session) in [
        ("fake-acme-model-a", "plan.1-fake-acme-model-a-v1-a1"),
        ("fake-acme-model-b", "plan.1-fake-acme-model-b-v1-a1"),
    ] {
        assert!(
            log.lines().any(|line| {
                line.contains("visit=2")
                    && line.contains(&format!("target={target}"))
                    && line.contains(&format!("resume={session}"))
            }),
            "visit 2 target {target} must continue its own visit 1:\n{log}"
        );
    }
}

/// A separately authored named emission coexists with continuation and records
/// the same successfully selected `_state` parent. §FS-rhei-snapshots.4.7
#[test]
fn named_emit_coexists_and_records_the_implicit_parent() {
    let transitions = simple_transitions(2);
    let fixture = continuation_fixture(
        "session-continue-named-emit",
        2,
        1,
        "supported",
        "    snapshot:\n      emit: { name: context, on: always }\n",
        &transitions,
    );
    let run = run_continuation(&fixture, &[]);
    assert_success(&run);
    for name in ["_state", "context"] {
        let parent = &manifest(&fixture, 2, "fake-acme-model-a", name)["parent_ref"];
        assert_eq!(parent["snapshot_name"], "_state", "{name} parent: {parent}");
        assert_eq!(parent["visit"], 1, "{name} parent: {parent}");
        assert_eq!(parent["generation"], 1, "{name} parent: {parent}");
    }
}
