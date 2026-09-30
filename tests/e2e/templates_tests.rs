use std::fs;
use std::process::Command;

use super::*;

/// Spawn the built binary standing in `cwd`, with its state under a private
/// home of this call's own.
///
/// Where a spawned process writes and where it stands are two decisions, and
/// this helper used to spell the first in terms of the second —
/// `rhei_command(cwd.join(".home"))`. That made the pair impossible to vary:
/// every caller that wanted a real directory to stand in got that same
/// directory as its state home, and got it silently, because the home is
/// created before the spawn and so raises nothing to notice. The first caller
/// that passed the checkout wrote into it. §REQ-test-isolation.2
///
/// The home is a temporary directory of this call's own, removed when the call
/// returns, so nothing of the spawn outlives the process it was pinned for
/// (§REQ-test-isolation.1). A test that has to *choose* its home — one that
/// plants a user-tier template under it, say — spawns through [`rhei_command`]
/// directly, which already takes the home and the working directory as the two
/// separate arguments they are.
pub fn run_raw(args: &[&str], cwd: &std::path::Path) -> CliRun {
    let home = unique_temp_dir("run-raw-home");
    let output =
        rhei_command(&home).current_dir(cwd).args(args).output().expect("rhei command should run");
    CliRun::from(&output)
}

/// §FS-rhei-templates.6.2: a standalone workspace inside a git repository gets
/// a versioning note when untracked, and stays quiet once it is gitignored —
/// instantiation never edits `.gitignore` itself.
#[test]
fn standalone_instantiation_notes_untracked_workspace_in_git_repo() {
    let dir = unique_temp_dir("templates-standalone-git");
    let git = Command::new("git").arg("init").arg("-q").current_dir(&dir).status();
    if !git.map(|status| status.success()).unwrap_or(false) {
        eprintln!("skipping: git unavailable");
        return;
    }
    let template_dir = dir.join(".agent-grounds/rhei/templates/hello");
    fs::create_dir_all(&template_dir).expect("create template dir");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        "name: hello\nversion: 1.0.0\ndescription: Hello\ninputs:\n  - name: target\n    description: Greeting target\n",
    );
    write_fixture_file(
        &template_dir,
        "plan.rhei.md",
        "# Rhei: Hello {{target}}\n\n## Tasks\n\n### Task 1: Greet {{target}}\n**State:** pending\n",
    );

    let result = run_raw(&["instantiate", "hello", "target=world", "--output", "out"], &dir);
    assert_success(&result);
    assert!(
        result.stdout.contains("is not gitignored") && result.stdout.contains("`out/`"),
        "untracked standalone workspace should get the versioning note; got:\n{}",
        result.stdout
    );

    // Once ignored, the note disappears — the user has made the call.
    fs::write(dir.join(".gitignore"), "out2/\n").expect("write gitignore");
    let result = run_raw(&["instantiate", "hello", "target=world", "--output", "out2"], &dir);
    assert_success(&result);
    assert!(
        !result.stdout.contains("is not gitignored"),
        "ignored standalone workspace must not get the note; got:\n{}",
        result.stdout
    );
}

/// §FS-rhei-templates.6.3.1: the JSON entry carries every key an input
/// declared, so a caller can build a form from it without opening
/// `template.yaml` — which a built-in template has nowhere on disk to open.
#[test]
fn templates_json_carries_the_whole_input_schema() {
    let dir = unique_temp_dir("templates-json-schema");

    let result = run_raw(&["templates", "changeset-review", "--json"], &dir);
    assert_success(&result);
    let value: serde_json::Value =
        serde_json::from_str(&result.stdout).expect("detail should be valid JSON");
    let inputs = value["inputs"].as_array().expect("inputs array");
    let input = |name: &str| -> &serde_json::Value {
        inputs
            .iter()
            .find(|input| input["name"] == name)
            .unwrap_or_else(|| panic!("input '{name}' in:\n{}", result.stdout))
    };

    // A scalar execution target says so, instead of looking like free text.
    assert_eq!(input("smart_target")["format"], "execution-target", "got:\n{}", result.stdout);

    // And an array of them says so about its elements.
    let targets = input("review_targets");
    assert_eq!(targets["type"], "array", "got:\n{}", result.stdout);
    assert_eq!(targets["items"]["format"], "execution-target", "got:\n{}", result.stdout);
    assert_eq!(targets["items"]["type"], "string", "got:\n{}", result.stdout);

    // Every key is present even where the input declared none of it, so a
    // reader tests values rather than testing for the absence of keys.
    let plain = input("change_ref");
    for key in ["default", "validate", "format", "items", "properties", "positional"] {
        assert!(
            plain[key].is_null(),
            "'{key}' should be null on an input that declares none; got:\n{}",
            result.stdout
        );
    }
    assert_eq!(
        input("fix_prepare")["validate"],
        "^(none|branch|worktree|fork)$",
        "got:\n{}",
        result.stdout
    );
}

/// §FS-rhei-templates.6.3: naming a template after reading the list answers
/// with its detail — source, input schema, and an instantiation hint — instead
/// of an argument error.
#[test]
fn templates_with_a_name_shows_the_template_detail() {
    let dir = unique_temp_dir("templates-detail");

    let result = run_raw(&["templates", "spec-review"], &dir);
    assert_success(&result);
    for expected in [
        "Template: spec-review".to_string(),
        "spec (string, required)".to_string(),
        "Source: built-in".to_string(),
        "Instantiate it with:".to_string(),
        format!("rhei instantiate spec-review spec={}", shell_quote("<value>")),
    ] {
        assert!(
            result.stdout.contains(&expected),
            "detail should contain '{expected}'; got:\n{}",
            result.stdout
        );
    }

    // JSON detail is one object in the list's entry shape.
    let result = run_raw(&["templates", "spec-review", "--json"], &dir);
    assert_success(&result);
    let value: serde_json::Value =
        serde_json::from_str(&result.stdout).expect("detail should be valid JSON");
    assert_eq!(value["name"], "spec-review", "got:\n{}", result.stdout);
    assert!(value["inputs"].is_array(), "got:\n{}", result.stdout);

    // A miss still gets the resolver's error, with its suggestion machinery.
    let result = run_raw(&["templates", "spec-reveiw"], &dir);
    assert!(!result.status.success(), "unknown template should fail");
    assert!(
        result.stderr.contains("Did you mean 'spec-review'?"),
        "unknown template should suggest the close name; got:\n{}",
        result.stderr
    );
}

#[test]
fn templates_lists_project_local_templates() {
    let dir = unique_temp_dir("templates-list");
    let template_dir = dir.join(".agent-grounds/rhei/templates/hello");
    fs::create_dir_all(&template_dir).expect("create template dir");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        r#"name: hello
version: 1.0.0
description: Simple hello-world template
inputs:
  - name: target
    description: Greeting target
  - name: punctuation
    description: Greeting suffix
    required: false
    default: "!"
"#,
    );
    write_fixture_file(
        &template_dir,
        "plan.rhei.md",
        r#"# Rhei: Hello {{target}}

## Tasks

### Task 1: Greet {{target}}
**State:** pending
"#,
    );

    let result = run_raw(&["templates", "--source", "project"], &dir);
    assert_success(&result);
    assert!(
        result.stdout.contains("hello"),
        "expected template name in output; got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("inputs: target, punctuation?"),
        "expected input summary in output; got:\n{}",
        result.stdout
    );
    assert!(
        !result.stdout.contains(&template_dir.display().to_string()),
        "expected short template name only, without template path; got:\n{}",
        result.stdout
    );
}

#[test]
fn instantiate_without_template_lists_available_templates() {
    let dir = unique_temp_dir("templates-instantiate-list");
    let template_dir = dir.join(".agent-grounds/rhei/templates/hello");
    fs::create_dir_all(&template_dir).expect("create template dir");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        r#"name: hello
version: 1.0.0
description: Simple hello-world template
"#,
    );
    write_fixture_file(
        &template_dir,
        "plan.rhei.md",
        r#"# Rhei: Hello

## Tasks

### Task 1: Greet
**State:** pending
"#,
    );

    let result = run_raw(&["instantiate"], &dir);
    assert_success(&result);
    assert!(
        result.stdout.contains("Templates:") && result.stdout.contains("hello  1.0.0  project"),
        "expected instantiate without template to list templates; got:\n{}",
        result.stdout
    );
}

#[test]
fn instantiate_unknown_template_suggests_close_match() {
    let dir = unique_temp_dir("templates-instantiate-suggest");
    let template_dir = dir.join(".agent-grounds/rhei/templates/code-review");
    fs::create_dir_all(&template_dir).expect("create template dir");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        r#"name: code-review
version: 1.0.0
description: Review code changes
"#,
    );
    write_fixture_file(
        &template_dir,
        "plan.rhei.md",
        r#"# Rhei: Code Review

## Tasks

### Task 1: Review
**State:** pending
"#,
    );

    let result = run_raw(&["instantiate", "code-reveiw"], &dir);
    assert!(
        !result.status.success(),
        "command should fail for unknown template\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stderr.contains("Did you mean 'code-review'?"),
        "expected close template suggestion; got:\n{}",
        result.stderr
    );
}

#[test]
fn instantiate_renders_template_variables_and_validates_output() {
    let dir = unique_temp_dir("templates-instantiate");
    let template_dir = dir.join("hello-template");
    fs::create_dir_all(&template_dir).expect("create template dir");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        r#"name: hello-template
version: 1.0.0
description: Simple hello-world template
inputs:
  - name: target
    description: Greeting target
"#,
    );
    write_fixture_file(
        &template_dir,
        "plan.rhei.md",
        r#"# Rhei: Hello {{target}}

## Tasks

### Task 1: Greet {{target}}
**State:** pending

Say hello to {{target}}.
"#,
    );

    let output_dir = dir.join("output");
    let result = run_raw(
        &[
            "instantiate",
            template_dir.to_str().expect("template path"),
            "--set",
            "target=World",
            "--output",
            output_dir.to_str().expect("output path"),
        ],
        &dir,
    );
    assert_success(&result);
    assert!(
        result.stdout.contains("Instantiate this template with:"),
        "expected instantiate hint in output; got:\n{}",
        result.stdout
    );
    // The repro command renders the output path relative to the working
    // directory it is pasted from. §FS-rhei-templates.6.1.3

    // The template path is matched on its own, because the printed command is
    // quoted for a shell and a Windows path's backslashes make it a quoted word.
    assert!(
        result.stdout.contains(&template_dir.display().to_string()),
        "the repro command names the template it instantiated; got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("--set target=World --output output"),
        "expected reproducible instantiate command in output; got:\n{}",
        result.stdout
    );

    let rendered = fs::read_to_string(output_dir.join("plan.rhei.md")).expect("read rendered plan");
    assert!(rendered.contains("# Rhei: Hello World"));
    assert!(rendered.contains("### Task 1: Greet World"));
    assert!(rendered.contains("Say hello to World."));
}

// Report paths compare against `current_dir`, which resolves symlinks, so a
// symlinked parent made every path fall back to its absolute spelling.
// §FS-rhei-templates.6.1.3

// macOS resolves `/tmp` and `/var` into `/private`, which is where that came
// from. Unix-only: the case needs a directory symlink, which an unprivileged
// Windows process cannot create.
#[cfg(unix)]
#[test]
fn instantiate_report_is_relative_under_a_symlinked_working_directory() {
    let real = unique_temp_dir("templates-symlinked-real");
    let link_root = unique_temp_dir("templates-symlinked-link");
    let link = link_root.join("workdir");
    std::os::unix::fs::symlink(&real, &link).expect("symlink the working directory");

    let template_dir = link.join("hello-template");
    fs::create_dir_all(&template_dir).expect("create template dir");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        r#"name: hello-template
version: 1.0.0
description: Simple hello-world template
inputs:
  - name: target
    description: Greeting target
"#,
    );
    write_fixture_file(
        &template_dir,
        "plan.rhei.md",
        r#"# Rhei: Hello {{target}}

## Tasks

### Task 1: Greet {{target}}
**State:** pending
"#,
    );

    let output_dir = link.join("output");
    let result = run_raw(
        &[
            "instantiate",
            template_dir.to_str().expect("template path"),
            "--set",
            "target=World",
            "--output",
            output_dir.to_str().expect("output path"),
        ],
        &link,
    );
    assert_success(&result);
    assert!(
        result.stdout.contains("--output output"),
        "the output path should render relative to the symlinked working directory; got:\n{}",
        result.stdout
    );
    assert!(
        !result.stdout.contains(&format!("--output {}", output_dir.display())),
        "the absolute spelling should not appear; got:\n{}",
        result.stdout
    );
}

#[test]
fn instantiate_prints_output_tree_task_tail_and_stop_reason() {
    let dir = unique_temp_dir("templates-instantiate-summary");
    let template_dir = dir.join("summary-template");
    fs::create_dir_all(&template_dir).expect("create template dir");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        r#"name: summary-template
version: 1.0.0
description: Template with enough tasks to exercise the summary
"#,
    );
    write_fixture_file(
        &template_dir,
        "plan.rhei.md",
        r#"# Rhei: Summary Demo

## Tasks

### Task 1: Step 1
**State:** pending

### Task 2: Step 2
**State:** pending

Body for step 2.

### Task 3: Step 3
**State:** pending

### Task 4: Step 4
**State:** pending

### Task 5: Step 5
**State:** pending

### Task 6: Step 6
**State:** pending

Body for step 6.
"#,
    );

    let output_dir = dir.join("output");
    let result = run_raw(
        &[
            "instantiate",
            template_dir.to_str().expect("template path"),
            "--output",
            output_dir.to_str().expect("output path"),
        ],
        &dir,
    );
    assert_success(&result);
    assert!(
        result.stdout.contains("=== Instantiation Summary ===")
            && result.stdout.contains("Files:\n"),
        "expected pretty instantiate summary in output; got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("`-- plan.rhei.md"),
        "expected materialized file tree in instantiate output; got:\n{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("Task tree:\n  - Task plan.1: Step 1 [pending]"),
        "expected task tree in instantiate output; got:\n{}",
        result.stdout
    );
    let last_tasks = result
        .stdout
        .split("Recent task definitions:\n")
        .nth(1)
        .and_then(|section| section.split("Stopped:\n").next())
        .unwrap_or_else(|| {
            panic!("expected Recent task definitions section; got:\n{}", result.stdout)
        });
    assert!(
        last_tasks.contains("--- Task plan.2: Step 2 [pending] ---")
            && last_tasks
                .contains("### Task plan.2: Step 2\n**State:** pending\n\nBody for step 2.")
            && last_tasks.contains("--- Task plan.6: Step 6 [pending] ---")
            && last_tasks
                .contains("### Task plan.6: Step 6\n**State:** pending\n\nBody for step 6.")
            && !last_tasks.contains("Task plan.1: Step 1 [pending]"),
        "expected the last five rendered task definitions, excluding task 1; got:\n{}",
        last_tasks
    );
    assert!(
        result.stdout.contains("Stopped:\n  instantiation stopped before execution; next ready task is Task plan.1: Step 1 [pending]."),
        "expected stop reason in instantiate output; got:\n{}",
        result.stdout
    );
}

/// §FS-rhei-templates.1: the built-in `hourly-human-intervention` template
/// instantiates and reports what it laid down.
///
/// It stands in a temporary directory of its own. It used to stand in the
/// checkout — `CARGO_MANIFEST_DIR/../..` canonicalized — and that is how the
/// derived home above put a `.home/` at the repository root. Nothing the
/// summary asserts depends on the working directory: the template is built into
/// the binary rather than found by searching upwards, the task ids come from
/// `--output`, and the summary is byte-identical from a directory that is no
/// git repository at all. §REQ-test-isolation.1
#[test]
fn instantiate_project_hourly_human_intervention_template_prints_summary() {
    let dir = unique_temp_dir("templates-hourly-human-intervention");
    let output_dir = dir.join("hourly");

    let result = run_raw(
        &[
            "instantiate",
            "hourly-human-intervention",
            "--output",
            output_dir.to_str().expect("output path"),
        ],
        &dir,
    );
    assert_success(&result);
    assert!(
        result.stdout.contains("=== Instantiation Summary ===")
            && result.stdout.contains("Files:")
            && result.stdout.contains(".agent-grounds/")
            && result.stdout.contains("Task tree:")
            && result.stdout.contains(
                "Task hourly.fetch-issues: Fetch and classify human-intervention issues [fetch]"
            )
            && result.stdout.contains("Recent task definitions:")
            && result.stdout.contains(
                "### Task hourly.fetch-prs: Fetch and classify human-intervention pull requests"
            )
            && result.stdout.contains(
                "Task hourly.fetch-prs: Fetch and classify human-intervention pull requests [fetch]"
            )
            && result.stdout.contains(
                "Task hourly.follow-up-rhei-prs: Follow up on RHEI pull requests [rhei-pr-follow-up]"
            )
            && result.stdout.contains("Stopped:"),
        "expected hourly template instantiation summary; got:\n{}",
        result.stdout
    );
}

#[test]
fn instantiate_accepts_manifest_declared_positional_input() {
    let dir = unique_temp_dir("templates-positional");
    let template_dir = dir.join("positional-template");
    fs::create_dir_all(&template_dir).expect("create template dir");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        r#"name: positional-template
version: 1.0.0
description: Template with positional input
inputs:
  - name: target
    description: Greeting target
    positional: 1
"#,
    );
    write_fixture_file(
        &template_dir,
        "plan.rhei.md",
        r#"# Rhei: Hello {{target}}

## Tasks

### Task 1: Greet {{target}}
**State:** pending
"#,
    );

    let output_dir = dir.join("output");
    let result = run_raw(
        &[
            "instantiate",
            template_dir.to_str().expect("template path"),
            "World",
            "--output",
            output_dir.to_str().expect("output path"),
        ],
        &dir,
    );
    assert_success(&result);

    let rendered = fs::read_to_string(output_dir.join("plan.rhei.md")).expect("read rendered plan");
    assert!(rendered.contains("# Rhei: Hello World"));
}

#[test]
fn instantiate_execute_accepts_run_args_after_separator() {
    let dir = unique_temp_dir("templates-execute-run-args");
    let template_dir = dir.join("execute-template");
    fs::create_dir_all(&template_dir).expect("create template dir");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        r#"name: execute-template
version: 1.0.0
description: Template that immediately executes
"#,
    );
    write_fixture_file(
        &template_dir,
        "plan.rhei.md",
        r#"# Rhei: Execute Template
**States:** execute-template

## Tasks

### Task 1: Step
**State:** pending
"#,
    );
    write_fixture_file(
        &template_dir,
        "states.yaml",
        r#"name: execute-template
version: 1
states:
  pending:
    description: Pending
  completed:
    description: Done
    final: true
transitions:
  - from: pending
    to: completed
profiles:
  default: { initial: pending, allowed: [pending, completed] }
node_policy:
  root: default
  default: default
"#,
    );

    let output_dir = dir.join("output");
    let result = run_raw(
        &[
            "instantiate",
            template_dir.to_str().expect("template path"),
            "--execute",
            "--output",
            output_dir.to_str().expect("output path"),
            "--",
            "--dry-run",
            "--parallel",
            "3",
            "--no-agent",
        ],
        &dir,
    );
    assert_success(&result);
    assert!(
        result.stdout.contains("Instantiated template 'execute-template'")
            && result.stdout.contains("Running plan 'Execute Template'"),
        "expected instantiation followed by run output; got stdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        !result.stderr.contains("does not accept positional inputs"),
        "run arguments after -- must not be treated as template inputs; got stderr:\n{}",
        result.stderr
    );
}

#[test]
fn instantiate_maps_single_required_input_to_one_bare_value() {
    let dir = unique_temp_dir("templates-single-required");
    let template_dir = dir.join("single-template");
    fs::create_dir_all(&template_dir).expect("create template dir");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        r#"name: single-template
version: 1.0.0
description: Template with one required input
inputs:
  - name: target
    description: Greeting target
"#,
    );
    write_fixture_file(
        &template_dir,
        "plan.rhei.md",
        r#"# Rhei: Hello {{target}}

## Tasks

### Task 1: Greet {{target}}
**State:** pending
"#,
    );

    let output_dir = dir.join("output");
    let result = run_raw(
        &[
            "instantiate",
            template_dir.to_str().expect("template path"),
            "World",
            "--output",
            output_dir.to_str().expect("output path"),
        ],
        &dir,
    );
    assert_success(&result);

    let rendered = fs::read_to_string(output_dir.join("plan.rhei.md")).expect("read rendered plan");
    assert!(rendered.contains("# Rhei: Hello World"));
}

#[test]
fn instantiate_relocates_root_settings_json_into_agents_rhei_dir() {
    let dir = unique_temp_dir("templates-settings-bundling");
    let template_dir = dir.join("settings-template");
    fs::create_dir_all(&template_dir).expect("create template dir");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        r#"name: settings-template
version: 1.0.0
description: Template that bundles project settings
inputs:
  - name: workspace_id
    description: Linear workspace id
"#,
    );
    write_fixture_file(
        &template_dir,
        "plan.rhei.md",
        r#"# Rhei: Bundled settings demo

## Tasks

### Task 1: Demo
**State:** pending
"#,
    );
    write_fixture_file(
        &template_dir,
        "settings.json",
        r#"{
  "mcp_servers": {
    "linear": {
      "command": ["npx", "-y", "@modelcontextprotocol/server-linear"],
      "env": { "LINEAR_WORKSPACE": "{{workspace_id}}" }
    }
  }
}
"#,
    );

    let output_dir = dir.join("output");
    let result = run_raw(
        &[
            "instantiate",
            template_dir.to_str().expect("template path"),
            "--set",
            "workspace_id=acme-engineering",
            "--output",
            output_dir.to_str().expect("output path"),
        ],
        &dir,
    );
    assert_success(&result);

    assert!(
        !output_dir.join("settings.json").exists(),
        "template settings.json should not be written at output root"
    );
    assert!(
        !output_dir.join(".rhei/settings.json").exists(),
        "template settings.json should not be written under .rhei"
    );
    let rendered_settings =
        fs::read_to_string(output_dir.join(".agent-grounds/rhei/settings.json"))
            .expect("read .agent-grounds/rhei/settings.json");
    let parsed: serde_json::Value =
        serde_json::from_str(&rendered_settings).expect("rendered settings.json is valid JSON");
    assert_eq!(
        parsed["mcp_servers"]["linear"]["env"]["LINEAR_WORKSPACE"], "acme-engineering",
        "instantiation variable should be substituted in settings.json"
    );
}

#[test]
fn instantiate_enforces_validate_on_nested_array_item_property() {
    let dir = unique_temp_dir("templates-nested-validate");
    let template_dir = dir.join("nested-validate-template");
    fs::create_dir_all(&template_dir).expect("create template dir");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        r#"name: nested-validate-template
version: 1.0.0
description: Template with a validate on a nested array-item property
inputs:
  - name: targets
    description: Target list
    type: array
    items:
      type: object
      properties:
        id:
          type: string
          validate: "[a-z][a-z0-9-]*"
        path:
          type: string
"#,
    );
    write_fixture_file(
        &template_dir,
        "plan.rhei.md",
        r#"# Rhei: Nested validate

## Tasks

### Task review: Review targets
**State:** pending

{% for target in targets %}
- {{ target.id }} :: {{ target.path }}
{% endfor %}
"#,
    );

    // An id that violates the nested `validate` pattern must fail
    // instantiation, with an error that points at the offending nested path.
    write_fixture_file(&dir, "bad-values.yaml", "targets:\n  - id: Bad_ID\n    path: src\n");
    let bad = run_raw(
        &[
            "instantiate",
            template_dir.to_str().expect("template path"),
            "--values",
            dir.join("bad-values.yaml").to_str().expect("values path"),
            "--output",
            dir.join("bad-output").to_str().expect("output path"),
        ],
        &dir,
    );
    assert!(
        !bad.status.success(),
        "expected instantiation to fail on invalid nested id; stdout:\n{}\nstderr:\n{}",
        bad.stdout,
        bad.stderr
    );
    let combined = format!("{}{}", bad.stdout, bad.stderr);
    assert!(
        combined.contains("targets[0].id")
            && combined.contains("does not match validation pattern"),
        "error should point at the nested property; got stdout:\n{}\nstderr:\n{}",
        bad.stdout,
        bad.stderr
    );

    // A valid id renders successfully.
    write_fixture_file(&dir, "good-values.yaml", "targets:\n  - id: backend\n    path: src\n");
    let good = run_raw(
        &[
            "instantiate",
            template_dir.to_str().expect("template path"),
            "--values",
            dir.join("good-values.yaml").to_str().expect("values path"),
            "--output",
            dir.join("good-output").to_str().expect("output path"),
        ],
        &dir,
    );
    assert_success(&good);
}

#[test]
fn instantiate_rejects_template_settings_json_with_malformed_render() {
    let dir = unique_temp_dir("templates-settings-malformed");
    let template_dir = dir.join("bad-template");
    fs::create_dir_all(&template_dir).expect("create template dir");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        r#"name: bad-template
version: 1.0.0
description: Template with malformed settings.json
inputs:
  - name: key
    description: Arbitrary string
"#,
    );
    write_fixture_file(
        &template_dir,
        "plan.rhei.md",
        r#"# Rhei: Broken settings

## Tasks

### Task 1: Demo
**State:** pending
"#,
    );
    // Missing opening brace after rendering makes this invalid JSON.
    write_fixture_file(
        &template_dir,
        "settings.json",
        r#"{
  "mcp_servers": {
    "linear": "{{key}}"
  }
"#,
    );

    let output_dir = dir.join("output");
    let result = run_raw(
        &[
            "instantiate",
            template_dir.to_str().expect("template path"),
            "--set",
            "key=oops",
            "--output",
            output_dir.to_str().expect("output path"),
        ],
        &dir,
    );
    assert!(
        !result.status.success(),
        "expected instantiation to fail on malformed settings.json; stdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stderr.contains("settings.json") || result.stdout.contains("settings.json"),
        "error should mention settings.json; got stdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
}

/// §FS-rhei-templates.1: built-in templates ship inside the binary, so a
/// directory with no `.agent-grounds/` and an empty HOME still has a usable library.
#[test]
fn templates_ships_a_builtin_library_with_the_binary() {
    let dir = unique_temp_dir("templates-builtin");
    let home = dir.join(".home");
    fs::create_dir_all(&home).expect("create isolated home");

    let run = |args: &[&str]| -> CliRun {
        let output = rhei_command(&home)
            .current_dir(&dir)
            .args(args)
            .output()
            .expect("rhei command should run");
        CliRun::from(&output)
    };

    let listing = run(&["templates"]);
    assert!(listing.status.success(), "templates should succeed: {}", listing.stderr);
    assert!(
        !listing.stdout.contains("No templates found"),
        "a fresh install must have templates:\n{}",
        listing.stdout
    );
    assert!(
        listing.stdout.contains("changeset-review") && listing.stdout.contains("built-in"),
        "built-ins are listed and labelled:\n{}",
        listing.stdout
    );

    // A built-in instantiates like any other template, from a directory that
    // holds no templates of its own.
    let out = dir.join("out");
    let instantiated = run(&[
        "instantiate",
        "parallel-worktrees",
        "--set",
        "task=Bump the linter",
        "--output",
        out.to_str().expect("utf-8 path"),
    ]);
    assert!(instantiated.status.success(), "instantiate should succeed: {}", instantiated.stderr);
    assert!(out.join("index.rhei.md").is_file(), "the workspace was rendered");
    assert!(out.join("states.yaml").is_file(), "the bundled state machine came along");
    assert!(out.join("tasks").is_dir(), "nested template directories are extracted too");
}

/// §FS-rhei-templates.1: built-ins sit last in the search order, so a project
/// template of the same name shadows one — that is how a built-in is customized.
#[test]
fn a_project_template_shadows_a_builtin_of_the_same_name() {
    let dir = unique_temp_dir("templates-shadow");
    let home = dir.join(".home");
    fs::create_dir_all(&home).expect("create isolated home");
    let template_dir = dir.join(".agent-grounds/rhei/templates/spec-review");
    fs::create_dir_all(&template_dir).expect("create template dir");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        "name: spec-review\nversion: 9.9.9\ndescription: Locally overridden spec review\n",
    );
    write_fixture_file(
        &template_dir,
        "plan.rhei.md",
        "# Rhei: Local\n\n## Tasks\n\n### Task 1: Go\n**State:** pending\n",
    );

    let output = rhei_command(&home)
        .current_dir(&dir)
        .arg("templates")
        .output()
        .expect("rhei templates should run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Locally overridden spec review"),
        "the project template wins:\n{stdout}"
    );
    assert_eq!(
        stdout.matches("spec-review").count(),
        1,
        "the shadowed built-in is not listed twice:\n{stdout}"
    );
}

/// A minimal template whose plan declares its own state machine, so placement
/// inside a project has something to reconcile. §FS-rhei-templates.6.2
fn write_machine_template(dir: &std::path::Path, name: &str) {
    let template_dir = dir.join(".agent-grounds/rhei/templates").join(name);
    fs::create_dir_all(&template_dir).expect("create template dir");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        &format!(
            r#"name: {name}
version: 1.0.0
description: Template declaring the {name} machine
inputs:
  - name: subject
    description: What the task covers
"#
        ),
    );
    write_fixture_file(
        &template_dir,
        "states.yaml",
        &format!(
            r#"name: {name}
version: 1
states:
  review:
    description: Look at it
    initial: true
  done:
    description: Finished
    final: true
transitions:
  - from: review
    to: done
"#
        ),
    );
    // A Directory Workspace, because that is what project discovery counts as
    // a rhei — a single-file template renders a plain directory. §AR-rhei-panta.1
    write_fixture_file(
        &template_dir,
        "index.rhei.md",
        &format!(
            r#"# Rhei: {name}
**States:** {name}
"#
        ),
    );
    fs::create_dir_all(template_dir.join("tasks")).expect("create tasks dir");
    write_fixture_file(
        &template_dir,
        "tasks/01-review.md",
        r#"### Task 1: Review {{subject}}
**State:** review
"#,
    );
}

/// §FS-rhei-templates.6.2: inside a project the default output is the project,
/// and the member rhei keeps the machine it declares — the manifest stays bare.
#[test]
fn instantiate_defaults_into_the_enclosing_project_keeping_its_machine() {
    let dir = unique_temp_dir("instantiate-project-default");
    write_machine_template(&dir, "audit");
    fs::create_dir_all(dir.join(".git")).expect("mark repo root");
    assert!(run_raw(&["init", "--here"], &dir).status.success(), "init should succeed");

    let result = run_raw(&["instantiate", "audit", "subject=payments"], &dir);
    assert!(
        result.status.success(),
        "instantiate should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stdout.contains("Added to the Panta project"),
        "placement must be reported:\n{}",
        result.stdout
    );
    assert!(
        !result.stdout.contains("Adopted state machine"),
        "nothing is adopted — the rhei keeps its own machine:\n{}",
        result.stdout
    );
    assert!(dir.join("audit").is_dir(), "output belongs next to index.panta.md");

    let manifest = fs::read_to_string(dir.join("index.panta.md")).expect("read manifest");
    assert!(!manifest.contains("**States:**"), "manifest stays bare:\n{manifest}");

    // The whole point: the project can be listed and validated afterwards.
    let listed = run_raw(&["list"], &dir);
    assert!(listed.status.success(), "list should succeed: {}", listed.stderr);
    assert!(listed.stdout.contains("audit.1"), "ticket should be listed:\n{}", listed.stdout);
    assert!(run_raw(&["validate"], &dir).status.success(), "project should validate");
}

/// Like `write_machine_template`, but the review state carries a mock agent
/// target, so a project composed from these templates can actually `run`.
fn write_runnable_machine_template(dir: &std::path::Path, name: &str) {
    write_machine_template(dir, name);
    let template_dir = dir.join(".agent-grounds/rhei/templates").join(name);
    write_fixture_file(
        &template_dir,
        "states.yaml",
        &format!(
            r#"name: {name}
version: 1
models: [default-model]
states:
  review:
    description: Look at it
    initial: true
    target: mock[yolo]:mock:default-model
    agent_timeout: 5s
  done:
    description: Finished
    final: true
transitions:
  - from: review
    to: done
"#
        ),
    );
}

/// Project-root settings wiring the `mock` agent the runnable templates
/// target: a no-op script, so `run` exercises machine dispatch, not agents.
fn write_mock_agent_settings(dir: &std::path::Path) {
    // §FS-rhei-states.3.3: a state that can finish the ticket writes its result.
    let script = write_python_agent(
        dir,
        "mock-agent.py",
        r#"result('## Result\n\nMock agent finished {}.\n'.format(env('RHEI_STATE')))
"#,
    );
    let command = fixture_command(&script);
    let settings_dir = dir.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    fs::write(
        settings_dir.join("settings.json"),
        format!(
            r#"{{
  "agents": {{
    "mock": {{
      "command": {command},
      "stdin_prompt": true,
      "timeout": "5s",
      "modes": {{ "yolo": [] }}
    }}
  }},
  "models": {{
    "default-model": {{ "provider": "mock", "model": "default-model", "default_agent": "mock" }}
  }}
}}"#
        ),
    )
    .expect("write settings");
}

/// §FS-rhei-templates.6.2: templates declaring different machines compose in
/// one project — each member rhei is validated, listed, and run under the
/// machine it declares. The journey the removed single-machine rule refused.

// §DA-per-rhei-state-machines
#[test]
fn instantiate_composes_templates_with_different_machines_into_one_project() {
    let dir = unique_temp_dir("instantiate-project-compose");
    write_runnable_machine_template(&dir, "audit");
    write_runnable_machine_template(&dir, "triage");
    fs::create_dir_all(dir.join(".git")).expect("mark repo root");
    assert!(run_raw(&["init", "--here"], &dir).status.success(), "init should succeed");
    write_mock_agent_settings(&dir);

    let first = run_raw(&["instantiate", "audit", "subject=payments"], &dir);
    assert!(
        first.status.success(),
        "the first template should land\nstdout:\n{}\nstderr:\n{}",
        first.stdout,
        first.stderr
    );
    let second = run_raw(&["instantiate", "triage", "subject=inbox"], &dir);
    assert!(
        second.status.success(),
        "a template with a different machine joins the same project\nstdout:\n{}\nstderr:\n{}",
        second.stdout,
        second.stderr
    );

    let manifest = fs::read_to_string(dir.join("index.panta.md")).expect("read manifest");
    assert!(!manifest.contains("**States:**"), "no machine is hoisted:\n{manifest}");

    let listed = run_raw(&["list"], &dir);
    assert!(listed.status.success(), "list should succeed: {}", listed.stderr);
    assert!(
        listed.stdout.contains("audit.1") && listed.stdout.contains("triage.1"),
        "both rheis' tickets should be listed:\n{}",
        listed.stdout
    );
    assert!(run_raw(&["validate"], &dir).status.success(), "project should validate");

    // One run drives both rheis, each ticket under its own machine.
    let run = run_raw(&["run", "--no-tui", "--no-callbacks"], &dir);
    assert!(
        run.status.success(),
        "run should drive both machines\nstdout:\n{}\nstderr:\n{}",
        run.stdout,
        run.stderr
    );
    for rhei in ["audit", "triage"] {
        let task = fs::read_to_string(dir.join(rhei).join("tasks/01-review.md"))
            .expect("read ticket after run");
        assert!(task.contains("**State:** done"), "{rhei}.1 should finish as done:\n{task}");
    }
}

/// §FS-rhei-templates.4: a member rhei's settings resolve at the project root,
/// so a template's bundled registry is hoisted there instead of being ignored.
#[test]
fn instantiate_hoists_template_settings_to_the_project_root() {
    let dir = unique_temp_dir("instantiate-project-settings");
    write_machine_template(&dir, "audit");
    write_fixture_file(
        &dir.join(".agent-grounds/rhei/templates/audit"),
        "settings.json",
        r#"{
  "defaults": { "agent_timeout": "42m" },
  "agents": {
    "codex": {
      "command": ["codex", "exec"],
      "modes": { "xhigh": ["--effort", "xhigh"] }
    }
  }
}
"#,
    );
    fs::create_dir_all(dir.join(".git")).expect("mark repo root");
    assert!(run_raw(&["init", "--here"], &dir).status.success(), "init should succeed");

    let result = run_raw(&["instantiate", "audit", "subject=payments"], &dir);
    assert!(result.status.success(), "instantiate should succeed:\n{}", result.stderr);
    assert!(
        result.stdout.contains("Merged the template's agent settings"),
        "the hoist must be reported:\n{}",
        result.stdout
    );

    let hoisted = dir.join(".agent-grounds/rhei/settings.json");
    let settings = fs::read_to_string(&hoisted).expect("project settings should exist");
    assert!(settings.contains("codex"), "the agent registry should land here:\n{settings}");
    assert!(
        !dir.join("audit/.agent-grounds/rhei/settings.json").exists(),
        "no dead copy may stay in the workspace, where nothing reads it"
    );
}
