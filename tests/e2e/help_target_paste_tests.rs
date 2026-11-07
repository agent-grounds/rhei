//! The lines that named a command with no target at all, or with a spelling that
//! came apart, each pasted from the directory the command was typed in:
//! §FS-rhei-errors.1.2.1 on the failing exits §FS-rhei-errors.6 covers.
//!
//! Printing the right words is not what these hold. A line is right when it does,
//! from there, what the sentence around it says it will. Pasting needs `sh`, so
//! each is a Unix claim; the spelling is `shell_quote`'s on every platform.
#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};

use super::help_target_tests::{
    assert_lists, assert_offers, listing, pasted, Case, HELD_MACHINE, HELD_PLAN,
};
use super::*;

/// One ticket mid-way through a machine with one more state than the built-in's.
const CUSTOM_MACHINE: &str = r#"name: custom
version: 1
states:
  open:
    initial: true
    description: Open.
  middle:
    description: Between.
  done:
    description: Finished.
    final: true
transitions:
  - { from: open, to: middle, description: On. }
  - { from: middle, to: done, description: Done. }
"#;

pub(super) const OPEN_PLAN: &str =
    "# Rhei: Project\n\n## Tasks\n\n### Task 1: A ticket\n**State:** open\n";

/// A supervisor whose child is held for as long as its visit is pending.
const SUPERVISOR_MACHINE: &str = r#"name: sup
version: 1
states:
  supervising:
    initial: true
    description: Supervising.
    execute_on: child-terminal
    agent: claude-code
  ready:
    initial: true
    description: Ready.
  done:
    description: Finished.
    final: true
transitions:
  - { from: supervising, to: supervising, description: Again. }
  - { from: supervising, to: done, description: Done. }
  - { from: ready, to: done, description: Done. }
"#;

const SUPERVISED_PLAN: &str = "# Rhei: Project\n\n## Tasks\n\n\
                               ### Task 1: The supervisor\n**State:** supervising\n\n\
                               #### Task 1.1: A child\n**State:** ready\n";

/// `name` in the directory, made first.
pub(super) fn write(dir: &Path, name: &str, contents: &str) -> PathBuf {
    let path = dir.join(name);
    fs::create_dir_all(path.parent().expect("a parent")).expect("create the directory");
    fs::write(&path, contents).expect("write the fixture");
    path
}

/// A project of two rheis under the machine they share.
fn two_member_project(case: &Case) -> PathBuf {
    let project = case.root.join("project");
    write(&project, "index.panta.md", "# Panta: Project\n");
    write(&project, "states.yaml", HELD_MACHINE);
    write(&project, "auth.rhei.md", HELD_PLAN);
    write(&project, "billing.rhei.md", HELD_PLAN);
    project
}

/// The physical line of `rendered` that holds `needle`, for a command with prose
/// after it, where [`assert_offers`] would read the prose as part of the command.
#[track_caller]
pub(super) fn assert_on_one_line(rendered: &str, needle: &str) {
    assert!(
        rendered.lines().any(|line| line.contains(needle)),
        "`{needle}` should be whole on one line; got:\n{rendered}"
    );
}

/// `rhei list` run bare from the parent finds no plan, and it was given one.
/// §FS-rhei-errors.1.2.1 §FS-rhei-panta.6.1
#[test]
fn help_target_a_scope_refusal_lists_the_project_it_was_given() {
    let case = Case::new("help-target-scope");
    two_member_project(&case);

    let out = case.rhei(
        &case.root,
        &["next", "project", "--rhei", "auth", "--task", "billing.1", "--no-callbacks"],
    );

    let rendered = raw_stderr(&out);
    assert_eq!(out.status.code(), Some(1), "the scope refuses:\n{rendered}");
    let expected = listing("project");
    assert_offers(&rendered, &expected);
    assert_lists(&expected, &case.root, &case.home, "billing.1", &[]);
}

/// `rhei new --project` is told about that project, and bare `rhei validate` is
/// about whatever the directory holds. §FS-rhei-errors.1.2.1 §FS-rhei-new.5.2
#[test]
fn help_target_a_new_ticket_refusal_validates_the_project_it_was_given() {
    let case = Case::new("help-target-new");
    let other = two_member_project(&case);
    write(&other, "theirs.rhei.md", "# Not A Rhei Heading\n");

    let out =
        case.rhei(&case.root, &["new", "A title", "--project", "project", "--under", "theirs"]);

    let rendered = raw_stderr(&out);
    assert_eq!(out.status.code(), Some(1), "the create refuses:\n{rendered}");
    assert_on_one_line(&rendered, "`rhei validate project` reports it with a code frame");
    let again = pasted("rhei validate project", &case.root, &case.home);
    assert!(
        again.stderr.contains("theirs") && !again.stderr.contains("no Rhei plan found"),
        "`rhei validate project` should report that rhei:\n{}",
        again.stderr
    );
}

/// The form a held ticket's help offers, and the one a claim conflict now prints,
/// is the one that runs from the parent. §FS-rhei-errors.1.2.1 §FS-rhei-release
#[test]
fn help_target_a_held_ticket_releases_from_the_parent() {
    let case = Case::new("help-target-held");
    case.held("sub");

    let out = case.rhei(&case.root, &["next", "sub/plan.rhei.md", "--task", "1", "--no-callbacks"]);

    let rendered = raw_stderr(&out);
    assert_eq!(out.status.code(), Some(1), "the ticket is held:\n{rendered}");
    let expected = "rhei release sub/plan.rhei.md --task plan.1";
    assert_on_one_line(&rendered, expected);
    let again = pasted(expected, &case.root, &case.home);
    assert!(again.status.success(), "`{expected}` should run as printed:\n{}", again.stderr);
}

/// A second `rhei init` lists the project in the way, which is not the host: `rhei
/// list host` refuses a host whose project sits in `panta/`. §FS-rhei-errors.1.2.1
/// §FS-rhei-init.2
#[test]
fn help_target_an_init_conflict_lists_the_project_in_the_way() {
    let case = Case::new("help-target-init");
    let host = case.root.join("host");

    let first = case.rhei(&case.root, &["init", "host"]);
    assert!(first.status.success(), "the first init works:\n{}", raw_stderr(&first));

    for (cwd, typed, expected) in [
        (&*case.root, Some("host"), "rhei list host/panta"),
        (host.as_path(), None, "rhei list panta"),
    ] {
        let mut args = vec!["init"];
        args.extend(typed);
        let out = case.rhei(cwd, &args);

        let rendered = raw_stderr(&out);
        assert_eq!(out.status.code(), Some(1), "the second init refuses:\n{rendered}");
        assert_on_one_line(
            &rendered,
            &format!("{expected}, then re-run init with the flag named above."),
        );
        let again = pasted(expected, cwd, &case.home);
        assert!(again.status.success(), "`{expected}` should run:\n{}", again.stderr);
    }
}

/// Listing a directory of two rheis and no manifest is the refusal itself, so the
/// help names a file it refused on, spelled from where `init` was typed.
/// §FS-rhei-errors.1.2.1 §FS-rhei-init.2
#[test]
fn help_target_an_init_refused_on_its_rheis_lists_one_of_them() {
    let case = Case::new("help-target-init-rheis");
    let host = case.root.join("host");
    write(&host, "a.rhei.md", OPEN_PLAN);
    write(&host, "b.rhei.md", OPEN_PLAN);

    for (cwd, typed, expected) in [
        (&*case.root, Some("host"), "rhei list host/a.rhei.md"),
        (host.as_path(), None, "rhei list a.rhei.md"),
    ] {
        let mut args = vec!["init"];
        args.extend(typed);
        let out = case.rhei(cwd, &args);

        let rendered = raw_stderr(&out);
        assert_eq!(out.status.code(), Some(1), "init refuses:\n{rendered}");
        assert_on_one_line(&rendered, &format!("{expected}, then re-run init"));
        assert_lists(expected, cwd, &case.home, "a.1", &["b.1"]);
    }
}

/// `rhei states` with a machine of its own describes that machine; bare, it would
/// describe the built-in and exit 0, which no reader can see is wrong.
/// §FS-rhei-errors.1.2.1 §FS-rhei-states
#[test]
fn help_target_states_describes_the_machine_the_command_used() {
    let case = Case::new("help-target-states");
    write(&case.root.join("machines"), "custom.yaml", CUSTOM_MACHINE);
    write(&case.root.join("sub"), "plan.rhei.md", OPEN_PLAN);

    let out = case.rhei(
        &case.root,
        &[
            "transition",
            "sub/plan.rhei.md",
            "--task",
            "1",
            "--from",
            "open",
            "--to",
            "nonesuch",
            "--state-machine",
            "machines/custom.yaml",
            "--no-callbacks",
        ],
    );

    let rendered = raw_stderr(&out);
    assert_eq!(out.status.code(), Some(1), "no such state:\n{rendered}");
    let expected = "rhei states sub/plan.rhei.md --state-machine machines/custom.yaml";
    assert_offers(&rendered, expected);
    let again = pasted(expected, &case.root, &case.home);
    assert!(again.status.success(), "`{expected}` should run:\n{}", again.stderr);
    assert!(
        again.stdout.contains("Source: 'machines/custom.yaml'") && again.stdout.contains("middle"),
        "`{expected}` should describe the machine the command used:\n{}",
        again.stdout
    );
}

/// With no machine given, the plan's own is the one the pasted line must describe.
/// §FS-rhei-errors.1.2.1 §FS-rhei-states
#[test]
fn help_target_states_describes_the_plan_own_machine_from_the_parent() {
    let case = Case::new("help-target-states-own");
    let sub = case.root.join("sub");
    write(&sub, "plan.rhei.md", OPEN_PLAN);
    write(&sub, "states.yaml", CUSTOM_MACHINE);

    let out = case.rhei(
        &case.root,
        &["transition", "sub/plan.rhei.md", "--task", "1", "--from", "open", "--to", "nonesuch"],
    );

    let rendered = raw_stderr(&out);
    assert_eq!(out.status.code(), Some(1), "no such state:\n{rendered}");
    let expected = "rhei states sub/plan.rhei.md";
    assert_offers(&rendered, expected);
    let again = pasted(expected, &case.root, &case.home);
    assert!(
        again.stdout.contains("Source: 'sub/states.yaml'"),
        "`{expected}` should describe the plan's own machine:\n{}",
        again.stdout
    );
}

/// Sealing without the machine the run sealed under seals another file set, and
/// the refusal comes back on every retry. §FS-rhei-errors.1.2.1 §FS-rhei-seal.8.4
#[test]
fn help_target_a_strict_refusal_seals_what_the_run_sealed() {
    let case = Case::new("help-target-strict");
    let machine = write(
        &case.root.join("machines"),
        "custom.yaml",
        &HELD_MACHINE.replace("Ready to work, were it not held.", "As sealed."),
    );
    write(&case.root.join("sub"), "plan.rhei.md", OPEN_PLAN);
    let given = ["--state-machine", "machines/custom.yaml"];
    let sealed = case.rhei(&case.root, &[&["seal", "sub/plan.rhei.md"], &given[..]].concat());
    assert!(sealed.status.success(), "the seal works:\n{}", raw_stderr(&sealed));
    let changed =
        fs::read_to_string(&machine).expect("read the machine").replace("As sealed.", "Changed.");
    fs::write(&machine, changed).expect("change the machine");

    let strict_run = [
        &["run", "sub/plan.rhei.md"][..],
        &given[..],
        &["--strict-inputs", "--no-tui", "--no-callbacks"][..],
    ]
    .concat();
    let refused = case.rhei(&case.root, &strict_run);

    let rendered = raw_stderr(&refused);
    assert_eq!(refused.status.code(), Some(1), "the changed machine is refused:\n{rendered}");
    let expected = "rhei seal sub/plan.rhei.md --state-machine machines/custom.yaml";
    assert_on_one_line(&rendered, &format!("accept the change with `{expected}`"));
    let again = pasted(expected, &case.root, &case.home);
    assert!(again.status.success(), "`{expected}` should run:\n{}", again.stderr);

    let started = case.rhei(&case.root, &strict_run);
    let rendered = raw_stderr(&started);
    assert!(
        CliRun::from(&started).stdout.contains("Spawning program for Task plan.1"),
        "the next strict run should start; got:\n{}\n{rendered}",
        CliRun::from(&started).stdout
    );
    assert!(!rendered.contains("was not started"), "the seal did not cure it:\n{rendered}");
}

/// The target follows a flag here, and a line naming one is not wrapped, inside its
/// quotes or anywhere else. §FS-rhei-errors.1.2.1 §FS-rhei-errors.2
#[test]
fn help_target_a_target_after_a_flag_stays_on_one_line() {
    let case = Case::new("help-target-intervene");
    let relative = format!("{}/sub dir", super::help_target_tests::WIDER_THAN_THE_RENDER);
    case.held(&relative);
    let typed = format!("{relative}/plan.rhei.md");

    let out =
        case.rhei(&case.root, &["intervene", "--plan", &typed, "--task", "1", "--message", "   "]);

    let rendered = raw_stderr(&out);
    assert_eq!(out.status.code(), Some(1), "a blank message refuses:\n{rendered}");
    let command = format!("rhei intervene --plan {} --task 1", shell_quote(&typed));
    assert_offers(&rendered, &format!("{command} --message <text>"));
    let again = pasted(&format!("{command} --message hello"), &case.root, &case.home);
    assert!(
        !again.stderr.contains("no plan or project at"),
        "the pasted target should resolve:\n{}",
        again.stderr
    );
}

/// A machine with a space, given after the verb: the state-changing `rhei next` is
/// one line, with the machine last, and it claims the ticket it names.
/// §FS-rhei-errors.1.2.1 §FS-rhei-supervision.3.4
#[test]
fn help_target_a_spaced_machine_on_next_stays_on_one_line_and_pastes() {
    let case = Case::new("help-target-next-machine");
    write(&case.root.join("machines dir"), "sup.yaml", SUPERVISOR_MACHINE);
    write(&case.root.join("sub"), "plan.rhei.md", SUPERVISED_PLAN);
    let machine = "machines dir/sup.yaml";

    let out = case.rhei(
        &case.root,
        &[
            "next",
            "sub/plan.rhei.md",
            "--task",
            "1.1",
            "--state-machine",
            machine,
            "--no-callbacks",
        ],
    );

    let rendered = raw_stderr(&out);
    assert_eq!(out.status.code(), Some(1), "the child is held:\n{rendered}");
    let expected = format!(
        "rhei next sub/plan.rhei.md --task plan.1 --state-machine {}",
        shell_quote(machine)
    );
    assert_offers(&rendered, &expected);
    let again = pasted(&format!("{expected} --peek"), &case.root, &case.home);
    assert!(
        again.status.success() && again.stdout.contains("current state: 'supervising'"),
        "`{expected}` should reach the supervisor:\n{}\n{}",
        again.stdout,
        again.stderr
    );
}

/// `state_machine_flag` printed what the command resolved, and the holder prints
/// what it was given. A plan's own machine needs no flag: the pasted line resolves
/// it the same way. §FS-rhei-errors.1.2.1 §FS-rhei-supervision.3.4
#[test]
fn help_target_a_machine_the_command_resolved_is_not_passed_on() {
    let case = Case::new("help-target-next-resolved");
    let sub = case.root.join("sub");
    write(&sub, "plan.rhei.md", SUPERVISED_PLAN);
    write(&sub, "states.yaml", SUPERVISOR_MACHINE);

    let out =
        case.rhei(&case.root, &["next", "sub/plan.rhei.md", "--task", "1.1", "--no-callbacks"]);

    let rendered = raw_stderr(&out);
    assert_eq!(out.status.code(), Some(1), "the child is held:\n{rendered}");
    let expected = "rhei next sub/plan.rhei.md --task plan.1";
    assert_offers(&rendered, expected);
    let again = pasted(&format!("{expected} --peek"), &case.root, &case.home);
    assert!(
        again.status.success() && again.stdout.contains("current state: 'supervising'"),
        "`{expected}` should resolve the plan's own machine:\n{}\n{}",
        again.stdout,
        again.stderr
    );
}

/// A plan path with a space, refused for naming no ticket: the form its message
/// offers parses as the one path. §FS-rhei-errors.2 §FS-rhei-errors.1.2.1
#[test]
fn help_target_a_plan_path_refusal_quotes_the_path_it_offers() {
    let case = Case::new("help-target-plan-path");
    case.held("sub dir");
    let typed = "sub dir/plan.rhei.md";
    let quoted = shell_quote(typed);

    let show = case.rhei(&case.root, &["show", typed]);
    assert_on_one_line(&raw_stderr(&show), &format!("rhei show {quoted} --task <ticket-id>"));
    let again = pasted(&format!("rhei show {quoted} --task 1"), &case.root, &case.home);
    assert!(again.status.success(), "the offered form should run:\n{}", again.stderr);

    let complete = case.rhei(&case.root, &["complete", typed, "--result", "done"]);
    assert_on_one_line(
        &raw_stderr(&complete),
        &format!("rhei complete {quoted} --task <ticket-id> --result <message>"),
    );
    let transition =
        case.rhei(&case.root, &["transition", typed, "--from", "open", "--to", "done"]);
    assert_on_one_line(
        &raw_stderr(&transition),
        &format!("rhei transition {quoted} --task <ticket-id> --from <state> --to <state>"),
    );
}

/// A launcher that cannot report a run it started still says how to reach it, and
/// `/dev/full` is the one failure a test can ask stdout for. §FS-rhei-run-headless.1.1
#[cfg(target_os = "linux")]
#[test]
fn help_target_a_launch_it_could_not_report_reaches_the_run() {
    let case = Case::new("help-target-launch");
    let project = case.held("project");
    let full = fs::OpenOptions::new().write(true).open("/dev/full").expect("open /dev/full");

    let out = rhei_command(&case.home)
        .current_dir(&project)
        .args(["run", "--headless", "plan.rhei.md", "--no-callbacks"])
        .stdin(std::process::Stdio::null())
        .stdout(full)
        .stderr(std::process::Stdio::piped())
        .output()
        .expect("rhei should run");

    let rendered = raw_stderr(&out);
    assert_eq!(out.status.code(), Some(1), "the report did not fit:\n{rendered}");
    assert_on_one_line(&rendered, "the run started regardless: `rhei attach .` reaches it");
    super::help_target_tests::logged_diagnostic(&project);
    let again = pasted("rhei attach .", &project, &case.home);
    assert!(again.status.success(), "`rhei attach .` should run:\n{}", again.stderr);
}

/// A plan and machine beside the template, which is what discovery would answer with.
fn plan_beside_a_template(case: &Case) {
    write(&case.root, "other.rhei.md", &OPEN_PLAN.replace("Project", "Other"));
    write(
        &case.root,
        "states.yaml",
        &CUSTOM_MACHINE.replace("name: custom", "name: the-other-machine"),
    );
}

fn instantiate_and_execute(case: &Case, template: &str) -> std::process::Output {
    let source = format!("./{template}");
    case.rhei(
        &case.root,
        &[
            "instantiate",
            &source,
            "--output",
            "out",
            "--execute",
            "--",
            "--no-tui",
            "--no-callbacks",
        ],
    )
}

/// A command with no target of its own does not answer from what the directory
/// holds: `--execute` lists what it laid, and never the plan beside the template.
/// §FS-rhei-errors.1.2.1 §FS-rhei-templates
#[test]
fn help_target_instantiate_execute_names_what_it_laid() {
    let case = Case::new("help-target-execute");
    plan_beside_a_template(&case);
    let template = case.root.join("execute-template");
    write(
        &template,
        "template.yaml",
        "name: execute-template\nversion: 1.0.0\ndescription: A template whose run halts.\n",
    );
    write(&template, "plan.rhei.md", HELD_PLAN);
    write(&template, "states.yaml", HELD_MACHINE);

    let out = instantiate_and_execute(&case, "execute-template");

    let rendered = raw_stderr(&out);
    assert_eq!(out.status.code(), Some(1), "the run halts:\n{rendered}");
    let expected = listing("out/plan.rhei.md");
    assert_offers(&rendered, &expected);
    assert_lists(&expected, &case.root, &case.home, "plan.1", &["other.1"]);
}

/// A machine whose first ticket waits on a handoff no transition produced, which is
/// refused before any agent starts.
const REQUIRED_HANDOFF_MACHINE: &str = r#"name: the-laid-machine
version: 1
states:
  implement:
    initial: true
    description: Implement.
    target: claude-code:anthropic:model-a
    agent_timeout: 60s
    outputs:
      - { name: brief, kind: handoff, path: runtime/handoff/brief.md }
  review:
    description: Review.
    target: claude-code:anthropic:model-b
    agent_timeout: 60s
    handoff:
      inherit: [{ from: transition.previous, required: true }]
  completed:
    final: true
    description: Done.
transitions:
  - { from: implement, to: review, description: On. }
  - { from: review, to: completed, description: Done. }
"#;

/// `rhei states` under `--execute` describes the machine it laid, not the one beside
/// the template. §FS-rhei-errors.1.2.1 §FS-rhei-templates
#[test]
fn help_target_instantiate_execute_describes_the_machine_it_laid() {
    let case = Case::new("help-target-execute-states");
    plan_beside_a_template(&case);
    let template = case.root.join("handoff-template");
    write(
        &template,
        "template.yaml",
        "name: handoff-template\nversion: 1.0.0\ndescription: A template whose run refuses.\n",
    );
    write(&template, "plan.rhei.md", &OPEN_PLAN.replace("**State:** open", "**State:** review"));
    write(&template, "states.yaml", REQUIRED_HANDOFF_MACHINE);

    let out = instantiate_and_execute(&case, "handoff-template");

    let rendered = raw_stderr(&out);
    assert_eq!(out.status.code(), Some(1), "the run refuses:\n{rendered}");
    let expected = "rhei states out/plan.rhei.md";
    assert_on_one_line(&rendered, expected);
    let again = pasted(expected, &case.root, &case.home);
    assert!(
        again.stdout.contains("the-laid-machine"),
        "the pasted line should describe the laid machine:\n{}",
        again.stdout
    );
}
