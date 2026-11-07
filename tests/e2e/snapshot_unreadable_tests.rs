//! One unreadable generation costs that generation: `rhei snapshot list` and every
//! `rhei snapshot gc` policy carry on past it, name it, leave it on disk and exit 1,
//! while every other reader of the store stays strict. §FS-rhei-snapshot-operations.1.6
#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use super::help_target_tests::Case;
use super::summary_repricing_support::hold_run_lock;
use super::*;

const PLAN: &str = "p/plan.rhei.md";
const MACHINE: &str = "p/states.yaml";
const LINEAGE: &str = "plan.9/implementation/work/1/agent";
const DAMAGED: &str = "plan.1/implementation/work/1/agent/g1";
const POLICIES: [&[&str]; 3] =
    [&["--orphaned"], &["--older-than", "0s"], &["--keep-generations", "1"]];

#[path = "snapshot_unreadable_mode_tests.rs"]
mod mode_tests;

const ONE_TASK: &str = "# Rhei: Plan\n\n## Tasks\n\n### Task 1: One\n**State:** pending\n";

/// A task waiting in `consume`, whose `latest` inherit makes the newest orchestrator
/// generation of its `source` lineage protected. §FS-rhei-snapshot-operations.1.4
const LATEST_INHERIT_TASK: &str =
    "# Rhei: Plan\n\n## Tasks\n\n### Task 1: One\n**State:** consume\n";
const LATEST_INHERIT_MACHINE: &str = r#"name: inherits
version: 1
states:
  source:
    description: Emits the snapshot
    agent: mock
    snapshot:
      emit: { name: implementation, on: always }
  consume:
    initial: true
    description: Inherits the latest generation
    agent: mock
    snapshot:
      inherit:
        name: implementation
        from: self
        select: { state: source, generation: latest }
  done:
    description: Done
    final: true
transitions:
  - { from: source, to: consume, description: Emitted }
  - { from: consume, to: done, description: Consumed }
"#;
const LATEST_LINEAGE: &str = "plan.1/implementation/source/1/agent";
const ELSEWHERE: &str = "plan.7/implementation/work/1/agent/g1";

/// The manifest failures §FS-rhei-snapshot-operations.1.6 counts as one condition, each
/// with the words its reason must carry.
#[derive(Clone, Copy)]
enum Damage {
    NotJson,
    Version2,
    NotUtf8,
}

const DAMAGE: [Damage; 3] = [Damage::NotJson, Damage::Version2, Damage::NotUtf8];

impl Damage {
    fn reason(self) -> &'static str {
        match self {
            Damage::NotJson => "key must be a string",
            Damage::Version2 => "version must be 1",
            Damage::NotUtf8 => "valid UTF-8",
        }
    }
}

struct Store {
    case: Case,
    project: PathBuf,
}

impl Store {
    fn empty(prefix: &str, plan: &str) -> Self {
        let case = Case::new(prefix);
        let project = case.root.join("p");
        fs::create_dir_all(&project).expect("create the project");
        write_fixture_file(&project, "plan.rhei.md", plan);
        Self { case, project }
    }

    /// The reproducer's store: readable `g1` and `g2` of an orphaned task's lineage.
    fn lineage(prefix: &str) -> Self {
        let store = Self::empty(prefix, ONE_TASK);
        store.readable(&format!("{LINEAGE}/g1"), 1);
        store.readable(&format!("{LINEAGE}/g2"), 2);
        store
    }

    /// That store plus the report's damaged generation.
    fn damaged(prefix: &str, damage: Damage) -> Self {
        let store = Self::lineage(prefix);
        store.damage(DAMAGED, damage);
        store
    }

    fn cache(&self) -> PathBuf {
        self.project.join(".rhei/cache/snapshots")
    }

    fn generation_dir(&self, relative: &str) -> PathBuf {
        let dir = self.cache().join(relative);
        fs::create_dir_all(&dir).expect("create the generation directory");
        dir
    }

    fn readable(&self, relative: &str, generation: u64) {
        let dir = self.generation_dir(relative);
        fs::write(dir.join("manifest.json"), manifest(relative, generation, 1).to_string())
            .expect("write the manifest");
        fs::write(dir.join("transcript.jsonl"), "").expect("write the transcript");
    }

    fn damage(&self, relative: &str, damage: Damage) {
        let path = self.generation_dir(relative).join("manifest.json");
        match damage {
            Damage::NotJson => fs::write(path, "{not json"),
            Damage::Version2 => fs::write(path, manifest(relative, 1, 2).to_string()),
            Damage::NotUtf8 => fs::write(path, [0xff, 0xfe, 0x00]),
        }
        .expect("damage the manifest");
    }

    fn point_current_at(&self, lineage: &str, generation: &str) {
        std::os::unix::fs::symlink(generation, self.cache().join(lineage).join("current"))
            .expect("write the current pointer");
    }

    fn current(&self, lineage: &str) -> PathBuf {
        fs::read_link(self.cache().join(lineage).join("current")).expect("a current pointer")
    }

    fn on_disk(&self, relative: &str) -> bool {
        self.cache().join(relative).exists()
    }

    fn list(&self, extra: &[&str]) -> Output {
        let mut args = vec!["snapshot", "list", "--plan", PLAN];
        args.extend_from_slice(extra);
        self.case.rhei(&self.case.root, &args)
    }

    fn gc(&self, extra: &[&str]) -> Output {
        let mut args = vec!["snapshot", "gc", "--plan", PLAN];
        args.extend_from_slice(extra);
        self.case.rhei(&self.case.root, &args)
    }

    fn gc_with_machine(&self, extra: &[&str]) -> Output {
        let mut args = vec!["--state-machine", MACHINE, "snapshot", "gc", "--plan", PLAN];
        args.extend_from_slice(extra);
        self.case.rhei(&self.case.root, &args)
    }

    fn latest_inherit(prefix: &str) -> Self {
        let store = Self::empty(prefix, LATEST_INHERIT_TASK);
        write_fixture_file(&store.project, "states.yaml", LATEST_INHERIT_MACHINE);
        store.readable(&format!("{LATEST_LINEAGE}/g1"), 1);
        store.readable(&format!("{LATEST_LINEAGE}/g2"), 2);
        store
    }
}

/// A manifest the store accepts at `version: 1`, with its identity read off its path.
fn manifest(relative: &str, generation: u64, version: u64) -> serde_json::Value {
    let parts: Vec<&str> = relative.split('/').collect();
    let [task, snapshot, state, visit, target, _] = parts[..] else {
        panic!("a generation path has six components: {relative}");
    };
    serde_json::json!({
        "version": version,
        "rhei_version": "0.0.0",
        "snapshot_name": snapshot,
        "task_id": task,
        "emitting_state": state,
        "visit": visit.parse::<u64>().expect("a numeric visit"),
        "generation": generation,
        "created_at": "2020-01-01T00:00:00Z",
        "completion": "success",
        "produced_by": "orchestrator",
        "declared_provider": "acme",
        "declared_model": "model-a",
        "observed_provider": "acme",
        "observed_model": "model-a",
        "session_id": "s",
        "transcript_path": "transcript.jsonl",
        "transcript_sha256": "0",
        "transcript_bytes": 0,
        "target": { "selector": target, "slug": target, "resolved": {} },
        "session_layout": { "kind": "file", "ext": "jsonl" },
        "parent_ref": null,
    })
}

#[track_caller]
fn assert_exit(out: &Output, code: i32, context: &str) {
    assert_eq!(
        out.status.code(),
        Some(code),
        "{context}\nstdout:\n{}\nstderr:\n{}",
        stdout(out),
        raw_stderr(out)
    );
}

/// The one physical stderr line naming `generation`: `<verb> <directory>: <reason>`,
/// whole, because a wrapped path cannot be pasted. Returns its byte offset.
#[track_caller]
fn assert_named(out: &Output, verb: &str, generation: &str, reason: &str) -> usize {
    let rendered = raw_stderr(out);
    let suffix = format!("{generation}: ");
    let mut named = rendered.lines().filter(|line| {
        line.starts_with(&format!("{verb} ")) && line.contains(&suffix) && line.contains(reason)
    });
    let line = named.next().unwrap_or_else(|| {
        panic!("no whole line `{verb} .../{generation}: ...{reason}...` on stderr:\n{rendered}")
    });
    assert!(named.next().is_none(), "{generation} is named once:\n{rendered}");
    rendered.find(line).expect("the line is in what it came from")
}

fn rows(out: &Output, task: &str) -> usize {
    let prefix = format!("{task}\t");
    stdout(out).lines().filter(|line| line.starts_with(&prefix)).count()
}

/// All three failures in one store: every readable row, every damaged generation named
/// with its own reason in path order, the store untouched. §FS-rhei-snapshot-operations.1.6
#[test]
fn list_shows_the_readable_rows_and_names_each_unreadable_generation() {
    let store = Store::lineage("snapshot-unreadable-list");
    let damaged: Vec<String> =
        (1..=3).map(|n| format!("plan.{n}/implementation/work/1/agent/g1")).collect();
    for (generation, damage) in damaged.iter().zip(DAMAGE) {
        store.damage(generation, damage);
    }

    let out = store.list(&[]);

    assert_exit(&out, 1, "damage stands, so the command ends in an error");
    assert!(stdout(&out).starts_with("task\tsnapshot\t"), "the header:\n{}", stdout(&out));
    assert_eq!(rows(&out, "plan.9"), 2, "every readable row:\n{}", stdout(&out));
    assert!(!stdout(&out).contains("plan.1"), "damage adds nothing to stdout:\n{}", stdout(&out));
    let offsets: Vec<usize> = damaged
        .iter()
        .zip(DAMAGE)
        .map(|(generation, damage)| assert_named(&out, "unreadable", generation, damage.reason()))
        .collect();
    assert!(offsets.is_sorted(), "path order:\n{}", raw_stderr(&out));
    assert!(damaged.iter().all(|generation| store.on_disk(generation)), "list deletes nothing");
    let rendered = stderr(&out);
    let closing: Vec<&str> =
        rendered.lines().filter(|line| !line.starts_with("unreadable ")).collect();
    assert!(closing.concat().contains('3'), "the error counts what it could not read:\n{rendered}");
    assert!(rendered.contains("generation director"), "the help says what to delete:\n{rendered}");
    assert!(!rendered.contains("rhei snapshot gc"), "no command removes them:\n{rendered}");
}

/// JSON stays one array a caller can parse, whatever the store holds.
/// §FS-rhei-snapshot-operations.1.6
#[test]
fn list_json_stays_one_array_of_the_readable_rows() {
    let store = Store::damaged("snapshot-unreadable-list-json", Damage::NotJson);

    let out = store.list(&["--format", "json"]);

    assert_exit(&out, 1, "damage stands");
    let rows: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("one JSON array");
    let tasks: Vec<&str> = rows
        .as_array()
        .expect("an array")
        .iter()
        .map(|row| row["task_id"].as_str().expect("a task id"))
        .collect();
    assert_eq!(tasks, ["plan.9", "plan.9"], "the readable rows only");
    assert_named(&out, "unreadable", DAMAGED, Damage::NotJson.reason());
}

/// A store with nothing readable keeps the empty-state output of §FS-rhei-snapshot-operations.1.2.
/// §FS-rhei-snapshot-operations.1.6
#[test]
fn list_of_only_damage_keeps_its_empty_state_output() {
    let store = Store::empty("snapshot-unreadable-list-empty", ONE_TASK);
    store.damage(DAMAGED, Damage::NotJson);

    let text = store.list(&[]);
    assert_exit(&text, 1, "damage stands");
    assert!(stdout(&text).starts_with("task\tsnapshot\t"), "the header:\n{}", stdout(&text));
    assert_named(&text, "unreadable", DAMAGED, Damage::NotJson.reason());

    let json = store.list(&["--format", "json"]);
    assert_exit(&json, 1, "damage stands");
    let rows: serde_json::Value = serde_json::from_str(&stdout(&json)).expect("one JSON array");
    assert_eq!(rows, serde_json::json!([]), "an empty array");
}

/// A filter chooses among readable rows and cannot say a damaged generation is out of
/// scope, because its manifest cannot say what it belongs to. §FS-rhei-snapshot-operations.1.6
#[test]
fn list_filter_narrows_the_rows_and_does_not_hide_damage() {
    let store = Store::damaged("snapshot-unreadable-list-filter", Damage::Version2);
    store.readable("plan.8/implementation/work/1/agent/g1", 1);

    let out = store.list(&["--task", "plan.9"]);

    assert_exit(&out, 1, "damage is not hidden by a filter");
    assert_eq!(rows(&out, "plan.9"), 2, "the filter selects these:\n{}", stdout(&out));
    assert_eq!(rows(&out, "plan.8"), 0, "and not this:\n{}", stdout(&out));
    assert_named(&out, "unreadable", DAMAGED, Damage::Version2.reason());
}

/// Each policy against each failure: it deletes the readable generations it selects,
/// skips the damaged one, leaves it on disk and exits 1. §FS-rhei-snapshot-operations.1.6
#[test]
fn each_gc_policy_deletes_what_it_selects_and_skips_the_damage() {
    for policy in POLICIES {
        for damage in DAMAGE {
            let store = Store::damaged("snapshot-unreadable-gc", damage);

            let out = store.gc(policy);

            let context = format!("gc {policy:?}");
            assert_exit(&out, 1, &context);
            assert!(!store.on_disk(&format!("{LINEAGE}/g1")), "{context} selects g1");
            let keeps_g2 = policy[0] == "--keep-generations";
            assert_eq!(store.on_disk(&format!("{LINEAGE}/g2")), keeps_g2, "{context} and g2");
            assert!(store.on_disk(DAMAGED), "{context}: gc never deletes what it cannot read");
            assert_named(&out, "skipped", DAMAGED, damage.reason());
            let printed = stdout(&out);
            assert!(
                printed.lines().all(|line| line.starts_with("deleted ")),
                "{context}: stdout keeps its grammar:\n{printed}"
            );
            assert!(printed.contains(&format!("{LINEAGE}/g1")), "{context}:\n{printed}");
            assert!(!printed.contains("plan.1/"), "{context}: not a deletion:\n{printed}");
            let rendered = stderr(&out);
            assert!(!rendered.contains("rhei snapshot gc"), "no command removes it:\n{rendered}");
        }
    }
}

/// `--dry-run` reports the same selection and the same skip, and touches nothing.
/// §FS-rhei-snapshot-operations.1.6
#[test]
fn gc_dry_run_prints_the_selection_and_skips_the_damage() {
    let store = Store::damaged("snapshot-unreadable-gc-dry-run", Damage::NotJson);

    let out = store.gc(&["--keep-generations", "1", "--dry-run"]);

    assert_exit(&out, 1, "damage stands under --dry-run too");
    let printed = stdout(&out);
    let would = format!("{LINEAGE}/g1");
    assert!(
        printed.lines().any(|line| line.starts_with("would delete ") && line.ends_with(&would)),
        "the selection:\n{printed}"
    );
    assert!(store.on_disk(&would), "a dry run deletes nothing");
    assert!(store.on_disk(DAMAGED), "a dry run deletes nothing");
    assert_named(&out, "skipped", DAMAGED, Damage::NotJson.reason());
}

/// The issue's scoped attempt: `--task` does the scoped work and still names the damage.
/// §FS-rhei-snapshot-operations.1.6
#[test]
fn gc_filter_acts_on_its_scope_and_does_not_hide_damage() {
    let store = Store::damaged("snapshot-unreadable-gc-filter", Damage::NotJson);
    store.readable("plan.8/implementation/work/1/agent/g1", 1);

    let out = store.gc(&["--task", "plan.9", "--older-than", "0s"]);

    assert_exit(&out, 1, "damage is not hidden by a filter");
    assert!(!store.on_disk(&format!("{LINEAGE}/g1")), "in scope");
    assert!(!store.on_disk(&format!("{LINEAGE}/g2")), "in scope");
    assert!(store.on_disk("plan.8/implementation/work/1/agent/g1"), "out of scope");
    assert_named(&out, "skipped", DAMAGED, Damage::NotJson.reason());
}

/// Deleting from a lineage sends gc back to the store to refresh `current`. That read
/// must get past damage in another lineage too, or gc deletes and then stops with the
/// pointer dangling. §FS-rhei-snapshot-operations.1.6
#[test]
fn gc_refreshes_current_on_a_lineage_it_deleted_from_past_damage_elsewhere() {
    let store = Store::damaged("snapshot-unreadable-gc-current", Damage::NotJson);
    store.point_current_at(LINEAGE, "g1");

    let out = store.gc(&["--keep-generations", "1"]);

    assert!(!store.on_disk(&format!("{LINEAGE}/g1")), "selected:\n{}", raw_stderr(&out));
    assert_eq!(store.current(LINEAGE), Path::new("g2"), "\n{}", raw_stderr(&out));
    assert_exit(&out, 1, "damage stands");
    assert_named(&out, "skipped", DAMAGED, Damage::NotJson.reason());
}

/// A damaged `g3` is the newest directory of a readable lineage: it holds no retention
/// slot, and `current` goes to the newest generation that can be read.
/// §FS-rhei-snapshot-operations.1.6
#[test]
fn a_damaged_newest_generation_holds_no_retention_slot() {
    let store = Store::lineage("snapshot-unreadable-gc-slot");
    let g3 = format!("{LINEAGE}/g3");
    store.damage(&g3, Damage::NotJson);
    store.point_current_at(LINEAGE, "g3");

    let keep_both = store.gc(&["--keep-generations", "2"]);
    assert_exit(&keep_both, 1, "damage stands");
    assert!(store.on_disk(&format!("{LINEAGE}/g1")), "two readable generations are kept");
    assert!(store.on_disk(&format!("{LINEAGE}/g2")), "two readable generations are kept");

    let keep_one = store.gc(&["--keep-generations", "1"]);
    assert_exit(&keep_one, 1, "damage stands");
    assert!(!store.on_disk(&format!("{LINEAGE}/g1")), "g3 holds no slot, so g1 goes");
    assert!(store.on_disk(&format!("{LINEAGE}/g2")), "g2 is the newest readable");
    assert!(store.on_disk(&g3), "gc never deletes what it cannot read");
    assert_named(&keep_one, "skipped", &g3, Damage::NotJson.reason());
    assert_eq!(store.current(LINEAGE), Path::new("g2"), "\n{}", raw_stderr(&keep_one));
}

/// The fixture the two tests below rest on: with no damage, `g1` is collectable and only
/// the `latest` `g2` is protected. §FS-rhei-snapshot-operations.1.4
#[test]
fn a_latest_inherit_protects_only_the_newest_generation_of_an_undamaged_store() {
    let store = Store::latest_inherit("snapshot-unreadable-latest-clean");

    let refused = store.gc_with_machine(&["--older-than", "0s"]);
    assert_exit(&refused, 1, "the older-than policy selects the protected g2");
    assert!(stderr(&refused).contains("active snapshot.inherit"), "{}", stderr(&refused));
    assert!(stderr(&refused).contains("/g2"), "{}", stderr(&refused));

    let kept = store.gc_with_machine(&["--keep-generations", "1"]);
    assert_exit(&kept, 0, "g1 is older than the latest generation");
    assert!(!store.on_disk(&format!("{LATEST_LINEAGE}/g1")), "selected and not protected");
    assert!(store.on_disk(&format!("{LATEST_LINEAGE}/g2")), "retained");
}

/// Damage elsewhere must not make every generation read as `latest`: a strict read under
/// `latest_orchestrator_generation` falls back to the record's own generation, so gc would
/// refuse the collectable `g1` as protected. §FS-rhei-snapshot-operations.1.6
#[test]
fn a_latest_inherit_with_damage_elsewhere_does_not_protect_older_generations() {
    let store = Store::latest_inherit("snapshot-unreadable-latest-elsewhere");
    store.damage(ELSEWHERE, Damage::NotJson);

    let out = store.gc_with_machine(&["--keep-generations", "1"]);

    assert!(!stderr(&out).contains("active snapshot.inherit"), "{}", stderr(&out));
    assert!(!store.on_disk(&format!("{LATEST_LINEAGE}/g1")), "{}", raw_stderr(&out));
    assert!(store.on_disk(&format!("{LATEST_LINEAGE}/g2")), "retained");
    assert_exit(&out, 1, "damage stands");
    assert_named(&out, "skipped", ELSEWHERE, Damage::NotJson.reason());
}

/// The interlock still comes first: a selection holding the `latest` generation is refused
/// whole, as it is with no damage. §FS-rhei-snapshot-operations.1.6
#[test]
fn the_inherit_interlock_still_refuses_a_protected_selection_past_damage() {
    let store = Store::latest_inherit("snapshot-unreadable-latest-refused");
    store.damage(ELSEWHERE, Damage::NotJson);

    let out = store.gc_with_machine(&["--older-than", "0s"]);

    assert_exit(&out, 1, "the interlock refuses");
    assert!(stderr(&out).contains("active snapshot.inherit"), "{}", stderr(&out));
    assert!(store.on_disk(&format!("{LATEST_LINEAGE}/g1")), "a refusal deletes nothing");
    assert!(store.on_disk(&format!("{LATEST_LINEAGE}/g2")), "a refusal deletes nothing");
}

/// The refusals ahead of the read are as they were, and hold the order against a change
/// that moves the scan up. §FS-rhei-snapshot-operations.1.6
#[test]
fn the_refusals_ahead_of_the_scan_are_unchanged_by_damage() {
    let store = Store::damaged("snapshot-unreadable-gc-refusals", Damage::NotJson);

    let no_policy = store.gc(&[]);
    assert_exit(&no_policy, 1, "no policy");
    assert!(stderr(&no_policy).contains("requires a deletion policy"), "{}", stderr(&no_policy));

    let keep_none = store.gc(&["--keep-generations", "0"]);
    assert_exit(&keep_none, 1, "--keep-generations 0");
    assert!(stderr(&keep_none).contains("must be at least 1"), "{}", stderr(&keep_none));

    let lock = hold_run_lock(&store.project, "snapshot-unreadable-other-run");
    let held = store.gc(&["--orphaned"]);
    drop(lock);
    assert_exit(&held, 1, "a held run lock");
    assert!(stderr(&held).contains(".rhei/run.lock is held"), "{}", stderr(&held));

    for refusal in [&no_policy, &keep_none, &held] {
        assert!(!raw_stderr(refusal).contains("plan.1/"), "{}", raw_stderr(refusal));
    }
    assert!(store.on_disk(&format!("{LINEAGE}/g1")), "a refusal deletes nothing");
    assert!(store.on_disk(DAMAGED), "a refusal deletes nothing");
}

/// With no damage the two commands print what they always did and nothing on stderr.
/// §FS-rhei-snapshot-operations.1.6
#[test]
fn a_store_with_no_damage_behaves_as_before() {
    let store = Store::lineage("snapshot-unreadable-clean");

    let list = store.list(&[]);
    assert_exit(&list, 0, "no damage");
    assert_eq!(rows(&list, "plan.9"), 2, "{}", stdout(&list));
    assert_eq!(raw_stderr(&list), "", "nothing added to stderr");

    let gc = store.gc(&["--keep-generations", "1"]);
    assert_exit(&gc, 0, "no damage");
    assert_eq!(raw_stderr(&gc), "", "nothing added to stderr");
    assert_eq!(stdout(&gc).lines().count(), 1, "{}", stdout(&gc));
    assert!(stdout(&gc).starts_with("deleted "), "{}", stdout(&gc));
    assert!(!store.on_disk(&format!("{LINEAGE}/g1")), "selected");
}

/// `show` stays strict, even for a reference to a readable generation: it resolves through
/// the same store, and tolerating damage there is not this rule. §FS-rhei-snapshot-operations.1.6
#[test]
fn snapshot_show_still_refuses_on_an_unreadable_generation() {
    let store = Store::damaged("snapshot-unreadable-show", Damage::NotJson);

    let out = store
        .case
        .rhei(&store.case.root, &["snapshot", "show", "plan.9:implementation", "--plan", PLAN]);

    assert_exit(&out, 1, "show stays strict");
    assert!(stderr(&out).contains("failed to parse snapshot manifest"), "{}", stderr(&out));
}

/// `rhei validate` stays strict. §FS-rhei-snapshot-operations.1.6
#[test]
fn validate_still_refuses_on_an_unreadable_generation() {
    let store = Store::damaged("snapshot-unreadable-validate", Damage::NotJson);

    let out = store.case.rhei(&store.case.root, &["validate", PLAN]);

    assert_exit(&out, 1, "validate stays strict");
    assert!(stderr(&out).contains("failed to parse snapshot manifest"), "{}", stderr(&out));
}

/// A `current` pointer on damage selects only that generation, so the interlock protects
/// nothing readable in the lineage, and gc deletes what its policy selects.
/// §FS-rhei-snapshot-operations.1.6
#[test]
fn a_current_inherit_on_a_damaged_generation_protects_nothing_readable() {
    let store = Store::empty("snapshot-unreadable-current-inherit", LATEST_INHERIT_TASK);
    let machine = LATEST_INHERIT_MACHINE.replace("generation: latest", "generation: current");
    write_fixture_file(&store.project, "states.yaml", &machine);
    store.readable(&format!("{LATEST_LINEAGE}/g1"), 1);
    store.readable(&format!("{LATEST_LINEAGE}/g2"), 2);
    let g3 = format!("{LATEST_LINEAGE}/g3");
    store.damage(&g3, Damage::NotJson);
    store.point_current_at(LATEST_LINEAGE, "g3");

    let out = store.gc_with_machine(&["--older-than", "0s"]);

    assert_exit(&out, 1, "damage stands");
    assert!(!stderr(&out).contains("active snapshot.inherit"), "{}", stderr(&out));
    assert!(!store.on_disk(&format!("{LATEST_LINEAGE}/g1")), "{}", raw_stderr(&out));
    assert!(!store.on_disk(&format!("{LATEST_LINEAGE}/g2")), "{}", raw_stderr(&out));
    assert!(store.on_disk(&g3), "gc never deletes what it cannot read");
    assert_named(&out, "skipped", &g3, Damage::NotJson.reason());
}
