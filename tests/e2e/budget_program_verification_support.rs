//! Isolated version of the witness-ahead polling-program reproducer for #501.
// §FS-rhei-budgets.5.4 §FS-rhei-states.2.2

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::*;

pub(super) struct World {
    _dir: TestDir,
    pub project: PathBuf,
    pub root: PathBuf,
    home: PathBuf,
    machine: PathBuf,
    pub task: PathBuf,
    pub parallel: usize,
}

impl World {
    pub fn new(parallel: usize, exit: i32, establish: bool) -> Self {
        let dir = unique_temp_dir("rhei-501-program-verification");
        let project = dir.join("project");
        let root = project.join("jobs");
        fs::create_dir_all(root.join("tasks")).unwrap();
        fs::write(project.join("index.panta.md"), "# Panta: Program verification\n").unwrap();
        fs::write(root.join("index.rhei.md"), "# Rhei: Claim\n").unwrap();
        let task = root.join("tasks/1.md");
        fs::write(&task, "### Task 1: Claim the issue\n**State:** hold\n").unwrap();
        let program = write_python_agent(
            &dir,
            "claim.py",
            &format!(
                r#"root = pathlib.Path(env('RHEI_ROOT'))
append(root / 'runtime/forge-calls.log', 'attempt=' + env('RHEI_ATTEMPT', '?') + '\n')
if (root.parent / '.agent-grounds/rhei/budgets').exists():
    append(root / 'runtime/account-at-start.log', 'present\n')
else:
    append(root / 'runtime/account-at-start.log', 'absent\n')
result('Claim held: already ours.\n')
sys.exit({exit})
"#
            ),
        );
        let machine = dir.join("states.yaml");
        fs::write(&machine, machine_text(&program)).unwrap();
        let home = dir.join("home");
        fs::create_dir_all(home.join(".config/rhei")).unwrap();
        fs::write(
            home.join(".config/rhei/settings.json"),
            r#"{"defaults":{"agent_timeout":"30s","transition_limit":100}}"#,
        )
        .unwrap();
        let world = Self { _dir: dir, project, root, home, machine, task, parallel };
        if establish {
            // Programs retain their allowance: verification is not an admission.
            // §FS-rhei-budgets.4.3
            assert_success(&world.cli(&[
                "budget",
                "init",
                ".",
                "--invocations",
                "10",
                "--reason",
                "fixture",
            ]));
            assert_success(&world.cli(&[
                "transition",
                "jobs",
                "--task",
                "1",
                "--from",
                "hold",
                "--to",
                "claim",
                "--no-callbacks",
            ]));
        } else {
            fs::write(&world.task, "### Task 1: Claim the issue\n**State:** claim\n").unwrap();
        }
        world
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
        self.cli(&[
            "run",
            "jobs",
            "--no-tui",
            "--no-callbacks",
            "--parallel",
            &self.parallel.to_string(),
        ])
    }

    pub fn truncate_journal(&self) -> (PathBuf, PathBuf) {
        let accounts = self.project.join(".agent-grounds/rhei/budgets");
        let journals: Vec<_> = fs::read_dir(accounts)
            .unwrap()
            .map(|entry| entry.unwrap().path().join("journal.jsonl"))
            .filter(|path| path.is_file())
            .collect();
        assert_eq!(journals.len(), 1, "one isolated account");
        let journal = journals[0].clone();
        let uuid = journal.parent().unwrap().file_name().unwrap();
        let witness =
            self.home.join("state/rhei/budget-authority").join(uuid).join("history.jsonl");
        let intact = fs::read_to_string(&journal).unwrap();
        assert_eq!(intact, fs::read_to_string(&witness).unwrap(), "premise: verified account");
        let mut receipts: Vec<_> = intact.lines().collect();
        assert!(receipts.len() > 1, "truncate a real established history");
        receipts.pop();
        fs::write(&journal, format!("{}\n", receipts.join("\n"))).unwrap();
        let shown = self.cli(&["budget", "show", "."]);
        assert!(!shown.status.success());
        assert!(
            shown.stderr.contains("fewer receipts than the committed history"),
            "{}",
            shown.stderr
        );
        (journal, witness)
    }

    pub fn calls(&self) -> String {
        fs::read_to_string(self.root.join("runtime/forge-calls.log")).unwrap_or_default()
    }

    pub fn state(&self) -> String {
        fs::read_to_string(&self.task)
            .unwrap()
            .lines()
            .find_map(|line| line.strip_prefix("**State:** "))
            .expect("task state")
            .to_string()
    }

    pub fn attempts(&self) -> BTreeMap<PathBuf, Vec<u8>> {
        let mut snapshot = BTreeMap::new();
        for path in [&self.task, &self.root.join("index.rhei.md")] {
            snapshot.insert(path.clone(), fs::read(path).unwrap());
        }
        snapshot_files(&self.root.join("runtime/logs"), &mut snapshot);
        snapshot_files(&self.root.join("runtime/spawns"), &mut snapshot);
        let ledger = self.root.join("runtime/state-transitions.log");
        snapshot.insert(ledger.clone(), fs::read(ledger).unwrap_or_default());
        snapshot
    }

    pub fn reports(&self) -> BTreeMap<PathBuf, Vec<u8>> {
        let mut reports = BTreeMap::new();
        snapshot_files(&self.project.join("runtime/run-reports"), &mut reports);
        reports
    }

    pub fn assert_pool_dispatch(&self, result: &CliRun) {
        if self.parallel > 1 {
            assert!(result.stdout.contains("Claim the issue (parallel)"), "{}", result.stdout);
            assert!(!result.stderr.contains("Falling back to sequential"), "{}", result.stderr);
            eprintln!(
                "worker-pool control: Spawning program for Task jobs.1: Claim the issue (parallel)"
            );
        }
    }
}

fn snapshot_files(dir: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries {
            let path = entry.unwrap().path();
            if path.is_dir() {
                snapshot_files(&path, files);
            } else if path.extension().is_some_and(|ext| ext != "lock") {
                files.insert(path.clone(), fs::read(path).unwrap());
            }
        }
    }
}

fn machine_text(program: &Path) -> String {
    format!(
        r#"name: program-budget-verification
version: 1
states:
  hold:
    initial: true
    description: Not started
  claim:
    description: Take the claim
    program:
      command: {}
    program_timeout: 30s
    poll:
      interval: 1s
      max_attempts: 2
      waiting_on: the forge
  human-review:
    description: Poll exhausted
    final: true
  completed:
    description: Claim held
    final: true
  cancelled:
    description: Dropped
    final: true
transitions:
  - {{ from: hold, to: claim }}
  - {{ from: claim, to: human-review, condition: pollAttempts >= pollMaxAttempts }}
  - {{ from: claim, to: claim, exit_code: 75 }}
  - {{ from: claim, to: completed, exit_code: 0 }}
  - {{ from: claim, to: human-review, exit_code: nonzero }}
  - {{ from: '*', to: cancelled }}
"#,
        fixture_command(program)
    )
}
