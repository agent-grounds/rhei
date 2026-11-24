//! The issue #764 loop, with real entries and successful mock workers.
// §FS-rhei-errors.1.5.1 §FS-rhei-run-report.2 §FS-rhei-transitions.4.3

use super::budget_support::{agent_defaults, home_for, write_machine_settings};
use super::*;

#[derive(Clone, Copy, Debug)]
pub(super) enum Worker {
    Agent,
    Program,
}

impl Worker {
    pub fn name(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Program => "program",
        }
    }
}

pub(super) const PATHS: [(Worker, usize); 4] =
    [(Worker::Agent, 1), (Worker::Agent, 2), (Worker::Program, 1), (Worker::Program, 2)];

pub(super) const REFUSAL: &str = "visit budget for state 'check-review' is exhausted (2/2 visits)";

pub(super) struct World {
    _temp: TestDir,
    pub dir: PathBuf,
    pub worker: Worker,
    parallel: usize,
}

impl World {
    pub fn new(worker: Worker, parallel: usize, missing: bool) -> Self {
        let temp = unique_temp_dir("rhei-764-spent-destination");
        let dir = temp.join("index");
        fs::create_dir_all(dir.join("tasks")).unwrap();
        write_fixture_file(&dir, "index.rhei.md", "# Rhei: Spent destination\n");
        // Separate files and concurrent states are essential: otherwise a
        // --parallel agent run falls back to the sequential completion path.
        for id in ["1", "2"] {
            write_fixture_file(
                &dir,
                &format!("tasks/{id}.md"),
                &format!("### Task {id}: Deliver {id}\n**State:** audit\n"),
            );
        }
        let audit = write_python_agent(
            &dir,
            "audit.py",
            &format!(
                r#"root = pathlib.Path(env('RHEI_ROOT'))
task = env('RHEI_TASK_ID')
calls = root / 'runtime' / (task + '-audit-calls')
seen = len(calls.read_text().splitlines()) if calls.exists() else 0
if seen >= 25:
    raise SystemExit(17)
append(calls, 'success\n')
output = root / 'runtime' / ('audit-' + task + '.md')
if {missing} and seen >= 2:
    output.unlink(missing_ok=True)
else:
    write(output, 'audit complete\n')
result('Audit complete\n')
"#,
                missing = if missing { "True" } else { "False" }
            ),
        );
        let review = write_python_agent(
            &dir,
            "review.py",
            r#"root = pathlib.Path(env('RHEI_ROOT'))
append(root / 'runtime' / (env('RHEI_TASK_ID') + '-review-calls'), 'success\n')
"#,
        );
        // Programs may make further attempts while a sibling still advances.
        // Keep attempt exhaustion out of the destination-budget scenario.
        let extra = match worker {
            Worker::Agent => "",
            Worker::Program => ", \"attempts\": 10",
        };
        write_machine_settings(&dir, &agent_defaults(&audit, extra));
        let execution = match worker {
            Worker::Agent => "agent: mock".to_string(),
            Worker::Program => format!("program: {{ command: {} }}", fixture_command(&audit)),
        };
        let exit = match worker {
            Worker::Agent => "",
            Worker::Program => ", exit_code: 0",
        };
        write_fixture_file(
            &dir,
            "states.yaml",
            &format!(
                r#"name: spent-destination-visits
version: 1
states:
  audit:
    initial: true
    concurrent: true
    description: Audit
    {execution}
    outputs: [{{ name: audit, path: 'runtime/audit-{{task_id}}.md' }}]
  check-review:
    concurrent: true
    description: Send it round again
    visits: 2
    program: {{ command: {review} }}
  done: {{ description: Accepted, final: true }}
  cancelled: {{ description: Dropped, final: true }}
transitions:
  - {{ from: audit, to: check-review, description: Audit written{exit} }}
  - {{ from: check-review, to: audit, description: Audit again, exit_code: 0 }}
  - {{ from: check-review, to: done, description: Accepted, exit_code: 1 }}
  - {{ from: "*", to: cancelled, description: Dropped }}
"#,
                review = fixture_command(&review),
            ),
        );
        Self { _temp: temp, dir, worker, parallel }
    }

    pub fn cli(&self, args: &[&str]) -> CliRun {
        let mut command = rhei_command(home_for(&self.dir));
        CliRun::from(
            &command
                .current_dir(&self.dir)
                .args(["--state-machine", "states.yaml"])
                .args(args)
                .output()
                .unwrap(),
        )
    }

    pub fn run(&self) -> CliRun {
        self.cli(&[
            "run",
            ".",
            "--no-tui",
            "--no-callbacks",
            "--continue-on-error",
            "--parallel",
            &self.parallel.to_string(),
        ])
    }

    pub fn read(&self, path: &str) -> String {
        fs::read_to_string(self.dir.join(path)).unwrap_or_default()
    }

    pub fn change_machine(&self, before: &str, after: &str) {
        let machine = self.read("states.yaml");
        assert!(machine.contains(before), "fixture edit must match");
        fs::write(self.dir.join("states.yaml"), machine.replace(before, after)).unwrap();
    }

    pub fn row(&self, id: &str) -> String {
        let report = self.read("runtime/run-report.md");
        report
            .split("## Attention\n")
            .nth(1)
            .expect("Attention section")
            .split("\n## ")
            .next()
            .unwrap()
            .lines()
            .find(|line| line.starts_with(&format!("| index.{id} |")))
            .unwrap_or_else(|| panic!("missing Attention row: {report}"))
            .to_string()
    }

    /// Establish the cause before any assertion about the new explanation.
    pub fn assert_spent(&self, result: &CliRun, outputs: bool) {
        let all = text(result);
        assert_eq!(result.status.code(), Some(1), "the run must halt: {all}");
        assert!(!all.contains("Falling back to sequential"), "wrong spawn path: {all}");
        let index = self.read("index.rhei.md");
        let yaml = index.split("---").nth(1).expect("persisted metadata");
        let metadata: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let ledger = self.read("runtime/state-transitions.log");
        for log in fs::read_dir(self.dir.join("runtime/logs")).unwrap() {
            let path = log.unwrap().path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("log") {
                let body = fs::read_to_string(&path).unwrap();
                assert!(
                    body.contains("\ncode: 0\n"),
                    "worker must exit successfully: {path:?}\n{body}"
                );
            }
        }
        for id in ["1", "2"] {
            let task = metadata["metadata"]["tasks"]
                .as_mapping()
                .unwrap()
                .iter()
                .find(|(key, _)| key.as_u64() == Some(id.parse().unwrap()))
                .expect("task metadata")
                .1;
            assert_eq!(
                task["stateVisits"]["check-review"].as_u64(),
                Some(2),
                "real destination counter: {index}"
            );
            assert_eq!(
                ledger
                    .lines()
                    .filter(|line| *line == format!("index.{id} audit@check-review"))
                    .count(),
                2,
                "two actual destination entries: {ledger}"
            );
            assert!(self.read(&format!("tasks/{id}.md")).contains("**State:** audit"));
            let calls = self.read(&format!("runtime/index.{id}-audit-calls"));
            assert!(
                calls.lines().count() >= 3 && calls.lines().all(|line| line == "success"),
                "the third audit must complete; later attempts may run: {calls}"
            );
            assert_eq!(
                self.read(&format!("runtime/index.{id}-review-calls")),
                "success\nsuccess\n"
            );
            assert_eq!(self.dir.join(format!("runtime/audit-index.{id}.md")).exists(), outputs);
            if outputs {
                let refused = self.cli(&[
                    "transition",
                    &format!("index.{id}"),
                    "--from",
                    "audit",
                    "--to",
                    "check-review",
                ]);
                assert_eq!(refused.status.code(), Some(1));
                assert!(text(&refused).contains(REFUSAL), "manual refusal: {}", text(&refused));
            }
        }
    }
}

pub(super) fn text(result: &CliRun) -> String {
    format!("{}\n{}", result.stdout, result.stderr)
}
