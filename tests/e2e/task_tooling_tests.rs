//! A task names the MCP servers and skills its own invocations need, and only
//! that task's agent is handed them. agent-grounds/rhei#475: tooling could be
//! attached only to a state, so a server one ticket needed reached every task in
//! the state, and naming it on the ticket was refused as an unknown metadata
//! field.
//!
//! The runs use the built-in `claude-code` profile, whose `claude` is the
//! compiled stand-in the `--mcp-config` tests use (§REQ-cross-platform.4). It
//! keeps every file it is handed, one per task and state. The servers are
//! registry entries that are never launched: what is pinned is what rhei hands
//! over, and to which task.
// §FS-rhei-task-tooling

use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;

use super::*;

/// The registry every case reads. `release-notes` points at a skill directory
/// that does not exist, so it can never attach.
fn settings() -> serde_json::Value {
    serde_json::json!({
        "defaults": { "agent_timeout": "30s" },
        "mcp_servers": {
            "thunderbird-mail": { "command": ["mcp-thunderbird-mail"] },
            "postgres": { "command": ["mcp-postgres"] }
        },
        "skills": {
            "release-notes": { "path": "./skills/release-notes" },
            "changelog-style": { "path": "./skills/changelog-style" }
        }
    })
}

/// The issue's machine, with `mcp_servers` taken off `pending`: the state
/// attaches nothing.
const INBOX_MACHINE: &str = "name: inbox
version: 1.0.0
states:
  pending:
    initial: true
    agent: claude-code
  completed:
    final: true
transitions:
  - from: pending
    to: completed
";

/// The issue's plan: only Task 1 needs the mail server.
const ISSUE_PLAN: &str = "# Rhei: Inbox

## Tasks

### Task 1: Summarise this week's release thread from the mail
**State:** pending
**MCP servers:** thunderbird-mail

### Task 2: Tidy the changelog
**State:** pending
";

/// A directory holding the settings, the machine and the plan, and the stand-in
/// staged as `claude` ahead of the real one.
struct Case {
    dir: TestDir,
    plan: PathBuf,
    machine: PathBuf,
}

impl Case {
    fn new(name: &str, machine: &str, plan: &str) -> Self {
        let dir = unique_temp_dir(name);
        let settings_dir = dir.join(".agent-grounds/rhei");
        fs::create_dir_all(&settings_dir).expect("create settings directory");
        write_fixture_file(&settings_dir, "settings.json", &settings().to_string());
        let machine = write_fixture_file(&dir, "states.yaml", machine);
        let plan = write_fixture_file(&dir, "plan.rhei.md", plan);
        Case { dir, plan, machine }
    }

    fn validate(&self) -> CliRun {
        run_cli("validate", &self.plan, &self.machine, &[])
    }

    /// `rhei run` on the built-in `claude-code` profile with the stand-in.
    fn run(&self) -> CliRun {
        let bin = self.dir.join("bin");
        fs::create_dir_all(&bin).expect("create stand-in directory");
        fs::copy(
            PathBuf::from(env!("CARGO_BIN_EXE_mcp-config-fixture")),
            bin.join(format!("claude{}", std::env::consts::EXE_SUFFIX)),
        )
        .expect("stage the stand-in as claude");
        fs::create_dir_all(self.copies()).expect("create the copy directory");

        let mut paths = vec![bin];
        paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
        let path: OsString = std::env::join_paths(paths).expect("join stand-in PATH");
        let output = rhei_command(self.dir.join("home"))
            .env("PATH", path)
            .env("RHEI_E2E_MCP_CONFIG_COPY_DIR", self.copies())
            .arg("--state-machine")
            .arg(&self.machine)
            .arg("run")
            .arg(&self.plan)
            .args(["--no-tui", "--no-callbacks"])
            .output()
            .expect("run rhei");
        CliRun::from(&output)
    }

    fn copies(&self) -> PathBuf {
        self.dir.join("mcp-config-copies")
    }

    /// The `--mcp-config` file Task `task` was handed in `state`, parsed, or
    /// `None` when rhei handed it no file there.
    fn handed(&self, task: &str, state: &str) -> Option<serde_json::Value> {
        let copy = self.copies().join(format!("plan.{task}-{state}.json"));
        let text = fs::read_to_string(&copy).ok()?;
        Some(serde_json::from_str(&text).unwrap_or_else(|err| panic!("--mcp-config: {err}")))
    }

    fn log_path(&self, task: &str, state: &str) -> PathBuf {
        self.dir.join("runtime/logs").join(format!("task-plan.{task}-{state}.log"))
    }

    fn log(&self, task: &str, state: &str) -> String {
        let path = self.log_path(task, state);
        fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("no agent log at {}: {err}", path.display()))
    }

    fn plan_text(&self) -> String {
        fs::read_to_string(&self.plan).expect("read the plan back")
    }
}

/// The value of one `key:` line in a log's header, or `None` when the header
/// has no such line.
fn header_value(log: &str, key: &str) -> Option<String> {
    let header = log.split_once("\n===\n").map_or(log, |(header, _)| header);
    header.lines().find_map(|line| line.strip_prefix(&format!("{key}: ")).map(str::to_string))
}

fn command_entry(command: &str) -> serde_json::Value {
    serde_json::json!({ "command": command, "args": [] })
}

fn assert_no_agent_log(case: &Case, task: &str, state: &str) {
    let path = case.log_path(task, state);
    assert!(!path.exists(), "an agent started for Task {task} in '{state}': {}", path.display());
}

/// The issue's own plan, once the server is named on the task rather than on
/// the state, is a valid plan.
// §FS-rhei-task-tooling.1
#[test]
#[ignore = "red until #475 lets a task name its own MCP servers and skills"]
fn the_issue_plan_validates() {
    let case = Case::new("task-tooling-issue-plan", INBOX_MACHINE, ISSUE_PLAN);

    let validate = case.validate();

    assert!(
        validate.status.success(),
        "the issue's plan, naming thunderbird-mail on Task 1 alone, must validate\n\
         stdout:\n{}\nstderr:\n{}",
        validate.stdout,
        validate.stderr
    );
}

/// Two tasks in one agent state: the server Task 1 names reaches Task 1's agent
/// and its log, and Task 2's agent is handed nothing at all.
// §FS-rhei-task-tooling.2
#[test]
#[ignore = "red until #475 lets a task name its own MCP servers and skills"]
fn a_server_one_task_names_reaches_only_that_task() {
    let case = Case::new("task-tooling-one-task", INBOX_MACHINE, ISSUE_PLAN);

    let run = case.run();
    assert_success(&run);

    let first = case.log("1", "pending");
    assert_eq!(
        header_value(&first, "mcp_servers").as_deref(),
        Some("thunderbird-mail"),
        "Task 1's log header must list the server it named:\n{first}"
    );
    assert_eq!(
        case.handed("1", "pending"),
        Some(serde_json::json!({
            "mcpServers": { "thunderbird-mail": command_entry("mcp-thunderbird-mail") }
        })),
        "Task 1's --mcp-config file must carry the server it named"
    );

    let second = case.log("2", "pending");
    assert_eq!(
        header_value(&second, "mcp_servers"),
        None,
        "Task 2 named no server and its state attaches none:\n{second}"
    );
    assert_eq!(case.handed("2", "pending"), None, "Task 2's agent must be handed no file");
}

/// A machine whose `review` must never see what a ticket brings: it keeps its
/// own `postgres` and withholds the task's servers.
const WITHHOLDING_MACHINE: &str = "name: inbox-review
version: 1.0.0
states:
  pending:
    initial: true
    agent: claude-code
  review:
    agent: claude-code
    mcp_servers: [postgres]
    withhold_task_tooling: true
  completed:
    final: true
transitions:
  - from: pending
    to: review
  - from: review
    to: completed
";

const MAIL_TASK_PLAN: &str = "# Rhei: Inbox

## Tasks

### Task 1: Summarise this week's release thread from the mail
**State:** pending
**MCP servers:** thunderbird-mail
";

/// The task passes through the withholding state rather than being refused
/// by it. Its agent there gets the state's server alone, and the log says what
/// was held back.
// §FS-rhei-task-tooling.4
#[test]
#[ignore = "red until #475 lets a task name its own MCP servers and skills"]
fn a_withholding_state_runs_the_task_without_its_servers() {
    let case = Case::new("task-tooling-withheld", WITHHOLDING_MACHINE, MAIL_TASK_PLAN);

    let validate = case.validate();
    assert_success(&validate);
    assert!(
        !validate.stderr.contains("thunderbird-mail"),
        "validation says nothing about an entry a state withholds:\n{}",
        validate.stderr
    );

    let run = case.run();
    assert_success(&run);

    assert_eq!(
        case.handed("1", "pending"),
        Some(serde_json::json!({
            "mcpServers": { "thunderbird-mail": command_entry("mcp-thunderbird-mail") }
        })),
        "outside the withholding state the task's server still reaches its agent"
    );
    let pending = case.log("1", "pending");
    assert_eq!(header_value(&pending, "mcp_servers_withheld"), None, "{pending}");

    assert_eq!(
        case.handed("1", "review"),
        Some(serde_json::json!({ "mcpServers": { "postgres": command_entry("mcp-postgres") } })),
        "the withholding state's agent gets its own server and not the task's"
    );
    let review = case.log("1", "review");
    assert_eq!(header_value(&review, "mcp_servers").as_deref(), Some("postgres"), "{review}");
    assert_eq!(
        header_value(&review, "mcp_servers_withheld").as_deref(),
        Some("thunderbird-mail"),
        "the log must record the server the state held back:\n{review}"
    );
}

/// A misspelt id fails validation the way an unknown id on a state does,
/// naming the task, the field and the id.
// §FS-rhei-task-tooling.6
#[test]
#[ignore = "red until #475 lets a task name its own MCP servers and skills"]
fn an_unknown_task_server_is_refused_by_validation() {
    let plan = MAIL_TASK_PLAN.replace("thunderbird-mail", "thunderbird-mial");
    let case = Case::new("task-tooling-unknown-id", INBOX_MACHINE, &plan);

    let validate = case.validate();

    assert!(!validate.status.success(), "an unknown task server must fail validation");
    assert!(
        validate.stderr.lines().any(|line| line.contains("plan.1")
            && line.contains("**MCP servers:**")
            && line.contains("unknown mcp server 'thunderbird-mial'")),
        "no line names Task plan.1, its **MCP servers:** field and the unknown id; stderr:\n{}",
        validate.stderr
    );
}

/// The machine routes a skill that cannot attach to a person, through a
/// `skill_unavailable` trigger written as `trigger`.
fn skill_machine(trigger: &str) -> String {
    format!(
        "name: release
version: 1.0.0
states:
  work:
    initial: true
    agent: claude-code
  blocked:
    gating: true
  completed:
    final: true
transitions:
  - from: work
    to: completed
  - from: work
    to: blocked
    skill_unavailable: {trigger}
  - from: blocked
    to: work
"
    )
}

const SKILL_TASK_PLAN: &str = "# Rhei: Release

## Tasks

### Task 1: Draft the release notes
**State:** work
**Skills:** release-notes
";

/// A required skill the task names, whose directory is missing, blocks the
/// spawn and is routed like a state's: `true` matches it, and a list naming a
/// different id does not.
// §FS-rhei-task-tooling.5
#[test]
#[ignore = "red until #475 lets a task name its own MCP servers and skills"]
fn a_required_task_skill_that_cannot_attach_fires_skill_unavailable() {
    let any = Case::new("task-tooling-skill-any", &skill_machine("true"), SKILL_TASK_PLAN);
    let run = any.run();
    assert_success(&run);
    assert!(
        run.stdout.contains(
            "Tooling-unavailable transition: Task plan.1 'work' -> 'blocked' \
             (skill unavailable: release-notes)"
        ),
        "`skill_unavailable: true` must fire for the task's skill; stdout:\n{}",
        run.stdout
    );
    assert!(any.plan_text().contains("**State:** blocked"), "{}", any.plan_text());
    assert_no_agent_log(&any, "1", "work");

    let other =
        Case::new("task-tooling-skill-other", &skill_machine("[changelog-style]"), SKILL_TASK_PLAN);
    let run = other.run();
    let said = format!("{}{}", run.stdout, run.stderr);
    assert!(
        said.contains("required tooling unavailable for task plan.1 in state 'work'")
            && said.contains("release-notes"),
        "a list naming a different id must not fire, and the failure is logged; got:\n{said}"
    );
    assert!(other.plan_text().contains("**State:** work"), "{}", other.plan_text());
    assert_no_agent_log(&other, "1", "work");
}
