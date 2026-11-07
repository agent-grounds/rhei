//! A generation directory that cannot be listed is an unreadable generation, and a directory
//! above one refuses. Stores here lose a permission bit, so those tests skip under root.
//! The reason a line prints stays on that line. §FS-rhei-snapshot-operations.1.6

use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use super::*;

/// A mode taken off a directory, put back on drop so the temp directory can be removed,
/// on a failed assertion too. Declare it after the store it locks.
struct Locked(PathBuf);

impl Locked {
    /// `None` when the mode binds nothing: root ignores it, so the caller's test has
    /// nothing to show and skips.
    fn new(dir: PathBuf, mode: u32) -> Option<Self> {
        fs::set_permissions(&dir, fs::Permissions::from_mode(mode)).expect("lock the directory");
        let locked = Self(dir);
        let denied = fs::metadata(locked.0.join("probe"))
            .is_err_and(|err| err.kind() == ErrorKind::PermissionDenied);
        if !denied {
            eprintln!("skipped: this user is not held by mode {mode:o}");
        }
        denied.then_some(locked)
    }
}

impl Drop for Locked {
    fn drop(&mut self) {
        let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
    }
}

/// Whichever permission bit a generation lost, it costs that generation and nothing else.
#[track_caller]
fn assert_a_locked_generation_costs_only_itself(prefix: &str, mode: u32, reason: &str) {
    let store = Store::lineage(prefix);
    store.readable(DAMAGED, 1);
    let Some(_locked) = Locked::new(store.cache().join(DAMAGED), mode) else {
        return;
    };

    let list = store.list(&[]);
    assert_exit(&list, 1, "damage stands");
    assert_eq!(rows(&list, "plan.9"), 2, "every readable row:\n{}", stdout(&list));
    assert_named(&list, "unreadable", DAMAGED, reason);

    let gc = store.gc(&["--keep-generations", "1"]);
    assert_exit(&gc, 1, "damage stands");
    assert!(!store.on_disk(&format!("{LINEAGE}/g1")), "selected:\n{}", raw_stderr(&gc));
    assert!(store.on_disk(&format!("{LINEAGE}/g2")), "retained");
    assert!(store.on_disk(DAMAGED), "gc never deletes what it cannot read");
    assert_named(&gc, "skipped", DAMAGED, reason);
}

/// §FS-rhei-snapshot-operations.1.6
#[test]
fn a_generation_directory_that_cannot_be_listed_costs_only_itself() {
    assert_a_locked_generation_costs_only_itself(
        "snapshot-unreadable-mode-000",
        0o000,
        "failed to read generation directory: Permission denied",
    );
}

/// §FS-rhei-snapshot-operations.1.6
#[test]
fn a_generation_directory_that_can_be_listed_and_not_entered_costs_only_itself() {
    assert_a_locked_generation_costs_only_itself(
        "snapshot-unreadable-mode-444",
        0o444,
        "failed to read snapshot manifest: Permission denied",
    );
}

/// A directory above a generation may hold any number of them, so it refuses, and the
/// error sends the operator to it rather than to the cache root. §FS-rhei-snapshot-operations.1.6
#[test]
fn a_directory_above_a_generation_refuses_and_names_that_directory() {
    let store = Store::lineage("snapshot-unreadable-mode-above");
    store.readable(DAMAGED, 1);
    let above = store.cache().join("plan.1/implementation/work/1/agent");
    let Some(_locked) = Locked::new(above.clone(), 0o000) else {
        return;
    };
    let named = above.display().to_string();

    for out in [store.list(&[]), store.gc(&["--older-than", "0s"])] {
        assert_exit(&out, 1, "a walk failure refuses");
        let rendered = stderr(&out);
        assert!(rendered.contains("failed to read snapshot cache"), "{rendered}");
        assert_eq!(rendered.matches(&named).count(), 2, "the message and the help:\n{rendered}");
        assert_eq!(stdout(&out), "", "a refusal prints nothing");
    }
    assert!(store.on_disk(&format!("{LINEAGE}/g1")), "a refusal deletes nothing");
    assert!(store.on_disk(&format!("{LINEAGE}/g2")), "a refusal deletes nothing");
}

/// `show` and `validate` stay strict, and their refusal names the generation.
/// §FS-rhei-snapshot-operations.1.6
#[test]
fn strict_readers_refuse_on_a_generation_that_cannot_be_listed() {
    let store = Store::lineage("snapshot-unreadable-mode-strict");
    store.readable(DAMAGED, 1);
    let Some(_locked) = Locked::new(store.cache().join(DAMAGED), 0o000) else {
        return;
    };
    let show: &[&str] = &["snapshot", "show", "plan.9:implementation", "--plan", PLAN];
    let validate: &[&str] = &["validate", PLAN];

    for args in [show, validate] {
        let out = store.case.rhei(&store.case.root, args);
        assert_exit(&out, 1, "stays strict");
        let rendered = stderr(&out);
        assert!(rendered.contains("failed to read generation directory"), "{rendered}");
        assert!(rendered.contains(DAMAGED), "{rendered}");
    }
}

/// Generations the walk could not list and manifests that failed share one path order.
/// §FS-rhei-snapshot-operations.1.6
#[test]
fn every_kind_of_unreadable_generation_is_named_in_one_path_order() {
    let store = Store::lineage("snapshot-unreadable-mode-order");
    let [g1, g10, g2] =
        ["g1", "g10", "g2"].map(|g| format!("plan.1/implementation/work/1/agent/{g}"));
    store.damage(&g1, Damage::NotJson);
    store.damage(&g2, Damage::Version2);
    let Some(_locked) = Locked::new(store.generation_dir(&g10), 0o000) else {
        return;
    };

    let out = store.list(&[]);

    assert_exit(&out, 1, "damage stands");
    let offsets = [
        assert_named(&out, "unreadable", &g1, Damage::NotJson.reason()),
        assert_named(&out, "unreadable", &g10, "failed to read generation directory"),
        assert_named(&out, "unreadable", &g2, Damage::Version2.reason()),
    ];
    assert!(offsets.is_sorted(), "path order:\n{}", raw_stderr(&out));
}

/// A manifest's own value reaches the reason, and must not break the line or forge another.
/// §FS-rhei-snapshot-operations.1.6
#[test]
fn a_manifest_value_cannot_forge_a_line_or_reach_the_terminal() {
    let store = Store::lineage("snapshot-unreadable-reason");
    let mut forged = manifest(DAMAGED, 1, 1);
    forged["completion"] = "é\nb\nunreadable /fake/path: spoofed\u{1b}[31m".into();
    let dir = store.generation_dir(DAMAGED);
    fs::write(dir.join("manifest.json"), forged.to_string()).expect("write the manifest");

    for (out, verb) in [(store.list(&[]), "unreadable"), (store.gc(&["--orphaned"]), "skipped")] {
        assert_exit(&out, 1, "damage stands");
        assert_named(
            &out,
            verb,
            DAMAGED,
            r"completion 'é\nb\nunreadable /fake/path: spoofed\u{1b}[31m'",
        );
        let rendered = raw_stderr(&out);
        let named = rendered.lines().filter(|line| line.starts_with(&format!("{verb} ")));
        assert_eq!(named.count(), 1, "one physical line per generation:\n{rendered}");
        assert!(!rendered.contains('\u{1b}'), "{rendered:?}");
    }
}
