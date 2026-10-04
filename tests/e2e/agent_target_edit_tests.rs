//! An agent state's target edited in place between two runs, after that
//! visit's work finished but before the ticket moved: the finished work counts
//! whether or not the new target's file-name slug equals the old one's, a
//! current fan-out sibling never answers for another, and an ambiguous edit is
//! warned about and spawned.

// §FS-rhei-agents.3.2 §FS-rhei-agents.8.4

use std::fs;
use std::path::PathBuf;

use super::agent_reentry_support::{ledger, spawn_lines, write_settings, PLAN};
use super::*;

/// Logs one line per spawn naming the target it ran for, and writes the one
/// output every identity shares, so a first entry's pre-populated-work rule
/// would see it on disk whichever identity wrote it.
const TARGET_AGENT: &str = r#"root = pathlib.Path(env('RHEI_ROOT'))
append(root / 'runtime' / 'spawn-count.log', os.environ.get('RHEI_TARGET', '?') + '\n')
write(root / 'runtime' / 'digest.md', 'written by an invocation\n')
"#;

/// [`TARGET_AGENT`], except that the invocation for `mock:mock:d` exits 1.
const D_FAILS_AGENT: &str = r#"root = pathlib.Path(env('RHEI_ROOT'))
target = os.environ.get('RHEI_TARGET', '?')
append(root / 'runtime' / 'spawn-count.log', target + '\n')
write(root / 'runtime' / 'digest.md', 'written by an invocation\n')
if target == 'mock:mock:d':
    sys.exit(1)
"#;

const NOTE_PREFIX: &str = "note: task plan.1 state 'work': reusing this visit's finished spawn (";

/// `binding` is the state's `target:` or `all_targets:` line.
fn machine(binding: &str) -> String {
    format!(
        r#"name: agent-target-edit
version: 1
states:
  work:
    initial: true
    description: Produce the digest
    {binding}
    agent_timeout: 10s
    outputs:
      - name: digest
        path: runtime/digest.md
  verifying:
    description: Inspect the digest
    gating: true
  cancelled:
    description: Stop
    final: true
transitions:
  - {{ from: work, to: verifying, description: Digest ready }}
  - {{ from: verifying, to: work, description: Revise it }}
  - {{ from: "*", to: cancelled, description: Stop }}
"#
    )
}

#[derive(Clone, Copy, PartialEq)]
enum Entry {
    First,
    Reentry,
}

/// A plan whose `work` visit finished under `before` and whose run then died
/// before the ticket moved: the plan file and the ledger are put back as they
/// were before that run, while its spawn records, logs and output stay.
struct Finished {
    dir: TestDir,
    plan: PathBuf,
    machine: PathBuf,
    spawns_before_edit: usize,
}

fn finished_then_died(name: &str, before: &str, entry: Entry) -> Finished {
    let dir = unique_temp_dir(name);
    let plan = write_fixture_file(&dir, "plan.rhei.md", PLAN);
    let machine = write_fixture_file(&dir, "states.yaml", &machine(before));
    let agent = write_python_agent(&dir, "mock-agent.py", TARGET_AGENT);
    write_settings(&dir, &agent, false);
    let args = ["--no-tui", "--no-callbacks"];
    if entry == Entry::Reentry {
        assert_success(&run_cli("run", &plan, &machine, &args));
        assert_success(&run_transition(&plan, &machine, "1", "verifying", "work"));
    }
    let plan_before = fs::read_to_string(&plan).expect("plan before the dying run");
    let ledger_before = ledger(&dir);
    let spawned_earlier = spawn_lines(&dir).len();

    assert_success(&run_cli("run", &plan, &machine, &args));
    let spawns_before_edit = spawn_lines(&dir).len();
    assert!(spawns_before_edit > spawned_earlier, "the run under {before} must spawn");

    fs::write(&plan, plan_before).expect("restore the plan: the move was never applied");
    fs::write(dir.join("runtime/state-transitions.log"), ledger_before)
        .expect("restore the ledger: the move was never recorded");
    Finished { dir, plan, machine, spawns_before_edit }
}

impl Finished {
    /// The operator's in-place edit, then the restarted run: what it printed
    /// and the targets it spawned.
    fn edit_and_run(&self, after: &str) -> (CliRun, Vec<String>) {
        fs::write(&self.machine, machine(after)).expect("edit the target in place");
        let run = run_cli("run", &self.plan, &self.machine, &["--no-tui", "--no-callbacks"]);
        assert_success(&run);
        let spawned = spawn_lines(&self.dir).split_off(self.spawns_before_edit);
        (run, spawned)
    }

    fn record(&self, slug: &str) -> PathBuf {
        self.dir.join("runtime/spawns").join(format!("task-plan.1-work-{slug}.json"))
    }
}

fn reuse_notes(run: &CliRun) -> Vec<&str> {
    run.stdout.lines().filter(|line| line.contains("reusing this visit's finished spawn")).collect()
}

/// Why the run did not announce pairing `record` with `target`, if it did not.
fn missing_reuse_note(run: &CliRun, record: &str, target: &str) -> Option<String> {
    let suffix = format!(" for {target}; `rhei reset` redoes it");
    let announced = reuse_notes(run).iter().any(|line| {
        line.starts_with(NOTE_PREFIX)
            && line.contains(&format!("{record})"))
            && line.ends_with(&suffix)
    });
    (!announced).then(|| {
        format!(
            "the pairing must be announced on stdout as `{NOTE_PREFIX}…{record}){suffix}`; \
             stdout:\n{}\nstderr:\n{}",
            run.stdout, run.stderr
        )
    })
}

fn target(selector: &str) -> String {
    format!("target: \"{selector}\"")
}

// §FS-rhei-agents.3.2: the slug-collision edit and the different-slug edit both keep the work.
#[test]
fn an_in_place_target_edit_keeps_the_visits_finished_work_whatever_the_slug() {
    let old = target("mock:mock:m/x");
    let old_record = "task-plan.1-work-mock-mock-m-x.json";
    let mut failures = Vec::new();
    for (entry, entry_label) in [(Entry::First, "first"), (Entry::Reentry, "reentry")] {
        for (new, slug_label) in
            [("mock:mock:m-x", "same-slug"), ("mock:mock:m-y", "different-slug")]
        {
            let cell = format!("{slug_label} edit m/x -> {new} at the {entry_label} entry");
            let fixture = finished_then_died(
                &format!("agent-target-edit-{slug_label}-{entry_label}"),
                &old,
                entry,
            );
            assert!(fixture.record("mock-mock-m-x").exists(), "{cell}: m/x left its record");
            let (run, spawned) = fixture.edit_and_run(&target(new));
            if !spawned.is_empty() {
                failures.push(format!("{cell}: spawned {spawned:?}, expected a skip"));
                continue;
            }
            if slug_label == "different-slug" {
                if let Some(missing) = missing_reuse_note(&run, old_record, new) {
                    failures.push(format!("{cell}: skipped, but {missing}"));
                }
            } else if !reuse_notes(&run).is_empty() {
                failures.push(format!(
                    "{cell}: the record is the invocation's own, so nothing is paired; stdout:\n{}",
                    run.stdout
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "finished work must survive an in-place target edit in all four cells:\n{}",
        failures.join("\n")
    );
}

// §FS-rhei-agents.3.2: one edited fan-out member pairs with its orphan like a single target.
#[test]
fn editing_one_fanout_member_pairs_its_orphan_and_spawns_nothing() {
    let fixture = finished_then_died(
        "agent-target-edit-fanout-one",
        r#"all_targets: ["mock:mock:a", "mock:mock:m/x"]"#,
        Entry::Reentry,
    );
    let (run, spawned) = fixture.edit_and_run(r#"all_targets: ["mock:mock:a", "mock:mock:m-y"]"#);
    assert!(
        spawned.is_empty(),
        "a's own record and m/x's orphan cover the visit; spawned={spawned:?}\nstdout:\n{}",
        run.stdout
    );
    if let Some(missing) =
        missing_reuse_note(&run, "task-plan.1-work-mock-mock-m-x.json", "mock:mock:m-y")
    {
        panic!("fan-out: {missing}");
    }
    assert_eq!(reuse_notes(&run).len(), 1, "only the edited member is paired:\n{}", run.stdout);
}

// §FS-rhei-agents.8.4: two orphans and two recordless invocations pair nothing, and say so.
#[test]
fn an_ambiguous_fanout_edit_warns_and_spawns_every_member() {
    let fixture = finished_then_died(
        "agent-target-edit-fanout-ambiguous",
        r#"all_targets: ["mock:mock:a", "mock:mock:b"]"#,
        Entry::First,
    );
    let (run, mut spawned) = fixture.edit_and_run(r#"all_targets: ["mock:mock:c", "mock:mock:d"]"#);
    spawned.sort();
    assert_eq!(
        spawned,
        ["mock:mock:c", "mock:mock:d"],
        "an ambiguous edit must not guess, and the state's records, not the digest on disk, \
         decide this first entry; stdout:\n{}\nstderr:\n{}",
        run.stdout,
        run.stderr
    );
    let warning =
        run.stderr.lines().filter(|line| line.contains("warning")).collect::<Vec<_>>().join("\n");
    for orphan in ["task-plan.1-work-mock-mock-a.json", "task-plan.1-work-mock-mock-b.json"] {
        assert!(
            warning.contains(orphan),
            "the warning on stderr must name orphan {orphan}; stderr:\n{}",
            run.stderr
        );
    }
    assert!(
        warning.contains("mock:mock:c, mock:mock:d have no spawn record of their own"),
        "the warning on stderr must name what the new targets lack; stderr:\n{}",
        run.stderr
    );
    assert!(reuse_notes(&run).is_empty(), "nothing is paired:\n{}", run.stdout);
}

// §FS-rhei-agents.3.2: an invocation whose own record of this visit failed is not recordless.
#[test]
fn an_orphan_never_answers_for_an_invocation_whose_own_spawn_failed() {
    let fixture = finished_then_died(
        "agent-target-edit-own-failed",
        r#"all_targets: ["mock:mock:x"]"#,
        Entry::Reentry,
    );
    let agent = write_python_agent(&fixture.dir, "mock-agent.py", D_FAILS_AGENT);
    write_settings(&fixture.dir, &agent, false);
    fs::write(&fixture.machine, machine(r#"all_targets: ["mock:mock:c", "mock:mock:d"]"#))
        .expect("edit the target in place");
    let args = ["--no-tui", "--no-callbacks"];
    let first = run_cli("run", &fixture.plan, &fixture.machine, &args);
    let mut spawned = spawn_lines(&fixture.dir).split_off(fixture.spawns_before_edit);
    spawned.sort();
    spawned.dedup();
    assert_eq!(spawned, ["mock:mock:c", "mock:mock:d"], "an ambiguous edit spawns both");
    assert!(first.stderr.contains("task-plan.1-work-mock-mock-x.json"), "{}", first.stderr);
    let state = || fs::read_to_string(&fixture.plan).expect("plan");
    assert!(state().contains("**State:** work"), "d failed, so the ticket stays in work");

    let before_second = spawn_lines(&fixture.dir).len();
    let second = run_cli("run", &fixture.plan, &fixture.machine, &args);
    let respawned = spawn_lines(&fixture.dir).split_off(before_second);
    assert!(
        respawned.iter().any(|target| target == "mock:mock:d"),
        "d's own failed record decides, so d spawns again; spawned={respawned:?}\nstdout:\n{}",
        second.stdout
    );
    assert!(reuse_notes(&second).is_empty(), "x must not answer for d:\n{}", second.stdout);
    assert!(state().contains("**State:** work"), "x's work must not advance the ticket");
}

// §FS-rhei-agents.8.4: a current sibling's record is never an orphan. A guard: it passes today.
#[test]
fn a_current_siblings_record_never_excuses_another_sibling() {
    let fanout = r#"all_targets: ["mock:mock:a", "mock:mock:b"]"#;
    let fixture = finished_then_died("agent-target-edit-sibling", fanout, Entry::Reentry);
    fs::remove_file(fixture.record("mock-mock-b")).expect("only a finished this visit");
    let (run, spawned) = fixture.edit_and_run(fanout);
    assert_eq!(spawned, ["mock:mock:b"], "a's record is a's own; b must spawn:\n{}", run.stdout);
    assert!(reuse_notes(&run).is_empty(), "nothing is paired:\n{}", run.stdout);
}
