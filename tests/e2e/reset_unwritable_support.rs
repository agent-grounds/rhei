//! Plans a filesystem refuses part of: a directory a reset has to write in, or to
//! remove from, made read-only. The whole tree is compared, because the defect these
//! pin is a reset that changes some files and stops (agent-grounds/rhei#609).

// §FS-rhei-reset.2.3 §FS-rhei-reset.4

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use super::reset_invalid_plan_support::*;
use super::reset_remedy_support::*;

/// What a reset the filesystem would stop says first. §FS-rhei-reset.4
pub(super) const UNWRITABLE_REFUSAL: &str =
    "Refusing to reset: it cannot write everywhere it needs to, so nothing was changed.";

pub(super) const OPEN_TASK: &str = "\n\n**State:** open\n\nDo the thing.\n";

/// A mode to put back when this leaves scope, so the test's tree can be removed. Hold
/// it after the world, so it drops first.
pub(super) struct RestoredMode {
    pub(super) path: PathBuf,
    pub(super) mode: u32,
}

impl Drop for RestoredMode {
    fn drop(&mut self) {
        let _ = fs::set_permissions(&self.path, fs::Permissions::from_mode(self.mode));
    }
}

/// `relative` under the world's root with its write permission off, or `None` where
/// that does not bind, as under root: such a run has nothing to show, so it skips.
#[must_use]
pub(super) fn read_only(world: &RemedyWorld, relative: &str) -> Option<RestoredMode> {
    let path = world.root.join(relative);
    let mode = fs::metadata(&path).expect("the directory exists").permissions().mode();
    fs::set_permissions(&path, fs::Permissions::from_mode(mode & !0o222)).expect("chmod a-w");
    let held = RestoredMode { path: path.clone(), mode };
    let probe = path.join(".chmod-probe");
    if fs::File::create(&probe).is_ok() {
        fs::remove_file(&probe).expect("remove the probe");
        eprintln!("skipped: chmod a-w does not bind on {}", path.display());
        return None;
    }
    Some(held)
}

/// A Directory Workspace named `ws`, one task per `(file, heading)` under `tasks/`.
pub(super) fn workspace(name: &str, tasks: &[(&str, &str)]) -> RemedyWorld {
    let world = RemedyWorld::at(name, "ws");
    world.write("states.yaml", MACHINE);
    world.write("index.rhei.md", "# Rhei: Reset fixture\n");
    for (file, heading) in tasks {
        world.write(&format!("tasks/{file}"), &format!("{heading}{OPEN_TASK}"));
    }
    world
}

/// Two members of one project, each a Directory Workspace of one task.
pub(super) fn project_of_workspaces(name: &str, members: &[&str]) -> RemedyWorld {
    let world = RemedyWorld::at(name, "project");
    world.write("states.yaml", MACHINE);
    world.write("index.panta.md", "# Panta: Reset fixture\n");
    for member in members {
        world.write(&format!("{member}/index.rhei.md"), &format!("# Rhei: {member}\n"));
        world.write(&format!("{member}/tasks/01-a.md"), &format!("### Task 1: A{OPEN_TASK}"));
    }
    world
}

/// Every lock sidecar a reset takes, laid the way a person's earlier reset lays them,
/// so a later read-only directory has nothing left to create.
pub(super) fn lay_sidecars(world: &RemedyWorld) {
    world.step("reset", &["--yes"]);
}

/// The paths two snapshots disagree on, so a failure says what the reset did.
pub(super) fn differing(
    before: &BTreeMap<String, Vec<u8>>,
    after: &BTreeMap<String, Vec<u8>>,
) -> Vec<String> {
    let paths = before.keys().chain(after.keys()).collect::<std::collections::BTreeSet<_>>();
    paths.into_iter().filter(|path| before.get(*path) != after.get(*path)).cloned().collect()
}

/// `rhei reset <scope> --dry-run` and `--yes` both refuse with the one refusal, before
/// any preview, naming each of `blamed` by the tail of its path, and neither changes
/// a byte of the tree. §FS-rhei-reset.2.3 §FS-rhei-reset.4
#[track_caller]
pub(super) fn assert_refused_intact(world: &RemedyWorld, scope: &[&str], blamed: &[&str]) {
    assert_validates(world);
    let before = snapshot(world);

    let dry = world.rhei("reset", &[scope, &["--dry-run"]].concat());
    let dry_changed = differing(&before, &snapshot(world));
    let yes = world.rhei("reset", &[scope, &["--yes"]].concat());
    let yes_changed = differing(&before, &snapshot(world));

    assert!(
        !dry.status.success()
            && !yes.status.success()
            && dry_changed.is_empty()
            && yes_changed.is_empty(),
        "both must refuse and change nothing.\n--dry-run exited {:?} and changed {dry_changed:?}\n\
         --yes exited {:?} and changed {yes_changed:?}\n--yes said:\n{}{}",
        dry.status.code(),
        yes.status.code(),
        yes.stdout,
        yes.stderr
    );
    for (flag, run) in [("--dry-run", &dry), ("--yes", &yes)] {
        assert!(run.stderr.contains(UNWRITABLE_REFUSAL), "{flag} says it refused:\n{}", run.stderr);
        assert!(run.stderr.contains("writable"), "{flag} has the help:\n{}", run.stderr);
        for tail in blamed {
            let names = format!("/{tail}: ");
            assert!(run.stderr.contains(&names), "{flag} names {tail}:\n{}", run.stderr);
        }
        assert!(
            !run.stdout.contains("Would"),
            "{flag} refuses before any preview:\n{}",
            run.stdout
        );
    }
    assert_eq!(dry.stderr, yes.stderr, "the dry run says what the reset says");
}

/// `rhei reset <scope>`, previewed and then run, goes through and leaves no temp file.
#[track_caller]
pub(super) fn assert_finishes(world: &RemedyWorld, scope: &[&str]) {
    assert_validates(world);
    for flag in ["--dry-run", "--yes"] {
        let run = world.rhei("reset", &[scope, &[flag]].concat());
        assert!(
            run.status.success(),
            "nothing it has to do is refused, so `rhei reset {scope:?} {flag}` goes through; \
             it exited {:?}:\n{}{}",
            run.status.code(),
            run.stdout,
            run.stderr
        );
    }
    let left = snapshot(world);
    let temps: Vec<_> = left.keys().filter(|path| path.contains(".tmp")).collect();
    assert!(temps.is_empty(), "no temp file is left: {temps:?}");
}
