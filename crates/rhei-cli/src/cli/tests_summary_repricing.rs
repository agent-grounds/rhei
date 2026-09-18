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
        lock
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
        let _lock = hold_lock(root.path(), Some("abc123"));

        assert_eq!(
            selected_root_activity(root.path(), "abc123"),
            SelectedRunActivity::Active
        );
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
        let _lock = hold_lock(root.path(), Some("def456"));

        assert_eq!(
            selected_root_activity(root.path(), "abc123"),
            SelectedRunActivity::Inactive
        );
    }
}
