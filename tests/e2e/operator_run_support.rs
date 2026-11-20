//! Two-member, provider-free reproduction of #757. §FS-rhei-transition-cmd.6
//! Scheduling scope is independent of the run's lock set. §FS-rhei-run.2.6

use super::live_gate_budget_support::wait_for_budget_run;
use super::operator_force_support::*;
use super::provider_limit_support::RunningChild;
use super::*;
use fs2::FileExt;
use std::process::{Command, Stdio};

pub(super) struct OperatorRunFixture {
    pub force: ForceFixture,
    pub home: PathBuf,
}

impl OperatorRunFixture {
    pub fn new() -> Self {
        let dir = unique_temp_dir("issue-757-operator-run");
        fs::create_dir_all(dir.join("constants/tasks")).unwrap();
        fs::create_dir_all(dir.join("profile/tasks")).unwrap();
        let plan = write_fixture_file(&dir, "index.panta.md", "# Panta: Operator run preflight\n");
        write_fixture_file(&dir.join("constants"), "index.rhei.md", "# Rhei: Constants\n");
        write_fixture_file(&dir.join("profile"), "index.rhei.md", "# Rhei: Profile\n");
        write_fixture_file(
            &dir.join("profile/tasks"), "deliver.md",
            "### Task deliver: Deliver\n**State:** human-gate\n\n### Task review: Review\n**State:** human-gate\n",
        );
        write_fixture_file(&dir.join("profile"), "states.yaml", FORCE_MACHINE);
        write_fixture_file(
            &dir.join("constants/tasks"),
            "1.md",
            "### Task 1: Keep visit in flight\n**State:** work\n",
        );
        write_fixture_file(&dir.join("constants"), "states.yaml",
            "name: constants\nversion: 1\nstates:\n  work: {initial: true, target: 'stub[yolo]:local:stub'}\n  done: {final: true}\ntransitions:\n  - {from: work, to: done}\n");
        let agent = write_fixture_file(&dir, "agent.py",
            "import pathlib, sys, time\nif '--version' in sys.argv: print('stub 1.0'); sys.exit(0)\nsys.stdin.read()\nroot = pathlib.Path(__file__).parent\n(root / 'started').touch()\nwhile not (root / 'finish').exists(): time.sleep(.01)\nresult = root / 'constants/runtime/results/constants.1.md'\nresult.parent.mkdir(parents=True, exist_ok=True)\nresult.write_text('Stand-in finished.\\n')\n(root / 'ended').touch()\n");
        let home = dir.join("attended-home");
        fs::create_dir_all(home.join(".config/rhei")).unwrap();
        fs::write(
            home.join(".config/rhei/settings.json"),
            serde_json::json!({
                "defaults": {"agent_timeout": "300s"},
                "agents": {"stub": {"command": [python_command(), "-I", agent.to_str().unwrap()],
                    "stdin_prompt": true, "modes": {"yolo": []}}}
            })
            .to_string(),
        )
        .unwrap();
        let machine = dir.join("profile/states.yaml");
        let fixture = Self { force: ForceFixture { dir, plan, machine }, home };
        let out = fixture.command().args(["validate", "."]).output().unwrap();
        assert_success(&CliRun::from(&out));
        fixture
    }

    pub fn command(&self) -> Command {
        let mut cmd = rhei_command(&self.home);
        cmd.current_dir(&*self.force.dir)
            .env("XDG_CONFIG_HOME", self.home.join(".config"))
            .stdin(Stdio::null());
        cmd
    }

    pub fn start(&self, scope: &[&str]) -> RunningChild {
        fs::remove_file(self.force.dir.join("started")).ok();
        let mut cmd = self.command();
        cmd.args(["run", ".", "--no-tui", "--no-dashboard", "--no-callbacks"]);
        for member in scope {
            cmd.args(["--rhei", member]);
        }
        let mut run = RunningChild::spawn(&mut cmd, &self.force.dir, "operator-run");
        wait_for_budget_run(&mut run, "stand-in agent visit in flight", || {
            self.force.dir.join("started").exists()
        });
        run
    }

    pub fn id(&self) -> String {
        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(self.force.dir.join("runtime/run.json")).unwrap())
                .unwrap();
        value["id"].as_str().unwrap().to_owned()
    }

    pub fn finish(&self, run: &mut RunningChild) {
        fs::write(self.force.dir.join("finish"), "").unwrap();
        assert!(run.wait_for_exit("stand-in visit completion").success(), "{}", run.output());
    }

    pub fn force_args(&self) -> Vec<&str> {
        vec![
            "transition",
            "profile.deliver",
            "--from",
            "human-gate",
            "--to",
            "implement",
            "--force",
            "--reason",
            "gates started by hand",
        ]
    }

    pub fn snapshot(&self) -> Vec<(PathBuf, Option<Vec<u8>>)> {
        // Include authored task/manifest bytes (visits, checkpoints, links and assignees),
        // all recovery/result/ledger paths, and owner records at every affected root.
        let mut paths = vec![
            self.force.plan.clone(),
            self.force.dir.join("profile/index.rhei.md"),
            self.force.dir.join("profile/tasks/deliver.md"),
        ];
        for root in [&*self.force.dir, &self.force.dir.join("profile")] {
            for relative in [
                "runtime/state-transitions.log",
                "runtime/transitions.log",
                "runtime/results/profile.deliver.md",
                ".rhei/forced-recovery.json",
                ".rhei/run.lock",
            ] {
                paths.push(root.join(relative));
            }
        }
        paths
            .into_iter()
            .map(|path| {
                let bytes = fs::read(&path).ok();
                (path, bytes)
            })
            .collect()
    }
}

impl Drop for OperatorRunFixture {
    fn drop(&mut self) {
        // Release the stand-in even when a regression assertion unwinds and the
        // scheduler handle has already been killed. No orphan agent waits 300s.
        let _ = fs::write(self.force.dir.join("finish"), "");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while self.force.dir.join("started").exists()
            && !self.force.dir.join("ended").exists()
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}

/// Semantic checks leave prose free, but commands must be pasteable and complete.
/// §FS-rhei-errors.1.2 §FS-rhei-transition-cmd.6
pub(super) fn assert_run_help(text: &str, fixture: &OperatorRunFixture, id: &str) {
    for part in [id, "constants", "profile.deliver", "outside", "whole project"] {
        assert!(text.contains(part), "missing run/scope explanation {part:?}:\n{text}");
    }
    assert_safe_remedy(text, &fixture.force.dir, id);
    assert!(
        text.contains("rhei transition --from=human-gate --to=implement --force --reason="),
        "missing complete retry:\n{text}"
    );
    assert!(
        text.contains(&shell_quote("gates started by hand")),
        "retry must quote the supplied reason:\n{text}"
    );
    assert!(
        text.contains(&fixture.force.dir.display().to_string()),
        "retry must preserve invocation directory:\n{text}"
    );
}

pub(super) fn assert_safe_remedy(text: &str, root: &Path, id: &str) {
    if cfg!(unix) {
        assert!(
            text.contains(&format!("rhei stop {id} --wait"))
                || text.contains(&format!(
                    "rhei stop {} --wait",
                    shell_quote(&root.display().to_string())
                )),
            "missing exact resolvable stop command with --wait:\n{text}"
        );
    } else {
        assert!(!text.contains(&format!("rhei stop {id}")), "unsupported stop offered:\n{text}");
        assert!(
            text.contains("original launcher") || text.contains("Ctrl-C"),
            "missing platform-supported remedy:\n{text}"
        );
    }
}

/// A synthetic holder pins owners not available through a second scheduler.
/// §FS-rhei-run.2.6 §FS-rhei-recover.3
pub(super) fn hold_operator(root: &Path) -> fs::File {
    fs::create_dir_all(root.join(".rhei")).unwrap();
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .read(true)
        .write(true)
        .open(root.join(".rhei/run.lock"))
        .unwrap();
    fs::write(
        root.join(".rhei/run.lock"),
        r#"{"operator":"other-operator","operation":"forced-recovery","pid":1}"#,
    )
    .unwrap();
    file.try_lock_exclusive().unwrap();
    file
}
