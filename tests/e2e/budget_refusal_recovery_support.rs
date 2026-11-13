//! Live admission evidence follows its arrival and requested invocation set.
//! §FS-rhei-run.3.4

use super::budget_admission_halt_support::ANCESTOR;
use super::budget_support::{home_for, write_machine_settings};
use super::*;
use std::process::{Command, Stdio};

pub(super) struct Recovery {
    _dir: TestDir,
    pub root: PathBuf,
    action: &'static str,
}

impl Recovery {
    pub fn new(action: &'static str, counted: bool) -> Self {
        let dir = unique_temp_dir("rhei-671-recovery");
        let root = dir.join("evidence");
        fs::create_dir_all(root.join("tasks")).unwrap();
        write_fixture_file(&root, "index.rhei.md", "# Rhei: Refusal evidence\n");
        let agent = write_python_agent(
            &root,
            "agent.py",
            "root = pathlib.Path(__file__).parent\n\
             append(root / 'spawned', env('RHEI_TASK_ID') + '\\n')\n\
             write(root / 'runtime/prime.txt', 'earned\\n')\n",
        );
        // Every transport is a local stand-in, including accidental defaults.
        let profile: serde_json::Value = serde_json::from_str(&format!(
            r#"{{"command": {}, "stdin_prompt": true, "timeout": "60s"}}"#,
            fixture_command(&agent)
        ))
        .unwrap();
        write_machine_settings(
            &root,
            &serde_json::json!({
                "defaults": {"agent": "cdx", "agent_timeout": "60s", "transition_limit": 30},
                "models": {
                    "old": {"default_agent": "cdx", "provider": "openai", "model": "old"},
                    "new": {"default_agent": "cdx", "provider": "openai", "model": "new"},
                    "blocked": {"default_agent": "cdx", "provider": "openai", "model": "blocked"}
                },
                "agents": {"cdx": profile, "codex": profile, "claude-code": profile, "pi": profile}
            })
            .to_string(),
        );
        let repair =
            write_python_agent(&root, "repair.py", include_str!("budget_refusal_recovery.py"));
        let prepare = write_python_agent(&root, "prepare.py", "pass\n");
        let arrival = action == "arrival";
        let state = if arrival { "write" } else { "prepare" };
        let model = if arrival || action == "fanout" { "" } else { "**Model:** old\n" };
        write_fixture_file(
            &root,
            "tasks/1.md",
            &format!("### Task 1: Refused work\n**State:** {state}\n{model}"),
        );
        write_fixture_file(&root, "tasks/2.md", "### Task 2: Repair\n**State:** repair\n");
        if !arrival {
            write_fixture_file(
                &root,
                "tasks/0.md",
                "### Task 0: Spend allowance\n**State:** prime\n",
            );
        }
        if action == "allocation" {
            write_fixture_file(
                &root,
                "tasks/3.md",
                "### Task 3: Refused allocation\n**State:** write\n**Model:** blocked\n",
            );
            fs::create_dir_all(root.join("runtime/logs")).unwrap();
            write_fixture_file(&root, "runtime/logs/task-evidence.3-write-blocked.log", "orphan\n");
        }
        let visits = if counted { "    visits: 3\n" } else { "" };
        let invocation = if action == "fanout" { "all_models: [old, new]" } else { "agent: cdx" };
        // Arrival transitions must have no missing output blocking the CLI hop.
        let outputs = if arrival {
            ""
        } else {
            "    outputs:\n      - {name: owed, path: runtime/owed.txt}\n"
        };
        write_fixture_file(
            &root,
            "states.yaml",
            &format!(
                r#"name: refusal-recovery
version: 1
models: [old, new, blocked]
states:
  prime:
    description: Spend allowance
    concurrent: true
    agent: cdx
    agent_timeout: 60s
    outputs:
      - {{name: prime, path: runtime/prime.txt}}
  prepare:
    initial: true
    description: Prepare
    concurrent: true
    program: {{command: {prepare}}}
  write:
    description: Write
    concurrent: true
{visits}    {invocation}
    agent_timeout: 60s
{outputs}  repair:
    description: Repair
    concurrent: true
    program: {{command: {repair}}}
    program_timeout: 180s
  pause:
    description: Paused
    gating: true
  parked:
    description: Parked
    gating: true
  done:
    description: Done
    final: true
transitions:
  - {{from: prime, to: parked, description: Spent}}
  - {{from: prepare, to: write, exit_code: 0, description: Prepared}}
  - {{from: write, to: done, description: Written}}
  - {{from: write, to: pause, description: Pause}}
  - {{from: pause, to: write, description: Resume}}
  - {{from: repair, to: parked, exit_code: 0, description: Repaired}}
  - {{from: parked, to: done, description: Finish}}
"#,
                prepare = fixture_command(&prepare),
                repair = fixture_command(&repair),
            ),
        );
        Self { _dir: dir, root, action }
    }

    pub fn command(&self) -> Command {
        let home = home_for(&self.root);
        let mut cmd = rhei_command(&home);
        cmd.current_dir(&self.root)
            .env("XDG_CONFIG_HOME", home.join(".config"))
            .env("XDG_DATA_HOME", home.join("data"))
            .env("XDG_CACHE_HOME", home.join("cache"))
            .env("GIT_CEILING_DIRECTORIES", &self.root)
            .env("ISSUE671_BINARY", rhei_binary())
            .env("ISSUE671_ACTION", self.action)
            .stdin(Stdio::null());
        cmd
    }

    pub fn run(&self) -> CliRun {
        if self.action != "arrival" {
            let init = self
                .command()
                .args([
                    "budget",
                    "init",
                    ".",
                    "--invocations",
                    "1",
                    "--reason",
                    "fixture exhaustion",
                ])
                .output()
                .unwrap();
            assert_success(&CliRun::from(&init));
        }
        let mut cmd = self.command();
        cmd.args([
            "--state-machine",
            "states.yaml",
            "run",
            ".",
            "--no-tui",
            "--no-callbacks",
            "--parallel",
            "3",
        ]);
        if self.action == "arrival" {
            cmd.env("RHEI_BUDGET_PARENT_RESERVATION", ANCESTOR);
        }
        let out = CliRun::from(&cmd.output().unwrap());
        let all = super::budget_admission_halt_support::text(&out);
        assert_eq!(out.status.code(), Some(1), "{all}");
        assert!(all.contains("no further advancement possible"), "{all}");
        assert!(!all.contains("Falling back to sequential"), "{all}");
        assert_eq!(fs::read_to_string(self.root.join("repaired")).unwrap(), self.action, "{all}");
        assert!(self.task(2).contains("**State:** parked"), "{all}");
        assert!(!self.root.join("runtime/owed.txt").exists());
        assert!(!self.root.join("runtime/results/evidence.1.md").exists());
        assert!(!self.task(1).contains("**Result:**"));
        eprintln!("action={}\n{all}", self.action);
        out
    }

    pub fn task(&self, n: u32) -> String {
        fs::read_to_string(self.root.join(format!("tasks/{n}.md"))).unwrap()
    }

    pub fn events(&self) -> Vec<serde_json::Value> {
        fs::read_to_string(self.root.join("runtime/events.jsonl"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    pub fn facts(&self) -> serde_json::Value {
        let out = CliRun::from(
            &self.command().args(["budget", "show", ".", "--format", "json"]).output().unwrap(),
        );
        assert_success(&out);
        serde_json::from_str(&out.stdout).unwrap()
    }
}
