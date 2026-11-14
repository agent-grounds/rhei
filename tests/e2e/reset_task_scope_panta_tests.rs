//! `rhei reset --task` across the rheis of a panta: a dependent in another rhei,
//! where `--rhei` and a member target bound id resolution and not the reach,
//! the records a ticket scope cannot attribute, and a closure that grows while
//! the reset takes its locks (agent-grounds/rhei#679).

// §FS-rhei-reset.2.5 §FS-rhei-reset.2.5.1 §FS-rhei-reset.2.5.4 §FS-rhei-reset.2.5.5
// §FS-rhei-reset.4 §FS-rhei-panta.6.4 §FS-rhei-evidence.10.1

use std::fs;
use std::time::{Duration, Instant};

use super::reset_task_scope_support::*;
use super::CliRun;

const RHEI_A: &str = "# Rhei: A\n\n## Tasks\n\n### Task 1: one\n**State:** pending\n\n\
                      ### Task 2: two\n**State:** pending\n";
const RHEI_B: &str = "# Rhei: B\n\n## Tasks\n\n### Task 1: after a\n**State:** pending\n";

/// A panta of `a` (two tickets) and `b` (one), every ticket finished, where
/// `b.1` depends on `a.2` when `cross` is set.
fn panta(name: &str, cross: bool) -> ScopeWorld {
    let world = ScopeWorld::new(name);
    world.write("index.panta.md", "# Panta: Scope\n");
    world.write("a.rhei.md", RHEI_A);
    let b = if cross { format!("{RHEI_B}**Prior:** a.2\n") } else { RHEI_B.to_string() };
    world.write("b.rhei.md", &b);
    world.finish(".", &["a.1", "a.2", "b.1"]);
    world
}

/// A cross-rhei dependent joins the scope, and a record keyed by a bare local
/// id at the shared root is kept and reported, as is the reset event's reach.
#[test]
fn a_cross_rhei_dependent_joins_the_scope_and_bare_id_records_are_kept() {
    let world = panta("reset-task-cross-rhei", true);
    world.write("runtime/results/2.md", "a record from before ids were qualified\n");
    fs::create_dir_all(world.path("runtime/evidence")).expect("an evidence store to keep");

    let reset = world.reset(".", &["--task", "a.2", "--yes"]);
    assert_line(
        &reset,
        "Scope: `rhei reset` narrowed to 2 ticket(s) that --task reaches, in 2 rheis: a, b",
    );
    assert_line(&reset, "  Task a.2: completed → pending");
    assert_line(&reset, "  Task b.1: completed → pending   (prior: a.2)");
    assert_line(&reset, "Kept 1 task(s) as they stand, with their results and logs: a.1");
    assert!(
        reset.stdout.contains("Kept records keyed by bare local ids at "),
        "the bare-id records are reported:\n{}",
        reset.stdout
    );
    assert!(world.exists("runtime/results/2.md"), "the bare-id record is kept");
    assert!(world.exists("runtime/results/a.1.md"), "a.1's result is kept");
    assert!(!world.exists("runtime/results/b.1.md"), "b.1's result goes");

    let events = fs::read_dir(world.path("runtime/evidence/events")).expect("a reset event");
    let event = events.map(|entry| entry.expect("an entry").path()).next().expect("one event");
    let event: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(event).expect("read")).expect("json");
    assert_eq!(event["rheis"], serde_json::json!(["a", "b"]), "{event}");
    assert_eq!(event["tickets"], serde_json::json!(["a.2", "b.1"]), "{event}");
}

/// The usual form inside a panta: a member target resolves `2` in `a` and the
/// closure still reaches `b`, which the preview says.
#[test]
fn a_member_rhei_target_resolves_ids_in_its_rhei_and_reaches_past_it() {
    let world = panta("reset-task-member-target", true);

    for (target, rhei) in [("a.rhei.md", &[][..]), (".", &["--rhei", "a"][..])] {
        let mut args = rhei.to_vec();
        args.extend(["--task", "2", "--dry-run"]);
        let preview = world.reset(target, &args);
        assert_line(&preview, "  Task a.2: completed → pending");
        assert_line(&preview, "  Task b.1: completed → pending   (prior: a.2)");
        assert_line(
            &preview,
            "The tickets --task reaches leave rhei a, where their ids were resolved: \
             1 ticket(s) in b.",
        );
    }
}

/// An unknown id and an ambiguous shorthand are refused before any preview with
/// nothing changed, and a ticket named twice is listed once.
#[test]
fn ids_are_resolved_before_the_preview_and_a_duplicate_is_listed_once() {
    let world = panta("reset-task-ids", false);
    let before = (world.read("a.rhei.md"), world.read("b.rhei.md"));

    assert_refused(&world.reset(".", &["--task", "a.9", "--yes"]), "a.9");
    let ambiguous = world.reset(".", &["--task", "1", "--dry-run"]);
    assert_refused(&ambiguous, "a.1");
    assert_refused(&ambiguous, "b.1");
    assert_eq!((world.read("a.rhei.md"), world.read("b.rhei.md")), before, "nothing changed");
    assert!(world.exists("runtime/results/a.1.md"), "nothing deleted");

    let twice = world.reset(".", &["--task", "a.2", "--task", "a.2", "--dry-run"]);
    assert_line(&twice, "Would move 1 task(s) back:");
    assert_eq!(twice.stdout.matches("Task a.2:").count(), 1, "listed once:\n{}", twice.stdout);
}

/// The kept list stops at five ids and counts the rest.
#[test]
fn the_kept_line_names_five_tickets_and_counts_the_rest() {
    let world = ScopeWorld::new("reset-task-kept-cap");
    let mut plan = String::from("# Rhei: Many\n\n## Tasks\n");
    let mut tickets = Vec::new();
    for n in 1..=8 {
        plan.push_str(&format!("\n### Task {n}: ticket {n}\n**State:** pending\n"));
        tickets.push(format!("z.{n}"));
    }
    world.write("z.rhei.md", &plan);
    let tickets: Vec<&str> = tickets.iter().map(String::as_str).collect();
    world.finish("z.rhei.md", &tickets);

    let preview = world.reset("z.rhei.md", &["--task", "z.1", "--dry-run"]);
    assert_line(
        &preview,
        "Kept 7 task(s) as they stand, with their results and logs: \
         z.2, z.3, z.4, z.5, z.6 and 2 more",
    );
}

/// A dependent added between the reset's first read and its locks puts a file
/// the reset does not hold into the closure: it refuses, and nothing changes.
///
/// Race-free: the test holds the ledger lock, which the reset takes last, and
/// adds the dependent only once the reset has created the sidecar of a plan it
/// locks first — that is, after its first read.
#[test]
fn a_closure_that_grew_while_the_reset_took_its_locks_is_refused() {
    let world = panta("reset-task-lock-race", false);
    for entry in fs::read_dir(&world.root).expect("list the root") {
        let path = entry.expect("an entry").path();
        if path.extension().is_some_and(|ext| ext == "lock") {
            fs::remove_file(path).expect("drop a sidecar the fixture left");
        }
    }
    let ledger = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(world.path("runtime.state-transitions.log.lock"))
        .expect("open the ledger lock");
    fs2::FileExt::lock_exclusive(&ledger).expect("hold the ledger lock");

    let mut reset = world.spawn_reset(".", &["--task", "a.2", "--yes"]);
    let deadline = Instant::now() + Duration::from_secs(60);
    while !world.exists("a.rhei.md.lock") {
        if let Some(status) = reset.try_wait().expect("poll the reset") {
            let output = CliRun::from(&reset.wait_with_output().expect("its output"));
            panic!(
                "the reset ended ({status}) before it locked a.rhei.md:\n{}{}",
                output.stdout, output.stderr
            );
        }
        if Instant::now() > deadline {
            let _ = reset.kill();
            panic!("the reset never reached a.rhei.md's lock");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let b = world.read("b.rhei.md").replacen(
        "**State:** completed\n",
        "**State:** completed\n**Prior:** a.2\n",
        1,
    );
    world.write("b.rhei.md", &b);
    let a = world.read("a.rhei.md");
    drop(ledger);

    let refused = CliRun::from(&reset.wait_with_output().expect("the reset ends"));
    assert_refused(&refused, "changed while the reset took its locks");
    assert_eq!((world.read("a.rhei.md"), world.read("b.rhei.md")), (a, b), "nothing changed");
    for ticket in ["a.1", "a.2", "b.1"] {
        assert!(world.exists(&format!("runtime/results/{ticket}.md")), "{ticket}'s result");
    }
}
