//! Project settings resolve from `.agent-grounds/rhei/settings.json` first and
//! from the deprecated `.agents/rhei/settings.json` second, and everything rhei
//! writes lands in the new home. §FS-rhei-agents.1.1
//!
//! The observable is `rhei validate`: it composes the merged settings and then
//! resolves every agent a state machine names, so a machine naming an agent
//! that only one settings file declares says which file was read.
//!
//! The `.agents/` cases here are permanent coverage of the deprecated path.

use std::fs;
use std::path::{Path, PathBuf};

use super::agent_grounds_support::{
    assert_deprecation_warning, assert_names_path, assert_silent_about_the_deprecated_home, run_in,
    DEPRECATED, GROUNDS,
};
use super::*;

fn write_plan(dir: &Path) -> PathBuf {
    write_fixture_file(
        dir,
        "plan.rhei.md",
        r#"# Rhei: Settings home fixture

## Tasks

### Task 1: Work
**State:** work
"#,
    )
}

/// A machine whose only outside reference is one agent id, so validation
/// passes exactly when the settings file declaring that id was the one read.
fn write_machine(dir: &Path, file_name: &str, agent: &str) -> PathBuf {
    write_fixture_file(
        dir,
        file_name,
        &format!(
            r#"name: settings-home
version: 1
states:
  work:
    initial: true
    agent: {agent}
    agent_timeout: 30m
    description: Do the work
  completed:
    final: true
    description: Done
transitions:
  - from: work
    to: completed
"#
        ),
    )
}

fn write_settings(dir: &Path, home: &str, agent: &str) {
    let settings_dir = dir.join(home);
    fs::create_dir_all(&settings_dir).expect("create settings directory");
    write_fixture_file(
        &settings_dir,
        "settings.json",
        &format!(
            r#"{{
  "agents": {{
    "{agent}": {{ "command": ["{agent}"], "modes": {{ "yolo": [] }} }}
  }}
}}
"#
        ),
    );
}

fn assert_unknown_agent(result: &CliRun, agent: &str) {
    assert!(
        !result.status.success(),
        "the agent must not resolve; stdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let output = format!("{}{}", result.stdout, result.stderr);
    assert!(
        output.contains("unknown agent") && output.contains(agent),
        "expected an unknown-agent error naming '{agent}'; output was:\n{output}"
    );
}

/// §FS-rhei-agents.1.1: the new home is the project settings file, and reading
/// it is silent. §FS-rhei-templates.1.3
#[test]
fn project_settings_resolve_from_agent_grounds() {
    let dir = unique_temp_dir("grounds-settings-project");
    let plan = write_plan(&dir);
    let machine = write_machine(&dir, "states.yaml", "ground-agent");
    write_settings(&dir, GROUNDS, "ground-agent");

    let result = run_cli("validate", &plan, &machine, &[]);
    assert_success(&result);
    assert_silent_about_the_deprecated_home(&result);
}

/// §FS-rhei-agents.1.1: the deprecated home is still read, and warns, naming
/// both paths. §FS-rhei-templates.1.3
#[test]
fn project_settings_fall_back_to_the_deprecated_home_with_a_warning() {
    let dir = unique_temp_dir("grounds-settings-fallback");
    let plan = write_plan(&dir);
    let machine = write_machine(&dir, "states.yaml", "legacy-agent");
    write_settings(&dir, DEPRECATED, "legacy-agent");

    let result = run_cli("validate", &plan, &machine, &[]);
    assert_success(&result);
    assert_deprecation_warning(
        &result,
        &dir,
        &format!("{DEPRECATED}/settings.json"),
        &format!("{GROUNDS}/settings.json"),
    );
}

/// §FS-rhei-agents.1.1: first match wins and the two project files are never
/// merged. Merging would invent a precedence tier between global and project
/// that nothing else has, and would leave the deprecation with no end.
#[test]
fn project_settings_files_are_never_merged() {
    let dir = unique_temp_dir("grounds-settings-no-merge");
    let plan = write_plan(&dir);
    write_settings(&dir, GROUNDS, "ground-agent");
    write_settings(&dir, DEPRECATED, "legacy-agent");

    let chosen = write_machine(&dir, "chosen.yaml", "ground-agent");
    let chosen_result = run_cli("validate", &plan, &chosen, &[]);
    assert_success(&chosen_result);
    assert_silent_about_the_deprecated_home(&chosen_result);

    let shadowed = write_machine(&dir, "shadowed.yaml", "legacy-agent");
    assert_unknown_agent(&run_cli("validate", &plan, &shadowed, &[]), "legacy-agent");
}

/// §FS-rhei-templates.1.1: rhei never writes to the deprecated home, so a
/// template's root `settings.json` lands in the new one on instantiation.
#[test]
fn instantiation_writes_root_settings_into_agent_grounds() {
    let dir = unique_temp_dir("grounds-instantiate-settings");
    let template_dir = dir.join("settings-template");
    fs::create_dir_all(&template_dir).expect("create template directory");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        "name: settings-template\nversion: 1.0.0\ndescription: Bundles project settings\ninputs: []\n",
    );
    write_fixture_file(
        &template_dir,
        "plan.rhei.md",
        r#"# Rhei: Bundled settings

## Tasks

### Task 1: Work
**State:** pending
"#,
    );
    write_fixture_file(
        &template_dir,
        "settings.json",
        r#"{
  "agents": {
    "bundled-agent": { "command": ["bundled-agent"], "modes": { "yolo": [] } }
  }
}
"#,
    );

    let output_dir = dir.join("output");
    let result = run_in(
        &[
            "instantiate",
            template_dir.to_str().expect("template path"),
            "--output",
            output_dir.to_str().expect("output path"),
        ],
        &dir,
        &dir.join("home"),
    );
    assert_success(&result);

    let written = fs::read_to_string(output_dir.join(GROUNDS).join("settings.json"))
        .expect("bundled settings should be written to the new home");
    assert!(
        written.contains("bundled-agent"),
        "the template's registry should land here:\n{written}"
    );
    assert!(
        !output_dir.join(DEPRECATED).join("settings.json").exists(),
        "rhei's own output must never be a path rhei then warns about"
    );
}

/// §FS-rhei-templates.1.1: the settings hoist writes the new home too. The
/// template deliberately sits in the deprecated home, so discovery keeps
/// exercising the fallback and the hoist target is the only thing under test.
#[test]
fn the_settings_hoist_targets_agent_grounds() {
    let dir = unique_temp_dir("grounds-hoist-settings");
    let template_dir = dir.join(DEPRECATED).join("templates/audit");
    fs::create_dir_all(template_dir.join("tasks")).expect("create template directory");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        "name: audit\nversion: 1.0.0\ndescription: Bundles project settings\ninputs: []\n",
    );
    write_fixture_file(
        &template_dir,
        "states.yaml",
        r#"name: audit
version: 1
states:
  review:
    initial: true
    description: Look at it
  done:
    final: true
    description: Finished
transitions:
  - from: review
    to: done
"#,
    );
    write_fixture_file(&template_dir, "index.rhei.md", "# Rhei: audit\n**States:** audit\n");
    write_fixture_file(
        &template_dir,
        "tasks/01-review.md",
        "### Task 1: Review\n**State:** review\n",
    );
    write_fixture_file(
        &template_dir,
        "settings.json",
        r#"{
  "agents": {
    "hoisted-agent": { "command": ["hoisted-agent"], "modes": { "yolo": [] } }
  }
}
"#,
    );

    fs::create_dir_all(dir.join(".git")).expect("mark the repository root");
    let home = dir.join("home");
    assert_success(&run_in(&["init", "--here"], &dir, &home));
    assert_success(&run_in(&["instantiate", "audit"], &dir, &home));

    let hoisted = fs::read_to_string(dir.join(GROUNDS).join("settings.json"))
        .expect("hoisted settings should land in the new home");
    assert!(
        hoisted.contains("hoisted-agent"),
        "the template's registry should land here:\n{hoisted}"
    );
    assert!(
        !dir.join(DEPRECATED).join("settings.json").exists(),
        "rhei's own output must never be a path rhei then warns about"
    );
}

/// The template for the hoist cases: a directory workspace shipping a root
/// `settings.json`, planted under whichever home the case is about.
fn write_hoist_template(dir: &Path, home: &str, agent: &str) {
    let template_dir = dir.join(home).join("templates/audit");
    fs::create_dir_all(template_dir.join("tasks")).expect("create template directory");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        "name: audit\nversion: 1.0.0\ndescription: Bundles project settings\ninputs: []\n",
    );
    write_fixture_file(
        &template_dir,
        "states.yaml",
        r#"name: audit
version: 1
states:
  review:
    initial: true
    description: Look at it
  done:
    final: true
    description: Finished
transitions:
  - from: review
    to: done
"#,
    );
    write_fixture_file(&template_dir, "index.rhei.md", "# Rhei: audit\n**States:** audit\n");
    write_fixture_file(
        &template_dir,
        "tasks/01-review.md",
        "### Task 1: Review\n**State:** review\n",
    );
    write_fixture_file(
        &template_dir,
        "settings.json",
        &format!(
            r#"{{
  "agents": {{
    "{agent}": {{ "command": ["{agent}"], "modes": {{ "yolo": [] }} }}
  }}
}}
"#
        ),
    );
}

/// §FS-rhei-templates.6.2: the hoist merges into the new home, so the operator's
/// deprecated file is superseded rather than material to move — the generic
/// warning would name the file rhei has just written and cost the template's
/// entries. The deprecated file is reported, and left alone: rhei did not write
/// it.
#[test]
fn the_hoist_reports_the_deprecated_project_settings_file_as_superseded() {
    let dir = unique_temp_dir("grounds-hoist-superseded");
    write_settings(&dir, DEPRECATED, "operator-agent");
    write_hoist_template(&dir, GROUNDS, "bundled-agent");

    fs::create_dir_all(dir.join(".git")).expect("mark the repository root");
    let home = dir.join("home");
    assert_success(&run_in(&["init", "--here"], &dir, &home));
    let result = run_in(&["instantiate", "audit"], &dir, &home);
    assert_success(&result);

    assert!(
        result.stderr.contains("supersedes it"),
        "the hoist must say the merged file supersedes the deprecated one; stderr was:\n{}",
        result.stderr
    );
    assert!(
        !result.stderr.contains("Move it to"),
        "obeying the generic move-warning here drops the template's entries; stderr was:\n{}",
        result.stderr
    );
    assert_names_path(&result.stderr, &dir, &format!("{DEPRECATED}/settings.json"));
    assert_names_path(&result.stderr, &dir, &format!("{GROUNDS}/settings.json"));

    let merged = fs::read_to_string(dir.join(GROUNDS).join("settings.json"))
        .expect("the merge lands in the new home");
    assert!(
        merged.contains("operator-agent") && merged.contains("bundled-agent"),
        "the merge carries the operator's values forward and adds the template's:\n{merged}"
    );

    let left_behind = fs::read_to_string(dir.join(DEPRECATED).join("settings.json"))
        .expect("rhei did not write this file, so it does not delete it");
    assert!(
        !left_behind.contains("bundled-agent"),
        "the deprecated file keeps its pre-merge content:\n{left_behind}"
    );
}

fn run_completion(dir: &Path, home: &Path, args: &[&str]) -> CliRun {
    run_with_complete(dir, home, "fish", args)
}

/// The same, with `COMPLETE` set to whatever the case is about.
fn run_with_complete(dir: &Path, home: &Path, complete: &str, args: &[&str]) -> CliRun {
    let output = rhei_command(home)
        .args(args)
        .current_dir(dir)
        .env("COMPLETE", complete)
        .output()
        .expect("rhei command should run");
    CliRun::from(&output)
}

/// §FS-rhei-templates.1.3: a completion request is answered by a fresh process
/// per Tab press, so the once-per-process guard cannot hold there. The settings
/// are still read — the deprecated home's agent is among the candidates — and
/// nothing is printed over them.
#[test]
fn shell_completion_is_silent_about_the_deprecated_home() {
    let dir = unique_temp_dir("grounds-completion-quiet");
    let plan = write_plan(&dir);
    write_settings(&dir, DEPRECATED, "legacy-agent");
    let home = dir.join("home");

    let completion = run_completion(&dir, &home, &["--", "rhei", "run", "--agent", ""]);
    assert!(
        completion.stdout.contains("legacy-agent"),
        "completion should still read the deprecated settings file; stdout was:\n{}",
        completion.stdout
    );
    assert!(
        completion.stderr.is_empty(),
        "a Tab press must not print over the candidate list; stderr was:\n{}",
        completion.stderr
    );

    // The control: the same tree warns under an ordinary command, so the
    // silence above is the suppression and not a quiet fixture.
    let machine = write_machine(&dir, "completion-control.yaml", "legacy-agent");
    let ordinary = run_cli("validate", &plan, &machine, &[]);
    assert_success(&ordinary);
    assert_deprecation_warning(
        &ordinary,
        &dir,
        &format!("{DEPRECATED}/settings.json"),
        &format!("{GROUNDS}/settings.json"),
    );
}

/// §FS-rhei-templates.1.3: an empty `COMPLETE` and `COMPLETE=0` are
/// `clap_complete`'s own way of turning dynamic completion off, so a run
/// carrying either is ordinary and warns. Reading them as a completion request
/// takes the warning off every command for as long as the switch is exported.
#[test]
fn the_completion_off_switch_is_an_ordinary_run_that_still_warns() {
    let dir = unique_temp_dir("grounds-completion-off");
    let plan = write_plan(&dir).display().to_string();
    let machine = write_machine(&dir, "states.yaml", "legacy-agent").display().to_string();
    write_settings(&dir, DEPRECATED, "legacy-agent");
    let home = dir.join("home");
    let args = ["validate", &plan, "--state-machine", &machine];

    for off in ["0", ""] {
        let result = run_with_complete(&dir, &home, off, &args);
        assert_success(&result);
        assert_deprecation_warning(
            &result,
            &dir,
            &format!("{DEPRECATED}/settings.json"),
            &format!("{GROUNDS}/settings.json"),
        );
    }
}

/// A run that fails at agent or model resolution, before anything is spawned.
fn run_args(flag: &str) -> [&str; 6] {
    ["run", "plan.rhei.md", "--state-machine", "states.yaml", flag, "nope"]
}

/// The output with miette's wrapping collapsed, so a path can be matched
/// together with the words that introduce it.
fn one_line(result: &CliRun) -> String {
    let whole = format!("{}{}", result.stdout, result.stderr);
    whole.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// §FS-rhei-agents.1.1: a message that tells an author where to define an agent
/// or a model names the file the project resolves. Naming the write path to a
/// project on the deprecated home has it create a file that shadows the very
/// registry the error just listed, and the operator's agents go silent.
#[test]
fn remediation_names_the_project_settings_file_that_was_read() {
    let dir = unique_temp_dir("grounds-remediation-path");
    write_plan(&dir);
    write_machine(&dir, "states.yaml", "legacy-agent");
    write_settings(&dir, DEPRECATED, "legacy-agent");
    let home = dir.join("home");

    for (flag, lead) in [("--agent", "`agents.<id>` in"), ("--model", "`models.nope` entry to")] {
        let help = one_line(&run_in(&run_args(flag), &dir, &home));
        assert!(
            help.contains(&format!("{lead} {DEPRECATED}/settings.json")),
            "{flag} must send the author to the file rhei read; output was:\n{help}"
        );
    }

    // The control: the same command in a project on the current home names that
    // home, so the assertion above pins the resolution and not a constant.
    let current = unique_temp_dir("grounds-remediation-current");
    write_plan(&current);
    write_machine(&current, "states.yaml", "ground-agent");
    write_settings(&current, GROUNDS, "ground-agent");
    let help = one_line(&run_in(&run_args("--agent"), &current, &current.join("home")));
    assert!(
        help.contains(&format!("`agents.<id>` in {GROUNDS}/settings.json")),
        "the current home is what this project resolves; output was:\n{help}"
    );
}

/// The hoist template again, with a machine nothing can validate, so
/// `rhei instantiate` fails *after* the hoist has written the project's file.
fn write_unvalidatable_hoist_template(dir: &Path, home: &str, agent: &str) {
    write_hoist_template(dir, home, agent);
    write_fixture_file(
        &dir.join(home).join("templates/audit"),
        "states.yaml",
        r#"name: audit
version: 1
states:
  review:
    initial: true
    agent: nobody-declares-this
    description: Look at it
  done:
    final: true
    description: Finished
transitions:
  - from: review
    to: done
"#,
    );
}

/// §FS-rhei-templates.6.2: validation runs after the hoist, so discarding the
/// output undoes the hoist too. A project with no file at the current home is
/// left reading its deprecated one, and a project that had one gets its
/// pre-merge content back: a failed command cannot change which file is read.
#[test]
fn a_discarded_instantiation_leaves_no_hoist_behind() {
    let dir = unique_temp_dir("grounds-hoist-rollback");
    write_settings(&dir, DEPRECATED, "operator-agent");
    write_unvalidatable_hoist_template(&dir, GROUNDS, "bundled-agent");

    fs::create_dir_all(dir.join(".git")).expect("mark the repository root");
    let home = dir.join("home");
    assert_success(&run_in(&["init", "--here"], &dir, &home));
    let result = run_in(&["instantiate", "audit"], &dir, &home);
    assert!(!result.status.success(), "the template's machine cannot validate");

    assert!(
        !dir.join(GROUNDS).join("settings.json").exists(),
        "the project had no file here, so the failed hoist must leave none"
    );
    let kept = fs::read_to_string(dir.join(DEPRECATED).join("settings.json"))
        .expect("the project still reads the file it always read");
    assert!(
        kept.contains("operator-agent") && !kept.contains("bundled-agent"),
        "the operator's registry is untouched:\n{kept}"
    );

    // The other case: a project that did have a file at the current home gets
    // its pre-merge content back rather than losing it to the merge.
    let existing = dir.join(GROUNDS).join("settings.json");
    write_settings(&dir, GROUNDS, "current-agent");
    let before = fs::read_to_string(&existing).expect("the project's own file");
    let result = run_in(&["instantiate", "audit"], &dir, &home);
    assert!(!result.status.success(), "the template's machine still cannot validate");
    assert_eq!(
        fs::read_to_string(&existing).expect("the project's own file survives"),
        before,
        "a discarded instantiation restores what the merge overwrote"
    );
}
