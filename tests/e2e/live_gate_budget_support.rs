//! Program-only fixture ported from the rhei.118 reproducer for #653.
//! §FS-rhei-transition-cmd.3.4 §FS-rhei-run.3

use super::provider_limit_support::RunningChild;
use super::*;
use fs2::FileExt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub(super) struct BudgetFixture {
    pub dir: TestDir,
    pub member: PathBuf,
    pub input: PathBuf,
    pub task_file: PathBuf,
    pub machine: PathBuf,
    pub id: &'static str,
}

impl BudgetFixture {
    pub fn new(project: bool) -> Self {
        let dir = unique_temp_dir("issue-653-live-gate-budget");
        let member = if project { dir.join("ticket") } else { dir.to_path_buf() };
        fs::create_dir_all(&member).unwrap();
        let tasks = "### Task 1: The gated ticket\n**State:** work\n\n\
                     ### Task 2: Keep the run alive\n**State:** keepalive\n";
        let (input, task_file, id) = if project {
            write_fixture_file(&dir, "index.panta.md", "# Panta: Budget release\n");
            write_fixture_file(&member, "index.rhei.md", "# Rhei: Ticket\n");
            fs::create_dir_all(member.join("tasks")).unwrap();
            let task_file = write_fixture_file(&member, "tasks/01-ticket.md", tasks);
            (member.clone(), task_file, "ticket.1")
        } else {
            let plan = write_fixture_file(
                &member,
                "plan.rhei.md",
                &format!("# Rhei: Budget release\n\n## Tasks\n\n{tasks}"),
            );
            (plan.clone(), plan, "plan.1")
        };
        // These Python scripts run as programs, without an agent transport.
        let work = write_python_agent(
            &member,
            "work.py",
            "append(pathlib.Path(__file__).parent / 'visits.txt', 'visit\\n')\n",
        );
        let keepalive = write_python_agent(
            &member,
            "keepalive.py",
            "if not (pathlib.Path(__file__).parent / 'finish').exists():\n    sys.exit(75)\n\
             result('keepalive finished\\n')\n",
        );
        let machine = write_fixture_file(
            &member,
            "states.yaml",
            &format!(
                r#"name: live-gate-budget
version: 1
states:
  work:
    description: Counted program
    visits: 2
    program:
      command: {work}
    program_timeout: {FIXTURE_MARGIN}
  human-review:
    description: Budget exhausted
    gating: true
  keepalive:
    description: Keep the same run alive
    program:
      command: {keepalive}
    program_timeout: {FIXTURE_MARGIN}
    poll:
      interval: 1s
      max_attempts: 600
  plain:
    description: Unbudgeted destination
  completed:
    description: Done
    final: true
  cancelled:
    description: Abandoned
    final: true
transitions:
  - {{ from: work, to: human-review, condition: visitCount >= visits }}
  - {{ from: work, to: work }}
  - {{ from: human-review, to: work }}
  - {{ from: human-review, to: plain }}
  - {{ from: plain, to: work }}
  - {{ from: keepalive, to: keepalive, exit_code: 75 }}
  - {{ from: keepalive, to: completed, exit_code: 0 }}
  - {{ from: "*", to: cancelled }}
profiles:
  default:
    initial: work
    allowed: [work, human-review, keepalive, plain, completed, cancelled]
node_policy:
  root: default
  default: default
"#,
                work = fixture_command(&work),
                keepalive = fixture_command(&keepalive),
            ),
        );
        Self { dir, member, input, task_file, machine, id }
    }

    pub fn command(&self) -> Command {
        let mut cmd = rhei_command(self.dir.join(".home"));
        cmd.current_dir(&*self.dir)
            .env("XDG_CONFIG_HOME", self.dir.join(".home/config"))
            .stdin(Stdio::null());
        cmd
    }

    pub fn start(&self, input: &Path, name: &str) -> RunningChild {
        let mut cmd = self.command();
        cmd.arg("run").arg(input).arg("--no-tui");
        RunningChild::spawn(&mut cmd, &self.dir, name)
    }

    pub fn transition(&self, input: &Path, from: &str, to: &str) -> CliRun {
        let output = self
            .command()
            .arg("transition")
            .arg(input)
            .args(["--task", self.id, "--from", from, "--to", to])
            .output()
            .unwrap();
        CliRun::from(&output)
    }

    pub fn visits(&self) -> usize {
        fs::read_to_string(self.member.join("visits.txt")).unwrap_or_default().lines().count()
    }

    pub fn gated(&self) -> bool {
        fs::read_to_string(&self.task_file).unwrap().contains("**State:** human-review")
    }

    pub fn set_source(&self, from: &str) {
        let text = fs::read_to_string(&self.task_file).unwrap();
        let state = text.lines().find(|line| line.starts_with("**State:**")).unwrap();
        fs::write(&self.task_file, text.replacen(state, &format!("**State:** {from}"), 1)).unwrap();
    }

    pub fn descriptor(&self, root: &Path) -> serde_json::Value {
        serde_json::from_str(&fs::read_to_string(root.join("runtime/run.json")).unwrap()).unwrap()
    }
}

pub(super) fn wait_for_budget_run(
    run: &mut RunningChild,
    label: &str,
    mut predicate: impl FnMut() -> bool,
) {
    // Execution margin, not a timeout whose expiry is under test. §REQ-cross-platform.8
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let status = run.child().try_wait().unwrap();
        if predicate() {
            return;
        }
        assert!(status.is_none(), "run exited {status:?} before {label}; {}", run.output());
        assert!(Instant::now() < deadline, "timed out waiting for {label}; {}", run.output());
        std::thread::sleep(Duration::from_millis(25));
    }
}

pub(super) fn finish_budget_run(run: &mut RunningChild) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if let Some(status) = run.child().try_wait().unwrap() {
            assert!(status.success(), "run failed {status}; {}", run.output());
            return;
        }
        assert!(Instant::now() < deadline, "run did not finish; {}", run.output());
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// A held OS lock models activity; the same file left unlocked models a stale record.
pub(super) fn run_lock(root: &Path, held: bool) -> fs::File {
    fs::create_dir_all(root.join(".rhei")).unwrap();
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .read(true)
        .write(true)
        .open(root.join(".rhei/run.lock"))
        .unwrap();
    if held {
        file.try_lock_exclusive().unwrap();
    }
    file
}

pub(super) fn assert_release_success(release: &CliRun, id: &str, from: &str, to: &str) {
    assert_success(release);
    assert_eq!(release.stdout, format!("Task {id} transitioned: '{from}' → '{to}'\n"));
}

pub(super) fn assert_advisory(release: &CliRun, id: &str, root: &Path, run_id: Option<&str>) {
    let text = &release.stderr;
    assert!(
        text.contains("held machine"),
        "missing live gate-release advisory about the held machine/old visits budget; stderr: {text:?}"
    );
    for part in [
        id,
        "work",
        "visits",
        "old",
        "validat",
        "release",
        "adopt",
        "rhei stop",
        "rhei transition",
        "gated",
        "rhei run",
    ] {
        assert!(text.contains(part), "advisory is missing {part:?}: {text}");
    }
    assert!(text.contains("may") || text.contains("can"), "risk must be conditional: {text}");
    assert!(
        text.contains(&root.display().to_string()) || run_id.is_some_and(|id| text.contains(id)),
        "advisory must identify the relevant run or execution root: {text}"
    );
}

pub(super) fn assert_no_advisory(release: &CliRun) {
    for text in [&release.stdout, &release.stderr] {
        for part in ["held machine", "old visits", "rhei stop"] {
            assert!(!text.contains(part), "unexpected live gate-release advisory: {text}");
        }
    }
}
