//! A help names the target of the command that printed it, so its line runs as
//! printed: §FS-rhei-errors.1.2.1, which makes §FS-rhei-errors.1.2 and
//! §FS-rhei-errors.2 specific, on the failing exits §FS-rhei-errors.6 covers.
//!
//! The routes are the ones `agent-grounds/rhei#555` reached with its reproducer.
//! Each reads the command out of what the binary printed and, where running it
//! is harmless, pastes it. These read raw stderr: where a line broke is the claim.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};

use super::*;

pub(super) const HELD_PLAN: &str = "# Rhei: Project\n\n## Tasks\n\n\
                         ### Task 1: A ticket someone else holds\n\
                         **State:** open\n\
                         **Assignee:** someone-else\n";

const UNCLAIMED_PLAN: &str = "# Rhei: Project\n\n## Tasks\n\n\
                              ### Task 1: A ticket nobody holds\n\
                              **State:** open\n";

const INVALID_PLAN: &str = "# Rhei: Project\n\n## Tasks\n\n\
                            ### Task 1: A ticket after one that does not exist\n\
                            **State:** open\n\
                            **Prior:** 99\n";

/// Never spawned: its one ticket is held, so the run has nothing it may pick up.
pub(super) const HELD_MACHINE: &str = r#"name: held
version: 1
states:
  open:
    initial: true
    description: Ready to work, were it not held.
    program:
      command: ["true"]
    program_timeout: 1m
  done:
    description: Finished.
    final: true
  failed:
    description: The program failed.
    final: true
transitions:
  - { from: open, to: done, exit_code: 0, description: It worked. }
  - { from: open, to: failed, exit_code: nonzero, description: It failed. }
"#;

/// A directory name that takes the space after it past the wrap column.
pub(super) const WIDER_THAN_THE_RENDER: &str =
    "a-directory-whose-name-alone-runs-past-the-column-a-rendered-help-wraps-at-eighty";

/// One directory for the project and one for the account, so a project written
/// beneath the root is the only thing discovery finds there. §REQ-test-isolation.2
pub(super) struct Case {
    pub(super) root: TestDir,
    pub(super) home: PathBuf,
}

impl Case {
    pub(super) fn new(prefix: &str) -> Self {
        let root = unique_temp_dir(prefix);
        let home = root.join(".home");
        fs::create_dir_all(&home).expect("create the account directory");
        Self { root, home }
    }

    /// A held ticket and its machine in `relative`, returning that directory.
    pub(super) fn held(&self, relative: &str) -> PathBuf {
        self.project(relative, HELD_PLAN)
    }

    fn project(&self, relative: &str, plan: &str) -> PathBuf {
        let dir = self.root.join(relative);
        fs::create_dir_all(&dir).expect("create the project directory");
        write_fixture_file(&dir, "plan.rhei.md", plan);
        write_fixture_file(&dir, "states.yaml", HELD_MACHINE);
        dir
    }

    pub(super) fn rhei(&self, cwd: &Path, args: &[&str]) -> Output {
        rhei_command(&self.home)
            .current_dir(cwd)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .expect("rhei should run")
    }
}

/// The physical line holding `start` in the last help block, from `start` to the
/// end of that line. A wrapped command comes back cut short, which is the point.
fn offered_command(rendered: &str, start: &str) -> Option<String> {
    let (_, help) = rendered.rsplit_once("help:")?;
    help.lines().find_map(|line| line.find(start).map(|at| line[at..].trim_end().to_owned()))
}

/// That the help ends on exactly this command, whole on one physical line.
#[track_caller]
pub(super) fn assert_offers(rendered: &str, expected: &str) {
    let start = expected.split(' ').take(2).collect::<Vec<_>>().join(" ");
    assert_eq!(
        offered_command(rendered, &start).as_deref(),
        Some(expected),
        "the help should end on this command, on one line; got:\n{rendered}"
    );
}

/// `command` pasted into `sh`, from `cwd`, finding the binary under test as `rhei`.
#[cfg(unix)]
pub(super) fn pasted(command: &str, cwd: &Path, home: &Path) -> CliRun {
    let binary = rhei_binary();
    let bin_dir = binary.parent().expect("the binary's directory").to_path_buf();
    let mut dirs = vec![bin_dir];
    dirs.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
    let out = rhei_process_at("sh")
        .arg("-c")
        .arg(command)
        .current_dir(cwd)
        .env("PATH", std::env::join_paths(dirs).expect("join PATH"))
        .env("HOME", home)
        .env("XDG_STATE_HOME", home.join("state"))
        .stdin(Stdio::null())
        .output()
        .expect("sh should run");
    CliRun::from(&out)
}

/// Pasted, `command` lists `listed` and none of `absent`. `cmd` quotes another way,
/// so the paste is a Unix claim; the printed spelling is `shell_quote`'s on each.
#[track_caller]
#[cfg(unix)]
pub(super) fn assert_lists(command: &str, cwd: &Path, home: &Path, listed: &str, absent: &[&str]) {
    let run = pasted(command, cwd, home);
    assert!(run.status.success(), "`{command}` should run as printed:\n{}", run.stderr);
    assert!(run.stdout.contains(listed), "`{command}` should list {listed}:\n{}", run.stdout);
    for other in absent {
        assert!(!run.stdout.contains(other), "`{command}` is wider than asked:\n{}", run.stdout);
    }
}

#[cfg(windows)]
pub(super) fn assert_lists(_: &str, _: &Path, _: &Path, _: &str, _: &[&str]) {}

/// What an omitted target discovers: whatever the OS reports as the current
/// directory, which `getcwd` resolves on Unix and Windows leaves as it was.
pub(super) fn discovered(path: &Path) -> PathBuf {
    if cfg!(unix) {
        rhei_core::platform::canonical_path(path).unwrap_or_else(|_| path.to_path_buf())
    } else {
        path.to_path_buf()
    }
}

/// `rhei list` of what the command was given, in the platform shell's quoting.
pub(super) fn listing(target: &str) -> String {
    format!("rhei list {}", shell_quote(target))
}

/// `rhei list` of a target the command found, or made absolute, for itself.
fn listing_found(path: &Path) -> String {
    listing(&discovered(path).display().to_string())
}

/// The ticket's own command: a target in the working directory. §FS-rhei-errors.1.2.1
#[test]
fn help_target_a_halted_run_names_the_target_it_was_given() {
    let case = Case::new("help-target-halt");
    let project = case.held("project");

    let out = case.rhei(&project, &["run", "plan.rhei.md", "--no-tui", "--no-callbacks"]);

    assert_eq!(out.status.code(), Some(1), "the run halts:\n{}", raw_stderr(&out));
    let expected = listing("plan.rhei.md");
    assert_offers(&raw_stderr(&out), &expected);
    assert_lists(&expected, &project, &case.home, "plan.1", &[]);
}

/// Printed as typed and quoted: a file name fails from the parent directory, an
/// unquoted path fails when pasted. §FS-rhei-errors.1.2.1 §FS-rhei-errors.2
#[test]
fn help_target_with_a_space_prints_quoted_and_pastes() {
    let case = Case::new("help-target-space");
    case.held("sub dir");
    let typed = "sub dir/plan.rhei.md";

    let out = case.rhei(&case.root, &["run", typed, "--no-tui", "--no-callbacks"]);

    let expected = listing(typed);
    assert_eq!(out.status.code(), Some(1), "the run halts:\n{}", raw_stderr(&out));
    assert_offers(&raw_stderr(&out), &expected);
    assert_lists(&expected, &case.root, &case.home, "plan.1", &[]);
}

/// A space past the wrap column would break inside the quotes and paste as a
/// different path, which the short target above cannot show. §FS-rhei-errors.1.2.1
#[test]
fn help_target_past_the_wrap_column_stays_on_one_line() {
    let case = Case::new("help-target-wrap");
    let typed = format!("{WIDER_THAN_THE_RENDER}/sub dir/plan.rhei.md");
    case.held(&format!("{WIDER_THAN_THE_RENDER}/sub dir"));

    let out = case.rhei(&case.root, &["run", &typed, "--no-tui", "--no-callbacks"]);

    let expected = listing(&typed);
    assert_offers(&raw_stderr(&out), &expected);
    assert_lists(&expected, &case.root, &case.home, "plan.1", &[]);
}

/// Without a space there is nothing to break inside, but `rhei list` and its
/// target on two lines still paste as two commands. §FS-rhei-errors.1.2.1
#[test]
fn help_target_past_the_wrap_column_without_a_space_stays_on_one_line() {
    let case = Case::new("help-target-long");
    let typed = format!("{WIDER_THAN_THE_RENDER}/plan.rhei.md");
    case.held(WIDER_THAN_THE_RENDER);

    let out = case.rhei(&case.root, &["run", &typed, "--no-tui", "--no-callbacks"]);

    let expected = listing(&typed);
    assert_offers(&raw_stderr(&out), &expected);
    assert_lists(&expected, &case.root, &case.home, "plan.1", &[]);
}

/// The dry run of the same halt, `nothing_claimable_help`'s second route.
/// §FS-rhei-errors.1.2.1 §FS-rhei-run.4
#[test]
fn help_target_a_dry_run_halt_names_its_target() {
    let case = Case::new("help-target-dry-run");
    let project = case.held("project");

    let out =
        case.rhei(&project, &["run", "plan.rhei.md", "--no-tui", "--no-callbacks", "--dry-run"]);

    assert_eq!(out.status.code(), Some(1), "the dry run halts:\n{}", raw_stderr(&out));
    let expected = listing("plan.rhei.md");
    assert_offers(&raw_stderr(&out), &expected);
    assert_lists(&expected, &project, &case.home, "plan.1", &[]);
}

/// The built-in manual machine refuses `rhei run`, the third route.
/// §FS-rhei-errors.1.2.1 §FS-rhei-run.4
#[test]
fn help_target_the_manual_only_refusal_names_its_target() {
    let case = Case::new("help-target-manual");
    let project = case.root.join("project");
    fs::create_dir_all(&project).expect("create the project directory");
    write_fixture_file(
        &project,
        "plan.rhei.md",
        "# Rhei: Project\n\n## Tasks\n\n### Task 1: Under the manual machine\n**State:** pending\n",
    );

    let out = case.rhei(&project, &["run", "plan.rhei.md", "--no-tui", "--no-callbacks"]);

    assert_eq!(out.status.code(), Some(1), "the run refuses:\n{}", raw_stderr(&out));
    let expected = listing("plan.rhei.md");
    assert_offers(&raw_stderr(&out), &expected);
    assert_lists(&expected, &project, &case.home, "plan.1", &[]);
}

/// `run_report_help`, and the message above it, which prints the target unquoted:
/// the ticket shows the real target and the placeholder in one diagnostic.
/// Pasted, it reaches the same refusal, which is what a re-run may do until the
/// cause is fixed. §FS-rhei-errors.1.2.1 §FS-rhei-errors.1.2
#[test]
fn help_target_a_run_with_no_agent_names_it_in_message_and_help() {
    let case = Case::new("help-target-no-agent");
    let project = case.root.join("sub dir");
    fs::create_dir_all(project.join(".agent-grounds/rhei")).expect("create the settings directory");
    write_fixture_file(
        &project,
        "plan.rhei.md",
        "# Rhei: Project\n\n## Tasks\n\n### Task 1: For an agent nobody configured\n**State:** work\n",
    );
    write_fixture_file(
        &project,
        "states.yaml",
        "name: no-agent\nversion: 1\nmodels:\n  - codex\nstates:\n  work:\n    initial: true\n    \
         description: An agent's work.\n    model: codex\n  done:\n    description: Finished.\n    \
         final: true\ntransitions:\n  - { from: work, to: done, description: It worked. }\n",
    );
    write_fixture_file(
        &project,
        ".agent-grounds/rhei/settings.json",
        r#"{ "models": { "codex": { "provider": "openai", "model": "some-model" } } }"#,
    );
    let typed = "sub dir/plan.rhei.md";

    let out = case.rhei(&case.root, &["run", typed, "--no-tui", "--no-callbacks"]);

    let rendered = raw_stderr(&out);
    assert_eq!(out.status.code(), Some(1), "the run refuses:\n{rendered}");
    let expected = format!("rhei run {}", shell_quote(typed));
    assert_offers(&rendered, &expected);
    let example = format!("{expected} --agent claude-code");
    assert!(
        rendered.lines().any(|line| line.contains(&example)),
        "the message's example should be `{example}`, whole on one line; got:\n{rendered}"
    );
    #[cfg(unix)]
    {
        let again = pasted(&expected, &case.root, &case.home);
        assert!(
            again.stderr.contains("no agent configured for model 'codex'"),
            "`{expected}` should reach the same refusal:\n{}",
            again.stderr
        );
    }
}

/// `timeout_transition_unfired_help`: the program damages the budget index so the
/// timeout edge cannot charge, as `timeout_charge_halt_tests` does.
/// §FS-rhei-errors.1.2.1 §FS-rhei-run.3
#[test]
fn help_target_a_failed_timeout_edge_names_its_target() {
    let case = Case::new("help-target-timeout");
    let project = case.root.join("project");
    fs::create_dir_all(&project).expect("create the project directory");
    let script = write_python_agent(
        &project,
        "build.py",
        "pathlib.Path(os.environ['XDG_STATE_HOME'], 'rhei/budget-authority/roots.json').mkdir(parents=True)\n\
         time.sleep(5)\n",
    );
    write_fixture_file(
        &project,
        "plan.rhei.md",
        "# Rhei: Project\n\n## Tasks\n\n### Task 1: Build artifact\n**State:** build\n",
    );
    write_fixture_file(
        &project,
        "states.yaml",
        &format!(
            "name: timeout-edge\nversion: 1\nstates:\n  build:\n    description: Build.\n    \
             program:\n      command: {}\n    program_timeout: 1s\n  timed-out:\n    \
             description: Timed out.\n    final: true\ntransitions:\n  - from: build\n    \
             to: timed-out\n    timeout: 1s\n",
            fixture_command(&script)
        ),
    );

    let out = case.rhei(&project, &["run", "plan.rhei.md", "--no-tui"]);

    assert_eq!(out.status.code(), Some(1), "the run halts:\n{}", raw_stderr(&out));
    assert_offers(&raw_stderr(&out), &format!("rhei run {}", shell_quote("plan.rhei.md")));
}

/// `resolve_plan_target` widens a member to its project, and printing that lists
/// a scope the user did not ask about. §FS-rhei-errors.1.2.1 §FS-rhei-panta.6
#[test]
fn help_target_a_member_target_prints_the_member_and_not_its_project() {
    let case = Case::new("help-target-member");
    let project = case.root.join("project");
    fs::create_dir_all(&project).expect("create the project directory");
    write_fixture_file(&project, "index.panta.md", "# Panta: Project\n");
    write_fixture_file(&project, "states.yaml", HELD_MACHINE);
    write_fixture_file(&project, "auth.rhei.md", HELD_PLAN);
    write_fixture_file(&project, "billing.rhei.md", HELD_PLAN);
    let typed = "project/auth.rhei.md";

    let out = case.rhei(&case.root, &["run", typed, "--no-tui", "--no-callbacks"]);

    assert_eq!(out.status.code(), Some(1), "the run halts:\n{}", raw_stderr(&out));
    let expected = listing(typed);
    assert_offers(&raw_stderr(&out), &expected);
    assert_lists(&expected, &case.root, &case.home, "auth.1", &["billing.1"]);
}

/// `rhei next` holds the project by the time it refuses, and a member is not that:
/// over a ticket someone holds, and with nothing to claim. §FS-rhei-errors.1.2.1 §FS-rhei-panta.6
#[test]
fn help_target_a_member_target_on_next_prints_the_member() {
    let case = Case::new("help-target-next-member");
    let project = case.held("project");
    write_fixture_file(&project, "index.panta.md", "# Panta: Project\n");
    write_fixture_file(&project, "auth.rhei.md", HELD_PLAN);
    write_fixture_file(&project, "billing.rhei.md", HELD_PLAN);
    let typed = "project/auth.rhei.md";

    let held = case.rhei(&case.root, &["next", typed, "--task", "auth.1", "--no-callbacks"]);
    assert_offers(&raw_stderr(&held), &format!("rhei next {}", shell_quote(typed)));

    let out = case.rhei(&case.root, &["next", typed, "--no-callbacks"]);
    let expected = listing(typed);
    assert_offers(&raw_stderr(&out), &expected);
    assert_lists(&expected, &case.root, &case.home, "auth.1", &["billing.1"]);
}

/// A bare `rhei run` has a target: the one it discovered, printed as
/// `rhei migrate export-priors` prints it. §FS-rhei-errors.1.2.1
#[test]
fn help_target_a_bare_run_prints_the_path_it_discovered() {
    let case = Case::new("help-target-bare");
    let project = case.held("project");

    let out = case.rhei(&project, &["run", "--no-tui", "--no-callbacks"]);

    assert_eq!(out.status.code(), Some(1), "the run halts:\n{}", raw_stderr(&out));
    let expected = listing_found(&project.join("plan.rhei.md"));
    assert_offers(&raw_stderr(&out), &expected);
    assert_lists(&expected, &project, &case.home, "plan.1", &[]);
}

/// The rule is not `rhei run`'s: one sample outside it, typed and discovered, holds
/// the wider scope, and the guard in `tests_help_target_guard.rs` holds the rest.
/// §FS-rhei-errors.1.2.1 §FS-rhei-release
#[test]
fn help_target_a_release_of_an_unclaimed_ticket_names_its_target() {
    let case = Case::new("help-target-release");
    let project = case.project("project", UNCLAIMED_PLAN);

    let typed = case.rhei(&project, &["release", "plan.rhei.md", "--task", "1"]);
    assert_eq!(typed.status.code(), Some(1), "nothing to release:\n{}", raw_stderr(&typed));
    assert_offers(&raw_stderr(&typed), &listing("plan.rhei.md"));

    let bare = case.rhei(&project, &["release", "plan.1"]);
    assert_eq!(bare.status.code(), Some(1), "nothing to release:\n{}", raw_stderr(&bare));
    let expected = listing_found(&project.join("plan.rhei.md"));
    assert_offers(&raw_stderr(&bare), &expected);
    assert_lists(&expected, &project, &case.home, "plan.1", &[]);
}

/// `settings_help` says no plan root is in scope, which is about the file: a run
/// that was given its target still knows it. §FS-rhei-errors.1.2.1
#[test]
fn help_target_a_broken_settings_file_names_the_target() {
    let case = Case::new("help-target-settings");
    let project = case.held("project");
    fs::create_dir_all(project.join(".agent-grounds/rhei")).expect("create the settings directory");
    write_fixture_file(&project, ".agent-grounds/rhei/settings.json", "{ not json");

    let out = case.rhei(&project, &["run", "plan.rhei.md", "--no-tui", "--no-callbacks"]);

    assert_eq!(out.status.code(), Some(1), "the run refuses:\n{}", raw_stderr(&out));
    assert_offers(&raw_stderr(&out), &format!("rhei validate {}", shell_quote("plan.rhei.md")));
}

/// Without the machine it was given, the pasted command resolves another one and
/// refuses over a state that machine does not declare, which is a different error.
/// §FS-rhei-errors.1.2.1 §FS-rhei-errors.1.2
#[test]
fn help_target_a_given_state_machine_is_passed_on() {
    let case = Case::new("help-target-machine");
    let machines = case.root.join("machines");
    fs::create_dir_all(&machines).expect("create the machines directory");
    write_fixture_file(&machines, "custom.yaml", HELD_MACHINE);
    let project = case.root.join("project");
    fs::create_dir_all(&project).expect("create the project directory");
    write_fixture_file(&project, "plan.rhei.md", HELD_PLAN);
    let machine = "../machines/custom.yaml";

    let halted = case.rhei(
        &project,
        &["run", "plan.rhei.md", "--state-machine", machine, "--no-tui", "--no-callbacks"],
    );
    assert_eq!(halted.status.code(), Some(1), "the run halts:\n{}", raw_stderr(&halted));
    let expected = format!("{} --state-machine {}", listing("plan.rhei.md"), shell_quote(machine));
    assert_offers(&raw_stderr(&halted), &expected);
    assert_lists(&expected, &project, &case.home, "plan.1", &[]);

    write_fixture_file(&project, "plan.rhei.md", INVALID_PLAN);
    let refused = case.rhei(&project, &["validate", "plan.rhei.md", "--state-machine", machine]);
    assert_eq!(refused.status.code(), Some(1), "validation refuses:\n{}", raw_stderr(&refused));
    let expected = format!(
        "rhei validate {} --state-machine {}",
        shell_quote("plan.rhei.md"),
        shell_quote(machine)
    );
    assert_offers(&raw_stderr(&refused), &expected);
    #[cfg(unix)]
    {
        let again = pasted(&expected, &project, &case.home);
        assert!(
            again.stderr.contains("Task plan.1 depends on missing Task plan.99"),
            "{}",
            again.stderr
        );
        assert!(!again.stderr.contains("invalid state"), "a different error:\n{}", again.stderr);
    }
}

/// A detached run's stderr is `runtime/run.log`, read later and from anywhere, so
/// what it prints runs from another directory. §FS-rhei-errors.1.2.1
#[cfg(unix)]
#[test]
fn help_target_a_detached_run_logs_it_absolute() {
    let case = Case::new("help-target-detached");
    let project = case.held("project");

    let launched = case.rhei(&project, &["run", "--headless", "plan.rhei.md", "--no-callbacks"]);
    assert!(launched.status.success(), "the launch works:\n{}", raw_stderr(&launched));

    let log = logged_diagnostic(&project);
    let expected = listing_found(&project.join("plan.rhei.md"));
    assert_offers(&log, &expected);
    assert_lists(&expected, &case.root, &case.home, "plan.1", &[]);
}

/// The machine resolves against the working directory the same as the target, so
/// a log read elsewhere needs it absolute too. §FS-rhei-errors.1.2.1
#[cfg(unix)]
#[test]
fn help_target_a_detached_run_logs_its_machine_absolute() {
    let case = Case::new("help-target-detached-machine");
    let machines = case.root.join("machines");
    fs::create_dir_all(&machines).expect("create the machines directory");
    write_fixture_file(&machines, "custom.yaml", HELD_MACHINE);
    let project = case.root.join("project");
    fs::create_dir_all(&project).expect("create the project directory");
    write_fixture_file(&project, "plan.rhei.md", HELD_PLAN);

    let launched = case.rhei(
        &case.root,
        &[
            "run",
            "--headless",
            "project/plan.rhei.md",
            "--state-machine",
            "machines/custom.yaml",
            "--no-callbacks",
        ],
    );
    assert!(launched.status.success(), "the launch works:\n{}", raw_stderr(&launched));

    let log = logged_diagnostic(&project);
    let machine = discovered(&machines.join("custom.yaml")).display().to_string();
    let expected = format!(
        "{} --state-machine {}",
        listing_found(&project.join("plan.rhei.md")),
        shell_quote(&machine)
    );
    assert_offers(&log, &expected);
    assert_lists(&expected, &project, &case.home, "plan.1", &[]);
}

/// `runtime/run.log` once the halt is written, which a blank line ends.
#[cfg(unix)]
pub(super) fn logged_diagnostic(project: &Path) -> String {
    let log = project.join("runtime/run.log");
    let halted =
        |text: &str| text.rsplit_once("help:").is_some_and(|(_, help)| help.contains("\n\n"));
    super::headless_support::wait_until(
        "the detached run to halt",
        std::time::Duration::from_secs(30),
        || halted(&raw_stderr_from_file(&log)),
    );
    raw_stderr_from_file(&log)
}

/// A refused start reaches the launcher as its message, and wraps there unless the
/// renderer checks the message too, as it does for the reset offer. §FS-rhei-errors.1.2.1
#[cfg(unix)]
#[test]
fn help_target_a_relayed_startup_refusal_keeps_the_command_whole() {
    let case = Case::new("help-target-relay");
    let relative = format!("{WIDER_THAN_THE_RENDER}/sub dir");
    let project = case.project(&relative, INVALID_PLAN);
    let typed = format!("{relative}/plan.rhei.md");

    let out = case.rhei(&case.root, &["run", "--headless", &typed, "--no-callbacks"]);

    let rendered = raw_stderr(&out);
    assert!(!out.status.success(), "the start is refused:\n{rendered}");
    let target = discovered(&project.join("plan.rhei.md")).display().to_string();
    let expected = format!("rhei validate {}", shell_quote(&target));
    assert!(
        rendered.lines().any(|line| line.trim_end().ends_with(&expected)),
        "the relayed help should end on `{expected}`, whole on one line; got:\n{rendered}"
    );
}
