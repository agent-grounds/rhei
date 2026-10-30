//! The world the failed-append and torn-witness cases share: one machine — one
//! home, one state directory, one witness — and a project `p` beside it whose
//! account has been initialized and charged, with its one task at `work`.
//!
//! The project sits in a subdirectory for the reason
//! [`super::budget_path_reuse_support`] gives: the home has to outlive what a
//! case does to the project's `budgets/`. Everything is built under the
//! harness's temporary directory, which honours `TMPDIR`.

// §FS-rhei-budgets.5.4.3
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use super::budget_edge_support::BARE_PING_PONG_MACHINE;
use super::budget_support::*;
use super::*;

pub(super) struct World {
    dir: TestDir,
    machine: PathBuf,
    pub(super) project: PathBuf,
}

/// A machine and a project whose account has been initialized and charged
/// twice, with task 1 back at `work`.
pub(super) fn world(prefix: &str) -> World {
    let world = bare(prefix);
    assert_success(&world.budget(&[
        "init",
        &world.plan().display().to_string(),
        "--invocations",
        "10",
        "--reason",
        "establish the account",
    ]));
    assert_success(&world.edge("work", "review"));
    assert_success(&world.edge("review", "work"));
    world
}

/// The same machine and project before anything has charged it: no account.
pub(super) fn bare(prefix: &str) -> World {
    let dir = unique_temp_dir(prefix);
    let agent = write_python_agent(&dir, "mock-agent.py", FINISHING_AGENT);
    write_machine_settings(&dir, &agent_defaults(&agent, ""));
    let machine = write_fixture_file(&dir, "states.yaml", BARE_PING_PONG_MACHINE);
    let project = dir.join("p");
    fs::create_dir_all(&project).expect("create the project");
    write_fixture_file(&project, "plan.rhei.md", PLAN);
    World { dir, machine, project }
}

impl World {
    pub(super) fn plan(&self) -> PathBuf {
        self.project.join("plan.rhei.md")
    }

    /// Every `rhei` these cases spawn. The parent reservation is removed
    /// because this suite may itself run inside a `rhei run`.
    /// §FS-rhei-budgets.7.1
    pub(super) fn rhei(&self) -> Command {
        let mut cmd = rhei_command(home_for(&self.dir));
        cmd.env_remove("RHEI_BUDGET_PARENT_RESERVATION");
        cmd
    }

    /// One hand edge on task 1, unrun, for a case that sets limits on it.
    pub(super) fn edge_command(&self, from: &str, to: &str) -> Command {
        let mut cmd = self.rhei();
        cmd.arg("--state-machine").arg(&self.machine).arg("transition").arg(self.plan()).args([
            "--task",
            "1",
            "--from",
            from,
            "--to",
            to,
            "--no-callbacks",
        ]);
        cmd
    }

    pub(super) fn edge(&self, from: &str, to: &str) -> CliRun {
        CliRun::from(&self.edge_command(from, to).output().expect("rhei transition should run"))
    }

    /// `rhei budget <args…>`.
    pub(super) fn budget(&self, args: &[&str]) -> CliRun {
        let mut cmd = self.rhei();
        cmd.arg("budget").args(args);
        CliRun::from(&cmd.output().expect("rhei budget should run"))
    }

    pub(super) fn forget(&self) -> CliRun {
        self.budget(&["forget", &self.project.display().to_string(), "--reason", "a torn witness"])
    }

    /// The error object `rhei budget show --format json` emits for a damaged
    /// account. §FS-rhei-budgets.10
    pub(super) fn damaged(&self) -> serde_json::Value {
        let mut cmd = self.rhei();
        cmd.arg("budget").arg("show").arg(&self.project).args(["--format", "json"]);
        let out = cmd.output().expect("rhei budget show should run");
        let raw = raw_stderr(&out);
        assert!(!out.status.success(), "a damaged account exits non-zero:\n{raw}");
        // The fixture's `mock` declares no family, so its warning precedes the
        // error on stderr. §FS-rhei-agents.1.1.2
        let line: String = raw
            .lines()
            .filter(|line| !line.trim().is_empty() && !line.contains("declares no family"))
            .collect();
        let parsed: serde_json::Value = serde_json::from_str(&line)
            .unwrap_or_else(|error| panic!("stderr is the JSON error object ({error}):\n{raw}"));
        parsed["error"].clone()
    }

    pub(super) fn authority(&self) -> PathBuf {
        home_for(&self.dir).join("state/rhei/budget-authority")
    }

    /// The one live account the witness directory holds.
    pub(super) fn uuid(&self) -> String {
        let mut found: Vec<String> = fs::read_dir(self.authority())
            .expect("the witness directory exists")
            .map(|entry| entry.expect("witness entry"))
            .filter(|entry| entry.file_type().expect("entry type").is_dir())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name != "retired")
            .collect();
        assert_eq!(found.len(), 1, "this world holds exactly one live account; got {found:?}");
        found.remove(0)
    }

    pub(super) fn witness(&self) -> PathBuf {
        self.authority().join(self.uuid()).join("history.jsonl")
    }

    pub(super) fn account_dir(&self, uuid: &str) -> PathBuf {
        self.project.join(".agent-grounds/rhei/budgets").join(uuid)
    }

    pub(super) fn journal(&self) -> PathBuf {
        self.account_dir(&self.uuid()).join("journal.jsonl")
    }

    /// Append the first 200 bytes of the witness's last receipt, with no
    /// newline: the hand-made tear of agent-grounds/rhei#536. Returns the
    /// witness as it was before.
    pub(super) fn tear(&self) -> Vec<u8> {
        let before = read(&self.witness());
        let last = before[..before.len() - 1]
            .rsplit(|byte| *byte == b'\n')
            .next()
            .expect("the witness holds a receipt");
        assert!(last.len() > 200, "a receipt is longer than the tear");
        let mut torn = before.clone();
        torn.extend_from_slice(&last[..200]);
        fs::write(self.witness(), &torn).expect("tear the witness");
        before
    }

    /// The kept copies a trim left beside the witness. §FS-rhei-budgets.5.4.3
    pub(super) fn kept(&self) -> Vec<PathBuf> {
        let mut kept: Vec<PathBuf> = fs::read_dir(self.authority().join(self.uuid()))
            .expect("the witness directory")
            .map(|entry| entry.expect("witness entry").path())
            .filter(|path| {
                let name = path.file_name().expect("a name").to_string_lossy().into_owned();
                name.starts_with("history-torn-") && name.ends_with(".jsonl")
            })
            .collect();
        kept.sort();
        kept
    }

    /// Run a command line a report offered, in the platform's own shell, from
    /// this world's directory, and require that it succeeded.
    /// §FS-rhei-budgets.10
    pub(super) fn shell(&self, line: &str) {
        let out: Output = rhei_core::platform::system_shell_command(line)
            .current_dir(&self.dir)
            .output()
            .expect("the offered command should run");
        assert!(
            out.status.success(),
            "the offered command runs: {line}\n{}{}",
            stdout(&out),
            stderr(&out)
        );
    }
}

pub(super) fn read(path: &Path) -> Vec<u8> {
    fs::read(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

/// The number of whole receipts in a witness or a journal.
pub(super) fn receipts(bytes: &[u8]) -> usize {
    bytes.iter().filter(|byte| **byte == b'\n').count()
}

/// Everything a command wrote, wherever it put it.
pub(super) fn said(result: &CliRun) -> String {
    format!("{}{}", result.stdout, result.stderr)
}
