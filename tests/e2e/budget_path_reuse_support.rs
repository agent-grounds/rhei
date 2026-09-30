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
    world_named(prefix, "p")
}

/// The same world with the project's own directory named, for the case that
/// needs a path an operator plausibly has and a printed command has to survive:
/// one with a space in it. §FS-rhei-budgets.10
pub(super) fn world_named(prefix: &str, project: &str) -> World {
    let dir = unique_temp_dir(prefix);
    let agent = write_python_agent(&dir, "mock-agent.py", FINISHING_AGENT);
    write_machine_settings(&dir, &agent_defaults(&agent, ""));
    let machine = write_fixture_file(&dir, "states.yaml", FINISHING_MACHINE);
    let project = dir.join(project);
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

    /// The witness base in the spelling the **tool** resolves it to.
    ///
    /// Two things have to match, not one. `Authority::lock_at` resolves the state
    /// directory before it joins anything, so an expectation built from the raw
    /// temporary path is a second spelling of one location — which is every macOS
    /// runner, where `/var` is a link to `/private/var`, and every Windows one,
    /// where the temporary path arrives in its 8.3 form. And it joins the
    /// multi-component literal `rhei/budget-authority` as a *single* component,
    /// so on Windows that inner `/` survives inside a `\` path: this joins the
    /// same literal the same way, because the expectation is the string the tool
    /// prints rather than the tidiest spelling of the same place.
    pub(super) fn authority(&self) -> PathBuf {
        resolved(&home_for(&self.dir).join("state")).join("rhei/budget-authority")
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

    /// The account directory in the spelling the tool prints, for the same two
    /// reasons [`World::authority`] gives: `Account` resolves the root first and
    /// joins `ACCOUNT_DIR` as one component.
    pub(super) fn account_dir(&self, uuid: &str) -> PathBuf {
        resolved(&self.project).join(".agent-grounds/rhei/budgets").join(uuid)
    }

    /// The project path in the spelling the product prints where it names the
    /// **resolved** root — the witness index's key, which every offered command
    /// and every path member carries. The `Project:` line and `project_root` are
    /// the target as this fixture typed it instead, so they are asserted against
    /// `self.project` rather than this. §FS-rhei-budgets.10
    pub(super) fn canonical_project(&self) -> String {
        resolved(&self.project).display().to_string()
    }

    /// Run a command line the report offered, from this world's own directory.
    ///
    /// The working directory matters: an unquoted path with a space in it makes
    /// `mkdir` build a tree *relative to where the operator stood*, so a case
    /// that ran the offered line from the checkout would write six directory
    /// levels into it. Here a badly quoted command can only litter a temporary
    /// directory, and the assertions that follow it still fail.
    /// §FS-rhei-budgets.10
    pub(super) fn shell(&self, line: &str) -> std::process::Output {
        rhei_core::platform::system_shell_command(line)
            .current_dir(&self.dir)
            .output()
            .expect("the offered command should run")
    }
}

/// `canonical_path`, resolving the deepest ancestor that exists and keeping the
/// rest — the same reading `authority.rs`'s `resolve_existing` makes, so a base
/// the first run has not created yet still answers with the spelling it will
/// have.
fn resolved(path: &Path) -> PathBuf {
    let mut ancestor = path;
    loop {
        if let Ok(base) = rhei_core::platform::canonical_path(ancestor) {
            let rest = path.strip_prefix(ancestor).expect("an ancestor prefixes its own path");
            // Never `join` an empty remainder: that appends a separator, and a
            // path with a trailing one is not the string the tool printed.
            return if rest.as_os_str().is_empty() { base } else { base.join(rest) };
        }
        match ancestor.parent() {
            Some(parent) => ancestor = parent,
            None => return path.to_path_buf(),
        }
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
