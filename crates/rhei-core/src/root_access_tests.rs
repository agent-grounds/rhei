//! Deterministic OS-lock and root-loader boundaries. §FS-rhei-panta.6.6

use super::*;
use std::sync::mpsc;
use std::time::Duration;

/// An already-active reader delays exclusive publication until its operation ends. §FS-rhei-recover.4
#[test]
fn operator_root_guard_excludes_an_active_reader() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let held = RootAccessGuard::shared(&root).unwrap();
    let (contended_tx, contended_rx) = mpsc::channel();
    let (acquired_tx, acquired_rx) = mpsc::channel();
    let writer = std::thread::spawn(move || {
        CONTENDED.with(|sender| *sender.borrow_mut() = Some(contended_tx));
        let _guard = RootAccessGuard::exclusive(&root).unwrap();
        acquired_tx.send(()).unwrap();
    });
    contended_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(acquired_rx.try_recv().is_err());
    drop(held);
    acquired_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    writer.join().unwrap();
}

/// A reader queued before publication rechecks the marker after acquisition. §FS-rhei-recover.4
#[test]
fn operator_queued_reader_refuses_newly_published_marker() {
    let dir = tempfile::tempdir().unwrap();
    let held = RootAccessGuard::exclusive(dir.path()).unwrap();
    let root = dir.path().to_path_buf();
    let (tx, rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        CONTENDED.with(|sender| *sender.borrow_mut() = Some(tx));
        RootAccessGuard::shared(&root).unwrap_err().to_string()
    });
    rx.recv_timeout(Duration::from_secs(10)).unwrap();
    fs::create_dir_all(dir.path().join(".rhei")).unwrap();
    fs::write(dir.path().join(MARKER), r#"{"hop":{"task_id":"a.1","from":"gate","to":"work"}}"#)
        .unwrap();
    drop(held);
    let error = reader.join().unwrap();
    assert!(error.contains("a.1 gate -> work"));
    assert_eq!(error.matches("rhei recover ").count(), 1);
}

/// Lenience, discovery, explicit projects and direct members use the same refusal. §FS-rhei-panta.6.6
#[test]
fn operator_pending_member_blocks_every_core_loader_class() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("index.panta.md"), "# Panta: Project\n").unwrap();
    let member = dir.path().join("member");
    fs::create_dir_all(member.join(".rhei")).unwrap();
    fs::write(member.join("index.rhei.md"), "# Rhei: Member\n").unwrap();
    fs::write(member.join(MARKER), r#"{"hop":{"task_id":"member.1","from":"gate","to":"work"}}"#)
        .unwrap();
    let errors = [
        crate::workspace::load_panta_project(dir.path()).unwrap_err().message,
        crate::workspace::load_panta_project_lenient(dir.path()).unwrap_err().message,
        crate::workspace::load_workspace(&member).unwrap_err().message,
        crate::workspace::load_implicit_panta(&member).unwrap_err().message,
        crate::workspace::discover_rhei_entries(dir.path()).unwrap_err().message,
        crate::source::read_to_string(&member.join("index.rhei.md")).unwrap_err().to_string(),
    ];
    for error in errors {
        assert!(error.contains("member.1 gate -> work"), "{error}");
    }
}

/// A reverse input ordering cannot invert the canonical lock order. §FS-rhei-panta.6.6
#[test]
fn operator_multiple_roots_are_sorted_and_reader_guards_are_external() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    fs::create_dir(&a).unwrap();
    fs::create_dir(&b).unwrap();
    let guards = shared_roots([b.clone(), a.clone(), b.clone()]).unwrap();
    assert_eq!(guards.len(), 2);
    assert_eq!(fs::read_dir(&a).unwrap().count(), 0);
    assert_eq!(fs::read_dir(&b).unwrap().count(), 0);
    assert!(!lock_path(&a).unwrap().starts_with(&a));
    drop(guards);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&a, fs::Permissions::from_mode(0o555)).unwrap();
        let result = RootAccessGuard::shared(&a);
        fs::set_permissions(&a, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(result.is_ok());
    }
}

/// A basin marker published while a reader queues is visible from every related entry. §FS-rhei-recover.4
#[test]
fn operator_basin_queued_readers_recheck_project_manifest_and_member_access() {
    for entry in ["", "index.panta.md", "basin", "basin/work.md", "member", "member/index.rhei.md"]
    {
        let dir = tempfile::tempdir().unwrap();
        let project = crate::platform::canonical_path(dir.path()).unwrap();
        fs::create_dir(project.join("basin")).unwrap();
        fs::create_dir(project.join("member")).unwrap();
        fs::write(project.join("index.panta.md"), "# Panta: Project\n").unwrap();
        fs::write(project.join("member/index.rhei.md"), "# Rhei: Member\n").unwrap();
        fs::write(project.join("basin/work.md"), "### Task 1: Work\n**State:** gate\n").unwrap();
        let roots = input_roots(&project.join(entry)).unwrap();
        assert_eq!(roots, [project.clone(), project.join("basin"), project.join("member")]);
        let holds =
            roots.iter().map(|root| RootAccessGuard::exclusive(root).unwrap()).collect::<Vec<_>>();
        let input = project.join(entry);
        let (tx, rx) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            CONTENDED.with(|sender| *sender.borrow_mut() = Some(tx));
            for_input(&input).unwrap_err().to_string()
        });
        rx.recv_timeout(Duration::from_secs(10)).unwrap();
        fs::create_dir(project.join("basin/.rhei")).unwrap();
        fs::write(
            project.join("basin").join(MARKER),
            r#"{"hop":{"task_id":"basin.1","from":"gate","to":"work"}}"#,
        )
        .unwrap();
        drop(holds);
        let error = reader.join().unwrap();
        assert!(error.contains("basin.1 gate -> work"), "{entry}: {error}");
        assert!(error.contains(&project.join("basin").display().to_string()));
        assert_eq!(error.matches("rhei recover ").count(), 1);
    }
}

/// Direct files retain both basin and project guards without creating project files. §FS-rhei-panta.6.6
#[test]
fn operator_basin_file_access_retains_all_owners_on_read_only_project() {
    let dir = tempfile::tempdir().unwrap();
    let project = crate::platform::canonical_path(dir.path()).unwrap();
    let basin = project.join("basin");
    fs::create_dir(&basin).unwrap();
    fs::write(project.join("index.panta.md"), "# Panta: Project\n").unwrap();
    let task = basin.join("task.md");
    fs::write(&task, "### Task 1: Work\n**State:** gate\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&project, fs::Permissions::from_mode(0o555)).unwrap();
        fs::set_permissions(&basin, fs::Permissions::from_mode(0o555)).unwrap();
    }
    let access = for_file(&task);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&project, fs::Permissions::from_mode(0o755)).unwrap();
        fs::set_permissions(&basin, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let guards = access.unwrap();
    assert_eq!(guards.len(), 2);
    for root in [&project, &basin] {
        assert!(RootAccessGuard::try_exclusive(root).unwrap().is_none());
    }
    assert!(!project.join(".rhei").exists());
    assert!(!basin.join(".rhei").exists());
    drop(guards);
    assert!(RootAccessGuard::try_exclusive(&project).unwrap().is_some());
    assert!(RootAccessGuard::try_exclusive(&basin).unwrap().is_some());
}

/// Give this thread its own account state directory for root guards, so a case
/// can decide whether the account can hold a lock without touching the
/// process-wide environment every other test reads. §FS-rhei-recover.4.1
fn lock_base(base: Option<&Path>) {
    LOCK_BASE.with(|slot| *slot.borrow_mut() = base.map(Path::to_path_buf));
}

/// An account state directory that cannot hold a guard directory anywhere: its
/// parent is a regular file, so locating the lock succeeds and creating it
/// cannot. The predicate is the failure, never an error code. §REQ-cross-platform
fn unwritable_base(dir: &Path) -> PathBuf {
    let file = dir.join("not-a-directory");
    fs::write(&file, b"").unwrap();
    file.join("state")
}

fn execution_root(dir: &Path, name: &str) -> PathBuf {
    let root = dir.join(name);
    fs::create_dir_all(&root).unwrap();
    crate::platform::canonical_path(&root).unwrap()
}

/// A shared acquisition proceeds where the lock cannot be created, and creates
/// nothing in the account it could not write. §FS-rhei-recover.4.1
#[test]
fn operator_shared_access_degrades_where_the_lock_cannot_be_created() {
    let dir = tempfile::tempdir().unwrap();
    let root = execution_root(dir.path(), "root");
    let base = unwritable_base(dir.path());
    lock_base(Some(&base));
    let acquired = RootAccessGuard::shared(&root);
    lock_base(None);
    let guard = acquired.expect("a shared acquisition proceeds without the lock");
    assert!(guard.0.file.is_none(), "the hold carries no OS lock");
    assert!(!base.exists(), "{} was never created", base.display());
}

/// The marker check needs nothing from the account, so the safety property the
/// lock stood in for survives the degradation. §FS-rhei-recover.4.1
#[test]
fn operator_degraded_shared_access_still_refuses_a_marked_root() {
    let dir = tempfile::tempdir().unwrap();
    let root = execution_root(dir.path(), "root");
    fs::create_dir_all(root.join(".rhei")).unwrap();
    fs::write(root.join(MARKER), r#"{"hop":{"task_id":"a.1","from":"gate","to":"work"}}"#).unwrap();
    lock_base(Some(&unwritable_base(dir.path())));
    let refused = RootAccessGuard::shared(&root);
    lock_base(None);
    let error = refused.unwrap_err().to_string();
    assert!(error.contains("forced recovery pending: a.1 gate -> work"), "{error}");
}

/// A hold is uniformly unguarded: nested loads share it, and it still reads as
/// an active reader rather than as an absent one. §FS-rhei-recover.4.1
#[test]
fn operator_an_unguarded_hold_still_reads_as_a_reader() {
    let dir = tempfile::tempdir().unwrap();
    let root = execution_root(dir.path(), "root");
    lock_base(Some(&unwritable_base(dir.path())));
    let outer = RootAccessGuard::shared(&root).expect("first degraded hold");
    let inner = RootAccessGuard::shared(&root).expect("a nested load reuses the hold");
    let refusal = RootAccessGuard::exclusive(&root).unwrap_err().to_string();
    let probe = RootAccessGuard::try_exclusive(&root);
    lock_base(None);
    assert!(Arc::ptr_eq(&outer.0, &inner.0), "every nested load gets the same hold");
    assert_eq!(refusal, "release shared root access before exclusive recovery");
    assert!(probe.unwrap().is_none(), "an unguarded reader is reported, never stepped over");
}

/// Nothing caches the degradation: once the last hold drops, an account that has
/// become writable is locked for real again. §FS-rhei-recover.4.1
#[test]
fn operator_a_writable_account_heals_the_next_acquisition() {
    let dir = tempfile::tempdir().unwrap();
    let root = execution_root(dir.path(), "root");
    lock_base(Some(&unwritable_base(dir.path())));
    let degraded = RootAccessGuard::shared(&root).expect("degraded hold");
    assert!(degraded.0.file.is_none());
    drop(degraded);

    let base = dir.path().join("writable");
    fs::create_dir(&base).unwrap();
    lock_base(Some(&base));
    let healed = RootAccessGuard::shared(&root).expect("a writable account locks again");
    assert!(healed.0.file.is_some(), "the next acquisition tries the lock again");
    assert!(guard_lock(&base, &root).is_file(), "a real lock file exists");

    let observer = {
        let (root, base) = (root.clone(), base.clone());
        std::thread::spawn(move || {
            lock_base(Some(&base));
            let probe = RootAccessGuard::try_exclusive(&root);
            lock_base(None);
            probe.unwrap().is_none()
        })
    };
    let contended = observer.join().unwrap();
    lock_base(None);
    assert!(contended, "a healed hold excludes another process's exclusive probe");
}

/// Exclusive acquisition refuses, and the refusal names the lock, the root, the
/// error and the lever instead of a bare `os error`.
/// §FS-rhei-recover.4.1 §FS-rhei-errors.1.5
#[test]
fn operator_exclusive_refusal_names_the_lock_the_root_and_the_lever() {
    let dir = tempfile::tempdir().unwrap();
    let root = execution_root(dir.path(), "root");
    let base = unwritable_base(dir.path());
    lock_base(Some(&base));
    let refusal = RootAccessGuard::exclusive(&root).unwrap_err().to_string();
    let probe = RootAccessGuard::try_exclusive(&root);
    lock_base(None);
    for subject in [
        guard_lock(&base, &root).display().to_string(),
        root.display().to_string(),
        "XDG_STATE_HOME".to_owned(),
    ] {
        assert!(refusal.contains(&subject), "the refusal should name {subject}:\n{refusal}");
    }
    // A lock that cannot be created is not a lock somebody else holds.
    assert!(probe.is_err(), "a nonblocking probe returns the failure, never Ok(None)");
}

/// The two overrides the warning case installs, put back from a drop: every
/// assertion between them can panic, and a warning sink left behind is
/// process-wide, so it would swallow the rest of the binary's warnings into a
/// vector nobody reads. §FS-rhei-recover.4.1
struct Overrides;

impl Drop for Overrides {
    fn drop(&mut self) {
        lock_base(None);
        set_warning_sink(None);
    }
}

/// One line per canonical root per process, and none at all where the account
/// holds the lock. §FS-rhei-recover.4.1
#[test]
fn operator_degradation_warns_once_per_root() {
    let dir = tempfile::tempdir().unwrap();
    let (first, second) = (execution_root(dir.path(), "a"), execution_root(dir.path(), "b"));
    let quiet = execution_root(dir.path(), "c");
    let writable = dir.path().join("writable");
    fs::create_dir(&writable).unwrap();

    let captured = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let sink = Arc::clone(&captured);
    let _overrides = Overrides;
    set_warning_sink(Some(Box::new(move |text| {
        sink.lock().unwrap().push(text.to_owned());
    })));
    lock_base(Some(&unwritable_base(dir.path())));
    for root in [&first, &first, &second] {
        drop(RootAccessGuard::shared(root).expect("degraded hold"));
    }
    lock_base(Some(&writable));
    drop(RootAccessGuard::shared(&quiet).expect("guarded hold"));

    // Other cases share the sink while it is installed, so count by root.
    let lines = captured.lock().unwrap().clone();
    let about = |root: &Path| {
        lines.iter().filter(|line| line.contains(&root.display().to_string())).collect::<Vec<_>>()
    };
    assert_eq!(about(&first).len(), 1, "two acquisitions of one root warn once: {lines:?}");
    assert_eq!(about(&second).len(), 1, "a second root warns on its own: {lines:?}");
    assert_eq!(about(&quiet).len(), 0, "a lock that is taken is silent: {lines:?}");
    let warning = about(&first)[0];
    assert!(warning.starts_with("warning: cannot create root guard lock "), "{warning}");
    assert!(warning.contains("proceeding without exclusion"), "{warning}");
    assert!(warning.contains("XDG_STATE_HOME"), "{warning}");
}

/// A signal is not a verdict about the filesystem: an interrupted open is tried
/// again rather than degraded. §FS-rhei-recover.5
#[test]
fn operator_an_interrupted_lock_open_is_retried_rather_than_degraded() {
    let dir = tempfile::tempdir().unwrap();
    let root = execution_root(dir.path(), "root");
    let base = dir.path().join("state");
    lock_base(Some(&base));
    INTERRUPTS.with(|left| left.set(1));
    let acquired = RootAccessGuard::shared(&root);
    let remaining = INTERRUPTS.with(std::cell::Cell::get);
    INTERRUPTS.with(|left| left.set(0));
    lock_base(None);
    let guard = acquired.expect("an interrupted open is retried, not degraded");
    assert_eq!(remaining, 0, "the injected interruption was consumed");
    assert!(guard.0.file.is_some(), "the retry produced a real lock");
    assert!(guard_lock(&base, &root).is_file());
}
