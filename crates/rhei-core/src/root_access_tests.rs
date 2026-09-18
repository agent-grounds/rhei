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
