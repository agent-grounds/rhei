//! Symlinks in what a reset removes. A removal takes a link and leaves what it points
//! at, so neither the check nor the removal may follow one, and a link the removal
//! skips is nothing to refuse over (agent-grounds/rhei#609).

// §FS-rhei-reset.2 §FS-rhei-reset.2.1 §FS-rhei-reset.2.3

use std::fs;
use std::os::unix::fs::symlink;
use std::process::Stdio;
use std::time::{Duration, Instant};

use super::reset_invalid_plan_support::*;
use super::reset_remedy_support::*;
use super::reset_unwritable_support::*;
use super::*;

/// Far past what a reset of these fixtures takes, and short of what a walk round a
/// link loop eats: at this point the test fails instead of holding the suite.
const PATIENCE: Duration = Duration::from_secs(30);

/// `rhei reset <flag>` in `world`, killed and failed if it outlasts [`PATIENCE`].
#[track_caller]
fn bounded_reset(world: &RemedyWorld, flag: &str) -> std::process::ExitStatus {
    let mut command = rhei_command(&world.home);
    command.current_dir(&world.cwd);
    scrub_inherited_rhei_env(&mut command);
    command.arg("reset").arg(&world.target).arg(flag);
    command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    let mut child = command.spawn().expect("rhei reset should start");
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().expect("wait for rhei reset") {
            return status;
        }
        if started.elapsed() > PATIENCE {
            let _ = child.kill();
            let _ = child.wait();
            panic!("`rhei reset {flag}` was still running after {PATIENCE:?}");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Links that lead back up the tree, at two depths: a walk that follows them never ends.
#[test]
fn a_full_reset_over_links_that_loop_finishes_and_removes_the_tree() {
    let world = workspace("reset-unwritable-loop", &[("01-a.md", "### Task 1: A")]);
    finish(&world, "1", &[]);
    fs::create_dir(world.root.join("runtime/d")).expect("a directory for the deep link");
    symlink("..", world.root.join("runtime/l1")).expect("link up");
    symlink(".", world.root.join("runtime/l2")).expect("link to itself");
    symlink("../..", world.root.join("runtime/d/up")).expect("link up twice");

    assert!(bounded_reset(&world, "--dry-run").success(), "the dry run goes through");
    assert!(bounded_reset(&world, "--yes").success(), "the reset goes through");

    assert_eq!(listed_state(&world, "ws.1"), "open", "the task goes back");
    assert!(!world.root.join("runtime").exists(), "the tree is gone");
}

/// A link out of the tree takes the link and leaves what it points at.
#[test]
fn a_full_reset_over_a_link_out_of_the_tree_leaves_what_it_points_at() {
    let world = workspace("reset-unwritable-outside", &[("01-a.md", "### Task 1: A")]);
    finish(&world, "1", &[]);
    let outside = world.dir.join("outside");
    fs::create_dir_all(outside.join("nested")).expect("a tree outside");
    fs::write(outside.join("nested/kept.txt"), "kept\n").expect("a file outside");
    symlink(&outside, world.root.join("runtime/out")).expect("link out");

    assert!(bounded_reset(&world, "--dry-run").success(), "the dry run goes through");
    assert!(bounded_reset(&world, "--yes").success(), "the reset goes through");

    assert!(!world.root.join("runtime").exists(), "the tree is gone");
    let kept = fs::read_to_string(outside.join("nested/kept.txt")).expect("the outside file");
    assert_eq!(kept, "kept\n", "what the link pointed at is intact");
}

/// A narrowed reset leaves a dangling link where it is, so a read-only directory holding
/// one is nothing it has to remove from. Passes at base, and must keep passing.
#[test]
fn a_dangling_link_a_narrowed_reset_skips_does_not_refuse_it() {
    let world = workspace("reset-unwritable-dangling", &[("01-a.md", "### Task 1: A")]);
    finish(&world, "1", &[]);
    fs::create_dir(world.root.join("runtime/logs")).expect("the logs directory");
    let link = world.root.join("runtime/logs/task-ws.1-open.log");
    symlink("/nonexistent/rhei-609", &link).expect("a dangling link");
    let Some(_logs) = read_only(&world, "runtime/logs") else { return };

    // `assert_finishes` compares the tree, and a dangling link cannot be read.
    for flag in ["--dry-run", "--yes"] {
        let run = world.rhei("reset", &["--rhei", "ws", flag]);
        assert!(run.status.success(), "{flag} goes through:\n{}{}", run.stdout, run.stderr);
    }

    assert_eq!(listed_state(&world, "ws.1"), "open", "the named rhei goes back");
    assert!(fs::symlink_metadata(&link).is_ok(), "the link is left in place");
}
