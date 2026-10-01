//! Trees for state-machine resolution: a Panta project whose manifest may or
//! may not declare, holding members that may or may not declare, with machine
//! files placed where each case is about.
//!
//! Every machine here uses states the built-in machine lacks, so a plan that
//! validates can only have loaded the file the case placed, and a plan that
//! fails can only have been judged by a machine the case did not place.

use std::path::{Path, PathBuf};

use super::*;

/// A machine named `name` whose two states are `working` and `done`. Callers
/// pick names the built-in machine lacks.
pub fn machine(name: &str, working: &str, done: &str) -> String {
    format!(
        "name: {name}\nversion: 1\nstates:\n  {working}:\n    initial: true\n    \
         description: Not a built-in state\n  {done}:\n    final: true\n    \
         description: Done\ntransitions:\n  - from: {working}\n    to: {done}\n"
    )
}

/// A ticket machine with two terminals, one of them `cancelled` — the shape
/// every instantiated ticket template ships, cut to what these cases need.
pub fn ticket_machine(name: &str) -> String {
    format!(
        "name: {name}\nversion: 1\nstates:\n  pending:\n    initial: true\n    \
         description: Ready for work\n  completed:\n    final: true\n    \
         description: Done\n  cancelled:\n    final: true\n    \
         description: Abandoned\ntransitions:\n  - from: pending\n    to: completed\n  \
         - from: \"*\"\n    to: cancelled\n"
    )
}

/// Run `rhei <args>` from `cwd`. No `--state-machine` unless `args` passes one:
/// where the file sits is what every case here is about.
pub fn rhei_in(cwd: &Path, home: &Path, args: &[&str]) -> CliRun {
    let mut cmd = rhei_command(home);
    cmd.current_dir(cwd).args(args);
    CliRun::from(&cmd.output().expect("rhei command should run"))
}

pub fn assert_validates(result: &CliRun) {
    assert_success(result);
    assert!(
        result.stdout.contains("Validation succeeded"),
        "validation should succeed; got:\n{}\n{}",
        result.stdout,
        result.stderr
    );
}

/// A Panta project root. `declares` is the manifest's `**States:**` value, or
/// `None` for a manifest that declares nothing at all — the shape an agent
/// runtime's work root has once it stops writing the line.
pub fn project(dir: &Path, declares: Option<&str>) -> PathBuf {
    let root = dir.join("project");
    std::fs::create_dir_all(&root).expect("create the project");
    let manifest = match declares {
        Some(name) => format!("# Panta: Resolution\n**States:** {name}\n"),
        None => "# Panta: Resolution\n".to_owned(),
    };
    write_fixture_file(&root, "index.panta.md", &manifest);
    root
}

/// The `states.yaml` at the project root.
pub fn project_machine(root: &Path, contents: &str) -> PathBuf {
    write_fixture_file(root, "states.yaml", contents)
}

/// A directory-workspace member rhei with one task. `declares` is its own
/// `**States:**` value, or `None` for an index that declares nothing.
pub fn member(root: &Path, id: &str, declares: Option<&str>, task_state: &str) -> PathBuf {
    let rhei = root.join(id);
    std::fs::create_dir_all(rhei.join("tasks")).expect("create the member");
    let index = match declares {
        Some(name) => format!("# Rhei: {id}\n**States:** {name}\n"),
        None => format!("# Rhei: {id}\n"),
    };
    write_fixture_file(&rhei, "index.rhei.md", &index);
    write_fixture_file(
        &rhei.join("tasks"),
        "01.md",
        &format!("### Task 1: Placed\n**State:** {task_state}\n"),
    );
    rhei
}

/// The `states.yaml` at a member's own execution root.
pub fn member_machine(rhei: &Path, contents: &str) -> PathBuf {
    write_fixture_file(rhei, "states.yaml", contents)
}

/// The `Source:` line `rhei states` prints for a project-wide default.
pub fn default_source(path: &Path) -> String {
    format!("Source: '{}'", path.display())
}

/// The `Source:` line `rhei states` prints for a machine some rheis run under.
pub fn rhei_source(path: &Path, rheis: &str) -> String {
    format!("Source: '{}' (rhei: {rheis})", path.display())
}

pub fn assert_source(result: &CliRun, expected: &str) {
    assert!(
        result.stdout.lines().any(|line| line == expected),
        "`rhei states` should list {expected:?}; got:\n{}",
        result.stdout
    );
}

/// Every `warning:` line on stderr. The deprecation warnings are the only
/// stderr warnings these trees produce, and resolved-budget lines go to stdout,
/// so counting these counts the deprecation.
/// §FS-rhei-states-deprecation.3
pub fn warnings(result: &CliRun) -> Vec<&str> {
    result.stderr.lines().filter(|line| line.trim_start().starts_with("warning:")).collect()
}

pub fn assert_silent(result: &CliRun, what: &str) {
    assert!(
        warnings(result).is_empty(),
        "{what} must print no deprecation warning; stderr was:\n{}",
        result.stderr
    );
}

/// One `warning:` line, carrying every fragment in `names`. A warning that does
/// not name the file it read and what to do about it sends the reader hunting.
/// §FS-rhei-states-deprecation.3
pub fn assert_one_warning(result: &CliRun, names: &[&str]) {
    let found = warnings(result);
    assert_eq!(
        found.len(),
        1,
        "exactly one warning per distinct declaration; stderr was:\n{}",
        result.stderr
    );
    let said = result.stderr.replace('\n', " ");
    for fragment in names {
        assert!(
            said.contains(fragment),
            "the warning should name {fragment:?}; stderr was:\n{}",
            result.stderr
        );
    }
}
