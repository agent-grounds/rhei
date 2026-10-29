//! A member of a Panta project reads its project's settings on every read, and
//! a settings file inside the member is read by nothing. §FS-rhei-agents.1.1.8
//!
//! The observable is a malformed settings file: every path below loads merged
//! settings before it does anything else, so a command that reads the file
//! fails naming it, and one that ignores it goes on. The member keeps its own
//! execution root, and a plan the project does not discover keeps its own
//! settings. §FS-rhei-panta.6.3

use std::fs;
use std::path::{Path, PathBuf};

use super::agent_grounds_support::run_in;
use super::*;

const MALFORMED: &str = "{ not json\n";
const MEMBER_SETTINGS: &str = "proj/m1/.agent-grounds/rhei/settings.json";
const ROOT_SETTINGS: &str = "proj/.agent-grounds/rhei/settings.json";

fn write_settings(base: &Path, contents: &str) {
    let home = base.join(".agent-grounds/rhei");
    fs::create_dir_all(&home).expect("create settings home");
    write_fixture_file(&home, "settings.json", contents);
}

/// A project `proj` with one Directory Workspace member `m1` holding one
/// pending ticket, and the two settings files the case gives.
struct MemberProject {
    _dir: TestDir,
    root: PathBuf,
    project: PathBuf,
    member: PathBuf,
    home: PathBuf,
}

impl MemberProject {
    fn new(prefix: &str, root_settings: Option<&str>, member_settings: Option<&str>) -> Self {
        let dir = unique_temp_dir(prefix);
        let root = dir.to_path_buf();
        let project = root.join("proj");
        let member = project.join("m1");
        fs::create_dir_all(member.join("tasks")).expect("create member tasks");
        write_fixture_file(&project, "index.panta.md", "# Panta: p\n");
        write_fixture_file(&member, "index.rhei.md", "# Rhei: Member\n**Id:** m1\n");
        write_fixture_file(
            &member.join("tasks"),
            "01-one.md",
            "### Task 1: One\n**State:** pending\n\nDo it.\n",
        );
        for (base, contents) in [(&project, root_settings), (&member, member_settings)] {
            if let Some(contents) = contents {
                write_settings(base, contents);
            }
        }
        let home = root.join("home");
        Self { _dir: dir, root, project, member, home }
    }

    fn run(&self, args: &[&str]) -> CliRun {
        run_in(args, &self.root, &self.home)
    }

    fn project_arg(&self) -> String {
        self.project.display().to_string()
    }

    fn member_arg(&self) -> String {
        self.member.display().to_string()
    }

    fn ticket_file(&self) -> String {
        fs::read_to_string(self.member.join("tasks/01-one.md")).expect("read member ticket")
    }

    /// Whether `output` names `relative`, a path under the test directory.
    /// Separators differ per platform, so compare with `/`.
    fn names(&self, output: &str, relative: &str) -> bool {
        let leaf = self.root.file_name().expect("test directory has a name").to_string_lossy();
        output.replace('\\', "/").contains(&format!("{leaf}/{relative}"))
    }

    fn assert_member_file_unread(&self, result: &CliRun, what: &str) {
        assert!(
            !self.names(&result.stderr, MEMBER_SETTINGS) && !self.names(&result.stdout, MEMBER_SETTINGS),
            "{what} read the member's own settings file, which nothing reads:\nstdout:\n{}\nstderr:\n{}",
            result.stdout,
            result.stderr
        );
    }

    fn assert_completed(&self) {
        let ticket = self.ticket_file();
        assert!(ticket.contains("**State:** completed"), "m1.1 should be completed:\n{ticket}");
    }
}

/// The reported case: a transition on a member's ticket, the project named,
/// must not parse the member's malformed settings file. §FS-rhei-agents.1.1.8
#[test]
fn a_member_transition_ignores_the_members_own_settings_file() {
    let fixture = MemberProject::new("member-settings-transition", Some("{}\n"), Some(MALFORMED));
    let project = fixture.project_arg();

    // What validation says about the file is the contract the transition breaks.
    let validate = fixture.run(&["validate", &project]);
    assert_success(&validate);
    let report = format!("{}{}", validate.stdout, validate.stderr);
    assert!(
        fixture.names(&report, MEMBER_SETTINGS) && report.contains("is ignored"),
        "validate should warn that the member's settings file is ignored:\n{report}"
    );

    let result = fixture.run(&[
        "transition",
        "--task",
        "m1.1",
        "--from",
        "pending",
        "--to",
        "completed",
        "--result",
        "done",
        &project,
    ]);
    fixture.assert_member_file_unread(&result, "the transition");
    assert_success(&result);
    fixture.assert_completed();
}

/// The same transition with the member's own path as the target, which loads
/// through the project and narrows to `m1`. §FS-rhei-agents.1.1.8 §FS-rhei-panta.6
#[test]
fn a_member_transition_named_by_the_member_path_ignores_its_settings_file() {
    let fixture = MemberProject::new("member-settings-member-path", Some("{}\n"), Some(MALFORMED));
    let member = fixture.member_arg();

    let result = fixture.run(&[
        "transition",
        "--task",
        "1",
        "--from",
        "pending",
        "--to",
        "completed",
        "--result",
        "done",
        &member,
    ]);
    fixture.assert_member_file_unread(&result, "the transition");
    assert_success(&result);
    fixture.assert_completed();
}

/// The positive side: the transition reads the project root's file. A malformed
/// root file fails it by name, and a valid member file does not stand in.
/// §FS-rhei-agents.1.1.8
#[test]
fn a_member_transition_reads_the_project_root_settings_file() {
    let fixture = MemberProject::new("member-settings-root-read", Some(MALFORMED), Some("{}\n"));
    let project = fixture.project_arg();

    let result = fixture.run(&[
        "transition",
        "--task",
        "m1.1",
        "--from",
        "pending",
        "--to",
        "completed",
        "--result",
        "done",
        &project,
    ]);
    assert!(
        !result.status.success(),
        "the project root's settings file is malformed, so the transition must fail on it; it \
         succeeded, so it read the member's file instead:\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert!(
        fixture.names(&result.stderr, ROOT_SETTINGS)
            && result.stderr.contains("failed to parse settings"),
        "the failure should name the project root's settings file:\n{}",
        result.stderr
    );
    let ticket = fixture.ticket_file();
    assert!(ticket.contains("**State:** pending"), "a refused transition moves nothing:\n{ticket}");
}

/// A project run finishes a member's ticket after its agent exits. The run
/// reads the root's settings at start, which is where the agent is declared;
/// selecting and applying the terminal edge that follows must read the same.
/// §FS-rhei-agents.1.1.8 §FS-rhei-run.3
#[test]
fn a_project_run_finishes_a_member_agent_without_reading_its_settings_file() {
    let fixture = MemberProject::new("member-settings-agent-run", None, Some(MALFORMED));
    let agent =
        write_python_agent(&fixture.root, "finish.py", "result('## Result\\n\\nDone.\\n')\n");
    let command: serde_json::Value =
        serde_json::from_str(&fixture_command(&agent)).expect("fixture command");
    let profile = serde_json::json!({"command": command, "stdin_prompt": true, "timeout": "20s"});
    write_settings(&fixture.project, &serde_json::json!({"agents": {"fake": profile}}).to_string());
    write_fixture_file(
        &fixture.member,
        "states.yaml",
        r#"name: member-agent
version: 1
states:
  pending:
    initial: true
    description: Do the work
    agent: fake
    agent_timeout: 30s
  completed:
    final: true
    description: Done
transitions:
  - from: pending
    to: completed
"#,
    );
    let project = fixture.project_arg();

    let result = fixture.run(&["run", &project, "--no-tui"]);
    fixture.assert_member_file_unread(&result, "the run");
    assert_success(&result);
    fixture.assert_completed();
}

/// A project run moves a member's ticket on after its program exits: the
/// program's exit-code edge is applied as a transition, which must read the
/// settings the run read at start. §FS-rhei-agents.1.1.8 §FS-rhei-run.3
#[test]
fn a_project_run_moves_a_member_program_on_without_reading_its_settings_file() {
    let fixture = MemberProject::new("member-settings-run", Some("{}\n"), Some(MALFORMED));
    let program = write_python_agent(&fixture.root, "finish.py", "result('done\\n')\n");
    write_fixture_file(
        &fixture.member,
        "states.yaml",
        &format!(
            r#"name: member-program
version: 1
states:
  pending:
    initial: true
    description: Run the program
    program:
      command: {}
    program_timeout: 20s
  completed:
    final: true
    description: Done
transitions:
  - from: pending
    to: completed
    exit_code: 0
"#,
            fixture_command(&program)
        ),
    );
    let project = fixture.project_arg();

    let result = fixture.run(&["run", &project, "--no-tui"]);
    fixture.assert_member_file_unread(&result, "the run");
    assert_success(&result);
    fixture.assert_completed();
}

/// `--force` prepares its correction against the same settings. On a declared
/// edge it is refused for that reason, not for a file nothing reads.
/// §FS-rhei-agents.1.1.8 §FS-rhei-transition-cmd.6
#[test]
fn a_forced_member_transition_ignores_the_members_own_settings_file() {
    let fixture = MemberProject::new("member-settings-force", Some("{}\n"), Some(MALFORMED));
    let project = fixture.project_arg();

    let result = fixture.run(&[
        "transition",
        "--force",
        "--reason",
        "recovery",
        "--task",
        "m1.1",
        "--from",
        "pending",
        "--to",
        "completed",
        "--result",
        "done",
        &project,
    ]);
    fixture.assert_member_file_unread(&result, "the forced transition");
    assert!(
        !result.status.success() && result.stderr.contains("already declares this edge"),
        "a forced move along a declared edge is refused as one:\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
}

/// The snapshot commands pointed at the member's own path read the project's
/// settings, while the member keeps its own cache. §FS-rhei-agents.1.1.8
#[test]
fn snapshot_commands_pointed_at_a_member_ignore_its_settings_file() {
    let fixture = MemberProject::new("member-settings-snapshot", Some("{}\n"), Some(MALFORMED));
    let member = fixture.member_arg();

    for args in [
        vec!["snapshot", "list", "--plan", &member],
        vec!["snapshot", "gc", "--plan", &member, "--orphaned", "--dry-run"],
    ] {
        let result = fixture.run(&args);
        fixture.assert_member_file_unread(&result, &format!("`rhei {}`", args[..2].join(" ")));
        assert_success(&result);
    }
}

/// Completing `--agent` for a command line naming the member offers the
/// project root's agents, not the member file's. §FS-rhei-agents.1.1.8
#[test]
fn completion_for_a_member_path_offers_the_project_root_agents() {
    let agent = |id: &str| {
        format!(
            r#"{{ "agents": {{ "{id}": {{ "command": ["{id}"], "modes": {{ "yolo": [] }} }} }} }}"#
        )
    };
    let fixture = MemberProject::new(
        "member-settings-completion",
        Some(agent("root-agent").as_str()),
        Some(agent("member-agent").as_str()),
    );

    for target in [fixture.member_arg(), fixture.member.join("index.rhei.md").display().to_string()]
    {
        let output = rhei_command(&fixture.home)
            .args(["--", "rhei", "run", &target, "--agent", ""])
            .current_dir(&fixture.root)
            .env("COMPLETE", "fish")
            .output()
            .expect("rhei completion should run");
        let completion = CliRun::from(&output);
        assert!(
            completion.stdout.contains("root-agent") && !completion.stdout.contains("member-agent"),
            "completion for {target} should offer the project's agent and not the member file's:\n{}",
            completion.stdout
        );
    }
}

/// Only the settings root widens: the member's result stays under its own
/// execution root. Passes today and must keep passing. §FS-rhei-panta.6.3
#[test]
fn a_member_transition_keeps_its_result_under_the_member() {
    let fixture = MemberProject::new("member-settings-result-root", Some("{}\n"), None);
    let project = fixture.project_arg();

    let result = fixture.run(&[
        "transition",
        "--task",
        "m1.1",
        "--from",
        "pending",
        "--to",
        "completed",
        "--result",
        "done",
        &project,
    ]);
    assert_success(&result);
    assert!(
        fixture.member.join("runtime/results/m1.1.md").is_file(),
        "the result belongs to the member"
    );
    assert!(
        !fixture.project.join("runtime/results/m1.1.md").exists(),
        "the result must not move to the project root"
    );
}

/// A workspace nested in a project's grouping folder is not a member, so it
/// is its own project and reads its own settings file. Passes today and must
/// keep passing. §FS-rhei-agents.1.1.8
#[test]
fn a_plan_nested_in_a_project_it_does_not_discover_reads_its_own_settings() {
    let fixture = MemberProject::new("member-settings-undiscovered", Some("{}\n"), None);
    let nested = fixture.project.join("scratch/w");
    fs::create_dir_all(nested.join("tasks")).expect("create nested workspace");
    write_fixture_file(&nested, "index.rhei.md", "# Rhei: Nested\n");
    write_fixture_file(&nested.join("tasks"), "01-one.md", "### Task 1: One\n**State:** pending\n");
    write_settings(&nested, MALFORMED);
    let target = nested.display().to_string();

    let result = fixture.run(&[
        "transition",
        "--task",
        "1",
        "--from",
        "pending",
        "--to",
        "completed",
        "--result",
        "done",
        &target,
    ]);
    assert!(
        !result.status.success()
            && fixture.names(&result.stderr, "proj/scratch/w/.agent-grounds/rhei/settings.json"),
        "a plan the project does not discover reads its own settings, so its malformed file fails it:\n\
         stdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
}
