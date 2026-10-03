//! The world the copied-project cases share: one machine — one home, one state
//! directory, one budget authority — and an `original` project beside it that
//! already holds a charged account, which a case then copies or moves.
//!
//! The project sits in a subdirectory rather than at the test root for the same
//! reason as in [`super::budget_path_reuse_support`]: the home has to outlive
//! what the case does to the project, and here it also has to be **one** home
//! seen by two roots, which is the whole of the incident behind
//! `agent-grounds/rhei#411`.

// §FS-rhei-budgets.5.4.1
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::budget_edge_support::BARE_PING_PONG_MACHINE;
use super::budget_support::*;
use super::*;

pub(super) struct World {
    dir: TestDir,
    machine: PathBuf,
}

/// A machine and an `original` project whose account has been initialized and
/// charged twice, the reproducer's first step.
pub(super) fn world(prefix: &str) -> World {
    let dir = unique_temp_dir(prefix);
    let agent = write_python_agent(&dir, "mock-agent.py", FINISHING_AGENT);
    write_machine_settings(&dir, &agent_defaults(&agent, ""));
    let machine = write_fixture_file(&dir, "states.yaml", BARE_PING_PONG_MACHINE);
    let world = World { dir, machine };
    let original = world.root("original");
    fs::create_dir_all(&original).expect("create the original project");
    write_fixture_file(&original, "plan.rhei.md", PLAN);
    assert_success(&world.budget_in(
        "home",
        &["init", &original.join("plan.rhei.md").display().to_string(), "--invocations", "10"],
        "establish the account",
    ));
    assert_success(&world.edge("original", "work", "review"));
    assert_success(&world.edge("original", "review", "work"));
    world
}

impl World {
    /// A project root under this world, by name.
    pub(super) fn root(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    /// The root as the tool spells it where it names one: canonical. The root
    /// has to exist; a moved-away root is spelled from before the move.
    pub(super) fn canonical(&self, name: &str) -> String {
        rhei_core::platform::canonical_path(&self.root(name))
            .expect("the root exists")
            .display()
            .to_string()
    }

    /// `cp -r original <name>`, account directory and all.
    pub(super) fn copy(&self, name: &str) {
        copy_tree(&self.root("original"), &self.root(name));
    }

    /// `mv original <name>`.
    pub(super) fn move_to(&self, name: &str) {
        fs::rename(self.root("original"), self.root(name)).expect("move the project");
    }

    /// Every `rhei` these cases spawn, under the named home of this world.
    ///
    /// `home` is the machine: two cases need a second one to show that a copy
    /// under its own `XDG_STATE_HOME` is adopted rather than refused. The
    /// parent reservation is removed because this suite may itself run inside a
    /// `rhei run`. §FS-rhei-budgets.7.1
    pub(super) fn rhei(&self, home: &str) -> Command {
        let base = self.dir.join(format!(".{home}"));
        let settings = base.join(".config/rhei/settings.json");
        if !settings.exists() {
            // A second machine carries the same settings and nothing else.
            fs::create_dir_all(settings.parent().expect("settings directory")).expect("create");
            fs::copy(home_for(&self.dir).join(".config/rhei/settings.json"), &settings)
                .expect("copy the machine settings");
        }
        let mut cmd = rhei_command(base);
        cmd.env_remove("RHEI_BUDGET_PARENT_RESERVATION");
        cmd
    }

    /// One hand edge on task 1 of the project at `name`, on the first machine.
    pub(super) fn edge(&self, name: &str, from: &str, to: &str) -> CliRun {
        self.edge_in("home", name, from, to)
    }

    pub(super) fn edge_in(&self, home: &str, name: &str, from: &str, to: &str) -> CliRun {
        let mut cmd = self.rhei(home);
        cmd.arg("--state-machine")
            .arg(&self.machine)
            .arg("transition")
            .arg(self.root(name).join("plan.rhei.md"))
            .args(["--task", "1", "--from", from, "--to", to, "--no-callbacks"]);
        CliRun::from(&cmd.output().expect("rhei transition should run"))
    }

    /// `rhei budget <verb> <args…> --reason <reason>` on the named machine.
    pub(super) fn budget_in(&self, home: &str, args: &[&str], reason: &str) -> CliRun {
        let mut cmd = self.rhei(home);
        cmd.arg("budget").args(args).args(["--reason", reason]);
        CliRun::from(&cmd.output().expect("rhei budget should run"))
    }

    /// `rhei budget show <root> --format json`, undecoded, on the first machine.
    pub(super) fn show_json(&self, name: &str) -> std::process::Output {
        let mut cmd = self.rhei("home");
        cmd.arg("budget").arg("show").arg(self.root(name)).args(["--format", "json"]);
        cmd.output().expect("rhei budget show should run")
    }

    pub(super) fn authority(&self, home: &str) -> PathBuf {
        self.dir.join(format!(".{home}")).join("state/rhei/budget-authority")
    }

    /// The uuid of the one live account directory a project root holds.
    pub(super) fn account_uuid(&self, name: &str) -> String {
        let accounts = self.root(name).join(".agent-grounds/rhei/budgets");
        let mut found: Vec<String> = fs::read_dir(&accounts)
            .unwrap_or_else(|error| panic!("{} holds an account: {error}", accounts.display()))
            .map(|entry| entry.expect("account entry").file_name().to_string_lossy().into_owned())
            .filter(|name| name != "retired")
            .collect();
        assert_eq!(found.len(), 1, "{name} holds exactly one live account; got {found:?}");
        found.remove(0)
    }

    /// The witness history of a uuid on the named machine, byte for byte.
    pub(super) fn witness_bytes(&self, home: &str, uuid: &str) -> Vec<u8> {
        fs::read(self.authority(home).join(uuid).join("history.jsonl"))
            .expect("the witness history is readable")
    }

    /// The witness index of the named machine: canonical root → uuid.
    pub(super) fn roots_index(&self, home: &str) -> BTreeMap<String, String> {
        match fs::read(self.authority(home).join("roots.json")) {
            Ok(bytes) => serde_json::from_slice(&bytes).expect("roots.json is a JSON object"),
            Err(_) => BTreeMap::new(),
        }
    }
}

/// `cp -r`, portably: every file and directory under `from`, lock files
/// included, as the reproducer's copy takes them.
fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("create the copy's directory");
    for entry in fs::read_dir(from).expect("read the directory being copied") {
        let entry = entry.expect("directory entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("entry type").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("copy a file");
        }
    }
}

/// Everything a command wrote, wherever it put it.
pub(super) fn said(result: &CliRun) -> String {
    format!("{}{}", result.stdout, result.stderr)
}
