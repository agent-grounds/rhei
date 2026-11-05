//! Completion-record failures leave a diagnostic, never authority inferred from a log.
//! Adapted from the rhei.73 reproducer's one-shot, rerun and polling fixtures.
//! §FS-rhei-agents.8.4 §FS-rhei-agents.8.1 §FS-rhei-run.3

use super::*;

pub(super) struct Fixture {
    pub dir: TestDir,
    pub parallel: &'static str,
    parallel_agent: bool,
}

impl Fixture {
    pub fn new(agent: bool, parallel: &'static str, fault: &str, code: i32, poll: bool) -> Self {
        let dir = unique_temp_dir("548-spawn-record-failure");
        write_fixture_file(&dir, "plan.rhei.md", super::agent_reentry_support::PLAN);
        let parallel_agent = agent && parallel != "1";
        if parallel_agent {
            let plan = dir.join("plan.rhei.md");
            let body = fs::read_to_string(&plan).unwrap();
            fs::write(plan, format!("{body}\n### Task 2: Pool companion\n**State:** work\n"))
                .unwrap();
        }
        let body = format!(
            r#"root = pathlib.Path(__file__).parent
if env('RHEI_TASK_ID') != 'plan.1':
    while not (root / 'fault-active').exists():
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
code = {code}
if {poll} and len((root / 'calls').read_text().splitlines()) > 1:
    code = 0
sys.exit(code)
"#,
            poll = if poll { "True" } else { "False" },
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
                 \x20 work:\n    initial: true\n    description: Work\n    {invocation}\n    {polling}\n\
                 \x20 done: {{final: true, description: Done}}\n\
                 \x20 unanswered: {{final: true, description: Out of looks}}\n\
                 transitions:\n  - {{from: work, to: done{exit_code}}}\n{poll_edges}"
            ),
        );
        Self { dir, parallel, parallel_agent }
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
            .args(["--state-machine", "states.yaml", "run", "plan.rhei.md"])
            .args(["--no-tui", "--no-callbacks", "--parallel", self.parallel]);
        cmd
    }

    pub fn run(&self) -> CliRun {
        CliRun::from(&self.command().output().expect("fixture run"))
    }

    pub fn record(&self) -> PathBuf {
        self.dir.join("runtime/spawns/task-plan.1-work.json")
    }

    pub fn log(&self) -> PathBuf {
        self.dir.join("runtime/logs/task-plan.1-work.log")
    }

    pub fn assert_worker(&self, out: &CliRun, code: i32, done: bool) {
        let all = text(out);
        assert_eq!(out.status.code(), Some(if done { 0 } else { 1 }), "{all}");
        assert_eq!(fs::read_to_string(self.dir.join("calls")).unwrap(), "ran\n", "{all}");
        assert!(fs::read_to_string(self.log()).unwrap().contains(&format!("code: {code}")));
        let plan = fs::read_to_string(self.dir.join("plan.rhei.md")).unwrap();
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
        if self.parallel_agent {
            // A single agent uses the sequential path even at --parallel 2.
            // A new companion makes the refused rerun enter the pool too.
            let plan = self.dir.join("plan.rhei.md");
            let body = fs::read_to_string(&plan).unwrap();
            fs::write(plan, format!("{body}\n### Task 3: Rerun companion\n**State:** work\n"))
                .unwrap();
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
    let help = out.stderr.split("help:").last().expect("halt help");
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
