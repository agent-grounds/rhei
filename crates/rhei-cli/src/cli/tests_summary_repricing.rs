// Focused completion-activity regressions for alternate-book summaries.
// §FS-rhei-summary.1 §FS-rhei-summary.5 §FS-rhei-run-report.1

mod summary_repricing_activity_tests {
    use super::super::*;

    fn running_descriptor(root: &Path, id: &str, pid: u32) -> RunDescriptor {
        RunDescriptor {
            id: id.to_string(),
            pid,
            status: RunStatus::Running,
            workspace: root.to_path_buf(),
            plan: root.join("plan.rhei.md"),
            state_machine: None,
            control_url: None,
            started_at: "2026-09-01T10:00:00Z".to_string(),
            headless: false,
            parallel: 1,
            log: None,
            events: root.join("runtime/events.jsonl"),
            exit_code: None,
        }
    }

    fn hold_lock(root: &Path, owner: Option<&str>) -> HeldRunLock {
        let mut lock = try_acquire_run_lock(root).expect("probe lock").expect("acquire lock");
        if let Some(id) = owner {
            write_run_lock_owner(&mut lock, id, std::process::id()).expect("write lock owner");
        }
        assert!(matches!(probe_run_lock(root), RunLockProbe::Held));
        lock
    }

    fn lock_body(lock: &mut HeldRunLock) -> Vec<u8> {
        lock.file.rewind().expect("rewind owner");
        let mut body = Vec::new();
        lock.file.read_to_end(&mut body).expect("read owner");
        body
    }

    fn assert_invalid_owner_is_indeterminate(edit: impl FnOnce(&mut serde_json::Value)) {
        let root = tempfile::tempdir().expect("workspace");
        let mut lock = hold_lock(root.path(), Some("def456"));
        lock.file.rewind().expect("rewind owner");
        let mut owner: serde_json::Value = serde_json::from_reader(&lock.file).expect("owner");
        edit(&mut owner);
        let body = serde_json::to_vec(&owner).expect("serialize modified owner");
        lock.file.rewind().expect("rewind owner");
        lock.file.set_len(0).expect("clear owner");
        lock.file.write_all(&body).expect("write modified owner");
        lock.file.flush().expect("flush modified owner");

        let activity = selected_root_activity(root.path(), "abc123");
        let SelectedRunActivity::Unknown(reason) = activity else {
            panic!("invalid ownership must be indeterminate, got {activity:?}");
        };
        assert!(reason.contains("run.lock"), "{reason}");
        lock.file.rewind().expect("rewind inspected owner");
        let mut after = Vec::new();
        lock.file.read_to_end(&mut after).expect("read inspected owner");
        assert_eq!(after, body, "inspection must not repair the owner record");
    }

    #[test]
    fn a_stale_nonterminal_descriptor_and_free_lock_are_inactive() {
        let root = tempfile::tempdir().expect("workspace");
        let descriptor = running_descriptor(root.path(), "abc123", u32::MAX);
        write_descriptor(&run_descriptor_path(root.path()), &descriptor).expect("descriptor");
        drop(hold_lock(root.path(), None));

        assert_eq!(
            selected_root_activity(root.path(), "abc123"),
            SelectedRunActivity::Inactive
        );
    }

    #[test]
    fn a_held_lock_with_exact_portable_ownership_is_active() {
        let root = tempfile::tempdir().expect("workspace");
        let mut lock = hold_lock(root.path(), Some("abc123"));
        let before = lock_body(&mut lock);

        assert_eq!(
            selected_root_activity(root.path(), "abc123"),
            SelectedRunActivity::Active
        );
        assert_eq!(lock_body(&mut lock), before);
    }

    #[test]
    fn a_held_lock_without_ownership_is_indeterminate() {
        let root = tempfile::tempdir().expect("workspace");
        let _lock = hold_lock(root.path(), None);

        let SelectedRunActivity::Unknown(reason) =
            selected_root_activity(root.path(), "abc123")
        else {
            panic!("an ownerless held lock must be indeterminate");
        };
        assert!(reason.contains("no run ownership record"), "{reason}");
    }

    #[test]
    fn a_lock_owned_by_another_exact_run_is_not_misattributed() {
        let root = tempfile::tempdir().expect("workspace");
        let mut lock = hold_lock(root.path(), Some("def456"));
        let before = lock_body(&mut lock);

        assert_eq!(
            selected_root_activity(root.path(), "abc123"),
            SelectedRunActivity::Inactive
        );
        assert_eq!(lock_body(&mut lock), before);
    }

    #[test]
    fn a_held_lock_with_an_unsupported_version_and_another_id_is_indeterminate() {
        assert_invalid_owner_is_indeterminate(|owner| owner["version"] = 99.into());
    }

    #[test]
    fn a_held_lock_with_malformed_ownership_and_another_id_is_indeterminate() {
        assert_invalid_owner_is_indeterminate(|owner| owner["pid"] = "invalid".into());
        assert_invalid_owner_is_indeterminate(|owner| owner["pid"] = 0.into());
        assert_invalid_owner_is_indeterminate(|owner| owner["workspace"] = 42.into());
        assert_invalid_owner_is_indeterminate(|owner| owner["id"] = " ".into());
    }

    #[test]
    fn a_held_lock_with_legacy_ownership_containing_another_id_is_indeterminate() {
        assert_invalid_owner_is_indeterminate(|owner| {
            *owner = serde_json::json!({"id": "def456"});
        });
    }

    #[test]
    fn a_held_lock_with_ownership_for_another_workspace_is_indeterminate() {
        let other = tempfile::tempdir().expect("other workspace");
        assert_invalid_owner_is_indeterminate(|owner| {
            owner["workspace"] = serde_json::json!(other.path());
        });
    }
}
