//! A reset stopped after the check, on demand: a limit on the size of a file the
//! process may write, which the check's empty temp file passes. Linux only, as
//! `a_witness_write_cut_short_leaves_both_files_as_they_were` says of this limit, so
//! everything only these tests use lives here too (agent-grounds/rhei#609).

// §FS-rhei-reset.2.3 §FS-rhei-reset.4

use super::reset_invalid_plan_support::*;
use super::reset_remedy_support::*;
use super::reset_unwritable_support::*;
use super::*;

/// What a reset stopped part way adds to the error it has. §FS-rhei-reset.4
const PARTLY_RESET: &str = "The reset stopped part way, so the plan is partly reset: \
     fix the cause and run the reset again, which finishes what this one left.";

/// `rhei reset --yes` under a limit on the size of a file the process may write, which
/// the check's empty temp file passes and a write of a larger file does not. So the
/// failure the check cannot foresee can be had on demand. §FS-rhei-reset.2.3
fn reset_with_file_size_limit(world: &RemedyWorld, limit: u64) -> CliRun {
    use nix::sys::resource::{setrlimit, Resource};
    use nix::sys::signal::{signal, SigHandler, Signal};
    use std::os::unix::process::CommandExt;

    let mut command = rhei_command(&world.home);
    command.current_dir(&world.cwd);
    scrub_inherited_rhei_env(&mut command);
    command.arg("reset").arg(&world.target).arg("--yes");
    // SAFETY: both calls are single system calls, safe between fork and exec.
    unsafe {
        command.pre_exec(move || {
            signal(Signal::SIGXFSZ, SigHandler::SigIgn).map_err(std::io::Error::from)?;
            setrlimit(Resource::RLIMIT_FSIZE, limit, limit).map_err(std::io::Error::from)
        });
    }
    CliRun::from(&command.output().expect("rhei reset should start"))
}

/// Two finished tasks, one of them holding a file too large to rewrite under the limit.
fn world_with_a_large_task(name: &str, large: &str, small: &str) -> RemedyWorld {
    let world = RemedyWorld::at(name, "ws");
    world.write("states.yaml", MACHINE);
    world.write("index.rhei.md", "# Rhei: Reset fixture\n");
    let padding = "pad ".repeat(2048);
    world.write(large, &format!("### Task 1: Large{OPEN_TASK}\n{padding}\n"));
    world.write(small, &format!("### Task 2: Small{OPEN_TASK}"));
    finish(&world, "1", &[]);
    finish(&world, "2", &[]);
    world
}

/// A write that fails once the first has landed says what has become true of the
/// plan, and the second reset it offers does finish. Linux only, as
/// `a_witness_write_cut_short_leaves_both_files_as_they_were` says of this limit.
#[test]
fn a_write_failing_after_the_first_landed_says_partly_reset_and_a_second_reset_finishes() {
    let world = world_with_a_large_task(
        "reset-unwritable-partway",
        "tasks/02-large.md",
        "tasks/01-small.md",
    );

    let stopped = reset_with_file_size_limit(&world, 4096);

    assert!(!stopped.status.success(), "the limit stops it:\n{}", stopped.stderr);
    assert_eq!(listed_state(&world, "ws.2"), "open", "fixture: the small file's write landed");
    assert_eq!(listed_state(&world, "ws.1"), "done", "fixture: the large file's did not");
    assert!(stopped.stderr.contains(PARTLY_RESET), "it says so:\n{}", stopped.stderr);

    assert_success(&world.rhei("reset", &["--yes"]));

    assert_eq!(listed_state(&world, "ws.1"), "open", "the second reset finishes");
    assert!(!world.root.join("runtime").exists(), "and removes the runtime tree");
}

/// The first write failing changed nothing, so the message must not say it did.
/// Passes today, and must keep passing.
#[test]
fn a_first_write_that_fails_does_not_call_the_plan_partly_reset() {
    let world =
        world_with_a_large_task("reset-unwritable-first", "tasks/01-large.md", "tasks/02-small.md");
    let before = snapshot(&world);

    let stopped = reset_with_file_size_limit(&world, 4096);

    assert!(!stopped.status.success(), "the limit stops it:\n{}", stopped.stderr);
    assert_eq!(snapshot(&world), before, "fixture: nothing landed");
    assert!(!stopped.stderr.contains(PARTLY_RESET), "nothing was reset:\n{}", stopped.stderr);
}
