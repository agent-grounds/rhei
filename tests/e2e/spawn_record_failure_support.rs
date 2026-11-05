//! Completion-record failures leave a diagnostic, never authority inferred from a log.
//! Adapted from the rhei.73 reproducer's one-shot, rerun and polling fixtures.
//! §FS-rhei-agents.8.4 §FS-rhei-agents.8.1 §FS-rhei-run.3

use super::*;

pub(super) struct Fixture {
    pub dir: PathBuf,
    pub parallel: &'static str,
    _temp: TestDir,
    refill: bool,
}

impl Fixture {
    pub fn new(agent: bool, parallel: &'static str, fault: &str, code: i32, poll: bool) -> Self {
        Self::at_dispatch(agent, parallel, fault, code, poll, false)
    }

    pub fn at_dispatch(
        agent: bool,
        parallel: &'static str,
        fault: &str,
        code: i32,
        poll: bool,
        refill: bool,
    ) -> Self {
        let temp = unique_temp_dir("548-spawn-record-failure");
        let dir = if parallel == "1" { temp.to_path_buf() } else { temp.join("plan") };
        if parallel == "1" {
            write_fixture_file(&dir, "plan.rhei.md", super::agent_reentry_support::PLAN);
        } else {
            fs::create_dir_all(dir.join("tasks")).expect("directory workspace task files");
            write_fixture_file(&dir, "index.rhei.md", "# Rhei: Spawn record failure\n");
            let prior = if refill { "**Prior:** 2\n" } else { "" };
            write_fixture_file(
                &dir,
                "tasks/01-target.md",
                &format!("### Task 1: Work\n**State:** work\n{prior}",),
            );
            for id in [2, 3] {
                write_fixture_file(
                    &dir,
                    &format!("tasks/0{id}-companion.md"),
                    &format!("### Task {id}: Pool companion\n**State:** companion\n",),
                );
            }
        }
        let body = format!(
            r#"root = pathlib.Path(__file__).parent
task = env('RHEI_TASK_ID')
if task != 'plan.1':
    if {fault:?} == 'race':
        sys.exit(1)  # No successful sibling may earn a masking preflight pass.
    if not ({refill} and task == 'plan.2'):
        deadline = time.monotonic() + 8
        while not (root / 'companions-ready').exists():
            if time.monotonic() > deadline:
                raise RuntimeError('target worker never activated its fault')
            time.sleep(0.01)
    result('Companion finished.\n')
    sys.exit(0)
append(root / 'calls', 'ran\n')
result('Worker finished.\n')
fault = {fault:?}
if fault == 'directory':
    write(root / 'runtime/spawns', 'not a directory')
elif fault == 'lock':
    write(root / '.rhei/spawn-locks', 'not a directory')
elif fault == 'read':
    (root / 'runtime/spawns/task-plan.1-work.json').mkdir(parents=True)
write(root / 'fault-active', 'yes')
write(root / 'companions-ready', 'yes')
code = {code}
if {poll} and len((root / 'calls').read_text().splitlines()) > 1:
    code = 0
sys.exit(code)
"#,
            poll = if poll { "True" } else { "False" },
            refill = if refill { "True" } else { "False" },
        );
        let worker = write_python_agent(&dir, "worker.py", &body);
        super::agent_reentry_support::write_settings(&dir, &worker, false);
        let invocation = if agent {
            "agent: mock\n    agent_timeout: 10s".to_string()
        } else {
            format!("program: {{command: {}}}\n    program_timeout: 10s", fixture_command(&worker))
        };
        let polling = if poll { "poll: {interval: 1s, max_attempts: 3}" } else { "" };
        let exit_code = if agent { "" } else { ", exit_code: 0" };
        let poll_edges = if poll {
            "  - {from: work, to: work, exit_code: 75}\n\
             \x20 - {from: work, to: unanswered, condition: 'pollAttempts >= pollMaxAttempts'}\n"
        } else {
            ""
        };
        write_fixture_file(
            &dir,
            "states.yaml",
            &format!(
                "name: lost-spawn-record\nversion: 1\nstates:\n\
                 \x20 work:\n    initial: true\n    concurrent: true\n    description: Work\n    {invocation}\n    {polling}\n\
                 \x20 companion:\n    concurrent: true\n    description: Companion\n    {invocation}\n\
                 \x20 done: {{final: true, description: Done}}\n\
                 \x20 unanswered: {{final: true, description: Out of looks}}\n\
                 transitions:\n  - {{from: work, to: done{exit_code}}}\n\
                 \x20 - {{from: companion, to: done{exit_code}}}\n{poll_edges}"
            ),
        );
        Self { dir, parallel, _temp: temp, refill }
    }

    pub fn command(&self) -> std::process::Command {
        let mut cmd = rhei_command(self.dir.join("home"));
        // Nested runs must not inherit the invoking worker's budget reservation or identity.
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("RHEI_") {
                cmd.env_remove(key);
            }
        }
        cmd.current_dir(&self.dir)
            .args(["--state-machine", "states.yaml", "run"])
            .arg(if self.parallel == "1" { "plan.rhei.md" } else { "." })
            .args([
                "--no-tui",
                "--no-callbacks",
                "--continue-on-error",
                "--parallel",
                self.parallel,
            ]);
        if self.dir.join("json-output").exists() {
            cmd.arg("--json");
        }
        cmd
    }

    pub fn run(&self) -> CliRun {
        let out = CliRun::from(&self.command().output().expect("fixture run"));
        self.assert_dispatch(&out);
        out
    }

    // Effective capacity alone does not prove pool dispatch. Cold runs fill two
    // slots before reaping; reruns whose target was refused fill them on refill.
    // A third same-pass assignment pins the live refill too. §FS-rhei-run.3
    pub fn assert_dispatch(&self, out: &CliRun) {
        let all = text(out);
        assert!(!all.contains("Falling back to sequential"), "{all}");
        let records = fs::read_to_string(self.dir.join("runtime/events.jsonl")).unwrap();
        let events: Vec<serde_json::Value> = records
            .lines()
            .map(|line| serde_json::from_str(line).expect("run event JSON"))
            .collect();
        let start = events.iter().rposition(|e| e["event"] == "run_started").unwrap();
        let events = &events[start..];
        assert_eq!(events[0]["parallel"], self.parallel.parse::<u64>().unwrap(), "{all}");
        if self.parallel == "1" {
            return;
        }
        let assignments: Vec<_> =
            events.iter().enumerate().filter(|(_, e)| e["event"] == "slot_assigned").collect();
        assert!(assignments.len() >= 3, "pool must fill and refill: {records}");
        let release = events.iter().position(|e| e["event"] == "slot_released").unwrap();
        let target_assigned = assignments.iter().any(|(_, e)| e["task"] == "plan.1");
        if target_assigned {
            assert!(assignments[1].0 < release, "cold run must fill both initial slots: {records}");
        }
        assert!(
            assignments.windows(2).any(|pair| {
                pair[0].1["slot"] != pair[1].1["slot"]
                    && !events[pair[0].0..pair[1].0].iter().any(|e| e["event"] == "slot_released")
            }),
            "two pool slots must actually be occupied together: {records}"
        );
        assert!(assignments[2].0 > release, "third assignment must refill: {records}");
        assert!(
            !events[assignments[0].0..assignments[2].0]
                .iter()
                .any(|e| e["event"] == "pass_started"),
            "refill must stay in the pool pass: {records}"
        );
        if self.refill && target_assigned {
            assert_eq!(assignments[2].1["task"], "plan.1", "target must use refill: {records}");
        } else if target_assigned {
            assert!(
                assignments[..2].iter().any(|(_, e)| e["task"] == "plan.1"),
                "target must use initial fill: {records}"
            );
        }
    }

    pub fn record(&self) -> PathBuf {
        self.dir.join("runtime").join("spawns").join("task-plan.1-work.json")
    }

    pub fn log(&self) -> PathBuf {
        self.dir.join("runtime/logs/task-plan.1-work.log")
    }

    pub fn assert_worker(&self, out: &CliRun, code: i32, done: bool) {
        let all = text(out);
        assert_eq!(out.status.code(), Some(if done { 0 } else { 1 }), "{all}");
        assert_eq!(fs::read_to_string(self.dir.join("calls")).unwrap(), "ran\n", "{all}");
        assert!(fs::read_to_string(self.log()).unwrap().contains(&format!("code: {code}")));
        let plan = fs::read_to_string(self.dir.join(if self.parallel == "1" {
            "plan.rhei.md"
        } else {
            "tasks/01-target.md"
        }))
        .unwrap();
        let state = plan.split("**State:** ").nth(1).unwrap().lines().next().unwrap();
        assert_eq!(state, if done { "done" } else { "work" }, "{plan}");
        assert!(!self.record().is_file(), "fixture must lose the completion record");
    }

    pub fn clear_fault(&self, fault: &str) {
        let obstacle = match fault {
            "directory" => self.dir.join("runtime/spawns"),
            "lock" => self.dir.join(".rhei/spawn-locks"),
            "read" => self.record(),
            _ => self.dir.join("fault-active"),
        };
        if obstacle.is_dir() {
            fs::remove_dir(&obstacle).unwrap();
        } else {
            fs::remove_file(&obstacle).unwrap();
        }
        if self.parallel != "1" {
            // The refused rerun must also fill and refill the actual pool.
            for id in [4, 5, 6] {
                write_fixture_file(
                    &self.dir,
                    &format!("tasks/0{id}-rerun.md"),
                    &format!("### Task {id}: Rerun companion\n**State:** companion\n",),
                );
            }
        }
    }

    pub fn cause(&self, fault: &str) -> String {
        match fault {
            "directory" => fs::create_dir_all(self.dir.join("runtime/spawns")).unwrap_err(),
            "lock" => fs::create_dir_all(self.dir.join(".rhei/spawn-locks")).unwrap_err(),
            "read" => fs::read_to_string(self.record()).unwrap_err(),
            _ => panic!("unknown filesystem obstacle"),
        }
        .to_string()
    }

    pub fn assert_warning(&self, out: &CliRun, cause: &str) {
        let all = text(out);
        let starts: Vec<_> = all
            .match_indices('\n')
            .map(|(at, _)| at + 1)
            .chain(std::iter::once(0))
            .filter(|at| {
                let line = all[*at..].lines().next().unwrap_or_default();
                line.to_lowercase().contains("spawn record")
                    && !line.contains("no spawn record accounts for it")
            })
            .collect();
        let diagnostic = starts
            .iter()
            .find_map(|at| {
                let block = all[*at..].split("\n\n").next().unwrap();
                block.contains(&self.record().display().to_string()).then_some(block)
            })
            .unwrap_or_else(|| panic!("missing spawn record diagnostic:\n{all}"));
        for expected in ["plan.1", "work", &self.record().display().to_string(), cause] {
            assert!(
                diagnostic.contains(expected),
                "record diagnostic must include {expected:?}:\n{all}"
            );
        }
    }
}

pub(super) fn text(out: &CliRun) -> String {
    format!("{}\n{}", out.stdout, out.stderr)
}

pub(super) fn assert_refusal_halt(out: &CliRun) {
    let all = text(out);
    assert!(all.contains("refusing to spawn:"), "the unaccounted log must be refused:\n{all}");
    assert!(all.contains("no spawn record accounts for it"), "{all}");
    let json_help = out
        .stderr
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find_map(|e| e["error"]["help"].as_str().map(str::to_owned));
    let help = json_help
        .as_deref()
        .unwrap_or_else(|| out.stderr.split("help:").last().expect("halt help"));
    assert!(
        !help.contains("blocked, gated, or assigned"),
        "a refused spawn is not ordinary blockage:\n{all}"
    );
    assert!(
        help.contains("spawn") && (help.contains("refus") || help.contains("could not")),
        "halt help must identify refused spawning:\n{all}"
    );
    assert!(
        help.contains("above")
            || help.contains("diagnostic")
            || help.contains("task-plan.1-work.log"),
        "halt help must point to the concrete refusal or repeat its cause:\n{all}"
    );
}
