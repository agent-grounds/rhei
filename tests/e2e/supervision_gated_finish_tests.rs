// A supervisor that finishes through a human gate, on the three surfaces that
// print a machine's warnings: `rhei validate`, `rhei instantiate`, and the
// start of `rhei run`. What those surfaces say about a supervisor that really
// cannot finish lives next door in `supervision_surfaces_tests.rs`.

// §AR-source-file-size.3 §FS-rhei-supervision.1.2

use std::fs;
use std::path::Path;

use super::supervision_tests::setup_supervision;
use super::templates_tests::run_raw;
use super::*;

/// The substring every surface's no-terminal-edge warning carries.
const NO_WAY_TO_FINISH: &str = "no way to finish";

/// The machine from `agent-grounds/rhei#158`: `moderating` supervises, its only
/// `openDescendants` edge lands on the gate `human-judgment`, and the gate
/// transitions to the final state `completed`. It finishes; the path that shows
/// it is two hops rather than one.
// §FS-rhei-supervision.4.1
const GATED_SUPERVISOR: &str = r#"name: gated-supervisor
version: 1.0

states:
  moderating:
    description: Supervise the subtree
    execute_on: child-terminal
    agent: pi
    agent_timeout: 30s
    visits: 4
  human-judgment:
    description: A human rules on the verdict
    gating: true
  completed:
    description: Done
    final: true
  cancelled:
    description: Dropped
    final: true

transitions:
  - { from: moderating, to: human-judgment, description: Subtree closed; a human judges, condition: openDescendants < 1 }
  - { from: moderating, to: moderating, description: Released the subtree }
  - { from: human-judgment, to: completed, description: The ruling is recorded }
  - { from: "*", to: cancelled, description: Dropped }

profiles:
  default:
    initial: moderating
    allowed: [moderating, human-judgment, completed, cancelled]

node_policy:
  root: default
  default: default
"#;

const GATED_PLAN: &str = r#"# Rhei: Gated supervisor

---
structure:
  maxLevels: 3
---

## Tasks

### Task 1: Matter
**State:** moderating

#### Task 1.1: Statement
**State:** moderating
"#;

/// A template whose workspace carries the same machine, so `rhei instantiate`
/// loads it the way the report did.
fn write_gated_template(dir: &Path) -> PathBuf {
    let template_dir = dir.join(".agent-grounds/rhei/templates/gated-supervisor");
    fs::create_dir_all(template_dir.join("tasks")).expect("create template dir");
    write_fixture_file(
        &template_dir,
        "template.yaml",
        "name: gated-supervisor\nversion: 1.0.0\ndescription: A supervisor that finishes \
         through a gate.\n",
    );
    write_fixture_file(&template_dir, "index.rhei.md", "# Rhei: Gated supervisor\n");
    write_fixture_file(&template_dir, "states.yaml", GATED_SUPERVISOR);
    write_fixture_file(
        &template_dir.join("tasks"),
        "01-matter.md",
        "### Task matter: Matter\n**State:** moderating\n\n#### Task matter.statement: Statement\n\
         **State:** moderating\n",
    );
    template_dir
}

/// Everything a surface printed, so one assertion can name every surface that
/// warned rather than stopping at the first.
fn combined(result: &CliRun) -> String {
    format!("{}{}", result.stdout, result.stderr)
}

/// A supervisor whose `openDescendants` edge reaches a final state *through* a
/// gate is not warned about, on any surface that prints the warning.
///
/// The rule is reachability from the target of the `openDescendants` edge, not
/// the target's own `final:` flag. `moderating -> human-judgment -> completed`
/// is how the machine is meant to end, and it is the path a recorded run took,
/// so a warning that the supervisor "has no way to finish" is false — and its
/// wording invites the one repair that would delete the deliberate human gate.
// §FS-rhei-supervision.1.2 §FS-rhei-supervision.4.1 §FS-rhei-run.3
#[test]
fn a_supervisor_that_finishes_through_a_gate_is_not_warned_about() {
    let (dir, plan_path, machine_path) =
        setup_supervision("supervision-gated-finish", GATED_PLAN, GATED_SUPERVISOR, "");
    write_gated_template(&dir);

    let validated = run_cli("validate", &plan_path, &machine_path, &[]);
    assert_success(&validated);
    // §FS-rhei-run.3: the run prints the machine's warnings before it schedules
    // anything, so the false positive greets every run of the workspace.
    let ran =
        run_cli("run", &plan_path, &machine_path, &["--no-callbacks", "--no-tui", "--dry-run"]);
    assert_success(&ran);
    let instantiated = run_raw(&["instantiate", "gated-supervisor", "--output", "out"], &dir);
    assert_success(&instantiated);

    // Every surface at once: which of the three still warn is the whole
    // finding, so stopping at the first would report a third of it.
    let warned: Vec<&str> = [
        ("`rhei validate`", combined(&validated)),
        ("the start of `rhei run`", combined(&ran)),
        ("`rhei instantiate`", combined(&instantiated)),
    ]
    .iter()
    .filter(|(_, output)| output.contains(NO_WAY_TO_FINISH))
    .map(|(surface, _)| *surface)
    .collect();
    assert!(
        warned.is_empty(),
        "{} said a supervisor which reaches 'completed' through the gate 'human-judgment' has \
         no way to finish; `rhei validate` said:\n{}",
        warned.join(", "),
        combined(&validated)
    );
}
