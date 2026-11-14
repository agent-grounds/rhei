//! A task written under a content section instead of under `## Tasks` must be
//! refused whichever field opens its metadata, and never load as a plan that
//! silently lacks it (#719). `**State:**` is the control throughout: it is
//! refused in every shape here today, and the other fields must match it.
//! §FS-rhei-plan-language.1.1.1 §FS-rhei-validate.4.2

use std::fs;
use std::path::PathBuf;

use super::new_tests::flattened_output;
use super::*;

/// The refusal on `main` today. #685 rewords it; if that lands first, this is
/// the one place to retarget.
const OUTSIDE_A_TASK: &str = "Metadata field appears outside a task";

/// One valued line per kind of task field, the control first.
const VALUED_FIELDS: [&str; 5] = [
    "**State:** pending",
    "**Model:** sample",
    "**Target:** sample",
    "**MCP servers:** github",
    "**Skills:** release-notes",
];

/// The reporter's plan: a task heading under `## Notes`, its field on line 5.
fn misplaced_task(field: &str) -> String {
    format!(
        "# Rhei: Italy\n\n## Notes\n### Task 1: Misplaced\n{field}\n\n\
         ## Tasks\n\n### Task 2: Real\n**State:** pending\n"
    )
}

/// §FS-rhei-validate.4.2's plan: two task headings under `## Overview` and no
/// `## Tasks` at all, their fields on lines 7 and 11.
fn tasks_under_overview(field: &str) -> String {
    format!(
        "# Rhei: A\n\n## Overview\n\n### Task 1: one\n\n{field}\n\n### Task 2: two\n\n{field}\n"
    )
}

/// Where one plan is put: on its own, as the reporter ran it, and as the one
/// rhei of a Panta project.
struct Scopes {
    _dir: TestDir,
    file: PathBuf,
    project: PathBuf,
    machine: PathBuf,
}

impl Scopes {
    fn new(prefix: &str, plan: &str) -> Self {
        let dir = unique_temp_dir(prefix);
        let alone = dir.join("alone");
        let project = dir.join("project");
        fs::create_dir_all(&alone).expect("create plan dir");
        fs::create_dir_all(&project).expect("create project");
        fs::write(project.join("index.panta.md"), "# Panta: Italy\n").expect("write manifest");
        write_fixture_file(&project, "italy.rhei.md", plan);
        let file = write_fixture_file(&alone, "italy.rhei.md", plan);
        let machine = write_fixture_file(&dir, "states.yaml", STATE_MACHINE);
        Scopes { _dir: dir, file, project, machine }
    }

    fn run(&self, command: &str, through_project: bool, extra: &[&str]) -> CliRun {
        let target = if through_project { &self.project } else { &self.file };
        run_cli(command, target, &self.machine, extra)
    }
}

/// Every way `run` failed to report the `field` line, so one red run shows
/// them all. A project `list` skips a rhei it cannot load and warns rather than
/// failing (§FS-rhei-validate.4.4), so only `must_fail` runs must exit non-zero.
fn misses(
    field: &str,
    command: &str,
    run: &CliRun,
    must_fail: bool,
    said: &[String],
) -> Vec<String> {
    let output = flattened_output(run);
    if must_fail && run.status.success() {
        return vec![format!("{field}: `{command}` accepted the plan:\n{output}")];
    }
    said.iter()
        .filter(|fragment| !output.contains(fragment.as_str()))
        .map(|fragment| format!("{field}: `{command}` never said {fragment:?}:\n{output}"))
        .collect()
}

fn assert_no_misses(misses: Vec<String>) {
    assert!(misses.is_empty(), "{} miss(es):\n\n{}", misses.len(), misses.join("\n\n"));
}

/// 1 · The ticket. Every command that loads the plan refuses it at the field
/// line, directly and through its project. §FS-rhei-plan-language.1.1.1
#[test]
#[ignore = "red until #719: rhei list, render and validate accept a misplaced task whose first \
            field is Model, Target, MCP servers or Skills; implement removes this"]
fn every_command_refuses_a_misplaced_task_whichever_field_opens_it() {
    let mut missed = Vec::new();
    for field in VALUED_FIELDS {
        let scopes = Scopes::new("rhei-719-misplaced", &misplaced_task(field));
        let framed = [OUTSIDE_A_TASK.to_owned(), format!("5| {field}")];
        let runs = [
            ("list", scopes.run("list", false, &[])),
            ("render", scopes.run("render", false, &["--format", "json"])),
            ("validate", scopes.run("validate", false, &[])),
            ("validate (project)", scopes.run("validate", true, &[])),
        ];
        for (command, run) in &runs {
            missed.extend(misses(field, command, run, true, &framed));
        }
        let warned = [OUTSIDE_A_TASK.to_owned(), "italy.rhei.md:5".to_owned()];
        let listed = scopes.run("list", true, &[]);
        missed.extend(misses(field, "list (project)", &listed, false, &warned));
    }
    assert_no_misses(missed);
}

/// 2 · The cascade. `rhei validate` reports each field line and then the
/// structural diagnostic that explains them, the same in both scopes.
/// §FS-rhei-validate.4.2
#[test]
#[ignore = "red until #719: validate reports only the structural error for misplaced tasks opened \
            by Model, Target, MCP servers or Skills; implement removes this"]
fn validate_explains_misplaced_tasks_in_both_scopes_whichever_field_opens_them() {
    let said = [
        "(3 problems)".to_owned(),
        format!("line 7: {OUTSIDE_A_TASK}"),
        format!("line 11: {OUTSIDE_A_TASK}"),
        "Tasks section must be the final '##' chapter".to_owned(),
    ];
    let mut missed = Vec::new();
    for field in VALUED_FIELDS {
        let scopes = Scopes::new("rhei-719-cascade", &tasks_under_overview(field));
        let in_project = scopes.project.join("italy.rhei.md");
        let runs = [
            ("validate", scopes.run("validate", false, &[])),
            ("validate (project file)", run_cli("validate", &in_project, &scopes.machine, &[])),
            ("validate (project)", scopes.run("validate", true, &[])),
        ];
        for (scope, run) in &runs {
            missed.extend(misses(field, scope, run, true, &said));
        }
    }
    assert_no_misses(missed);
}

/// 3 · No heading needed. A field line in a content section is refused at its
/// line even with no task heading above it. §FS-rhei-plan-language.1.1.1
#[test]
#[ignore = "red until #719: a valued Model, Target, MCP servers or Skills line in a content \
            section is kept as section text; implement removes this"]
fn a_task_field_in_a_content_section_is_refused_without_a_heading() {
    let mut missed = Vec::new();
    for field in VALUED_FIELDS {
        let plan = format!(
            "# Rhei: Italy\n\n## Notes\n{field}\n\n## Tasks\n\n### Task 2: Real\n**State:** pending\n"
        );
        let scopes = Scopes::new("rhei-719-unheaded", &plan);
        let framed = [OUTSIDE_A_TASK.to_owned(), format!("4| {field}")];
        for command in ["list", "validate"] {
            missed.extend(misses(field, command, &scopes.run(command, false, &[]), true, &framed));
        }
    }
    assert_no_misses(missed);
}

/// 4 · **Guard.** What the refusal must not reach: a fenced example in a
/// content section, a bold label outside the vocabulary, and the overrides and
/// tooling fields where they belong. §FS-rhei-plan-language.2.1
#[test]
fn list_keeps_fenced_examples_labels_and_in_task_fields() {
    let plan = "# Rhei: Italy\n\n## Notes\n**Target audience:** operators\n\n```markdown\n\
                ### Task 1: Example\n**State:** pending\n**Model:** sample\n**Skills:** notes\n\
                ```\n\n## Tasks\n\n### Task 2: Real\n**State:** pending\n**Model:** sample\n\n\
                ### Task 3: Tooled\n**State:** pending\n**Target:** cdx:openai:gpt-5\n\
                **MCP servers:** github\n**Skills:** release-notes\n";
    let list = Scopes::new("rhei-719-guard", plan).run("list", false, &[]);
    assert_success(&list);
    assert!(
        list.stdout.contains("Task italy.2: Real") && list.stdout.contains("Task italy.3: Tooled"),
        "both real tasks are listed; got:\n{}",
        list.stdout
    );
    assert!(!list.stdout.contains("Example"), "the fenced example is not a task:\n{}", list.stdout);
}
