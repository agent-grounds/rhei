//! §FS-rhei-errors: every failing command says what to do next, and every
//! command it prints survives a paste into an interactive shell.

use std::fs;

use super::*;

fn run_raw(args: &[&str], cwd: &std::path::Path) -> CliRun {
    CliRun::from(&capture(args, cwd))
}

/// The finished process itself, for the one test below whose subject is where
/// the renderer broke a line rather than what the binary said.
fn capture(args: &[&str], cwd: &std::path::Path) -> std::process::Output {
    super::rhei_command(cwd.join(".home"))
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("rhei command should run")
}

/// A template with two required inputs and one execution-target input, named
/// something other than `agent` so a hardcoded `agent=` repair example is
/// caught rather than accidentally passing. §FS-rhei-errors.1.2
fn write_agent_template(dir: &std::path::Path) {
    let template_dir = dir.join(".agent-grounds/rhei/templates/guided");
    fs::create_dir_all(&template_dir).expect("create template dir");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        r#"name: guided
version: 1.0.0
description: Template used to exercise error guidance
inputs:
  - name: subject
    description: What to work on
    type: string
    required: true

  - name: brief
    description: How to work on it, at whatever length the work needs
    type: string
    required: true

  - name: worker_agent
    description: Agent target that does the work
    type: string
    format: execution-target
    default: claude-code[yolo]:anthropic:claude-opus-4-7

  - name: reviewers
    description: Agent targets that review the work
    type: array
    items:
      type: string
    default:
      - claude-code[yolo]:anthropic:claude-opus-4-7
      - codex[xhigh]:openai:gpt-5.5
"#,
    );
    write_fixture_file(
        &template_dir,
        "plan.rhei.md",
        r#"# Rhei: {{subject}}

{{brief}}

## Tasks

### Task 1: Work on {{subject}}
**State:** pending
"#,
    );
}

#[test]
fn missing_inputs_are_reported_together_with_a_runnable_command() {
    let dir = unique_temp_dir("errors-missing-inputs");
    write_agent_template(&dir);

    let result = run_raw(&["instantiate", "guided", "--dry-run"], &dir);
    assert!(!result.status.success(), "instantiate should fail: {}", result.stdout);

    // §FS-rhei-errors.1.1: both missing inputs in one report, not one per run.
    assert!(
        result.stderr.contains("missing 2 required inputs"),
        "expected both inputs reported at once; got:\n{}",
        result.stderr
    );
    assert!(result.stderr.contains("subject"), "got:\n{}", result.stderr);
    assert!(result.stderr.contains("brief"), "got:\n{}", result.stderr);

    // §FS-rhei-errors.1.2: the help line is a command, with the placeholders
    // quoted so `<value>` is not read as a shell redirection.
    let placeholder = shell_quote("<value>");
    assert!(
        result.stderr.contains(&format!(
            "rhei instantiate guided subject={placeholder} brief={placeholder}"
        )),
        "expected a runnable suggestion; got:\n{}",
        result.stderr
    );
    assert!(
        result.stderr.contains("rhei instantiate guided --list-inputs"),
        "expected a pointer to --list-inputs; got:\n{}",
        result.stderr
    );
}

#[test]
fn missing_input_suggestion_echoes_arguments_already_supplied() {
    let dir = unique_temp_dir("errors-missing-echo");
    write_agent_template(&dir);

    let result = run_raw(&["instantiate", "guided", "subject=docs", "--dry-run"], &dir);
    assert!(!result.status.success(), "instantiate should fail: {}", result.stdout);
    assert!(
        result.stderr.contains("missing 1 required input"),
        "expected the singular form; got:\n{}",
        result.stderr
    );
    // §FS-rhei-errors.1.2: the suggestion carries what the user already typed.
    assert!(
        result.stderr.contains(&format!(
            "rhei instantiate guided subject=docs brief={}",
            shell_quote("<value>")
        )),
        "expected supplied arguments to be echoed; got:\n{}",
        result.stderr
    );
}

#[test]
fn unknown_input_names_the_near_miss() {
    let dir = unique_temp_dir("errors-unknown-input");
    write_agent_template(&dir);

    let result = run_raw(&["instantiate", "guided", "subjekt=docs", "--dry-run"], &dir);
    assert!(!result.status.success(), "instantiate should fail: {}", result.stdout);
    // §FS-rhei-errors.1.3
    assert!(result.stderr.contains("has no input named 'subjekt'"), "got:\n{}", result.stderr);
    assert!(result.stderr.contains("Did you mean 'subject'?"), "got:\n{}", result.stderr);
}

#[test]
fn unknown_template_names_the_near_miss() {
    let dir = unique_temp_dir("errors-unknown-template");
    write_agent_template(&dir);

    let result = run_raw(&["instantiate", "guidedd", "--dry-run"], &dir);
    assert!(!result.status.success(), "instantiate should fail: {}", result.stdout);
    assert!(result.stderr.contains("Did you mean 'guided'?"), "got:\n{}", result.stderr);
    assert!(result.stderr.contains("rhei templates"), "got:\n{}", result.stderr);
}

#[test]
fn malformed_execution_target_fails_against_the_input_the_user_typed() {
    let dir = unique_temp_dir("errors-execution-target");
    write_agent_template(&dir);

    let result = run_raw(
        &["instantiate", "guided", "subject=a", "brief=b", "worker_agent=codex", "--dry-run"],
        &dir,
    );
    assert!(!result.status.success(), "instantiate should fail: {}", result.stdout);

    // §FS-rhei-errors.3.1: the error names the input, not the rendered file.
    assert!(
        result.stderr.contains("input 'worker_agent' is not a valid execution target: 'codex'"),
        "expected the failure to name the input; got:\n{}",
        result.stderr
    );
    assert!(
        !result.stderr.contains("states.yaml"),
        "the error must not point at a rendered file the user never wrote; got:\n{}",
        result.stderr
    );
    // §FS-rhei-errors.2: quoted, because `[yolo]` is a glob in zsh.
    // §FS-rhei-errors.1.2: and keyed to the input the user typed, so it pastes
    // back instead of producing a fresh "no such input" error.
    assert!(
        result
            .stderr
            .contains(&format!("worker_agent={}", shell_quote("codex[yolo]:openai:gpt-5.5"))),
        "expected a shell-quoted example keyed to the input; got:\n{}",
        result.stderr
    );
}

#[test]
fn the_suggested_execution_target_repair_can_be_pasted_back() {
    let dir = unique_temp_dir("errors-execution-target-paste");
    write_agent_template(&dir);

    let failed = run_raw(
        &["instantiate", "guided", "subject=a", "brief=b", "worker_agent=codex", "--dry-run"],
        &dir,
    );
    // Recover the assignment rhei suggested and hand it straight back. The
    // quote it is wrapped in is the platform's, and stripping it is what the
    // shell the suggestion was printed for would have done.
    let quote = if cfg!(windows) { '"' } else { '\'' };
    let opening = format!("worker_agent={quote}");
    let suggestion = failed
        .stderr
        .split_whitespace()
        .find(|word| word.starts_with(&opening))
        .map(|word| word.trim_end_matches('.').replace(quote, ""))
        .unwrap_or_else(|| panic!("no suggestion found in:\n{}", failed.stderr));

    // §FS-rhei-errors.1.2: a suggested command is a next action, not a hint.
    let retry =
        run_raw(&["instantiate", "guided", "subject=a", "brief=b", &suggestion, "--dry-run"], &dir);
    assert!(
        !retry.stderr.contains("has no input named"),
        "the suggestion named an input that does not exist; got:\n{}",
        retry.stderr
    );
    assert!(
        !retry.stderr.contains("is not a valid execution target"),
        "the suggestion was itself rejected; got:\n{}",
        retry.stderr
    );
}

#[test]
fn every_rejected_input_is_reported_in_one_pass() {
    let dir = unique_temp_dir("errors-rejected-batch");
    write_agent_template(&dir);

    let result = run_raw(
        &[
            "instantiate",
            "guided",
            "subject=a",
            "brief=b",
            "worker_agent=codex",
            "reviewers=not-a-list",
            "--dry-run",
        ],
        &dir,
    );
    assert!(!result.status.success(), "instantiate should fail: {}", result.stdout);

    // §FS-rhei-errors.1.1: two bad values cost one round trip, not two.
    assert!(result.stderr.contains("2 inputs were rejected"), "got:\n{}", result.stderr);
    assert!(result.stderr.contains("worker_agent"), "got:\n{}", result.stderr);
    assert!(result.stderr.contains("reviewers"), "got:\n{}", result.stderr);
}

#[test]
fn agent_flag_given_a_selector_names_the_flags_that_carry_it() {
    let dir = unique_temp_dir("errors-agent-flag");
    write_fixture_file(
        &dir,
        "plan.rhei.md",
        "# Rhei: t\n\n## Tasks\n\n### Task 1: work\n**State:** pending\n",
    );

    let result = run_raw(&["run", "plan.rhei.md", "--agent", "claude-code:some-model"], &dir);
    assert!(!result.status.success(), "run should fail: {}", result.stdout);

    // §FS-rhei-errors.1.2: `--agent` takes a bare id, so pointing the user at
    // `agents.<id>` in settings.json would be a dead end.
    assert!(
        result.stderr.contains("--agent claude-code --model some-model"),
        "expected the flag split to be spelled out; got:\n{}",
        result.stderr
    );
}

#[test]
fn candidate_lists_name_each_agent_once() {
    let dir = unique_temp_dir("errors-agent-dupes");
    write_fixture_file(
        &dir,
        "plan.rhei.md",
        "# Rhei: t\n\n## Tasks\n\n### Task 1: work\n**State:** pending\n",
    );

    let result = run_raw(&["run", "plan.rhei.md", "--agent", "wholly-unrelated-name"], &dir);
    assert!(!result.status.success(), "run should fail: {}", result.stdout);

    // §FS-rhei-errors.1.3: the built-ins are already seeded into the merged
    // registry, so listing both sources would print every id twice.
    assert_eq!(
        result.stderr.matches("claude-code").count(),
        1,
        "expected each agent id once; got:\n{}",
        result.stderr
    );
}

#[test]
fn list_inputs_quotes_values_that_a_shell_would_glob() {
    let dir = unique_temp_dir("errors-list-inputs");
    write_agent_template(&dir);

    let result = run_raw(&["instantiate", "guided", "--list-inputs"], &dir);
    assert_success(&result);
    // §FS-rhei-errors.2: this listing is where users copy values from.
    assert!(
        result.stdout.contains(&format!(
            "default={}",
            shell_quote("claude-code[yolo]:anthropic:claude-opus-4-7")
        )),
        "expected the default selector to be quoted; got:\n{}",
        result.stdout
    );
    assert!(result.stdout.contains("format: execution-target"), "got:\n{}", result.stdout);
    let placeholder = shell_quote("<value>");
    assert!(
        result.stdout.contains(&format!(
            "rhei instantiate guided subject={placeholder} brief={placeholder}"
        )),
        "expected the listing to end on a runnable command; got:\n{}",
        result.stdout
    );
}

#[test]
fn list_inputs_gives_a_pasteable_form_of_a_multi_line_default() {
    let dir = unique_temp_dir("errors-list-inputs-block");
    write_agent_template(&dir);

    let result = run_raw(&["instantiate", "guided", "--list-inputs"], &dir);
    assert_success(&result);

    // The readable block is still there, but its scalars are bare YAML, so
    // §FS-rhei-errors.2 needs a quoted one-line form beside it.
    assert!(result.stdout.contains("default below"), "got:\n{}", result.stdout);
    assert!(
        result.stdout.contains(&format!(
            "copy: reviewers={}",
            shell_quote(
                "[\"claude-code[yolo]:anthropic:claude-opus-4-7\",\
                 \"codex[xhigh]:openai:gpt-5.5\"]"
            )
        )),
        "expected a quoted one-line default; got:\n{}",
        result.stdout
    );
}

#[test]
fn the_long_value_hint_names_the_input_most_likely_to_hold_prose() {
    let dir = unique_temp_dir("errors-long-value-hint");
    write_agent_template(&dir);

    let result = run_raw(&["instantiate", "guided", "--dry-run"], &dir);
    assert!(!result.status.success(), "instantiate should fail: {}", result.stdout);
    // `brief` carries the longer description; suggesting `--set-file` for the
    // one-word `subject` instead would read as noise.
    assert!(result.stderr.contains("--set-file brief=<path>"), "got:\n{}", result.stderr);
}

#[test]
fn missing_file_help_distinguishes_a_typo_from_a_missing_directory() {
    let dir = unique_temp_dir("errors-missing-file");

    // §FS-rhei-errors.6: the directory exists, so the remedy is to look at it.
    let result = run_raw(&["validate", "absent.rhei.md"], &dir);
    assert!(!result.status.success(), "validate should fail: {}", result.stdout);
    assert!(result.stderr.contains("Check the spelling"), "got:\n{}", result.stderr);

    // The directory is missing too, so the remedy is to create it.
    let result = run_raw(&["validate", "no/such/dir/absent.rhei.md"], &dir);
    assert!(!result.status.success(), "validate should fail: {}", result.stdout);
    assert!(result.stderr.contains("mkdir -p"), "got:\n{}", result.stderr);
}

#[test]
fn json_errors_carry_the_same_help_as_text_errors() {
    let dir = unique_temp_dir("errors-json");

    // §FS-rhei-errors.5
    let result = run_raw(&["list", "absent.rhei.md", "--json"], &dir);
    assert!(!result.status.success(), "list should fail: {}", result.stdout);
    let payload: serde_json::Value =
        serde_json::from_str(result.stderr.trim()).expect("stderr should be one JSON object");
    assert!(
        payload["error"]["message"].as_str().is_some_and(|msg| msg.contains("absent.rhei.md")),
        "got:\n{}",
        result.stderr
    );
    assert!(
        payload["error"]["help"].as_str().is_some_and(|help| help.contains("Check the spelling")),
        "expected the help to travel with the JSON error; got:\n{}",
        result.stderr
    );
}

#[test]
fn suggested_commands_are_never_wrapped_mid_command() {
    let dir = unique_temp_dir("errors-no-wrap");
    write_agent_template(&dir);

    let out = capture(&["instantiate", "guided", "--dry-run"], &dir);
    assert!(!out.status.success(), "instantiate should fail: {}", stdout(&out));
    // §FS-rhei-errors.2: the renderer must not break `--list-inputs` across
    // lines. Read raw: where the break fell is the claim, not what was said.
    let rendered = raw_stderr(&out);
    assert!(
        !rendered.contains("--list-\ninputs"),
        "the suggested command was wrapped; got:\n{rendered}"
    );
}

/// A template whose execution-target input is nested inside an array, so the
/// repair example cannot be a plain `name=value` assignment. §FS-rhei-errors.1.2
fn write_nested_target_template(dir: &std::path::Path) {
    let template_dir = dir.join(".agent-grounds/rhei/templates/nested");
    fs::create_dir_all(&template_dir).expect("create template dir");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        r#"name: nested
version: 1.0.0
description: Template whose execution targets live inside an array
inputs:
  - name: reviewers
    description: Agents that review
    type: array
    items:
      type: string
      format: execution-target
    default:
      - claude-code:m
"#,
    );
    write_fixture_file(
        &template_dir,
        "plan.rhei.md",
        "# Rhei: nested\n\n## Tasks\n\n### Task 1: Work\n**State:** pending\n",
    );
}

#[test]
fn a_nested_scalar_is_not_offered_an_assignment_it_cannot_have() {
    let dir = unique_temp_dir("errors-nested-repair");
    write_nested_target_template(&dir);

    let result = run_raw(&["instantiate", "nested", "reviewers=[nomodel]", "--dry-run"], &dir);
    assert!(!result.status.success(), "instantiate should fail: {}", result.stdout);

    // The label is path-qualified, and there is no `reviewers[0]=…` CLI syntax —
    // suggesting one reproduces the very bug this rule exists to prevent.
    assert!(
        result.stderr.contains("reviewers[0]"),
        "the failure should name the element; got:\n{}",
        result.stderr
    );
    assert!(
        !result.stderr.contains("reviewers[0]='"),
        "a nested scalar must not be offered an assignment form; got:\n{}",
        result.stderr
    );
    assert!(
        result.stderr.contains("whole 'reviewers' value"),
        "the remedy should point at the whole input; got:\n{}",
        result.stderr
    );
}

#[test]
fn a_template_with_no_required_inputs_still_states_the_naming_rule() {
    let dir = unique_temp_dir("errors-no-required");
    write_nested_target_template(&dir);

    // Every input is defaulted, so there is no list of names to hand back.
    let result = run_raw(&["instantiate", "nested", "somepositional", "--dry-run"], &dir);
    assert!(!result.status.success(), "instantiate should fail: {}", result.stdout);
    assert!(
        result.stderr.contains("supply values as KEY=VALUE"),
        "expected the naming rule; got:\n{}",
        result.stderr
    );
    assert!(
        !result.stderr.contains("name each value: ."),
        "an empty list must not be rendered into the help; got:\n{}",
        result.stderr
    );
}

#[test]
fn batched_remedies_are_labelled_and_survive_a_narrow_terminal() {
    let dir = unique_temp_dir("errors-batched-labels");
    write_agent_template(&dir);

    let result = run_raw(
        &[
            "instantiate",
            "guided",
            "subject=a",
            "brief=b",
            "worker_agent=bad",
            "reviewers=not-a-list",
            "--dry-run",
        ],
        &dir,
    );
    assert!(!result.status.success(), "instantiate should fail: {}", result.stdout);

    // Remedies live in the help block, which miette re-indents on wrap, and each
    // names the input it repairs now that it is no longer adjacent to it.
    assert!(result.stderr.contains("worker_agent:"), "got:\n{}", result.stderr);
    assert!(result.stderr.contains("reviewers:"), "got:\n{}", result.stderr);
}

#[test]
fn instantiating_over_an_existing_directory_suggests_a_free_name() {
    let dir = unique_temp_dir("errors-collision");
    // Keep the default destination inside this disposable fixture. Without a
    // local project marker, the project walk can discover an unrelated
    // ancestor project when TMPDIR is nested below the test checkout.
    write_fixture_file(&dir, "index.panta.md", "# Panta: Error guidance fixture\n");
    write_agent_template(&dir);

    let first = run_raw(&["instantiate", "guided", "subject=a", "brief=b"], &dir);
    assert_success(&first);

    let second = run_raw(&["instantiate", "guided", "subject=a", "brief=b"], &dir);
    assert!(!second.status.success(), "the second instantiation should fail: {}", second.stdout);
    // §FS-rhei-errors.1.2: the remedy is a command, on the help line.
    assert!(
        second.stderr.contains("help:"),
        "the collision must carry a help line; got:\n{}",
        second.stderr
    );
    assert!(
        second.stderr.contains("--output") && second.stderr.contains("guided-2"),
        "expected a free sibling to be suggested; got:\n{}",
        second.stderr
    );
}
