//! Token-free reconstruction of #749's Panta/member fixture. Two separately
//! stored tickets exercise the real worker pool, rather than its serial fallback.
//! §FS-rhei-run-report.3.1 §FS-rhei-run.5.1.1 §FS-rhei-retry.3

use super::*;

pub(super) const TASK: &str = "csa-constants.deliver";

pub(super) struct Fixture {
    _dir: TestDir,
    pub project: PathBuf,
    pub member: PathBuf,
    home: PathBuf,
    parallel: bool,
}

impl Fixture {
    pub fn new(parallel: bool, exit: i32) -> Self {
        let dir = unique_temp_dir("rhei-749-spent-attention");
        let project = dir.join("project");
        let member = project.join("csa-constants");
        let home = dir.join("home");
        fs::create_dir_all(member.join("tasks")).unwrap();
        fs::create_dir_all(home.join(".config/rhei")).unwrap();
        write_fixture_file(&project, "index.panta.md", "# Panta: Spent attempt budget\n");
        write_fixture_file(&member, "index.rhei.md", "# Rhei: CSA constants\n");
        for id in if parallel { &["deliver", "sibling"][..] } else { &["deliver"][..] } {
            write_fixture_file(
                &member,
                &format!("tasks/{id}.md"),
                &format!("### Task {id}: Deliver {id}\n**State:** win\n"),
            );
        }
        write_fixture_file(
            &project,
            "states.yaml",
            r#"name: spent-attention
version: 1
states:
  win:
    initial: true
    concurrent: true
    description: Win the ticket
    target: cdx:openai:fixture
    agent_timeout: 30s
    attempts: 2
    outputs:
      - name: delivery
        path: runtime/exports/{task_id}/delivery.md
  done: { description: Finished, final: true }
  cancelled: { description: Dropped, final: true }
transitions:
  - { from: win, to: done, description: Delivered }
  - { from: win, to: win, description: New visit }
  - { from: "*", to: cancelled, description: Dropped }
"#,
        );
        let agent = write_python_agent(
            &dir,
            "fake-cdx.py",
            &format!(
                r#"agent_prompt()
root = pathlib.Path(env('RHEI_ROOT'))
append(root / 'attempts.log', env('RHEI_TASK_ID') + ':' + env('RHEI_VISIT_COUNT') + ':' + env('RHEI_ATTEMPT') + '\n')
sys.exit({exit})
"#,
            ),
        );
        write_fixture_file(
            &home,
            ".config/rhei/settings.json",
            &format!(
                r#"{{"agents":{{"cdx":{{"command":{},"stdin_prompt":true,"timeout":"30s"}}}}}}"#,
                fixture_command(&agent),
            ),
        );
        Self { _dir: dir, project, member, home, parallel }
    }

    pub fn cli(&self, args: &[&str]) -> CliRun {
        let mut cmd = rhei_command(&self.home);
        cmd.current_dir(&self.project)
            .env("XDG_CONFIG_HOME", self.home.join(".config"))
            .env("XDG_DATA_HOME", self.home.join("data"))
            .env("XDG_CACHE_HOME", self.home.join("cache"))
            .env("GIT_CEILING_DIRECTORIES", &self.project)
            .args(["--state-machine", "states.yaml"])
            .args(args);
        CliRun::from(&cmd.output().expect("run the local fake cdx fixture"))
    }

    pub fn run(&self) -> CliRun {
        let mut args = vec![
            "run",
            ".",
            "--rhei",
            "csa-constants",
            "--no-tui",
            "--no-dashboard",
            "--no-callbacks",
            "--continue-on-error",
        ];
        if self.parallel {
            args.extend(["--parallel", "2"]);
        }
        self.cli(&args)
    }

    pub fn attempts(&self) -> Vec<String> {
        fs::read_to_string(self.member.join("attempts.log"))
            .unwrap_or_default()
            .lines()
            .filter(|line| line.starts_with(&format!("{TASK}:")))
            .map(str::to_owned)
            .collect()
    }

    pub fn report(&self) -> String {
        fs::read_to_string(self.project.join("runtime/run-report.md")).expect("project run report")
    }

    /// Establish the real spawn path and one charged attempt per run before
    /// asking the new classification anything. No production state is seeded.
    pub fn consume(&self, attempt: u64) -> CliRun {
        let out = self.run();
        assert_eq!(out.status.code(), Some(1), "fixture must halt:\n{}", text(&out));
        assert_eq!(self.attempts().len(), attempt as usize, "{}", text(&out));
        let spawn = out
            .stdout
            .lines()
            .find(|line| line.contains("Spawning agent 'cdx'") && line.contains(TASK))
            .unwrap_or_else(|| panic!("no fake cdx spawn:\n{}", text(&out)));
        assert_eq!(spawn.ends_with("(parallel)"), self.parallel, "wrong spawn path: {spawn}");
        assert!(!text(&out).contains("Falling back to sequential"), "{}", text(&out));
        let records = fs::read_dir(self.project.join("runtime/spawns")).expect("active records");
        let record = records
            .filter_map(Result::ok)
            .find_map(|entry| {
                let record: serde_json::Value =
                    serde_json::from_slice(&fs::read(entry.path()).ok()?).ok()?;
                (record["task"] == TASK && record["state"] == "win").then_some(record)
            })
            .expect("active current-state record");
        assert_eq!(record["attempt"], attempt, "actual attempt must reach exhaustion");
        assert_eq!(record["charged"], attempt, "an uncharged exit cannot prove exhaustion");
        assert!(!self.member.join(format!("runtime/exports/{TASK}/delivery.md")).exists());
        assert!(!self.member.join(format!("runtime/results/{TASK}.md")).exists());
        out
    }

    pub fn later(&self, spent: u64) -> CliRun {
        let before = self.attempts();
        let out = self.run();
        assert_eq!(out.status.code(), Some(1), "{}", text(&out));
        assert_eq!(self.attempts(), before, "a spent visit spawns no new worker");
        assert!(!out.stdout.contains("Spawning agent"), "{}", text(&out));
        assert!(
            out.stderr.contains(&format!("{spent} attempts spent on this visit")),
            "{}",
            text(&out)
        );
        assert!(
            out.stderr.contains(&format!("rhei retry {TASK}")),
            "existing recovery:\n{}",
            text(&out)
        );
        out
    }
}

pub(super) fn text(out: &CliRun) -> String {
    format!("{}{}", out.stdout, out.stderr)
}

pub(super) fn section<'a>(report: &'a str, title: &str) -> &'a str {
    report.split(&format!("## {title}\n")).nth(1).unwrap_or("").split("\n## ").next().unwrap_or("")
}

pub(super) fn row<'a>(report: &'a str, title: &str) -> &'a str {
    section(report, title)
        .lines()
        .find(|line| line.contains(TASK))
        .unwrap_or_else(|| panic!("no {title} row for {TASK}:\n{report}"))
}
