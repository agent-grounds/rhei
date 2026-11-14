//! A narrowed reset matches a declared path that carries another placeholder in
//! full, wherever `{task_id}` stands in it, and leaves a path a kept ticket's
//! longer id could stand in; a ticket it reaches stakes no claim against another
//! (agent-grounds/rhei#679).

// §FS-rhei-reset.2.1 §FS-rhei-reset.2.5 §FS-rhei-reset.2.5.3

use std::fs;

use super::reset_task_scope_support::*;
use super::supervision_tests::setup_supervision;
use super::{assert_success, run_cli};

/// A machine whose one working state declares `outputs`.
fn machine(outputs: &[&str]) -> String {
    let mut text = String::from(
        "name: full\nversion: 1\nstates:\n  pending:\n    initial: true\n    description: Open\n    outputs:\n",
    );
    for (n, path) in outputs.iter().enumerate() {
        text.push_str(&format!("      - name: out{n}\n        path: {path}\n"));
    }
    text.push_str(
        "  completed: { final: true, description: Done }\n\
         transitions:\n  - { from: pending, to: completed, description: Done }\n",
    );
    text
}

const BUILD_PLAN: &str = "# Rhei: X\n\n## Tasks\n\n### Task build: build\n**State:** pending\n\n\
                          ### Task build-docs: docs\n**State:** pending\n\n### Task lint: lint\n**State:** pending\n";

/// `x.build`, `x.build-docs` and `x.lint` under a machine declaring `outputs`, with
/// `files` laid down, each holding its own path, and `finished` of them completed.
fn build_world(name: &str, outputs: &[&str], files: &[&str], finished: &[&str]) -> ScopeWorld {
    let world = ScopeWorld::new(name);
    world.write("states.yaml", &machine(outputs));
    world.write("x.rhei.md", BUILD_PLAN);
    for file in files {
        world.write(file, &format!("{file}\n"));
    }
    world.finish("x.rhei.md", finished);
    world
}

/// `--task x.build`, after which every one of `kept` reads as it did, `gone` is gone,
/// and the plan validates.
#[track_caller]
fn reset_x_build(world: &ScopeWorld, kept: &[&str], gone: &[&str]) {
    let before: Vec<(&str, String)> = kept.iter().map(|file| (*file, world.read(file))).collect();
    let reset = world.reset("x.rhei.md", &["--task", "x.build", "--yes"]);
    assert_line(&reset, "  Task x.build: completed → pending");
    for (file, bytes) in &before {
        assert!(world.exists(file), "{file} is kept:\n{}", reset.stdout);
        assert_eq!(&world.read(file), bytes, "{file} is kept byte for byte");
    }
    for file in gone {
        assert!(!world.exists(file), "{file} goes:\n{}", reset.stdout);
    }
    let validate = world.rhei(&["validate", "x.rhei.md"]);
    assert!(validate.status.success(), "the plan still validates:\n{}", validate.stderr);
}

/// Under `runtime/supervise/{task_id}-{state}.md`, `x.build`'s path matches
/// `x.build-docs`'s brief `runtime/supervise/x.build-docs.md`, with `{state}` taking
/// `docs`. The kept ticket's claim wins, finished or pending.
#[test]
fn an_id_first_path_never_takes_a_kept_longer_ids_brief() {
    for finished in [&["x.build", "x.build-docs", "x.lint"][..], &["x.build", "x.lint"][..]] {
        let files = [
            "runtime/supervise/x.build-pending.md",
            "runtime/supervise/x.build-docs-pending.md",
            "runtime/supervise/x.lint-pending.md",
            "runtime/supervise/x.build-docs.md",
            "runtime/supervise/x.lint.md",
        ];
        let outputs = ["runtime/supervise/{task_id}-{state}.md"];
        let world = build_world("reset-id-first-brief", &outputs, &files, finished);
        reset_x_build(&world, &files[1..], &files[..1]);
    }
}

/// Two outputs in one directory, `runtime/notes/{task_id}.md` and
/// `runtime/notes/{task_id}-{state}.md`: `x.build`'s second path matches
/// `x.build-docs`'s first, which `--task x.build` leaves.
#[test]
fn two_paths_in_one_directory_never_take_a_kept_longer_ids_output() {
    for finished in [&["x.build", "x.build-docs", "x.lint"][..], &["x.build", "x.lint"][..]] {
        let files = [
            "runtime/notes/x.build.md",
            "runtime/notes/x.build-pending.md",
            "runtime/notes/x.build-docs.md",
            "runtime/notes/x.build-docs-pending.md",
            "runtime/notes/x.lint.md",
            "runtime/notes/x.lint-pending.md",
        ];
        let outputs = ["runtime/notes/{task_id}.md", "runtime/notes/{task_id}-{state}.md"];
        let world = build_world("reset-two-paths", &outputs, &files, finished);
        reset_x_build(&world, &files[2..], &files[..2]);
    }
}

/// The ci-heal summary, `runtime/fix-summary/{task_id}.{visit_count}.md`. `x.1.1`'s
/// path is `x.1.1.<visit>.md`, so `x.1.1.md`, the pending parent's first summary,
/// is never its: no id is longer than `x.1.1`, and only the declared `.md` after the
/// visit tells the two apart.
#[test]
fn the_declared_tail_keeps_a_parents_summary_from_its_childs_reset() {
    let world = ScopeWorld::new("reset-id-first-tail");
    world.write("states.yaml", &machine(&["runtime/fix-summary/{task_id}.{visit_count}.md"]));
    world.write(
        "x.rhei.md",
        "# Rhei: X\n\n---\nstructure:\n  maxLevels: 3\n---\n\n## Tasks\n\n\
         ### Task 1: parent\n**State:** pending\n\n#### Task 1.1: child\n**State:** pending\n\n\
         ### Task 2: other\n**State:** pending\n",
    );
    world.write("runtime/fix-summary/x.1.1.md", "x.1 visit 1\n");
    world.write("runtime/fix-summary/x.1.1.1.md", "x.1.1 visit 1\n");
    world.write("runtime/fix-summary/x.2.1.md", "x.2 visit 1\n");
    world.finish("x.rhei.md", &["x.1.1", "x.2"]);

    let reset = world.reset("x.rhei.md", &["--task", "x.1.1", "--yes"]);
    assert_line(&reset, "  Task x.1.1: completed → pending");
    assert_line(&reset, "Kept 2 task(s) as they stand, with their results and logs: x.1, x.2");
    assert_eq!(world.read("runtime/fix-summary/x.1.1.md"), "x.1 visit 1\n", "x.1's summary stays");
    assert_eq!(world.read("runtime/fix-summary/x.2.1.md"), "x.2 visit 1\n", "x.2's summary stays");
    assert!(!world.exists("runtime/fix-summary/x.1.1.1.md"), "x.1.1's own summary goes");
}

/// A project of two rheis at one root: `a` with `build` and `build-2`, `b` with
/// `other`, each ticket's output at `template` for visit 1, and `a.build`'s second
/// output `<dir>/a.build-2.md` beside them; every ticket finished.
fn two_builds(name: &str, template: &str, dir: &str) -> ScopeWorld {
    let world = ScopeWorld::new(name);
    world.write("states.yaml", &machine(&[template]));
    world.write("index.panta.md", "# Panta: Scope\n");
    world.write(
        "a.rhei.md",
        "# Rhei: A\n\n## Tasks\n\n### Task build: build\n**State:** pending\n\n\
         ### Task build-2: again\n**State:** pending\n",
    );
    world
        .write("b.rhei.md", "# Rhei: B\n\n## Tasks\n\n### Task other: other\n**State:** pending\n");
    for file in ["a.build-1.md", "a.build-2.md", "a.build-2-1.md", "b.other-1.md"] {
        world.write(&format!("{dir}/{file}"), &format!("{file}\n"));
    }
    world.finish(".", &["a.build", "a.build-2", "b.other"]);
    world
}

/// `a.build-2.md` reads as `a.build`'s second output and as `a.build-2`'s id.
/// `--rhei a` reaches both tickets, and neither stakes a claim against the other,
/// so it goes, as every file of rhei `a` does; `b.other`'s output stays. With
/// `--task a.build`, `a.build-2` is kept, and its claim wins.
#[test]
fn a_longer_in_scope_id_stakes_no_claim_against_a_shorter_one() {
    for (template, dir) in [
        ("runtime/{state}/{task_id}-{visit_count}.md", "runtime/pending"),
        ("runtime/findings/{task_id}-{visit_count}.md", "runtime/findings"),
    ] {
        let world = two_builds("reset-in-scope-claim", template, dir);
        let reset = world.reset(".", &["--rhei", "a", "--yes"]);
        assert!(reset.status.success(), "{template}:\n{}{}", reset.stdout, reset.stderr);
        for file in ["a.build-1.md", "a.build-2.md", "a.build-2-1.md"] {
            assert!(!world.exists(&format!("{dir}/{file}")), "{template}: {file} goes");
        }
        assert_eq!(world.read(&format!("{dir}/b.other-1.md")), "b.other-1.md\n", "{template}");

        let world = two_builds("reset-kept-claim", template, dir);
        let reset = world.reset(".", &["--task", "a.build", "--yes"]);
        assert_line(&reset, "  Task a.build: completed → pending");
        assert!(!world.exists(&format!("{dir}/a.build-1.md")), "{template}: its own output goes");
        for file in ["a.build-2.md", "a.build-2-1.md", "b.other-1.md"] {
            assert_eq!(
                world.read(&format!("{dir}/{file}")),
                format!("{file}\n"),
                "{template}: {file}"
            );
        }
    }
}

/// A supervisor over its whole subtree, `fix` and its `review` child, where `fix`
/// writes its summary at `runtime/supervise/{task_id}.{visit_count}.md` and `review`
/// declares nothing.
const SUMMARY_MACHINE: &str = "name: summary
version: 1
states:
  supervising:
    initial: true
    description: Supervise the subtree
    execute_on: descendant-terminal
    agent: mock
    visits: 12
    instructions: You supervise Task {task_id}.
  fix:
    description: Fix
    agent: mock
    outputs:
      - name: summary
        path: runtime/supervise/{task_id}.{visit_count}.md
    instructions: Fix it.
  review:
    description: Review
    agent: mock
    instructions: Review it.
  completed: { description: Done, final: true }
transitions:
  - { from: supervising, to: completed, description: Subtree done, condition: openDescendants < 1 }
  - { from: supervising, to: supervising, description: Released the subtree }
  - { from: fix, to: completed, description: Fixed }
  - { from: review, to: completed, description: Reviewed }
";

const SUMMARY_PLAN: &str = "# Rhei: Fix

---
structure:
  maxLevels: 3
---

## Tasks

### Task 1: Ticket
**State:** supervising

#### Task 1.1: Fix
**State:** fix

##### Task 1.1.1: Review
**State:** review

#### Task 1.2: Other
**State:** fix
";

/// `runtime/supervise/plan.1.1.1.md` reads as `plan.1.1`'s first summary and as a
/// file of `plan.1.1.1`, which declares none. `--task plan.1.1` reaches both and the
/// file goes, so both lose it: the released supervisor is held with a checkpoint for
/// each, the longer id's included, and none for `plan.1.2`, which keeps its summary.
/// §FS-rhei-reset.2.5.3
#[test]
fn a_file_two_reached_tickets_could_own_holds_the_supervisor_for_both() {
    let (dir, plan, machine) =
        setup_supervision("reset-full-match-losing", SUMMARY_PLAN, SUMMARY_MACHINE, "");
    let release =
        ["--task", "1", "--from", "supervising", "--to", "supervising", "--release-subtree"];
    assert_success(&run_cli("transition", &plan, &machine, &release));
    fs::create_dir_all(dir.join("runtime/supervise")).expect("the summaries");
    fs::write(dir.join("runtime/supervise/plan.1.1.1.md"), "review it\n").expect("a brief");
    fs::write(dir.join("runtime/supervise/plan.1.2.1.md"), "fixed\n").expect("a summary");

    let reset = run_cli("reset", &plan, &machine, &["--task", "plan.1.1", "--yes"]);
    assert_success(&reset);
    assert!(reset.stdout.contains("Held Task plan.1, which supervises them"), "{}", reset.stdout);
    assert!(!dir.join("runtime/supervise/plan.1.1.1.md").exists(), "the file goes");
    assert!(dir.join("runtime/supervise/plan.1.2.1.md").exists(), "a kept step keeps its summary");

    let next = run_cli("next", &plan, &machine, &["--task", "plan.1", "--peek", "--json"]);
    assert_success(&next);
    let payload: serde_json::Value = serde_json::from_str(&next.stdout).expect("json");
    let checkpoints = payload["checkpoints"].as_str().expect("a checkpoint section");
    for line in [
        "### Task plan.1.1: Fix \u{2014} fix \u{2192} fix (reset)",
        "### Task plan.1.1.1: Review \u{2014} review \u{2192} review (reset)",
    ] {
        assert!(checkpoints.contains(line), "{line}\nin: {checkpoints}");
    }
    assert!(!checkpoints.contains("Task plan.1.2"), "a kept step: {checkpoints}");
}
