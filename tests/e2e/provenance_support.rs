//! The repositories, templates and runs the provenance scenarios read.
//!
//! Every git fixture is `git init`-ed inside the test's own directory, and
//! every `git` and `rhei` spawned here runs with `GIT_CEILING_DIRECTORIES` set
//! to that directory, so discovery never climbs into the checkout the suite
//! runs from. §REQ-test-isolation.3 §REQ-test-isolation.4

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

use super::{
    fixture_command, git_command, rhei_command, unique_temp_dir, write_python_agent, CliRun,
    TestDir,
};

/// The template every scenario lays: one agent state, one program state, and a
/// final, so a run spawns an agent, then a program, then an agent again.
pub const TEMPLATE_NAME: &str = "tiny";
pub const TEMPLATE_VERSION: &str = "1.0.0";
pub const TEMPLATE_REMOTE: &str = "https://github.com/example-org/templates.git";
pub const WORKSPACE_REMOTE: &str = "https://github.com/example-org/example-repo.git";

/// A stand-in for Claude Code that reports usage and writes its result, so a
/// run writes a real invocation record. §FS-rhei-cost-accounting.4
const AGENT: &str = r#"capture = env('RHEI_ACCOUNTING_USAGE_PATH')
if capture:
    append(
        capture,
        '{"schema": "rhei.accounting.usage.v1", "usage": '
        '{"total_tokens": 4321, "input_tokens": 4000, '
        '"cached_input_tokens": 3000, "output_tokens": 321}}\n',
    )
result('## Result\n\nMock agent finished.\n')
"#;

/// The program step: what a `describe` program does, taking the checkpoint and
/// the live summary one after the other, with nothing run between them. It
/// always exits 0 so the run goes on to the later agent step whatever the two
/// commands said; the test reads what they wrote. §FS-rhei-summary.1
const SNAPSHOT_PROGRAM: &str = r#"import subprocess
rhei, workspace, out = sys.argv[1], sys.argv[2], pathlib.Path(sys.argv[3])
out.mkdir(parents=True, exist_ok=True)
for name, args in (
    ('checkpoint.json', ['--print-checkpoint']),
    ('live.md', ['--details']),
):
    done = subprocess.run([rhei, 'summary', workspace] + args, capture_output=True)
    (out / name).write_bytes(done.stdout)
    (out / (name + '.stderr')).write_bytes(done.stderr)
result('## Result\n\nSnapshot taken.\n')
"#;

/// A git command for a fixture under `ceiling`, with an identity of its own.
pub fn git(ceiling: &Path, dir: &Path, args: &[&str]) -> String {
    let mut cmd = git_command();
    cmd.current_dir(dir)
        .env("GIT_CEILING_DIRECTORIES", ceiling)
        .args(["-c", "user.email=provenance@example.org", "-c", "user.name=Provenance"])
        .args(["-c", "commit.gpgsign=false", "-c", "init.defaultBranch=main"])
        .args(args);
    let out = cmd.output().expect("git should start");
    assert!(
        out.status.success(),
        "git {args:?} in {} failed: {}",
        dir.display(),
        super::stderr(&out)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// `git init` + commit everything + an `origin` remote; returns the commit.
pub fn commit_repository(ceiling: &Path, dir: &Path, origin: Option<&str>) -> String {
    git(ceiling, dir, &["init", "-q"]);
    if let Some(origin) = origin {
        git(ceiling, dir, &["remote", "add", "origin", origin]);
    }
    git(ceiling, dir, &["add", "-A"]);
    git(ceiling, dir, &["commit", "-q", "-m", "fixture"]);
    git(ceiling, dir, &["rev-parse", "HEAD"])
}

/// Every `rhei` a scenario spawns: from `cwd`, discovery capped at `ceiling`.
pub fn rhei_in(ceiling: &Path, cwd: &Path) -> Command {
    let mut cmd = rhei_command(ceiling.join(".home"));
    cmd.current_dir(cwd).env("GIT_CEILING_DIRECTORIES", ceiling);
    cmd
}

pub fn run(mut cmd: Command, args: &[&str]) -> CliRun {
    let out = cmd.args(args).output().expect("rhei should start");
    CliRun::from(&out)
}

/// Write the `tiny` template into `template_dir`. `program` is the encoded
/// command of the program state.
pub fn write_template(template_dir: &Path, agent: &Path, program: &str) {
    fs::create_dir_all(template_dir.join("tasks")).expect("template tasks dir");
    fs::write(
        template_dir.join("template.yaml"),
        format!(
            "name: {TEMPLATE_NAME}\nversion: {TEMPLATE_VERSION}\n\
             description: Provenance probe.\n"
        ),
    )
    .expect("template manifest");
    fs::write(
        template_dir.join("states.yaml"),
        format!(
            r#"name: {TEMPLATE_NAME}
version: 1
models: [claude-sonnet-4-6]
states:
  work:
    initial: true
    description: Agent work
    target: claude-code[yolo]:anthropic:claude-sonnet-4-6
    agent_timeout: 60s
  snap:
    description: Take the checkpoint and the live summary
    program:
      command: {program}
    program_timeout: 2m
  completed:
    final: true
    description: Done
transitions:
  - from: work
    to: completed
  - from: snap
    to: completed
    exit_code: 0
"#
        ),
    )
    .expect("template machine");
    fs::write(template_dir.join("index.rhei.md"), "# Rhei: Tiny\n\n## Overview\n\nProbe.\n")
        .expect("template index");
    fs::write(
        template_dir.join("tasks/01-work.md"),
        "### Task t1: First agent step\n**State:** work\n\nWork.\n\n\
         ### Task t2: Snapshot\n**State:** snap\n**Prior:** t1\n\nSnapshot.\n\n\
         ### Task t3: Later agent step\n**State:** work\n**Prior:** t2\n\nWork again.\n",
    )
    .expect("template tasks");
    // Pretty JSON never puts two braces side by side, so the template renderer
    // reads it as text. §FS-rhei-templates.6.1.2
    let command: serde_json::Value =
        serde_json::from_str(&fixture_command(agent)).expect("agent command is JSON");
    let settings = serde_json::json!({
        "agents": { "claude-code": {
            "command": command, "timeout": "60s", "stdin_prompt": true,
            "intervene_stdin": false, "modes": { "yolo": [] }
        }},
        "models": { "claude-sonnet-4-6": {
            "provider": "anthropic", "model": "claude-sonnet-4-6", "default_agent": "claude-code"
        }}
    });
    fs::write(
        template_dir.join("settings.json"),
        serde_json::to_string_pretty(&settings).expect("settings serialize"),
    )
    .expect("template settings");
}

/// A test directory holding a committed template repository at `tpl/` and the
/// two fixture scripts, ready to lay.
pub struct Lay {
    pub dir: TestDir,
    pub template: PathBuf,
    pub template_commit: String,
    pub out: PathBuf,
}

impl Lay {
    /// The template committed in its own repository, `origin` set when given.
    pub fn new(prefix: &str, origin: Option<&str>) -> Self {
        let dir = unique_temp_dir(prefix);
        let agent = write_python_agent(&dir, "mock-claude-code.py", AGENT);
        let program = write_python_agent(&dir, "snapshot.py", SNAPSHOT_PROGRAM);
        let out = dir.join("out");
        let workspace = dir.join("ws");
        let rhei = super::rhei_binary();
        let program_command = super::python_fixture::fixture_command_with_args(
            &program,
            &[
                &rhei.display().to_string(),
                &workspace.display().to_string(),
                &out.display().to_string(),
            ],
        );
        let repo = dir.join("tpl");
        let template = repo.join(TEMPLATE_NAME);
        write_template(&template, &agent, &program_command);
        let template_commit = commit_repository(&dir, &repo, origin);
        Lay { dir, template, template_commit, out }
    }

    pub fn workspace(&self) -> PathBuf {
        self.dir.join("ws")
    }

    /// `rhei instantiate <template> --output ws [extra]`, from the test dir.
    pub fn instantiate(&self, template: &Path, extra: &[&str]) -> CliRun {
        let mut args = vec!["instantiate".to_string(), template.display().to_string()];
        args.extend(["--output".to_string(), "ws".to_string()]);
        args.extend(extra.iter().map(|arg| (*arg).to_string()));
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        run(rhei_in(&self.dir, &self.dir), &args)
    }

    /// The laid workspace's `states.yaml`.
    pub fn laid_machine(&self) -> String {
        fs::read_to_string(self.workspace().join("states.yaml")).expect("laid machine")
    }
}

/// The `laid` fence line of a machine, split on whitespace. §FS-rhei-library.7.1
pub fn laid_fence(machine: &str) -> Vec<String> {
    let line = machine
        .lines()
        .find(|line| line.starts_with("# --- laid "))
        .unwrap_or_else(|| panic!("no `# --- laid` fence in the laid machine:\n{machine}"));
    line.split_whitespace().map(str::to_string).collect()
}

/// Assert a fence's fixed shape and return its `git:` field, if any.
pub fn assert_fence_shape(fence: &[String], state: &str) -> Option<String> {
    let joined = fence.join(" ");
    assert_eq!(&fence[..5], ["#", "---", "laid", TEMPLATE_NAME, TEMPLATE_VERSION], "{joined}");
    assert_eq!(fence.last().map(String::as_str), Some("---"), "{joined}");
    let hex64 = |field: &str, prefix: &str| {
        field
            .strip_prefix(prefix)
            .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
    };
    assert!(hex64(&fence[5], "src:sha256:"), "src digest: {joined}");
    assert!(hex64(&fence[fence.len() - 2], "inputs:sha256:"), "inputs digest: {joined}");
    let middle = &fence[6..fence.len() - 2];
    assert_eq!(middle.last().map(String::as_str), Some(state), "state word: {joined}");
    middle.iter().find(|field| field.starts_with("git:")).cloned()
}

/// The `Source:` line of an instantiation summary. §FS-rhei-templates.6.1.3
pub fn source_line(stdout: &str) -> String {
    stdout
        .lines()
        .find(|line| line.starts_with("Source: "))
        .unwrap_or_else(|| panic!("no `Source:` line in the instantiation summary:\n{stdout}"))
        .to_string()
}

/// What `rhei version`'s build line says, as the provenance line prints it:
/// `<sha7>`, `<sha7> (dirty)`, or `unknown`. §FS-rhei-version.2
pub fn build_as_printed(ceiling: &Path) -> String {
    let version = run(rhei_in(ceiling, ceiling), &["version"]);
    let line = version
        .stdout
        .lines()
        .find_map(|line| line.strip_prefix("rhei-build "))
        .unwrap_or_else(|| panic!("`rhei version` prints no rhei-build line:\n{}", version.stdout))
        .to_string();
    match line.strip_suffix("-dirty") {
        Some(commit) => format!("{} (dirty)", &commit[..7]),
        None if line == "unknown" => line,
        None => line[..7].to_string(),
    }
}

/// The first 12 hex digits of the sha256 of `bytes`. §FS-rhei-summary.6.2
pub fn checkpoint_id(bytes: &[u8]) -> String {
    Sha256::digest(bytes)[..6].iter().fold(String::new(), |mut hex, byte| {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
        hex
    })
}
