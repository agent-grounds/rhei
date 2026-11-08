//! Display-based loaders share the mapping cause with states and validate,
//! retaining visualization and template-union context. §FS-rhei-errors.1.2.2

use std::path::Path;

use super::*;

const NON_MAPPINGS: [(&str, &str); 7] = [
    ("empty", ""),
    ("comment-only", "# nothing\n"),
    ("null", "null\n"),
    ("string", "just a string\n"),
    ("integer", "42\n"),
    ("sequence", "- a\n- b\n"),
    ("tagged sequence", "!foo\n- a\n- b\n"),
];

const PLAN: &str = "# Rhei: Plan\n\n## Tasks\n\n### Task a: First\n\n**State:** open\n\nDo it.\n";

const MACHINE: &str = "name: control
version: 1
states:
  open:
    description: waiting
  done:
    description: finished
    final: true
transitions:
  - {from: open, to: done}
profiles:
  work:
    initial: open
    allowed: [open, done]
node_policy:
  root: work
  default: work
";

fn run_in(args: &[&str], dir: &Path) -> CliRun {
    let home = unique_temp_dir("load-display-home");
    let output = rhei_command(&home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("rhei command should run");
    CliRun::from(&output)
}

fn rendered_line<'a>(stderr: &'a str, marker: &str) -> &'a str {
    stderr
        .lines()
        .find_map(|line| line.trim_start().strip_prefix(marker))
        .unwrap_or_else(|| panic!("no `{marker}` line in:\n{stderr}"))
        .trim_end()
}

fn assert_mapping_headline(run: &CliRun, label: &str, expected: &str) {
    assert_eq!(run.status.code(), Some(1), "{label}: {}", run.stderr);
    let headline = rendered_line(&run.stderr, "× ");
    assert_eq!(headline, expected, "{label}: {}", run.stderr);
    assert!(!headline.contains("struct StateMachine"), "{label}: {headline}");
    assert!(!headline.contains("missing field `name`"), "{label}: {headline}");
}

#[test]
fn visualization_uses_the_mapping_cause_for_every_non_mapping() {
    let dir = unique_temp_dir("load-display-viz");
    let plan = write_fixture_file(&dir, "plan.rhei.md", PLAN);
    let machine = write_fixture_file(&dir, "states.yaml", MACHINE);
    let args =
        ["viz", plan.to_str().unwrap(), "--state-machine", "states.yaml", "--output", "flow.html"];

    // The absolute plan path avoids the separate bare-filename history-root bug.
    let control = run_in(&args, &dir);
    assert_success(&control);
    assert!(dir.join("flow.html").is_file(), "the control produces a visualization");

    let expected = format!(
        "failed to collect plans from {}: failed to load state machine states.yaml: \
         invalid state machine: the document must be a mapping",
        plan.display()
    );
    for (label, text) in NON_MAPPINGS {
        std::fs::write(&machine, text).expect("write non-mapping machine");
        let run = run_in(&args, &dir);
        assert_mapping_headline(&run, label, &expected);
        let help = rendered_line(&run.stderr, "help: ");
        assert_eq!(
            help,
            format!(
                "check the path and re-run: rhei viz {} --state-machine states.yaml",
                rhei_core::platform::shell_quote(&plan.display().to_string())
            ),
            "{label}: visualization retains its contextual help"
        );
    }
}

#[test]
fn template_union_uses_the_mapping_cause_for_every_non_mapping() {
    let dir = unique_temp_dir("load-display-union");
    write_fixture_file(&dir, "host.rhei.md", PLAN);
    write_fixture_file(&dir, "states.yaml", MACHINE);
    let template = dir.join("part");
    std::fs::create_dir(&template).expect("create template");
    write_fixture_file(
        &template,
        "template.yaml",
        "name: part\nversion: 1.0.0\ndescription: Mapping diagnosis fixture\n",
    );
    write_fixture_file(&template, "plan.rhei.md", &PLAN.replace("Task a:", "Task b:"));
    let machine = write_fixture_file(&template, "states.yaml", MACHINE);
    let args = ["instantiate", template.to_str().unwrap(), "--into", "host", "--dry-run"];

    let control = run_in(&args, &dir);
    assert_success(&control);

    let expected = "template 'part' has an unreadable state machine: \
                    invalid state machine: the document must be a mapping";
    for (label, text) in NON_MAPPINGS {
        std::fs::write(&machine, text).expect("write non-mapping template machine");
        let run = run_in(&args, &dir);
        assert_mapping_headline(&run, label, expected);
        let (_, help) = run.stderr.split_once("help: ").expect("template refusal has help");
        assert!(help.contains("every template stands alone"), "{label}: {help}");
        assert!(help.contains("before it joins a machine."), "{label}: {help}");
    }
}
