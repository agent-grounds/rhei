//! The unaccounted-log refusal's remedy, in every shape a target or a rhei id
//! could come out wrong in. Each test reads the command out of what the run
//! printed and runs it from another directory (agent-grounds/rhei#549).

// §FS-rhei-reset.5.1 §FS-rhei-agents.8.1 §FS-rhei-panta.6

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use super::entry_log_support::names_in;
use super::reset_remedy_support::*;
use super::*;

/// A member `.rhei.md` named relative to the project, as the issue's recipe does.
#[test]
fn the_reset_remedy_runs_for_a_member_plan_named_relatively() {
    let world = RemedyWorld::at("reset-remedy-member", "project").with_found_machine();
    world.write("index.panta.md", "# Panta: Remedy\n");
    world.write("plan.rhei.md", TWO_TICKETS);
    let world = RemedyWorld { target: "plan.rhei.md".into(), ..world };

    world.work("plan.2", "measure");
    let said = world.refuse("plan.1", "measure", &world.root);

    assert_offers_runnable_reset(&said, &world.home, "plan", &["plan.1", "plan.2"]);
}

/// Sibling rheis, the shape the report never ran: run on the project directory
/// and applied for real, the named rhei goes back and the sibling keeps its
/// ticket, its result and its logs.
#[test]
fn the_reset_remedy_stops_at_the_named_rhei_when_applied_to_a_project_with_siblings() {
    let world = RemedyWorld::at("reset-remedy-siblings", "project").with_found_machine();
    world.write("index.panta.md", "# Panta: Remedy\n");
    world.write("alpha.rhei.md", TWO_TICKETS);
    world.write("beta.rhei.md", TWO_TICKETS);

    world.enter("alpha.2", "measure");
    world.work("beta.1", "measure");
    assert_eq!(world.state_of("alpha.2"), "waiting", "fixture: alpha.2 did its work");
    let runtime = world.root.join("runtime");
    let kept = owned_by(&runtime, "beta.1");
    assert_eq!(kept.len(), 3, "fixture: the sibling has a result, a log and a record: {kept:?}");
    let alpha_result = runtime.join("results/alpha.2.md");
    assert!(alpha_result.exists(), "fixture: alpha.2 left a result for the reset to take");

    let said = world.refuse("alpha.1", "work", &world.root);
    assert_offers_runnable_reset(&said, &world.home, "alpha", &["alpha.1", "alpha.2"]);

    let offers = offered_resets(&said);
    let applied = run_offer(&offers[0], "-y", &world.home);
    assert_success(&applied);

    assert_eq!(world.state_of("alpha.1"), "open", "the rhei the remedy names goes back");
    assert_eq!(world.state_of("alpha.2"), "open", "every ticket in it, as the message says");
    assert!(!alpha_result.exists(), "its results go with it");
    assert!(!runtime.join("logs/task-alpha.1-work.log").exists(), "and the planted log");
    assert_eq!(world.state_of("beta.1"), "waiting", "the sibling's ticket stays where it is");
    assert_eq!(world.state_of("beta.2"), "open");
    assert_eq!(owned_by(&runtime, "beta.1"), kept, "the sibling's files survive byte for byte");
}

/// Every result, log and spawn record `ticket` owns under `runtime`, with its bytes.
fn owned_by(runtime: &Path, ticket: &str) -> BTreeMap<String, Vec<u8>> {
    let mut owned = BTreeMap::new();
    for (sub, stem) in [
        ("results", format!("{ticket}.")),
        ("logs", format!("task-{ticket}-")),
        ("spawns", format!("task-{ticket}-")),
    ] {
        for name in names_in(runtime, sub).into_iter().filter(|name| name.starts_with(&stem)) {
            let path = runtime.join(sub).join(&name);
            owned.insert(format!("{sub}/{name}"), fs::read(path).expect("read the file"));
        }
    }
    owned
}

fn workspace_member(name: &str) -> RemedyWorld {
    let world = RemedyWorld::at(name, "project").with_found_machine();
    world.write("index.panta.md", "# Panta: Remedy\n");
    world.write("alpha.rhei.md", TWO_TICKETS);
    world.write("gamma/index.rhei.md", "# Rhei: Gamma\n");
    world.write("gamma/tasks/01-one.md", "### Task 1: One\n**State:** open\n");
    world
}

/// A directory-workspace member's program logs under its own root.
#[test]
fn the_reset_remedy_runs_for_a_workspace_members_program_state() {
    let world = workspace_member("reset-remedy-workspace-program");
    let said = world.refuse("gamma.1", "measure", &world.root.join("gamma"));
    assert_offers_runnable_reset(&said, &world.home, "gamma", &["gamma.1"]);
}

/// A directory-workspace member's agent logs under the project's root.
#[test]
fn the_reset_remedy_runs_for_a_workspace_members_agent_state() {
    let world = workspace_member("reset-remedy-workspace-agent");
    let said = world.refuse("gamma.1", "work", &world.root);
    assert_offers_runnable_reset(&said, &world.home, "gamma", &["gamma.1"]);
}

/// A lone `.rhei.md` with no `index.panta.md`: the target is that rhei alone,
/// and `--rhei` is still the id its tickets carry.
#[test]
fn the_reset_remedy_runs_for_a_lone_plan() {
    let world = RemedyWorld::at("reset-remedy-lone", "lone").with_found_machine();
    world.write("plan.rhei.md", TWO_TICKETS);
    let world = RemedyWorld { target: world.root.join("plan.rhei.md"), ..world };

    world.work("plan.2", "measure");
    let said = world.refuse("plan.1", "measure", &world.root);

    assert_offers_runnable_reset(&said, &world.home, "plan", &["plan.1", "plan.2"]);
}

/// A path that is not one shell word, and the shell is what reads it.
#[test]
fn the_reset_remedy_runs_for_a_project_path_containing_a_space() {
    let world = RemedyWorld::at("reset-remedy-space", "my projects/project").with_found_machine();
    world.write("index.panta.md", "# Panta: Remedy\n");
    world.write("alpha.rhei.md", TWO_TICKETS);
    world.write("beta.rhei.md", TWO_TICKETS);

    let said = world.refuse("alpha.1", "measure", &world.root);

    assert_offers_runnable_reset(&said, &world.home, "alpha", &["alpha.1"]);
}

/// A machine named on the run, somewhere the target does not find it on its own.
/// Without it the reset runs and cannot move the ticket back, which no exit code
/// shows, so the preview is what is held here. Relative paths on the run make the
/// command's absolute spelling part of the case.
#[test]
fn the_reset_remedy_carries_a_machine_named_on_the_run() {
    let world = RemedyWorld::at("reset-remedy-machine", "project");
    world.write("index.panta.md", "# Panta: Remedy\n");
    world.write("alpha.rhei.md", TWO_TICKETS);
    world.write("beta.rhei.md", TWO_TICKETS);
    let machines = world.dir.join("machines");
    fs::create_dir_all(&machines).expect("create the machine's directory");
    world.write_machine_at(&machines.join("states.yaml"));
    let world = RemedyWorld {
        cwd: world.dir.to_path_buf(),
        target: "project".into(),
        machine: Some("machines/states.yaml".into()),
        ..world
    };

    let said = world.refuse("alpha.1", "measure", &world.root);

    assert_offers_runnable_reset(&said, &world.home, "alpha", &["alpha.1"]);
}

/// A symlinked workspace's name is its rhei id (§FS-rhei-panta.6), so a target
/// resolved through the link names a rhei that `--rhei` is refused against.
#[cfg(unix)]
#[test]
fn the_reset_remedy_keeps_a_symlinked_workspaces_name() {
    let world = RemedyWorld::at("reset-remedy-symlink", "link/myws").with_found_machine();
    world.write("index.rhei.md", "# Rhei: Linked\n");
    world.write("tasks/01-one.md", "### Task 1: One\n**State:** open\n");
    let link = world.dir.join("link/billing");
    std::os::unix::fs::symlink("myws", &link).expect("link the workspace");
    let world = RemedyWorld { target: link, ..world };

    let said = world.refuse("billing.1", "measure", &world.root);

    assert_offers_runnable_reset(&said, &world.home, "billing", &["billing.1"]);
}

/// A workspace named `.` from inside it carries no name of its own, and a `.` in
/// the printed command would name wherever the reader stands.
#[test]
fn the_reset_remedy_runs_for_a_workspace_named_as_the_current_directory() {
    let world = RemedyWorld::at("reset-remedy-dot", "ws").with_found_machine();
    world.write("index.rhei.md", "# Rhei: Dot\n");
    world.write("tasks/01-one.md", "### Task 1: One\n**State:** open\n");
    let world = RemedyWorld { target: ".".into(), ..world };

    let said = world.refuse("ws.1", "measure", &world.root);

    assert_offers_runnable_reset(&said, &world.home, "ws", &["ws.1"]);
}

/// `--parallel` spawns a ticket at a site of its own for each kind of worker, and
/// each refuses there.
fn refuse_under_parallel(name: &str, ticket: &str, state: &str) {
    let world = RemedyWorld::at(name, "project").with_found_machine();
    world.write("index.panta.md", "# Panta: Remedy\n");
    world.write("alpha.rhei.md", TWO_TICKETS);
    world.write("beta.rhei.md", TWO_TICKETS);
    let world = RemedyWorld { parallel: true, ..world };

    let said = world.refuse(ticket, state, &world.root);

    let rhei = ticket.split('.').next().expect("a qualified id");
    assert_offers_runnable_reset(&said, &world.home, rhei, &[ticket]);
}

#[test]
fn the_reset_remedy_runs_for_a_program_under_parallel() {
    refuse_under_parallel("reset-remedy-parallel-program", "alpha.1", "measure");
}

#[test]
fn the_reset_remedy_runs_for_an_agent_under_parallel() {
    refuse_under_parallel("reset-remedy-parallel-agent", "beta.1", "work");
}
