//! First-visit reuse must explain the skip, including when another state
//! produced the shared output and unrelated work really spawns.
// §FS-rhei-agent-visit-pairing.6 §FS-rhei-reset.5.1 §FS-rhei-agents.3.2

use super::agent_first_visit_reuse_support::*;
use super::agent_reentry_support::*;
use super::reset_remedy_support::assert_offers_runnable_reset;
use super::*;
use std::fs;
use std::path::Path;

fn first_visit_notes(run: &CliRun) -> Vec<&str> {
    run.stdout
        .lines()
        .filter(|line| line.starts_with("note:") && line.contains(": reusing first-visit outputs"))
        .collect()
}

#[track_caller]
fn assert_reuse_note(run: &CliRun, task: &str, state: &str, paths: &[std::path::PathBuf]) {
    let notes = first_visit_notes(run);
    let prefix = format!("note: task {task} state '{state}':");
    let own = notes.iter().filter(|line| line.starts_with(&prefix)).collect::<Vec<_>>();
    assert_eq!(
        own.len(), 1,
        "missing or duplicate first-visit reuse announcement for {task} in {state}; \
         expected one note naming resolved outputs, no agent spawned, and redo remedy.\nstdout:\n{}\nstderr:\n{}",
        run.stdout, run.stderr
    );
    let note = own[0];
    for path in paths {
        assert!(note.contains(&path.display().to_string()), "missing {} in {note}", path.display());
    }
    assert!(
        note.contains("no agent spawned"),
        "the note must explain the absence of a spawn: {note}"
    );
    assert!(!note.contains("finished spawn"), "no spawn record proves this reuse: {note}");
}

fn shared_output_case(sibling_writer: bool, parallel: &str) {
    let label = if sibling_writer { "sibling" } else { "same-ticket" };
    let fixture = Revision::new(&format!("{label}-{parallel}"), sibling_writer);
    let initial = fixture.run(parallel);
    assert!(first_visit_notes(&initial).is_empty(), "the writer really spawns");
    let writer = if sibling_writer { "design.writer" } else { "design.design" };
    assert_eq!(fixture.spawns(), [format!("{writer} verdict")]);
    if sibling_writer {
        // Give reset a distinct authored state to return to. §FS-rhei-reset.2.2
        fixture.transition("design.design", "prepare", "approve-design");
    }
    fixture.transition("design.design", "approve-design", "revise");
    fixture.transition("design.other", "hold", "work");

    let judged = fixture.run(parallel);
    // Establish the reported behavior before asserting its missing explanation.
    assert_eq!(fixture.spawns(), [format!("{writer} verdict"), "design.other work".into()]);
    assert_eq!(fs::read_to_string(fixture.owner.join(OUTPUTS[0])).unwrap(), "# Design v1\n");
    assert_eq!(
        fs::read_to_string(fixture.owner.join(OUTPUTS[1])).unwrap(),
        "{\"version\":\"v1\"}\n"
    );
    assert_task_state(&fixture.project, &fixture.machine, "design.design", "approve-design");
    assert_task_state(&fixture.project, &fixture.machine, "design.other", "completed");
    let paths = OUTPUTS.iter().map(|path| fixture.owner.join(path)).collect::<Vec<_>>();
    assert_reuse_note(&judged, "design.design", "revise", &paths);
    assert_eq!(first_visit_notes(&judged).len(), 1, "only revise reused first-visit work");
    assert!(!first_visit_notes(&judged)[0].contains("neighbour"));
    assert_offers_runnable_reset(
        first_visit_notes(&judged)[0],
        &fixture.dir.join(".home"),
        "design",
        &["design.design", "design.other"],
    );
    // The issue's narrower workaround still starts a fresh revision on re-entry.
    fixture.transition("design.design", "approve-design", "revise");
    let reentry = fixture.run(parallel);
    assert!(first_visit_notes(&reentry).is_empty(), "a re-entry actually spawns");
    assert_eq!(fixture.spawns().last().unwrap(), "design.design revise");
    assert_eq!(fs::read_to_string(fixture.owner.join(OUTPUTS[0])).unwrap(), "# Design v2\n");
}

#[test]
fn first_visit_reuse_from_the_same_ticket_is_announced_in_a_mixed_run() {
    for parallel in ["3", "1"] {
        shared_output_case(false, parallel);
    }
}

#[test]
fn first_visit_reuse_from_a_sibling_ticket_is_announced_in_a_mixed_run() {
    for parallel in ["3", "1"] {
        shared_output_case(true, parallel);
    }
}

#[test]
fn first_visit_reuse_of_a_seed_written_before_arrival_is_announced() {
    let machine_text = BASIC_MACHINE
        .replace(
            "  work:\n    initial: true\n",
            "  prepare:\n    initial: true\n    description: Prepare\n    gating: true\n  work:\n",
        )
        .replace("transitions:\n", "transitions:\n  - { from: prepare, to: work }\n");
    let (dir, plan, machine) = setup("first-visit-reuse-preseed", &machine_text, COUNTING_AGENT);
    fs::write(&plan, PLAN.replace("**State:** work", "**State:** prepare")).unwrap();
    seed_output(&dir, "runtime/digest.md");
    assert_success(&run_transition(&plan, &machine, "1", "prepare", "work"));
    let run = run_cli("run", &plan, &machine, ARGS);
    assert_success(&run);
    assert!(spawn_lines(&dir).is_empty());
    assert_eq!(
        fs::read_to_string(dir.join("runtime/digest.md")).unwrap(),
        "seeded before this visit\n"
    );
    assert_task_state(&plan, &machine, "1", "verifying");
    assert_reuse_note(&run, "plan.1", "work", &[dir.join("runtime/digest.md")]);
    assert_offers_runnable_reset(
        first_visit_notes(&run)[0],
        &dir.join(".home"),
        "plan",
        &["plan.1"],
    );
}

fn assert_no_first_visit_note(run: &CliRun, root: &Path, expected_spawns: usize) {
    assert_success(run);
    assert_eq!(spawn_lines(root).len(), expected_spawns);
    assert!(first_visit_notes(run).is_empty(), "no first-visit skip occurred:\n{}", run.stdout);
}

// These boundary controls already pass: they guard against broadening the new
// announcement to any existing file or any completed invocation.
#[test]
fn first_visit_reuse_is_not_claimed_when_required_work_is_missing() {
    let (dir, plan, machine) =
        setup("first-visit-reuse-missing-output", BASIC_MACHINE, COUNTING_AGENT);
    let run = run_cli("run", &plan, &machine, ARGS);
    assert_no_first_visit_note(&run, &dir, 1);

    let terminal = BASIC_MACHINE
        .replace("    gating: true", "    final: true")
        .replace("  - { from: verifying, to: work, description: Revise it }\n", "");
    let agent = format!("{COUNTING_AGENT}\nresult('terminal result from current invocation\\n')\n");
    let (dir, plan, machine) = setup("first-visit-reuse-missing-result", &terminal, &agent);
    seed_output(&dir, "runtime/digest.md");
    let run = run_cli("run", &plan, &machine, ARGS);
    assert_no_first_visit_note(&run, &dir, 1);
    assert!(fs::read_to_string(dir.join("runtime/results/plan.1.md"))
        .unwrap()
        .contains("current invocation"));
}

#[test]
fn first_visit_reuse_is_distinct_from_own_record_and_orphan_pairing() {
    for orphan in [false, true] {
        let (dir, plan, machine) = setup("first-visit-reuse-record", BASIC_MACHINE, COUNTING_AGENT);
        seed_output(&dir, "runtime/digest.md");
        write_spawn_record(&dir, orphan.then_some("old-target"), 0, "exited", Some(0));
        let run = run_cli("run", &plan, &machine, ARGS);
        assert_no_first_visit_note(&run, &dir, 0);
        assert_task_state(&plan, &machine, "1", "verifying");
        assert_eq!(
            run.stdout.matches("reusing this visit's finished spawn").count(),
            usize::from(orphan)
        );
    }
}

#[test]
fn first_visit_reuse_is_not_claimed_on_a_recordless_reentry() {
    let (dir, plan, machine) = setup("first-visit-reuse-reentry", BASIC_MACHINE, COUNTING_AGENT);
    seed_output(&dir, "runtime/digest.md");
    fs::write(
        dir.join("runtime/state-transitions.log"),
        "plan.1 work@verifying\nplan.1 verifying@work\n",
    )
    .unwrap();
    let run = run_cli("run", &plan, &machine, ARGS);
    assert_no_first_visit_note(&run, &dir, 1);
    assert_eq!(
        fs::read_to_string(dir.join("runtime/digest.md")).unwrap(),
        "written by current invocation\n"
    );
}
