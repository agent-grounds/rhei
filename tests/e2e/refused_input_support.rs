//! The issue #770 fixture: a worker in `draft` finishes, but `read` requires an
//! input `note` nobody writes, so the only edge forward is refused at entry.
//! Ported from the intake reproducer of tool report rhei.139.
// §FS-rhei-errors.1.5.2 §FS-rhei-run-report.3.1 §FS-rhei-retry.2

use super::budget_support::{agent_defaults, home_for, write_machine_settings};
use super::*;

#[derive(Clone, Copy, Debug)]
pub(super) enum Worker {
    Agent,
    Program,
}

/// Whether `draft` declares its result as an output. With one, an agent's
/// finished visit is reusable and is never spawned again; without one, it is
/// re-spawned until its attempts are spent (§FS-rhei-agents.3.2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Outputs {
    Declared,
    Bare,
}

/// What no surface may tell the operator about a finished worker whose edge
/// its destination refused: every one of these leaves the ticket where it is.
pub(super) const MISLEADING: [&str; 9] = [
    "stalled",
    "not scheduled",
    "rerun to pick it up",
    "inspect logs",
    "rhei next",
    "no spawn record",
    "run the task first",
    "still unmet",
    "names nothing on disk",
];

pub(super) struct World {
    _temp: TestDir,
    pub dir: PathBuf,
}

impl World {
    pub fn new(worker: Worker, outputs: Outputs) -> Self {
        let temp = unique_temp_dir("rhei-770-refused-input");
        let dir = temp.join("plan");
        fs::create_dir_all(&dir).unwrap();
        write_fixture_file(
            &dir,
            "plan.rhei.md",
            "# Rhei: Refused edge\n\n## Tasks\n\n### Task 1: Draft\n**State:** draft\n",
        );
        // Each state counts its own calls, so a test can tell a reused visit
        // from a re-spawned one. The guard turns a runaway loop into a failure.
        let worker_script = write_python_agent(
            &dir,
            "worker.py",
            r#"root = pathlib.Path(env('RHEI_ROOT'))
calls = root / 'runtime' / (env('RHEI_STATE') + '-calls')
seen = len(calls.read_text().splitlines()) if calls.exists() else 0
if seen >= 25:
    raise SystemExit(17)
append(calls, env('RHEI_TASK_ID') + '\n')
write(root / 'runtime' / 'results' / (env('RHEI_TASK_ID') + '.md'), 'drafted\n')
"#,
        );
        write_machine_settings(&dir, &agent_defaults(&worker_script, ""));
        let execution = match worker {
            Worker::Agent => "agent: mock".to_string(),
            Worker::Program => {
                format!("program: {{ command: {} }}", fixture_command(&worker_script))
            }
        };
        let exit = match worker {
            Worker::Agent => "",
            Worker::Program => ", exit_code: 0",
        };
        let outputs = match outputs {
            Outputs::Declared => {
                "\n    outputs: [{ name: result, path: 'runtime/results/{task_id}.md' }]"
            }
            Outputs::Bare => "",
        };
        write_fixture_file(
            &dir,
            "states.yaml",
            &format!(
                r#"name: refused-edge
version: 1
states:
  todo: {{ initial: true, description: Not started }}
  draft:
    description: Draft the note
    {execution}{outputs}
  read:
    description: Read the note
    agent: mock
    inputs: [{{ name: note, path: 'runtime/{{task_id}}.note.md' }}]
  completed: {{ description: Done, final: true }}
  cancelled: {{ description: Dropped, final: true }}
transitions:
  - {{ from: todo, to: draft, description: Start }}
  - {{ from: draft, to: read, description: Drafted{exit} }}
  - {{ from: read, to: completed, description: Read }}
  - {{ from: "*", to: cancelled, description: Dropped }}
"#
            ),
        );
        Self { _temp: temp, dir }
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
        self.cli(&["run", "plan.rhei.md", "--no-tui", "--no-callbacks"])
    }

    pub fn read(&self, path: &str) -> String {
        fs::read_to_string(self.dir.join(path)).unwrap_or_default()
    }

    pub fn change_machine(&self, before: &str, after: &str) {
        let machine = self.read("states.yaml");
        assert!(machine.contains(before), "fixture edit must match: {machine}");
        fs::write(self.dir.join("states.yaml"), machine.replace(before, after)).unwrap();
    }

    pub fn state(&self) -> String {
        let plan = self.read("plan.rhei.md");
        plan.lines()
            .find_map(|line| line.strip_prefix("**State:** "))
            .unwrap_or_default()
            .to_string()
    }

    pub fn calls(&self, state: &str) -> usize {
        self.read(&format!("runtime/{state}-calls")).lines().count()
    }

    /// The spawns of `state` for one ticket alone, where tickets share a file.
    pub fn calls_of(&self, state: &str, ticket: &str) -> usize {
        self.read(&format!("runtime/{state}-calls")).lines().filter(|id| *id == ticket).count()
    }

    /// The attempts charged to `ticket`'s visit to `state`, as its spawn record
    /// says: an attempt interrupted by a run shutdown is spawned but not charged.
    pub fn charged(&self, state: &str, ticket: &str) -> u64 {
        let name = format!("task-{ticket}-{state}.json");
        fs::read(self.dir.join("runtime/spawns").join(name))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .and_then(|record| record["charged"].as_u64())
            .unwrap_or(0)
    }

    /// Every spawn record on disk, by path, with its bytes.
    pub fn spawn_records(&self) -> Vec<(PathBuf, Vec<u8>)> {
        let Ok(entries) = fs::read_dir(self.dir.join("runtime/spawns")) else { return Vec::new() };
        let mut records: Vec<_> = entries
            .map(|entry| entry.unwrap().path())
            .map(|path| {
                let bytes = fs::read(&path).unwrap();
                (path, bytes)
            })
            .collect();
        records.sort();
        records
    }

    /// Write the input the destination is waiting for, where it looks for it.
    pub fn write_note(&self) {
        write_fixture_file(&self.dir.join("runtime"), "plan.1.note.md", "note\n");
    }

    /// Every spelling of the input's absolute path a report may print: the
    /// temporary directory as created, and as the working directory resolves
    /// it where the two differ (macOS's `/var` is `/private/var`).
    pub fn note_paths(&self) -> Vec<String> {
        let mut roots = vec![self.dir.clone()];
        if let Ok(canonical) = fs::canonicalize(&self.dir) {
            roots.push(canonical);
        }
        roots
            .into_iter()
            .map(|root| root.join("runtime").join("plan.1.note.md").display().to_string())
            .map(|path| path.trim_start_matches(r"\\?\").to_string())
            .collect()
    }

    pub fn report(&self) -> String {
        self.read("runtime/run-report.md")
    }

    /// The Attention row of `plan.1`, split into its reason and next action.
    pub fn row(&self) -> (String, String) {
        let report = self.report();
        let line = report
            .split("## Attention\n")
            .nth(1)
            .unwrap_or_default()
            .split("\n## ")
            .next()
            .unwrap_or_default()
            .lines()
            .find(|line| line.starts_with("| plan.1 |"))
            .unwrap_or_else(|| panic!("missing Attention row for plan.1: {report}"))
            .to_string();
        let cells: Vec<&str> = line.split(" | ").collect();
        assert!(cells.len() >= 4, "Attention row has four cells: {line}");
        (cells[2].to_string(), cells[3].trim_end_matches(" |").to_string())
    }

    /// Establish the refused edge before any assertion about how it is told:
    /// the worker exited 0, the ticket stayed in `draft`, and the run halted.
    pub fn assert_refused(&self, result: &CliRun) {
        let all = text(result);
        assert_eq!(result.status.code(), Some(1), "the run must halt: {all}");
        assert_eq!(self.state(), "draft", "the edge into 'read' must be refused: {all}");
        assert_eq!(self.calls("read"), 0, "'read' must never run: {all}");
        assert!(!self.dir.join("runtime/plan.1.note.md").exists());
        let logs = self.dir.join("runtime/logs");
        for log in fs::read_dir(&logs).unwrap_or_else(|_| panic!("no logs: {all}")) {
            let path = log.unwrap().path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("log") {
                let body = fs::read_to_string(&path).unwrap();
                assert!(body.contains("\ncode: 0\n"), "worker must exit 0: {path:?}\n{body}");
            }
        }
    }

    /// What a surface must say: the destination, the input, and where it is
    /// looked for, absolutely. §FS-rhei-errors.1.5.2 §FS-rhei-run-report.3.1
    pub fn naming_gaps(&self, surface: &str, said: &str) -> Vec<String> {
        let mut gaps = Vec::new();
        if !said.contains("'read'") {
            gaps.push(format!("{surface} does not name the destination 'read': {said}"));
        }
        // The `<name> (<path>)` entry every artifact diagnostic uses.
        // §FS-rhei-agents.3.2.1
        let entries: Vec<String> =
            self.note_paths().iter().map(|path| format!("note ({path})")).collect();
        if !entries.iter().any(|entry| said.contains(entry.as_str())) {
            gaps.push(format!("{surface} does not name the input as one of {entries:?}: {said}"));
        }
        gaps
    }

    /// What a surface must not say, whatever else it says, quoting only the
    /// lines that say it.
    pub fn misleading_gaps(surface: &str, said: &str, also: &[&str]) -> Vec<String> {
        MISLEADING
            .iter()
            .chain(also)
            .filter(|wrong| said.contains(**wrong))
            .map(|wrong| {
                let lines: Vec<&str> =
                    said.lines().filter(|line| line.contains(*wrong)).map(str::trim).collect();
                format!("{surface} says {wrong:?}: {}", lines.join(" // "))
            })
            .collect()
    }
}

pub(super) fn text(result: &CliRun) -> String {
    format!("{}\n{}", result.stdout, result.stderr)
}
