// Registry-only unknowns do not weaken confirmed-marker exclusion. §FS-rhei-run-headless.3

fn operator_registry_descriptor(root: &Path) -> RunDescriptor {
    RunDescriptor {
        id: "registry-fixture".into(),
        pid: std::process::id(),
        status: RunStatus::Running,
        workspace: root.to_path_buf(),
        plan: root.join("plan.rhei.md"),
        state_machine: None,
        control_url: None,
        started_at: "2026-09-18T00:00:00Z".into(),
        headless: false,
        parallel: 1,
        log: None,
        events: root.join("runtime/events.jsonl"),
        exit_code: None,
    }
}

/// Corrupt and unreadable established markers both refuse without pruning or root changes. §FS-rhei-recover.4
#[test]
fn operator_registry_established_markers_always_refuse() {
    for corrupt in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        operator_init_pending(dir.path());
        let marker = dir.path().join(rhei_core::root_access::MARKER);
        if corrupt {
            fs::write(&marker, "corrupt marker").unwrap();
        }
        let run = operator_registry_descriptor(dir.path());
        let entry = dir.path().join("external-registry.json");
        fs::write(&entry, serde_json::to_vec(&run).unwrap()).unwrap();
        let before = operator_snapshot(dir.path());
        for pruning in [Pruning::Keep, Pruning::Prune] {
            let sweep = classify_registry_roots(
                vec![(entry.clone(), run.clone())],
                RegistrySweep::default(),
                pruning,
            );
            assert!(sweep
                .ensure_access()
                .unwrap_err()
                .to_string()
                .contains("forced recovery pending"));
            assert!(sweep.live.is_empty() && sweep.ended.is_empty() && sweep.undecided.is_empty());
            assert_eq!(operator_snapshot(dir.path()), before);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&marker, fs::Permissions::from_mode(0o000)).unwrap();
            let sweep = classify_registry_roots(
                vec![(entry, run)],
                RegistrySweep::default(),
                Pruning::Prune,
            );
            fs::set_permissions(&marker, fs::Permissions::from_mode(0o644)).unwrap();
            let error = sweep.ensure_access().unwrap_err().to_string();
            assert!(
                error.contains("forced recovery pending") && error.contains("unreadable"),
                "{error}"
            );
            assert_eq!(operator_snapshot(dir.path()), before);
        }
    }
}

/// Uncheckable roots cannot be probed for authoritative descriptors or pruned. §FS-rhei-run-headless.3
#[cfg(unix)]
#[test]
fn operator_registry_unknown_root_never_reads_missing_runtime_descriptor() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join(".rhei")).unwrap();
    let run = operator_registry_descriptor(dir.path());
    let entry = dir.path().join("external-registry.json");
    fs::write(&entry, serde_json::to_vec(&run).unwrap()).unwrap();
    let before = operator_snapshot(dir.path());
    for pruning in [Pruning::Keep, Pruning::Prune] {
        fs::set_permissions(dir.path().join(".rhei"), fs::Permissions::from_mode(0o000)).unwrap();
        let sweep = classify_registry_roots(
            vec![(entry.clone(), run.clone())],
            RegistrySweep::default(),
            pruning,
        );
        fs::set_permissions(dir.path().join(".rhei"), fs::Permissions::from_mode(0o755)).unwrap();
        sweep.ensure_access().unwrap();
        assert!(sweep.live.is_empty() && sweep.ended.is_empty());
        assert_eq!(sweep.undecided.len(), 1);
        assert_eq!(sweep.not_known_to_have_ended()[0].id, run.id);
        assert!(sweep.undecided[0].reason.contains("workspace access could not be checked"));
        assert!(!sweep.undecided[0].reason.contains("forced recovery pending"));
        assert_eq!(operator_snapshot(dir.path()), before);
    }
}
