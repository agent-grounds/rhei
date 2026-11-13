//! The #671 reproducer: a program prepares work whose neural admission is refused.
//! §FS-rhei-run.3.4 §FS-rhei-budgets.7.1

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use super::budget_support::{agent_defaults, home_for, ledger, spawns, write_machine_settings};
use super::*;

pub(super) const ANCESTOR: &str = "reservation:00000000-0000-0000-0000-000000000000";

pub(super) struct Fixture {
    _dir: TestDir,
    pub root: PathBuf,
    machine: PathBuf,
    parallel: usize,
}

impl Fixture {
    pub fn new(parallel: usize, travel: u32) -> Self {
        let dir = unique_temp_dir("rhei-671-admission-halt");
        let root = dir.join("admission");
        fs::create_dir_all(root.join("tasks")).unwrap();
        fs::write(root.join("index.rhei.md"), "# Rhei: Admission\n").unwrap();
        write_fixture_file(
            &root,
            "tasks/1.md",
            "### Task 1: Cover the library\n**State:** prepare\n",
        );
        // Two separately stored, concurrent tickets force a real agent batch
        // into the pool; a one-item batch takes the serial fast path instead.
        if parallel == 2 {
            write_fixture_file(
                &root,
                "tasks/2.md",
                "### Task 2: Cover the sibling\n**State:** prepare\n",
            );
        }
        let prepare = write_python_agent(
            &root,
            "prepare.py",
            "write(pathlib.Path(env('RHEI_ROOT')) / ('runtime/earned-' + env('RHEI_TASK_ID') + '.txt'), 'prepared\\n')\n",
        );
        // Even an accidentally admitted agent can only reach this local fixture.
        let forbidden = write_python_agent(
            &root,
            "forbidden.py",
            "append(pathlib.Path(env('RHEI_ROOT')) / 'runtime/spawn-count.log', 'FORBIDDEN\\n')\n\
             raise SystemExit(91)\n",
        );
        let mut settings: serde_json::Value = serde_json::from_str(&agent_defaults(
            &forbidden,
            &format!(r#", "transition_limit": {travel}"#),
        ))
        .unwrap();
        // Keep every built-in transport local too; no fixture can reach a model.
        for id in ["codex", "claude-code", "pi"] {
            settings["agents"][id] = settings["agents"]["mock"].clone();
        }
        write_machine_settings(&root, &settings.to_string());
        let machine = write_fixture_file(
            &root,
            "states.yaml",
            &format!(
                r#"name: refused-admission
version: 1
states:
  prepare:
    initial: true
    concurrent: true
    description: Prepare the library
    program:
      command: {prepare}
  write:
    concurrent: true
    description: Cover the library
    agent: mock
    agent_timeout: 30s
  done:
    description: Finished
    final: true
transitions:
  - {{ from: prepare, to: write, exit_code: 0, description: Prepared }}
  - {{ from: write, to: done, description: Written }}
"#,
                prepare = fixture_command(&prepare),
            ),
        );
        Self { _dir: dir, root, machine, parallel }
    }

    fn command(&self) -> Command {
        // rhei_command clears every inherited RHEI_*; the absent account is
        // deliberate legacy ancestry, never a different-account fallback.
        let home = home_for(&self.root);
        let mut cmd = rhei_command(&home);
        cmd.current_dir(&self.root)
            .env("XDG_CONFIG_HOME", home.join(".config"))
            .env("XDG_DATA_HOME", home.join("data"))
            .env("XDG_CACHE_HOME", home.join("cache"))
            .env("GIT_CEILING_DIRECTORIES", &*self.root)
            .env("RHEI_BUDGET_NOW", "2026-11-13T12:00:00Z")
            .arg("--state-machine")
            .arg(&self.machine)
            .stdin(Stdio::null());
        cmd
    }

    pub fn run(&self, ancestor: bool) -> CliRun {
        let mut cmd = self.command();
        cmd.args(["run", ".", "--no-tui", "--no-callbacks", "--parallel"])
            .arg(self.parallel.to_string());
        if ancestor {
            cmd.env("RHEI_BUDGET_PARENT_RESERVATION", ANCESTOR);
        }
        CliRun::from(&cmd.output().expect("the refusal fixture must run"))
    }

    /// Establish admission refusal, not a setup error, before checking help.
    pub fn assert_stranded(&self, out: &CliRun, cause: &str, prepared: bool) {
        let all = text(out);
        assert_eq!(out.status.code(), Some(1), "a no-progress halt exits 1:\n{all}");
        assert!(all.contains(cause), "fixture never reached the intended refusal:\n{all}");
        assert!(all.contains("no further advancement possible"), "not a final halt:\n{all}");
        assert!(!all.contains("Falling back to sequential"), "pool coverage was lost:\n{all}");
        if self.parallel == 2 && prepared {
            assert!(
                all.contains("Cover the library (parallel)"),
                "program must enter pool:\n{all}"
            );
        }
        assert!(spawns(&self.root).is_empty(), "a refused agent must never start:\n{all}");
        assert!(!all.contains("Spawning agent"), "no neural subprocess is started:\n{all}");
        assert_eq!(
            ledger(&self.root).len(),
            self.parallel,
            "only prepare -> write is applied:\n{all}"
        );
        for n in 1..=self.parallel {
            let task = fs::read_to_string(self.root.join(format!("tasks/{n}.md"))).unwrap();
            assert!(task.contains("**State:** write"), "no refused transition:\n{task}\n{all}");
            assert!(!task.contains("**Result:**"), "no invented result:\n{task}");
            assert!(!self.root.join(format!("runtime/results/admission.{n}.md")).exists());
            assert_eq!(
                fs::read_to_string(self.root.join(format!("runtime/earned-admission.{n}.txt")))
                    .unwrap(),
                "prepared\n"
            );
        }
        let budget = self
            .command()
            .args(["budget", "show", ".", "--format", "json"])
            .output()
            .expect("read the refused run's budget");
        let shown = CliRun::from(&budget);
        assert_success(&shown);
        let facts: serde_json::Value = serde_json::from_str(&shown.stdout).unwrap();
        assert_eq!(facts["health"], "verified", "{facts}");
        for dimension in ["invocations", "lifetime_invocations", "spend"] {
            assert_eq!(
                facts[dimension]["consumed"], 0,
                "refusal must not charge {dimension}: {facts}"
            );
            assert_eq!(
                facts[dimension]["outstanding"], 0,
                "refusal must not reserve {dimension}: {facts}"
            );
        }
        eprintln!("parallel={}, prepared={prepared}\n{all}", self.parallel);
    }

    pub fn events(&self) -> Vec<serde_json::Value> {
        fs::read_to_string(self.root.join("runtime/events.jsonl"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
}

pub(super) fn text(out: &CliRun) -> String {
    format!("{}\n{}", out.stdout, out.stderr)
}

pub(super) fn assert_refused_admission_help(out: &CliRun, cause: &str, tickets: usize) {
    let all = text(out);
    let help = out.stderr.split("help:").last().expect("final halt help");
    assert!(
        !help.contains("blocked, gated, or assigned"),
        "a refused admission is not ordinary blockage:\n{all}"
    );
    assert!(
        (help.contains("admission") || help.contains("spawn")) && help.contains("refus"),
        "final halt must identify refused admission/spawning:\n{all}"
    );
    for n in 1..=tickets {
        assert!(
            help.contains(&format!("admission.{n}")) && help.contains("write"),
            "final halt must name every affected ticket and refused state:\n{all}"
        );
    }
    assert!(
        help.contains(cause) || help.contains("above") || help.contains("diagnostic"),
        "final halt must repeat the cause or point to its diagnostic:\n{all}"
    );
}

pub(super) fn assert_ancestor_remedy(out: &CliRun) {
    let all = text(out);
    assert!(all.contains(ANCESTOR) && all.contains("RHEI_BUDGET_PARENT_RESERVATION"), "{all}");
    assert!(
        all.contains("unset") || all.contains("remove") || all.contains("outside"),
        "invalid inherited environment needs an actionable remedy:\n{all}"
    );
}
