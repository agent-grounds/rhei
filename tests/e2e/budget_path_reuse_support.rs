//! The world the path-reuse cases share: a home, machine settings and a fixture
//! agent that all **outlive** the project directory.
//!
//! That is the whole of what is special here, and it is why the project sits in
//! a subdirectory rather than at the test root: the sequence under test deletes
//! the project and lays it down again at the same path, and a home inside it
//! would be deleted with it — which is the *other* workaround the ticket names,
//! not the case being pinned.

// §FS-rhei-budgets.5.4
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::budget_support::*;
use super::*;

/// A home, machine settings and a fixture agent that all **outlive** the
/// project directory.
///
/// That is the whole fixture, and it is why the project sits in a subdirectory
/// rather than at the test root: the sequence under test deletes the project
/// and lays it down again at the same path, and a home inside it would be
/// deleted with it — which is the *other* workaround the ticket names, not the
/// case being pinned.
pub(super) struct World {
    dir: TestDir,
    pub(super) project: PathBuf,
    machine: PathBuf,
}

pub(super) fn world(prefix: &str) -> World {
    let dir = unique_temp_dir(prefix);
    let agent = write_python_agent(&dir, "mock-agent.py", FINISHING_AGENT);
    write_machine_settings(&dir, &agent_defaults(&agent, ""));
    let machine = write_fixture_file(&dir, "states.yaml", FINISHING_MACHINE);
    let project = dir.join("p");
    World { dir, project, machine }
}

impl World {
    /// Lay the project down at its path, as copying an example there does.
    pub(super) fn lay(&self) -> PathBuf {
        fs::create_dir_all(&self.project).expect("create the project directory");
        write_fixture_file(&self.project, "plan.rhei.md", PLAN)
    }

    /// Clear the scratch directory, as anyone clearing one would.
    pub(super) fn clear(&self) {
        fs::remove_dir_all(&self.project).expect("remove the project directory");
    }

    /// Every `rhei` these cases spawn.
    ///
    /// `rhei_command` pins `HOME` and `XDG_STATE_HOME` under this world's own
    /// home, which is what keeps a test off the developer's real budget
    /// authority: a case that reached it would write receipts and `roots.json`
    /// entries against real project paths.
    ///
    /// The parent reservation is removed because this suite is itself run from
    /// inside a `rhei run` on the machine that maintains it, so the descriptor
    /// is in the environment and every case here would refuse for an unrelated
    /// reason. §FS-rhei-budgets.7.1
    pub(super) fn rhei(&self) -> Command {
        let mut cmd = rhei_command(home_for(&self.dir));
        cmd.env_remove("RHEI_BUDGET_PARENT_RESERVATION");
        cmd
    }

    pub(super) fn run(&self) -> CliRun {
        let mut cmd = self.rhei();
        cmd.arg("--state-machine")
            .arg(&self.machine)
            .arg("run")
            .arg(self.project.join("plan.rhei.md"))
            .arg("--no-tui")
            .arg("--no-callbacks");
        CliRun::from(&cmd.output().expect("rhei run should run"))
    }

    /// `rhei budget <args…>` with the project directory as the target, which is
    /// the spelling an operator has and the one a refusal names back to them.
    pub(super) fn budget(&self, args: &[&str]) -> CliRun {
        CliRun::from(&self.budget_output(args))
    }

    /// The bytes, undecoded. The JSON case asserts that the error object is
    /// **one line**, so it cannot read a stderr the renderer's soft wraps have
    /// already been undone in. §FS-rhei-errors.2
    pub(super) fn budget_output(&self, args: &[&str]) -> std::process::Output {
        let mut cmd = self.rhei();
        cmd.arg("budget");
        for arg in args {
            cmd.arg(arg);
        }
        cmd.output().expect("rhei budget should run")
    }

    pub(super) fn show(&self) -> CliRun {
        self.budget(&["show", &self.project.display().to_string()])
    }

    pub(super) fn show_json(&self) -> std::process::Output {
        self.budget_output(&["show", &self.project.display().to_string(), "--format", "json"])
    }

    pub(super) fn forget(&self, reason: &str) -> CliRun {
        self.budget(&["forget", &self.project.display().to_string(), "--reason", reason])
    }

    pub(super) fn authority(&self) -> PathBuf {
        home_for(&self.dir).join("state/rhei/budget-authority")
    }

    pub(super) fn roots_index(&self) -> String {
        fs::read_to_string(self.authority().join("roots.json")).unwrap_or_default()
    }

    /// The one uuid the witness directory holds, which is the identity the
    /// first project was given.
    pub(super) fn sole_witness_uuid(&self) -> String {
        let mut found: Vec<String> = fs::read_dir(self.authority())
            .expect("the witness directory exists")
            .map(|entry| entry.expect("witness entry"))
            .filter(|entry| entry.file_type().expect("witness entry type").is_dir())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name != "retired")
            .collect();
        found.sort();
        assert_eq!(found.len(), 1, "this world holds exactly one live account; got {found:?}");
        found.remove(0)
    }

    pub(super) fn witness(&self, uuid: &str) -> PathBuf {
        self.authority().join(uuid).join("history.jsonl")
    }

    pub(super) fn account_dir(&self, uuid: &str) -> PathBuf {
        self.project.join(".agent-grounds/rhei/budgets").join(uuid)
    }

    /// The project path in the spelling the product prints it: canonical,
    /// because macOS exposes a temporary directory under two names and the
    /// refusal names the resolved one.
    pub(super) fn canonical_project(&self) -> String {
        rhei_core::platform::canonical_path(&self.project)
            .expect("the project path resolves")
            .display()
            .to_string()
    }
}

/// Everything a command wrote, wherever it put it. A report owed on a failure
/// may land on either stream, and a case reading only one would pass on a build
/// that moved it.
pub(super) fn said(result: &CliRun) -> String {
    format!("{}{}", result.stdout, result.stderr)
}

pub(super) fn receipt_count(witness: &Path) -> usize {
    fs::read_to_string(witness).expect("the witness history is readable").lines().count()
}

/// Lay the project, run it once, and clear and lay it again: the sequence
/// `examples/subtree-supervision/README.md` documents, performed twice with a
/// `rm -rf` in between.
///
/// Returns the uuid the first project was given and the receipts its witness
/// holds — the two facts the second run's refusal has to be able to state.
pub(super) fn reuse_the_path(world: &World) -> (String, usize) {
    world.lay();
    let first = world.run();
    assert_success(&first);
    assert_spawn_count(&world.project, 1, "the first project runs to its terminal state");

    let uuid = world.sole_witness_uuid();
    let receipts = receipt_count(&world.witness(&uuid));
    assert!(receipts > 0, "the first run committed receipts to the witness");

    world.clear();
    world.lay();
    assert!(
        !world.account_dir(&uuid).exists(),
        "the project laid down again has no account directory of its own"
    );
    (uuid, receipts)
}
