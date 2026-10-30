//! A project whose tickets have spent their visit, on each of the four spawn
//! paths, for #537: a program or an agent state with `attempts: 1`, run
//! sequentially or through the worker pool.
// §FS-rhei-budgets.5.4 §FS-rhei-agents.3.2.3

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::budget_program_verification_support::{drop_last_receipt, snapshot_files};
use super::*;

#[derive(Clone, Copy, Debug)]
pub(super) enum Kind {
    /// One ticket in a program state whose program fails and fires no edge.
    Program,
    /// Two tickets, in two files, in an agent state whose agent exits 0 without
    /// writing its result. With one ticket in the batch, or every ticket in one
    /// file, `--parallel 2` falls back to the sequential agent path.
    Agent,
}

pub(super) struct SpentWorld {
    _dir: TestDir,
    project: PathBuf,
    root: PathBuf,
    home: PathBuf,
    machine: PathBuf,
    calls: PathBuf,
    kind: Kind,
    parallel: usize,
}

impl SpentWorld {
    /// An established, healthy account and every ticket entered into the
    /// state under test, not yet spawned.
    pub fn new(kind: Kind, parallel: usize) -> Self {
        let dir = unique_temp_dir("rhei-537-spent-visit");
        let project = dir.join("project");
        let root = project.join("jobs");
        fs::create_dir_all(root.join("tasks")).unwrap();
        fs::write(project.join("index.panta.md"), "# Panta: Spent visit\n").unwrap();
        fs::write(root.join("index.rhei.md"), "# Rhei: Spent visit\n").unwrap();
        let calls = dir.join("calls.log");
        let calls_literal = serde_json::to_string(&calls.display().to_string()).unwrap();
        let home = dir.join("home");
        fs::create_dir_all(home.join(".config/rhei")).unwrap();
        let (machine_text, agents) = match kind {
            Kind::Program => {
                let gate = write_python_agent(
                    &dir,
                    "gate.py",
                    &format!(
                        "append({calls_literal}, 'attempt=' + env('RHEI_ATTEMPT', '?') + '\\n')\n\
                         sys.exit(3)\n"
                    ),
                );
                (program_machine(&gate), String::new())
            }
            Kind::Agent => {
                let agent = write_python_agent(
                    &dir,
                    "agent.py",
                    &format!(
                        "append({calls_literal}, env('RHEI_TASK_ID') + ' attempt=' + \
                         env('RHEI_ATTEMPT', '?') + '\\n')\n"
                    ),
                );
                let agents = format!(
                    r#","agents":{{"mock":{{"command":{},"stdin_prompt":true,"timeout":"30s"}}}}"#,
                    fixture_command(&agent)
                );
                (AGENT_MACHINE.to_string(), agents)
            }
        };
        let machine = dir.join("states.yaml");
        fs::write(&machine, machine_text).unwrap();
        fs::write(
            home.join(".config/rhei/settings.json"),
            format!(
                r#"{{"defaults":{{"agent":"mock","agent_timeout":"30s","transition_limit":100}}{agents}}}"#
            ),
        )
        .unwrap();
        let world = Self { _dir: dir, project, root, home, machine, calls, kind, parallel };
        for ticket in world.tickets() {
            fs::write(
                world.root.join(format!("tasks/{ticket}.md")),
                format!("### Task {ticket}: Piece {ticket}\n**State:** hold\n"),
            )
            .unwrap();
        }
        assert_success(&world.cli(&[
            "budget",
            "init",
            ".",
            "--invocations",
            "20",
            "--reason",
            "fixture",
        ]));
        // The applied edge is a receipt of its own, so the journal has a tail to lose.
        for ticket in world.tickets() {
            assert_success(&world.cli(&[
                "transition",
                "jobs",
                "--task",
                ticket,
                "--from",
                "hold",
                "--to",
                world.state_under_test(),
                "--no-callbacks",
            ]));
        }
        world
    }

    pub fn tickets(&self) -> &'static [&'static str] {
        match self.kind {
            Kind::Program => &["1"],
            Kind::Agent => &["1", "2"],
        }
    }

    fn state_under_test(&self) -> &'static str {
        match self.kind {
            Kind::Program => "gate",
            Kind::Agent => "work",
        }
    }

    pub fn cli(&self, args: &[&str]) -> CliRun {
        let mut cmd = rhei_command(&self.home);
        // A fixture must not borrow the supervising run's identity or reservation.
        // §FS-rhei-budgets.7.1
        for (name, _) in std::env::vars_os() {
            if name.to_string_lossy().starts_with("RHEI_") {
                cmd.env_remove(name);
            }
        }
        cmd.env("XDG_CONFIG_HOME", self.home.join(".config"));
        cmd.env("GIT_CEILING_DIRECTORIES", &self.project);
        cmd.current_dir(&self.project).arg("--state-machine").arg(&self.machine).args(args);
        CliRun::from(&cmd.output().expect("rhei fixture command"))
    }

    pub fn run(&self) -> CliRun {
        let mut args = vec!["run", "jobs", "--no-tui", "--no-callbacks"];
        let parallel = self.parallel.to_string();
        if self.parallel > 1 {
            args.extend(["--parallel", &parallel]);
        }
        self.cli(&args)
    }

    /// How many times the program or the agent was started, across every run.
    pub fn spawns(&self) -> usize {
        fs::read_to_string(&self.calls).unwrap_or_default().lines().count()
    }

    /// The run spawned every ticket once, on the path this world is for: the
    /// worker pool's spawn line ends in "(parallel)", the sequential one's
    /// does not.
    pub fn assert_spawned_on_its_path(&self, result: &CliRun) {
        let lines: Vec<_> = result
            .stdout
            .lines()
            .filter(|line| line.contains("Spawning program") || line.contains("Spawning agent"))
            .collect();
        assert_eq!(lines.len(), self.tickets().len(), "{:?}: {}", self.kind, result.stdout);
        for line in lines {
            assert_eq!(
                line.trim_end().ends_with("(parallel)"),
                self.parallel > 1,
                "{:?} --parallel {}: spawned off its path: {line}",
                self.kind,
                self.parallel
            );
        }
        assert!(!result.stderr.contains("Falling back to sequential"), "{}", result.stderr);
    }

    /// Damage the account by dropping the journal's last receipt, and confirm
    /// `rhei budget show` reads it as damaged.
    pub fn truncate_journal(&self) {
        drop_last_receipt(&self.project, &self.home);
        let shown = self.cli(&["budget", "show", "."]);
        assert!(!shown.status.success());
        assert!(
            shown.stderr.contains("fewer receipts than the committed history"),
            "{}",
            shown.stderr
        );
    }

    /// Everything a refused run must leave as it was: the plan and its task
    /// files, transcripts, spawn records, the move ledger, the account's
    /// journal and witness, and the spawn log.
    pub fn snapshot(&self) -> BTreeMap<PathBuf, Vec<u8>> {
        let mut snapshot = BTreeMap::new();
        snapshot_files(&self.root.join("tasks"), &mut snapshot);
        for path in [self.root.join("index.rhei.md"), self.calls.clone()] {
            snapshot.insert(path.clone(), fs::read(path).unwrap_or_default());
        }
        for runtime in [self.root.join("runtime"), self.project.join("runtime")] {
            snapshot_files(&runtime.join("logs"), &mut snapshot);
            snapshot_files(&runtime.join("spawns"), &mut snapshot);
            let ledger = runtime.join("state-transitions.log");
            snapshot.insert(ledger.clone(), fs::read(ledger).unwrap_or_default());
        }
        snapshot_files(&self.project.join(".agent-grounds/rhei/budgets"), &mut snapshot);
        snapshot_files(&self.home.join("state/rhei/budget-authority"), &mut snapshot);
        snapshot
    }
}

/// The paths a snapshot names whose bytes differ, for a readable failure.
pub(super) fn changed(
    before: &BTreeMap<PathBuf, Vec<u8>>,
    after: &BTreeMap<PathBuf, Vec<u8>>,
) -> Vec<PathBuf> {
    let mut paths: Vec<_> = before.keys().chain(after.keys()).cloned().collect();
    paths.sort();
    paths.dedup();
    paths.into_iter().filter(|path| before.get(path) != after.get(path)).collect()
}

fn program_machine(gate: &Path) -> String {
    format!(
        r#"name: spent-visit-program
version: 1
states:
  hold:
    initial: true
    description: Not started
  gate:
    description: One attempt per visit; the program fails and fires no edge
    attempts: 1
    program:
      command: {}
    program_timeout: 30s
  done:
    description: Green
    final: true
  cancelled:
    description: Dropped
    final: true
transitions:
  - {{ from: hold, to: gate }}
  - {{ from: gate, to: done, exit_code: 0 }}
  - {{ from: '*', to: cancelled }}
"#,
        fixture_command(gate)
    )
}

const AGENT_MACHINE: &str = r#"name: spent-visit-agent
version: 1
states:
  hold:
    initial: true
    description: Not started
  work:
    description: One attempt per visit; the agent never writes its result
    agent: mock
    attempts: 1
    concurrent: true
    agent_timeout: 30s
  done:
    description: Done
    final: true
  cancelled:
    description: Dropped
    final: true
transitions:
  - { from: hold, to: work }
  - { from: work, to: done }
  - { from: '*', to: cancelled }
"#;
