use std::fs;
use std::path::Path;

use super::{unique_temp_dir, write_fixture_file, CliRun, STATE_MACHINE};

fn run_completions(shell: &str) -> CliRun {
    let output = super::rhei_command(unique_temp_dir("completions-home"))
        .args(["completions", shell])
        .output()
        .expect("rhei command should run");
    CliRun::from(&output)
}

fn run_completions_with_home(home: &Path, args: &[&str]) -> CliRun {
    let output = super::rhei_command(home)
        .arg("completions")
        .args(args)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("XDG_DATA_HOME")
        .output()
        .expect("rhei command should run");
    CliRun::from(&output)
}

fn run_completions_with_xdg(
    home: &Path,
    xdg_config_home: Option<&Path>,
    xdg_data_home: Option<&Path>,
    args: &[&str],
) -> CliRun {
    let mut command = super::rhei_command(home);
    command.arg("completions").args(args);
    match xdg_config_home {
        Some(path) => {
            command.env("XDG_CONFIG_HOME", path);
        }
        None => {
            command.env_remove("XDG_CONFIG_HOME");
        }
    }
    match xdg_data_home {
        Some(path) => {
            command.env("XDG_DATA_HOME", path);
        }
        None => {
            command.env_remove("XDG_DATA_HOME");
        }
    }

    let output = command.output().expect("rhei command should run");
    CliRun::from(&output)
}

fn run_completions_in_dir(current_dir: &Path, home: &Path, args: &[&str]) -> CliRun {
    let output = super::rhei_command(home)
        .arg("completions")
        .args(args)
        .current_dir(current_dir)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("XDG_DATA_HOME")
        .output()
        .expect("rhei command should run");
    CliRun::from(&output)
}

fn run_dynamic_completion(current_dir: &Path, home: &Path, shell: &str, args: &[&str]) -> CliRun {
    let output = super::rhei_command(home)
        .args(args)
        .current_dir(current_dir)
        .env("COMPLETE", shell)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("XDG_DATA_HOME")
        .output()
        .expect("rhei command should run");
    CliRun::from(&output)
}

fn write_project_template(project: &Path, name: &str, description: &str) {
    write_project_template_with_manifest(
        project,
        name,
        &format!(
            r#"name: {name}
version: 1.0.0
description: {description}
inputs: []
"#
        ),
    );
}

fn write_project_template_with_manifest(project: &Path, name: &str, manifest: &str) {
    let template_dir = project.join(".agent-grounds/rhei/templates").join(name);
    fs::create_dir_all(&template_dir).expect("create template directory");
    write_fixture_file(&template_dir, "template.yaml", manifest);
    write_fixture_file(
        &template_dir,
        "plan.rhei.md",
        r#"# Rhei: Completion fixture

## Tasks

### Task 1: Fixture
**State:** pending
"#,
    );
}

fn write_completion_plan(dir: &Path) -> std::path::PathBuf {
    write_fixture_file(
        dir,
        "plan.rhei.md",
        r#"# Rhei: Completion Plan
**States:** integration-test

## Tasks

### Task 1: First step
**State:** draft

### Task 2: Second step
**State:** pending
**Prior:** Task 1
"#,
    )
}

#[test]
fn generates_all_supported_shell_completions() {
    let cases = [
        ("bash", "_clap_complete_rhei"),
        ("zsh", "#compdef rhei"),
        ("fish", "complete --keep-order --exclusive --command rhei"),
        ("powershell", "Register-ArgumentCompleter"),
        ("elvish", "set edit:completion:arg-completer[rhei]"),
    ];

    for (shell, marker) in cases {
        let result = run_completions(shell);
        assert!(
            result.status.success(),
            "{shell} completions should succeed\nstdout:\n{}\nstderr:\n{}",
            result.stdout,
            result.stderr
        );
        assert!(result.stdout.contains(marker), "{shell} output should contain shell marker");
        assert!(
            result.stdout.contains("COMPLETE"),
            "{shell} output should call back into rhei through COMPLETE"
        );
        assert!(
            result.stdout.contains(&format!("# rhei completions for {shell}.")),
            "{shell} output should carry the enablement comment header"
        );
        assert!(
            result.stdout.contains(&format!("rhei completions {shell} --install")),
            "{shell} header should mention the --install alternative"
        );
    }
}

/// `compinit` only autoloads files whose first line is `#compdef`, so the
/// header must come after it. §FS-rhei-completions.5
#[test]
fn zsh_completions_keep_compdef_on_the_first_line() {
    let result = run_completions("zsh");

    assert!(result.status.success(), "zsh completions should succeed");
    let first_line = result.stdout.lines().next().unwrap_or_default();
    assert!(
        first_line.starts_with("#compdef rhei"),
        "first line must stay the #compdef directive, got: {first_line}"
    );
    assert!(result.stdout.contains("# rhei completions for zsh."));
    assert!(result.stdout.contains("source <(rhei completions zsh)"));
}

#[test]
fn generates_bash_completions() {
    let result = run_completions("bash");

    assert!(
        result.status.success(),
        "completions should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(result.stdout.contains("_clap_complete_rhei"));
    assert!(result.stdout.contains("COMPLETE=\"bash\""));
    assert!(result.stdout.contains("-- \"${words[@]}\""));
    assert!(result.stderr.is_empty(), "plain generation should not write diagnostics");
}

#[test]
fn generates_fish_completions() {
    let result = run_completions("fish");

    assert!(
        result.status.success(),
        "completions should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(result.stdout.contains("complete --keep-order --exclusive --command rhei"));
    assert!(result.stdout.contains("COMPLETE=fish"));
    assert!(result.stdout.contains("commandline --current-token"));
    assert!(result.stderr.is_empty(), "plain generation should not write diagnostics");
}

#[test]
fn dynamic_completion_lists_commands() {
    let home = unique_temp_dir("completions-dynamic-home");
    let dir = unique_temp_dir("completions-dynamic-commands");

    let result = run_dynamic_completion(&dir, &home, "fish", &["--", "rhei", ""]);

    assert!(
        result.status.success(),
        "dynamic completion should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(result.stdout.contains("instantiate\t"));
    assert!(result.stdout.contains("templates\t"));
    assert!(result.stdout.contains("install-skills\t"));
}

#[test]
fn dynamic_completion_lists_instantiate_templates() {
    let home = unique_temp_dir("completions-template-home");
    let dir = unique_temp_dir("completions-template-project");
    write_project_template(&dir, "alpha-review", "Alpha review template");
    write_project_template(&dir, "beta-plan", "Beta plan template");

    let result = run_dynamic_completion(&dir, &home, "fish", &["--", "rhei", "instantiate", ""]);

    assert!(
        result.status.success(),
        "template completion should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(result.stdout.contains("alpha-review\tAlpha review template"));
    assert!(result.stdout.contains("beta-plan\tBeta plan template"));
}

#[test]
fn dynamic_completion_filters_instantiate_templates_by_prefix() {
    let home = unique_temp_dir("completions-template-prefix-home");
    let dir = unique_temp_dir("completions-template-prefix-project");
    write_project_template(&dir, "alpha-review", "Alpha review template");
    write_project_template(&dir, "beta-plan", "Beta plan template");

    let result = run_dynamic_completion(&dir, &home, "fish", &["--", "rhei", "instantiate", "alp"]);

    assert!(
        result.status.success(),
        "template completion should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(result.stdout.contains("alpha-review\tAlpha review template"));
    assert!(!result.stdout.contains("beta-plan"));
}

#[test]
fn template_ancestor_discovery_completion_crosses_project_markers() {
    let home = unique_temp_dir("completions-template-nearest-home");
    let outer = unique_temp_dir("completions-template-nearest-outer");
    write_project_template(&outer, "ancestor-review", "Ancestor review template");
    let dir = outer.join("project");
    fs::create_dir_all(&dir).expect("create nested project");
    fs::create_dir_all(dir.join(".git")).expect("create nested Git marker");
    write_fixture_file(&dir, "index.panta.md", "# Panta: Marker fixture\n");
    write_project_template(&dir, "member-plan", "Member plan template");

    let member =
        run_dynamic_completion(&dir, &home, "fish", &["--", "rhei", "instantiate", "member"]);
    let ancestor =
        run_dynamic_completion(&dir, &home, "fish", &["--", "rhei", "instantiate", "ancestor"]);

    assert!(
        member.status.success() && ancestor.status.success(),
        "template completion should succeed\nmember stderr:\n{}\nancestor stderr:\n{}",
        member.stderr,
        ancestor.stderr
    );
    assert!(
        member.stdout.contains("member-plan\tMember plan template"),
        "the nearer completion must remain visible; stdout was:\n{}",
        member.stdout
    );
    assert!(
        ancestor.stdout.contains("ancestor-review\tAncestor review template"),
        "Git and Panta markers must not stop ancestor template completion; stdout was:\n{}",
        ancestor.stdout
    );
}

/// The configured user home is a tier distinction, not an ancestor boundary.
/// Completion must skip its user copy as project-local and keep walking to the
/// genuine project copy above it. §FS-rhei-templates.1.2
#[test]
fn template_completion_crosses_a_nested_user_home() {
    let root = unique_temp_dir("completions-template-nested-home");
    let home = root.join("home");
    let dir = home.join("workspace/member");
    fs::create_dir_all(&dir).expect("create workspace beneath isolated HOME");
    write_project_template(&root, "shared-review", "Project review above HOME");
    write_project_template(&home, "shared-review", "User review at HOME");

    let result =
        run_dynamic_completion(&dir, &home, "fish", &["--", "rhei", "instantiate", "shared"]);

    assert!(
        result.status.success(),
        "template completion should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        result.stdout.contains("shared-review\tProject review above HOME")
            && !result.stdout.contains("User review at HOME"),
        "completion should expose the project copy above HOME; stdout was:\n{}",
        result.stdout
    );
}

#[test]
fn dynamic_completion_lists_template_input_assignments() {
    let home = unique_temp_dir("completions-template-input-home");
    let dir = unique_temp_dir("completions-template-input-project");
    write_project_template_with_manifest(
        &dir,
        "code-review",
        r#"name: code-review
version: 1.0.0
description: Code review template
inputs:
  - name: target
    description: File or directory to review
    type: path
    positional: 1
  - name: review_passes
    description: Number of review iterations
    type: number
    default: 2
"#,
    );

    let result = run_dynamic_completion(
        &dir,
        &home,
        "fish",
        &["--", "rhei", "instantiate", "code-review", ""],
    );

    assert!(
        result.status.success(),
        "input completion should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(result.stdout.contains("target=\tpath, required, positional 1"));
    assert!(result.stdout.contains("review_passes=\tnumber, default 2"));
}

#[test]
fn dynamic_completion_completes_set_keys_and_boolean_values() {
    let home = unique_temp_dir("completions-template-bool-home");
    let dir = unique_temp_dir("completions-template-bool-project");
    write_project_template_with_manifest(
        &dir,
        "toggle",
        r#"name: toggle
version: 1.0.0
description: Toggle template
inputs:
  - name: enabled
    description: Enable the behavior
    type: boolean
"#,
    );

    let keys = run_dynamic_completion(
        &dir,
        &home,
        "fish",
        &["--", "rhei", "instantiate", "toggle", "--set", ""],
    );
    assert!(
        keys.status.success(),
        "set key completion should succeed\nstdout:\n{}\nstderr:\n{}",
        keys.stdout,
        keys.stderr
    );
    assert!(keys.stdout.contains("enabled=\tboolean, required"));

    let values = run_dynamic_completion(
        &dir,
        &home,
        "fish",
        &["--", "rhei", "instantiate", "toggle", "enabled="],
    );
    assert!(
        values.status.success(),
        "boolean value completion should succeed\nstdout:\n{}\nstderr:\n{}",
        values.stdout,
        values.stderr
    );
    assert!(values.stdout.contains("enabled=true\tBoolean true"));
    assert!(values.stdout.contains("enabled=false\tBoolean false"));
}

#[test]
fn dynamic_completion_completes_task_ids_and_transition_targets() {
    let home = unique_temp_dir("completions-task-home");
    let dir = unique_temp_dir("completions-task-project");
    let plan = write_completion_plan(&dir);
    let machine = write_fixture_file(&dir, "states.yaml", STATE_MACHINE);
    let plan_arg = plan.to_str().expect("plan path");
    let machine_arg = machine.to_str().expect("machine path");

    let tasks = run_dynamic_completion(
        &dir,
        &home,
        "fish",
        &["--", "rhei", "next", plan_arg, "--task", ""],
    );
    assert!(
        tasks.status.success(),
        "task completion should succeed\nstdout:\n{}\nstderr:\n{}",
        tasks.stdout,
        tasks.stderr
    );
    assert!(tasks.stdout.contains("1\tFirst step [draft]"));
    assert!(tasks.stdout.contains("2\tSecond step [pending]"));

    // §FS-rhei-panta.6: with the plan positional omitted, task ids complete
    // against the target the command itself would resolve from the cwd.
    let bare = run_dynamic_completion(&dir, &home, "fish", &["--", "rhei", "next", "--task", ""]);
    assert!(
        bare.status.success(),
        "bare-form task completion should succeed\nstdout:\n{}\nstderr:\n{}",
        bare.stdout,
        bare.stderr
    );
    assert!(
        bare.stdout.contains("1\tFirst step [draft]"),
        "omitted positional should fall back to the resolved plan: {}",
        bare.stdout
    );

    let targets = run_dynamic_completion(
        &dir,
        &home,
        "fish",
        &[
            "--",
            "rhei",
            "--state-machine",
            machine_arg,
            "transition",
            plan_arg,
            "--task",
            "1",
            "--to",
            "",
        ],
    );
    assert!(
        targets.status.success(),
        "transition target completion should succeed\nstdout:\n{}\nstderr:\n{}",
        targets.stdout,
        targets.stderr
    );
    assert!(targets.stdout.contains("pending\tReady for work"));
}

/// `rhei transition --<TAB>` offers `--supervisor`, described by what it does.
/// clap drops a hidden candidate only when a visible one shares the typed
/// prefix, so today the flag is offered from `--sup` and denied from `--`:
/// completion half-advertises an option the help denies outright.
/// §FS-rhei-completions.7 §FS-rhei-completions.6.1
#[test]
fn dynamic_completion_offers_the_transition_supervisor_flag() {
    let home = unique_temp_dir("completions-supervisor-home");
    let dir = unique_temp_dir("completions-supervisor-project");
    write_completion_plan(&dir);

    let flags = run_dynamic_completion(&dir, &home, "fish", &["--", "rhei", "transition", "--"]);
    assert!(
        flags.status.success(),
        "transition flag completion should succeed\nstdout:\n{}\nstderr:\n{}",
        flags.stdout,
        flags.stderr
    );

    let candidate =
        flags.stdout.lines().find(|line| line.starts_with("--supervisor\t")).unwrap_or_else(|| {
            panic!("`rhei transition --` should offer `--supervisor`:\n{}", flags.stdout)
        });
    assert!(
        candidate.contains("checkpoint"),
        "the candidate should say what the flag does, not that it is internal: {candidate}"
    );
}

#[test]
fn dynamic_completion_completes_list_filters() {
    let home = unique_temp_dir("completions-list-home");
    let dir = unique_temp_dir("completions-list-project");
    let plan = write_fixture_file(
        &dir,
        "plan.rhei.md",
        r#"# Rhei: Completion List
**States:** integration-test

## Tasks

### Task 1: First step
**State:** draft
**Assignee:** alice

### Task 2: Second step
**State:** pending
**Prior:** Task 1
**Assignee:** bob
"#,
    );
    let machine = write_fixture_file(&dir, "states.yaml", STATE_MACHINE);
    let plan_arg = plan.to_str().expect("plan path");
    let machine_arg = machine.to_str().expect("machine path");

    let states = run_dynamic_completion(
        &dir,
        &home,
        "fish",
        &["--", "rhei", "--state-machine", machine_arg, "list", plan_arg, "--state", "d"],
    );
    assert!(
        states.status.success(),
        "list state completion should succeed\nstdout:\n{}\nstderr:\n{}",
        states.stdout,
        states.stderr
    );
    assert!(states.stdout.contains("draft\tAnalysis phase"));

    let assignees = run_dynamic_completion(
        &dir,
        &home,
        "fish",
        &["--", "rhei", "list", plan_arg, "--assignee", ""],
    );
    assert!(
        assignees.status.success(),
        "list assignee completion should succeed\nstdout:\n{}\nstderr:\n{}",
        assignees.stdout,
        assignees.stderr
    );
    assert!(assignees.stdout.contains("alice\t1 matching task"));
    assert!(assignees.stdout.contains("bob\t1 matching task"));

    let kinds = run_dynamic_completion(
        &dir,
        &home,
        "fish",
        &["--", "rhei", "list", plan_arg, "--kind", ""],
    );
    assert!(
        kinds.status.success(),
        "list kind completion should succeed\nstdout:\n{}\nstderr:\n{}",
        kinds.stdout,
        kinds.stderr
    );
    assert!(kinds.stdout.contains("task\t2 matching tasks"));

    let priors = run_dynamic_completion(
        &dir,
        &home,
        "fish",
        &["--", "rhei", "list", plan_arg, "--has-prior", ""],
    );
    assert!(
        priors.status.success(),
        "list prior completion should succeed\nstdout:\n{}\nstderr:\n{}",
        priors.stdout,
        priors.stderr
    );
    assert!(priors.stdout.contains("1\tFirst step [draft]"));
    assert!(priors.stdout.contains("2\tSecond step [pending]"));

    let limits = run_dynamic_completion(
        &dir,
        &home,
        "fish",
        &["--", "rhei", "list", plan_arg, "--limit", ""],
    );
    assert!(
        limits.status.success(),
        "list limit completion should succeed\nstdout:\n{}\nstderr:\n{}",
        limits.stdout,
        limits.stderr
    );
    assert!(limits.stdout.contains("10\tTen tasks"));
    assert!(limits.stdout.contains("0\tNo limit"));
}

#[test]
fn rejects_unknown_completion_shell() {
    let result = run_completions("tcsh");

    assert!(!result.status.success(), "unknown shell should fail");
    assert!(result.stderr.contains("invalid value"));
    assert!(result.stderr.contains("bash"));
    assert!(result.stderr.contains("powershell"));
}

#[test]
fn generating_to_stdout_does_not_touch_home_completion_paths() {
    let home = unique_temp_dir("completions-stdout-home");

    let result = run_completions_with_home(&home, &["fish"]);

    assert!(
        result.status.success(),
        "stdout generation should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(result.stdout.contains("complete --keep-order --exclusive --command rhei"));
    assert!(!home.join(".config").exists(), "stdout generation should not create files");
}

#[test]
fn installs_fish_completions_to_user_default_path() {
    let home = unique_temp_dir("completions-fish-home");

    let result = run_completions_with_home(&home, &["fish", "--install"]);

    assert!(
        result.status.success(),
        "install should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let path = home.join(".config/fish/completions/rhei.fish");
    assert!(path.exists(), "completion file should be written");
    let script = fs::read_to_string(path).expect("read fish completions");
    assert!(script.contains("complete --keep-order --exclusive --command rhei"));
    assert!(result.stdout.contains("Installed fish completions to"));
}

#[test]
fn installs_fish_completions_to_xdg_config_home() {
    let home = unique_temp_dir("completions-fish-xdg-home");
    let xdg_config_home = unique_temp_dir("completions-fish-xdg-config");

    let result =
        run_completions_with_xdg(&home, Some(&xdg_config_home), None, &["fish", "--install"]);

    assert!(
        result.status.success(),
        "install should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let path = xdg_config_home.join("fish/completions/rhei.fish");
    assert!(path.exists(), "completion file should be written under XDG_CONFIG_HOME");
    let script = fs::read_to_string(path).expect("read fish completions");
    assert!(script.contains("complete --keep-order --exclusive --command rhei"));
}

#[test]
fn installs_bash_completions_to_xdg_data_home() {
    let home = unique_temp_dir("completions-bash-xdg-home");
    let xdg_data_home = unique_temp_dir("completions-bash-xdg-data");

    let result =
        run_completions_with_xdg(&home, None, Some(&xdg_data_home), &["bash", "--install"]);

    assert!(
        result.status.success(),
        "install should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let path = xdg_data_home.join("bash-completion/completions/rhei");
    assert!(path.exists(), "completion file should be written under XDG_DATA_HOME");
    let script = fs::read_to_string(path).expect("read bash completions");
    assert!(script.contains("_clap_complete_rhei"));
}

#[test]
fn installs_completions_to_explicit_output_path() {
    let home = unique_temp_dir("completions-output-home");
    let output_dir = unique_temp_dir("completions-output");
    let output_path = output_dir.join("rhei.bash");
    let output_arg = output_path.to_string_lossy().into_owned();

    let result = run_completions_with_home(&home, &["bash", "--output", &output_arg]);

    assert!(
        result.status.success(),
        "install should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let script = fs::read_to_string(output_path).expect("read bash completions");
    assert!(script.contains("_clap_complete_rhei"));
}

#[test]
fn explicit_output_overwrites_existing_file() {
    let home = unique_temp_dir("completions-overwrite-home");
    let output_dir = unique_temp_dir("completions-overwrite-output");
    let output_path = output_dir.join("rhei.fish");
    fs::write(&output_path, "old contents").expect("write old completions");
    let output_arg = output_path.to_string_lossy().into_owned();

    let result = run_completions_with_home(&home, &["fish", "--output", &output_arg]);

    assert!(
        result.status.success(),
        "install should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let script = fs::read_to_string(output_path).expect("read fish completions");
    assert!(script.contains("complete --keep-order --exclusive --command rhei"));
    assert!(!script.contains("old contents"));
}

#[test]
fn explicit_output_without_parent_writes_to_current_directory() {
    let home = unique_temp_dir("completions-relative-output-home");
    let output_dir = unique_temp_dir("completions-relative-output");

    let result = run_completions_in_dir(&output_dir, &home, &["fish", "--output", "rhei.fish"]);

    assert!(
        result.status.success(),
        "install should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let script = fs::read_to_string(output_dir.join("rhei.fish")).expect("read fish completions");
    assert!(script.contains("complete --keep-order --exclusive --command rhei"));
}

#[test]
fn dry_run_reports_system_install_path_without_writing() {
    let home = unique_temp_dir("completions-dry-run-home");

    let result = run_completions_with_home(&home, &["zsh", "--install", "--system", "--dry-run"]);

    assert!(
        result.status.success(),
        "dry-run should succeed\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(result.stdout.contains("Would install zsh completions to"));
    // The system prefix is a fixed Unix path, but the leaf is joined onto it,
    // and `Path::join` spells the separator the host's way.
    let expected = Path::new("/usr/local/share/zsh/site-functions").join("_rhei");
    assert!(
        result.stdout.contains(&expected.display().to_string()),
        "expected the system install path in the output; got:\n{}",
        result.stdout
    );
}
