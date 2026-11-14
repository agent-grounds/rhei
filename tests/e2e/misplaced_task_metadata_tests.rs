//! A task written under a content section instead of under `## Tasks` must be
//! refused whichever field opens its metadata, and never load as a plan that
//! silently lacks it (#719). `**State:**` is the control throughout: it is
//! refused in every shape here, and the other fields must match it. Under a
//! task heading the refusal is #685's: at the heading, naming `## Tasks`.
//! §FS-rhei-plan-language.1.1.1 §FS-rhei-errors.1.7 §FS-rhei-validate.4.2

use std::fs;
use std::path::PathBuf;

use super::new_tests::flattened_output;
use super::*;

/// The refusal of a field with no task heading above it.
const OUTSIDE_A_TASK: &str = "Metadata field appears outside a task";

/// The refusal of a task heading in a content section above `## Tasks`.
/// §FS-rhei-errors.1.7
const HEADING_ABOVE_TASKS: &str = "Task heading outside the `## Tasks` section: it sits in a \
                                   content section above `## Tasks`";

/// The refusal of a task heading in a plan with no `## Tasks` at all.
/// §FS-rhei-errors.1.7
const NO_TASKS_SECTION: &str = "Task heading outside a `## Tasks` section: this plan has no \
                                `## Tasks` section";

/// The generic help, which points at task metadata that is not the mistake.
const METADATA_HELP: &str = "check the plan's task metadata";

/// One valued line per kind of task field, the control first.
const VALUED_FIELDS: [&str; 5] = [
    "**State:** pending",
    "**Model:** sample",
    "**Target:** sample",
    "**MCP servers:** github",
    "**Skills:** release-notes",
];

/// The reporter's plan: a task heading under `## Notes` on line 4, its field on
/// line 5.
fn misplaced_task(field: &str) -> String {
    format!(
        "# Rhei: Italy\n\n## Notes\n### Task 1: Misplaced\n{field}\n\n\
         ## Tasks\n\n### Task 2: Real\n**State:** pending\n"
    )
}

/// §FS-rhei-validate.4.2's plan: two task headings under `## Overview`, on lines
/// 5 and 9, their fields on lines 7 and 11, and no `## Tasks` at all.
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

/// [`misses`], and every fragment of `unsaid` the run said anyway: the field's
/// own refusal, a cascade, or help aimed at lines that are not the mistake.
fn misses_saying(
    field: &str,
    command: &str,
    run: &CliRun,
    must_fail: bool,
    said: &[String],
    unsaid: &[&str],
) -> Vec<String> {
    let mut missed = misses(field, command, run, must_fail, said);
    if !(must_fail && run.status.success()) {
        let output = flattened_output(run);
        missed.extend(
            unsaid
                .iter()
                .filter(|fragment| output.contains(*fragment))
                .map(|fragment| format!("{field}: `{command}` said {fragment:?}:\n{output}")),
        );
    }
    missed
}

fn assert_no_misses(misses: Vec<String>) {
    assert!(misses.is_empty(), "{} miss(es):\n\n{}", misses.len(), misses.join("\n\n"));
}

/// 1 · The ticket. Every command that loads the plan refuses it at the task
/// heading's line, naming `## Tasks` with the edit that moves the task there,
/// directly and through its project. §FS-rhei-plan-language.1.1.1
/// §FS-rhei-errors.1.7
#[test]
fn every_command_refuses_a_misplaced_task_at_its_heading_whichever_field_opens_it() {
    let mut missed = Vec::new();
    for field in VALUED_FIELDS {
        let scopes = Scopes::new("rhei-719-misplaced", &misplaced_task(field));
        let framed = [
            HEADING_ABOVE_TASKS.to_owned(),
            "4| ### Task 1: Misplaced".to_owned(),
            "move the task at line 4 under `## Tasks`".to_owned(),
        ];
        let unframed = [OUTSIDE_A_TASK, METADATA_HELP, "problems)"];
        let runs = [
            ("list", scopes.run("list", false, &[])),
            ("render", scopes.run("render", false, &["--format", "json"])),
            ("validate", scopes.run("validate", false, &[])),
            ("validate (project)", scopes.run("validate", true, &[])),
        ];
        for (command, run) in &runs {
            missed.extend(misses_saying(field, command, run, true, &framed, &unframed));
        }
        let warned = [HEADING_ABOVE_TASKS.to_owned(), "italy.rhei.md:4".to_owned()];
        let listed = scopes.run("list", true, &[]);
        let unwarned = [OUTSIDE_A_TASK];
        missed.extend(misses_saying(field, "list (project)", &listed, false, &warned, &unwarned));
    }
    assert_no_misses(missed);
}

/// 2 · No cascade. `rhei validate` reports the missing `## Tasks` once, at the
/// first task heading, with the edit that adds it, and neither field line nor
/// the structural diagnostic as a fault of its own, the same in every scope.
/// §FS-rhei-errors.1.7 §FS-rhei-validate.4.2
#[test]
fn validate_names_the_missing_tasks_section_once_in_every_scope_whichever_field_opens_them() {
    let said = [
        NO_TASKS_SECTION.to_owned(),
        "5| ### Task 1: one".to_owned(),
        "add a `## Tasks` line above line 5".to_owned(),
    ];
    let unsaid = [
        "problems)",
        OUTSIDE_A_TASK,
        "Tasks section must be the final '##' chapter",
        "Missing '## Tasks' section",
        METADATA_HELP,
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
            missed.extend(misses_saying(field, scope, run, true, &said, &unsaid));
        }
    }
    assert_no_misses(missed);
}

/// 3 · No heading needed. A field line in a content section is refused at its
/// line even with no task heading above it. §FS-rhei-plan-language.1.1.1
#[test]
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
