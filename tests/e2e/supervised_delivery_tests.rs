//! The `supervised-delivery` built-in template, end to end: a root task in a
//! `supervising` state that briefs, routes, and cancels every step beneath it,
//! with the brief as the release gate and plan exports as the channel.

// §FS-rhei-supervision §FS-rhei-plan-language.3.12

use std::fs;
use std::path::{Path, PathBuf};

use super::*;

fn template_dir() -> PathBuf {
    repo_root().join("crates/rhei-cli/templates/supervised-delivery")
}

/// Instantiate the built-in template into `<dir>/ws` with a clean HOME.
fn instantiate(dir: &Path, args: &[&str]) -> (PathBuf, CliRun) {
    let out = dir.join("ws");
    let home = dir.join(".home");
    fs::create_dir_all(&home).expect("create isolated home");
    let mut cmd = rhei_command(&home);
    cmd.current_dir(dir);
    cmd.arg("instantiate").arg(template_dir());
    for arg in args {
        cmd.arg(arg);
    }
    cmd.arg("--output").arg(&out);
    let output = cmd.output().expect("rhei instantiate should run");
    let run = CliRun::from(&output);
    (out, run)
}

/// The mock that stands in for all seven roles.
///
/// Every child writes the export its state declares and its result. The
/// supervisor writes the preparation note and then briefs the next step, choosing
/// it from which children have already finished — the same routing rule the
/// state's instructions give a real agent, reduced to what a shell can check.
fn mock_agent_script(dir: &Path) -> PathBuf {
    write_python_agent(
        dir,
        "mock-delivery-agent.py",
        r#"root = pathlib.Path(env('RHEI_ROOT'))
task = env('RHEI_TASK_ID')
state = env('RHEI_STATE')
visit = env('RHEI_VISIT_COUNT', '1')
for folder in ('logs', 'prompts', 'supervise', 'supervision'):
    (root / 'runtime' / folder).mkdir(parents=True, exist_ok=True)
(root / 'runtime' / 'exports' / task).mkdir(parents=True, exist_ok=True)
append(root / 'runtime' / 'logs' / 'spawns.log', '{} {}\n'.format(task, state))

prompt = ''
args = sys.argv[1:]
while args:
    if args.pop(0) == '--prompt' and args:
        prompt = args.pop(0)
write(root / 'runtime' / 'prompts' / '{}-{}.md'.format(task, state), prompt)


def export_json(name, payload):
    write(
        root / 'runtime' / 'exports' / task / (name + '.md'),
        '```json\n' + payload + '\n```\n',
    )


def finished(child):
    path = root / 'runtime' / 'results' / '{}.{}.md'.format(task, child)
    return path.is_file() and path.stat().st_size > 0


def brief(child):
    write(
        root / 'runtime' / 'supervise' / '{}.{}.md'.format(task, child),
        'Brief from the supervisor (visit {}) for {}.\n'.format(visit, child),
    )


if state == 'supervising':
    write(
        root / 'runtime' / 'supervision' / 'preparation.md',
        '# Preparation\n\nAcceptance criteria, risk areas, per-role focus.\n',
    )
    if not finished('implement'):
        brief('implement')
    elif not finished('review-1') or not finished('pm-1'):
        brief('review-1')
        brief('pm-1')
    elif not finished('fix-1'):
        brief('fix-1')
    elif not finished('coverage-1'):
        brief('coverage-1')
    elif not finished('coverage-fix-1'):
        brief('coverage-fix-1')
    elif not finished('docs-1'):
        brief('docs-1')
    else:
        result('## Result\n\nDelivered across {} supervisor visits.\n'.format(visit))
elif state in ('implement', 'docs'):
    export_json(
        'report',
        '{ "summary": "mock", "commits": [], "files": [], "ci": {}, "notes": "" }',
    )
elif state == 'review':
    export_json(
        'findings',
        '{ "round": 1, "role": "code-review", "verdict": "approve", "findings": [] }',
    )
elif state == 'pm-review':
    export_json(
        'findings',
        '{ "round": 1, "role": "product", "verdict": "approve", "findings": [] }',
    )
elif state == 'fix':
    export_json('resolutions', '{ "round": 1, "resolutions": [] }')
elif state == 'coverage':
    export_json('gaps', '{ "round": 1, "gaps": [] }')

if state != 'supervising':
    result('## Result\n\nTask {} finished {}.\n'.format(task, state))
"#,
    )
}

/// Point every agent the template's default targets name at the mock.
fn write_mock_settings(workspace: &Path, script: &Path) {
    let settings_dir = workspace.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create .agent-grounds/rhei");
    let profile = format!(
        r#"{{
      "command": {},
      "prompt_flag": "--prompt",
      "model_flag": "--model",
      "timeout": "30s",
      "modes": {{ "yolo": [], "xhigh": [] }}
    }}"#,
        fixture_command(script)
    );
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{ "agent": "mock", "agent_timeout": "30s" }},
  "agents": {{
    "mock": {profile},
    "claude-code": {profile},
    "codex": {profile}
  }}
}}"#
        ),
    )
    .expect("write mock settings");
}

/// `<local-task-suffix> <state>` per invocation, in spawn order.
fn spawns(workspace: &Path) -> Vec<String> {
    fs::read_to_string(workspace.join("runtime/logs/spawns.log"))
        .expect("the mock logs every invocation")
        .lines()
        .map(str::to_string)
        .collect()
}

/// §FS-rhei-templates.6.1: the template instantiates from its defaults and a
/// dry run shows the one shape supervision gives it — the supervisor ready,
/// everything else held. Validation emits only the required `Consumes`
/// advisory (§FS-rhei-validate.6), with no supervisor configuration warning
/// under §FS-rhei-supervision.1.2.
#[test]
fn the_default_instantiation_validates_clean_and_holds_every_child() {
    let dir = unique_temp_dir("supervised-delivery-defaults");
    let (workspace, instantiated) =
        instantiate(&dir, &["spec_path=docs/functional-spec/rhei-supervision.spec.md"]);
    assert!(
        instantiated.status.success(),
        "instantiate failed:\nstdout:\n{}\nstderr:\n{}",
        instantiated.stdout,
        instantiated.stderr
    );

    // Eleven tasks: the supervisor, the implementer, two review rounds of three
    // tasks each, one coverage round of two, and one documentation round.
    assert!(
        instantiated.stdout.contains("Tasks: 11"),
        "the default round ceilings unroll eleven tasks; got:\n{}",
        instantiated.stdout
    );

    let machine = workspace.join("states.yaml");
    let validated = run_cli("validate", &workspace, &machine, &[]);
    assert_success(&validated);
    assert_eq!(
        validated.stdout,
        concat!(
            "Validation succeeded\n",
            "warning: **Consumes:** declares export data-flow for prompt injection, not filesystem visibility. ",
            "Workers can read undeclared sibling exports under runtime/exports/. For a blind round, schedule ",
            "participants concurrently and brief them not to inspect sibling exports; neither measure enforces ",
            "blindness once an export exists.\n",
        ),
        "validation must emit only the exact Consumes advisory after success"
    );
    // §FS-rhei-supervision.1.2: this graph has both required supervisor
    // controls, so stderr must not contain a supervisor configuration warning.
    assert!(
        !validated.stderr.to_lowercase().contains("warning"),
        "the supervisor's configuration must not add a warning:\nstderr:\n{}",
        validated.stderr
    );

    // §FS-rhei-supervision.3.4: the dry run names the barrier and renders the
    // release self-loop as such.
    let dry = run_cli("run", &workspace, &machine, &["--dry-run", "--no-tui", "--parallel", "2"]);
    assert_success(&dry);
    assert!(
        dry.stdout.contains("Pass 1: 1 ready, 0 terminal, 11 total."),
        "one ticket is ready; got:\n{}",
        dry.stdout
    );
    assert!(
        dry.stdout.contains("Ready: Task ws.deliver: Supervised delivery"),
        "the supervisor is the ready one; got:\n{}",
        dry.stdout
    );
    assert!(
        dry.stdout.contains("10 ticket(s) held by supervisor Task ws.deliver"),
        "every child is held; got:\n{}",
        dry.stdout
    );
    assert!(
        dry.stdout.contains("supervising -> supervising (release)"),
        "the self-loop is the release edge; got:\n{}",
        dry.stdout
    );
}

/// The whole pipeline under mock agents: one round of everything, `--parallel 2`
/// so the two reviews of a round overlap, and a supervisor visit between every
/// phase. §FS-rhei-supervision.3.1
#[test]
fn the_supervisor_sends_every_step_and_finishes_after_the_last_one() {
    let dir = unique_temp_dir("supervised-delivery-run");
    let (workspace, instantiated) = instantiate(
        &dir,
        &[
            "spec_path=docs/functional-spec/rhei-supervision.spec.md",
            "title=Mock delivery",
            "review_rounds=1",
            "coverage_rounds=1",
            "docs_rounds=1",
        ],
    );
    assert!(
        instantiated.status.success(),
        "instantiate failed:\nstdout:\n{}\nstderr:\n{}",
        instantiated.stdout,
        instantiated.stderr
    );

    let script = mock_agent_script(&dir);
    write_mock_settings(&workspace, &script);

    let machine = workspace.join("states.yaml");
    let result =
        run_cli("run", &workspace, &machine, &["--no-tui", "--no-callbacks", "--parallel", "2"]);
    assert_success(&result);
    assert_all_tasks_in_state(&workspace, &machine, "completed");

    // The trace, with runs of supervisor visits collapsed: under `--parallel 2`
    // the two reviews drain into one visit, under `--parallel 1` they take two,
    // and neither is a difference in the pipeline. §FS-rhei-supervision.3.1
    let log = spawns(&workspace);
    let mut shape: Vec<String> = Vec::new();
    for line in &log {
        let (task, state) = line.split_once(' ').expect("logged task and state");
        let step = if state == "supervising" {
            "supervising".to_string()
        } else {
            task.rsplit('.').next().expect("task id has a leaf").to_string()
        };
        if shape.last().map(String::as_str) != Some("supervising") || step != "supervising" {
            shape.push(step);
        }
    }
    // The two reviews of a round are concurrent, so their order is not ours to
    // assert; everything around them is.
    shape[3..5].sort();
    assert_eq!(
        shape,
        vec![
            "supervising",
            "implement",
            "supervising",
            "pm-1",
            "review-1",
            "supervising",
            "fix-1",
            "supervising",
            "coverage-1",
            "supervising",
            "coverage-fix-1",
            "supervising",
            "docs-1",
            "supervising",
        ],
        "expected a supervisor visit between every phase; got:\n{log:#?}"
    );

    // §FS-rhei-supervision.5.2: the release gate is the brief — every child
    // read one, and it reached the prompt.
    for child in
        ["implement", "review-1", "pm-1", "fix-1", "coverage-1", "coverage-fix-1", "docs-1"]
    {
        let brief = workspace.join(format!("runtime/supervise/ws.deliver.{child}.md"));
        assert!(brief.is_file(), "the supervisor briefed every step it released: {child}");
    }
    let fix_prompt = fs::read_to_string(workspace.join("runtime/prompts/ws.deliver.fix-1-fix.md"))
        .expect("read the fixer's prompt");
    assert!(fix_prompt.contains("## Supervisor Brief"), "got:\n{fix_prompt}");

    // §FS-rhei-plan-language.3.12: the structured channel. The fixer's prompt
    // carries both reviews' findings, and every declared export was written.
    assert!(
        fix_prompt.contains("### findings from Task ws.deliver.review-1"),
        "got:\n{fix_prompt}"
    );
    assert!(fix_prompt.contains("### findings from Task ws.deliver.pm-1"), "got:\n{fix_prompt}");
    for (task, export) in [
        ("implement", "report"),
        ("review-1", "findings"),
        ("pm-1", "findings"),
        ("fix-1", "resolutions"),
        ("coverage-1", "gaps"),
        ("coverage-fix-1", "resolutions"),
        ("docs-1", "report"),
    ] {
        let path = workspace.join(format!("runtime/exports/ws.deliver.{task}/{export}.md"));
        assert!(path.is_file(), "{task} publishes its {export} export: {}", path.display());
    }

    // The preparation note is the memory a cold supervisor has of visit 1, and
    // its absence is what tells visit 1 apart from every later one.
    let first = fs::read_to_string(workspace.join("runtime/prompts/ws.deliver-supervising.md"))
        .expect("read the supervisor's prompt");
    assert!(
        first.contains("## This is not your first visit"),
        "the last visit knows it is not the first; got:\n{first}"
    );
}

/// The cancel command the supervisor is handed must be one the CLI accepts:
/// `--from` is required, so a prompt that omits it costs a failed command and a
/// read of the child's task file before the cancel lands (#117).
/// §FS-rhei-transition-cmd.2
#[test]
fn the_supervisor_prompt_cancels_with_the_required_from() {
    let dir = unique_temp_dir("supervised-delivery-cancel-guidance");
    let (workspace, instantiated) =
        instantiate(&dir, &["spec_path=docs/functional-spec/rhei-supervision.spec.md"]);
    assert!(instantiated.status.success(), "instantiate failed:\n{}", instantiated.stderr);

    let machine = workspace.join("states.yaml");
    let peeked = run_cli("next", &workspace, &machine, &["--task", "deliver", "--peek"]);
    assert_success(&peeked);

    // The same scan the templates are held to, over what the supervisor reads.
    let offenders = super::template_transition_guard_tests::offending_invocations(&peeked.stdout);
    assert!(
        offenders.is_empty(),
        "the supervisor prompt shows a transition the CLI rejects:\n{}",
        offenders.join("\n")
    );

    // The prompt wraps its commands, so read it as one line before matching.
    let prompt = peeked.stdout.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        prompt.contains("rhei transition <task-id> --from <current-state> --to cancelled"),
        "the cancel rule spells the guard out; got:\n{}",
        peeked.stdout
    );

    fs::remove_dir_all(dir).expect("cleanup");
}

/// The bundled supervisor authors the degradable state-local contract once.
/// Its default target therefore needs no machine-specific template input or
/// duplicate named snapshot. §FS-rhei-snapshots.4.7 §FS-rhei-supervision.6
#[test]
fn session_continuation_replaces_the_template_session_knob() {
    let dir = unique_temp_dir("supervised-delivery-snapshot");
    let (workspace, instantiated) =
        instantiate(&dir, &["spec_path=docs/functional-spec/rhei-supervision.spec.md"]);
    assert!(instantiated.status.success(), "instantiate failed:\n{}", instantiated.stderr);
    let machine = fs::read_to_string(workspace.join("states.yaml")).expect("read machine");
    assert!(
        machine.contains("\n    session: continue\n"),
        "the default supervisor declares degradable continuation; got:\n{machine}"
    );
    assert!(
        !machine.contains("\n    snapshot:\n")
            && !machine.contains("emit: { name: supervisor")
            && !machine.contains("inherit: { name: supervisor"),
        "continuation alone must not duplicate the transcript; got:\n{machine}"
    );

    let template = fs::read_to_string(template_dir().join("template.yaml")).expect("manifest");
    let values =
        fs::read_to_string(template_dir().join(".example-values.yaml")).expect("example values");
    let readme = fs::read_to_string(template_dir().join("README.md")).expect("template readme");
    assert!(
        !template.contains("supervisor_session")
            && !values.contains("supervisor_session")
            && !readme.contains("supervisor_session"),
        "the machine-local continuity input must be removed"
    );
    let example = repo_root().join("examples/supervised-delivery-example");
    let example_machine = fs::read_to_string(example.join("states.yaml")).expect("example machine");
    let example_values =
        fs::read_to_string(example.join("instantiation-values.yaml")).expect("example values");
    assert!(
        example_machine.contains("\n    session: continue\n")
            && !example_machine.contains("emit: { name: supervisor")
            && !example_values.contains("supervisor_session"),
        "the checked-in example must match the migrated template"
    );

    let old_value = instantiate(
        &unique_temp_dir("supervised-delivery-old-session-value"),
        &["spec_path=docs/functional-spec/rhei-supervision.spec.md", "supervisor_session=true"],
    )
    .1;
    assert!(
        !old_value.status.success()
            && format!("{}{}", old_value.stdout, old_value.stderr).contains("supervisor_session"),
        "old values must be rejected as an unknown input after migration"
    );
}
