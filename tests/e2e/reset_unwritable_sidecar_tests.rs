//! A lock sidecar that exists and will not open. The real reset stops at that lock
//! having changed nothing, so the dry run has to say the same, and name the file,
//! because the directory holding it may be writable (agent-grounds/rhei#609).

// §FS-rhei-reset.2.3 §FS-rhei-reset.4

use std::fs;
use std::os::unix::fs::PermissionsExt;

use super::reset_invalid_plan_support::*;
use super::reset_remedy_support::*;
use super::reset_unwritable_support::*;

/// The sidecar of the ledger at a Directory Workspace's root.
const LEDGER_SIDECAR: &str = "runtime.state-transitions.log.lock";

/// What the help adds where a line names a lock file. §FS-rhei-reset.4
const LOCK_FILE_HELP: &str = "lock file";

/// A finished workspace with every sidecar laid by an earlier reset.
fn finished_with_sidecars(name: &str) -> RemedyWorld {
    let world = workspace(name, &[("01-a.md", "### Task 1: A")]);
    lay_sidecars(&world);
    finish(&world, "1", &[]);
    world
}

/// `relative` at 0444, or `None` where that does not bind, as under root.
#[must_use]
fn read_only_file(world: &RemedyWorld, relative: &str) -> Option<RestoredMode> {
    let path = world.root.join(relative);
    assert!(path.exists(), "fixture: {relative} was laid");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).expect("chmod the sidecar");
    let held = RestoredMode { path: path.clone(), mode: 0o644 };
    if fs::OpenOptions::new().write(true).open(&path).is_ok() {
        eprintln!("skipped: chmod does not bind on {}", path.display());
        return None;
    }
    Some(held)
}

#[test]
fn a_plan_sidecar_that_will_not_open_refuses_the_reset_and_its_dry_run_intact() {
    let world = finished_with_sidecars("reset-unwritable-plan-sidecar");
    let Some(_sidecar) = read_only_file(&world, "tasks/01-a.md.lock") else { return };

    assert_refused_intact(&world, &[], &["ws/tasks/01-a.md.lock"]);

    let dry = world.rhei("reset", &["--dry-run"]);
    assert!(dry.stderr.contains(LOCK_FILE_HELP), "the help says so:\n{}", dry.stderr);
    assert!(!dry.stderr.contains("/ws/tasks: "), "the directory is fine:\n{}", dry.stderr);
}

#[test]
fn a_ledger_sidecar_that_will_not_open_refuses_the_reset_and_its_dry_run_intact() {
    let world = finished_with_sidecars("reset-unwritable-ledger-sidecar");
    let Some(_sidecar) = read_only_file(&world, LEDGER_SIDECAR) else { return };

    assert_refused_intact(&world, &[], &[&format!("ws/{LEDGER_SIDECAR}")]);
}

/// The natural way to get there: `chmod -R a-w` on a plan an earlier reset laid locks for.
#[test]
fn a_tree_made_read_only_with_its_sidecars_refuses_the_reset_and_its_dry_run_intact() {
    let world = finished_with_sidecars("reset-unwritable-whole-tree");
    let mut held = Vec::new();
    let mut pending = vec![world.root.clone()];
    while let Some(path) = pending.pop() {
        let mode = fs::metadata(&path).expect("stat the tree").permissions().mode();
        if path.is_dir() {
            pending.extend(fs::read_dir(&path).expect("list").flatten().map(|entry| entry.path()));
        }
        fs::set_permissions(&path, fs::Permissions::from_mode(mode & !0o222)).expect("chmod a-w");
        held.push(RestoredMode { path, mode });
    }
    if fs::OpenOptions::new().write(true).open(world.root.join("index.rhei.md.lock")).is_ok() {
        eprintln!("skipped: chmod a-w does not bind under {}", world.root.display());
        return;
    }

    assert_refused_intact(&world, &[], &["ws/index.rhei.md.lock", "ws/tasks/01-a.md.lock"]);
}

/// One refusal, one spelling: an absolute target that walks through `..` still prints
/// the directory it cannot list and the one it cannot write alike. §FS-rhei-reset.4
#[test]
fn one_refusal_prints_every_path_in_one_spelling() {
    let world = finished_with_sidecars("reset-unwritable-spelling");
    world.write("runtime/logs/task-ws.1-open.log", "the log\n");
    let logs = world.root.join("runtime/logs");
    fs::set_permissions(&logs, fs::Permissions::from_mode(0o300)).expect("chmod a-r");
    let _logs = RestoredMode { path: logs.clone(), mode: 0o755 };
    if fs::read_dir(&logs).is_ok() {
        eprintln!("skipped: chmod a-r does not bind on {}", logs.display());
        return;
    }
    let Some(_results) = read_only(&world, "runtime/results") else { return };

    let mut world = world;
    world.target = world.root.join("tasks").join("..");
    let dry = world.rhei("reset", &["--dry-run"]);

    assert!(!dry.status.success(), "it refuses:\n{}", dry.stderr);
    let named: Vec<&str> = dry.stderr.lines().filter(|line| line.contains("/ws/")).collect();
    assert!(named.iter().any(|line| line.contains("/runtime/logs: ")), "{}", dry.stderr);
    assert!(named.iter().any(|line| line.contains("/runtime/results: ")), "{}", dry.stderr);
    assert!(!dry.stderr.contains("/.."), "one canonical spelling:\n{}", dry.stderr);
}
