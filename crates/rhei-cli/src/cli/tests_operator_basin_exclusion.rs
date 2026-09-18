// Shared project ownership is enforced across every writer/reader entry. §FS-rhei-recover.4

/// Both owners identify their live run holder for initial force and explicit replay. §FS-rhei-recover.3
#[test]
fn operator_basin_force_and_replay_refuse_each_owners_live_run() {
    for project_owner in [true, false] {
        for replay in [true, false] {
            let (_dir, project, root, machine) = operator_basin_fixture();
            let request = operator_basin_request(&project, &machine);
            let prepared = prepare_forced_transition(&request).unwrap();
            if replay {
                interrupt_force_at("marker-after");
                assert!(commit_confirmed_force(&request, prepared, "test").is_err());
                clear_force_interrupt();
            }
            let owner_root = if project_owner { &project } else { &root };
            let _run = operator_run_locks(std::slice::from_ref(owner_root), "live-owner").unwrap();
            let images = [project.join("index.panta.md"), root.join("work.md")]
                .map(|path| (path.clone(), fs::read(&path).unwrap()));
            let error = if replay {
                let marker = recorded_marker(&root);
                forced_replay(&root, &marker, ForcedDecision::Rollback, "other").unwrap_err()
            } else {
                commit_confirmed_force(&request, prepare_forced_transition(&request).unwrap(), "other").unwrap_err()
            };
            assert!(error.to_string().contains("live-owner"), "{error}");
            assert!(error.to_string().contains(&owner_root.display().to_string()), "{error}");
            for (path, before) in images { assert_eq!(fs::read(path).unwrap(), before); }
            assert_eq!(root.join(rhei_core::root_access::MARKER).exists(), replay);
        }
    }
}

/// A writer already inside the boundary completes before capture, including unrelated edits. §FS-rhei-recover.4
#[test]
fn operator_basin_captures_the_completed_manifest_writer() {
    let (_dir, project, root, machine) = operator_basin_fixture();
    let request = operator_basin_request(&project, &machine);
    let prepared = prepare_forced_transition(&request).unwrap();
    let manifest_path = project.join("index.panta.md");
    let writer = LockedPlanFile::open(&manifest_path).unwrap();
    let updated = writer.read_to_string("fixture read").unwrap().replace("owner: keep-me", "owner: completed-update");
    let (tx, rx) = mpsc::channel();
    let worker_project = project.clone();
    let worker = std::thread::spawn(move || {
        FORCED_BOUNDARY.with(|hook| *hook.borrow_mut() = Some(Box::new(move |point| {
            if point == "root-contended" { tx.send(()).unwrap(); }
            if point == "marker-after" { return Err(miette!("interrupted after capture")); }
            Ok(())
        })));
        let result = commit_confirmed_force(&operator_basin_request(&worker_project, &machine), prepared, "test");
        clear_force_interrupt();
        result.err().map(|err| err.to_string())
    });
    rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(!root.join(rhei_core::root_access::MARKER).exists());
    let mut temporary = tempfile::NamedTempFile::new_in(&project).unwrap();
    temporary.write_all(updated.as_bytes()).unwrap();
    persist_locked(temporary, &manifest_path, Some(&writer)).unwrap();
    drop(writer);
    assert!(worker.join().unwrap().unwrap().contains("interrupted after capture"));
    let marker = recorded_marker(&root);
    let manifest = marker.files.iter().find(|file| file.owner == Some(ForcedOwner::BasinProjectMetadata)).unwrap();
    assert_eq!(manifest.before.bytes().unwrap().unwrap(), updated.as_bytes());
    assert!(String::from_utf8(manifest.after.bytes().unwrap().unwrap()).unwrap().contains("owner: completed-update\n"));
    forced_replay(&root, &marker, ForcedDecision::Rollback, "test").unwrap();
    assert_eq!(fs::read(&manifest_path).unwrap(), updated.as_bytes());
}

/// Refusal precedes reads, writer-sidecar creation and reinitialization, even with invalid manifest bytes. §FS-rhei-panta.6.6
#[test]
fn operator_basin_marker_blocks_project_manifest_basin_and_member_access() {
    let (_dir, project, root, machine) = operator_basin_fixture();
    let request = operator_basin_request(&project, &machine);
    interrupt_force_at("marker-after");
    assert!(commit_confirmed_force(&request, prepare_forced_transition(&request).unwrap(), "test").is_err());
    clear_force_interrupt();
    // Pending discovery must outrank errors in the in-doubt authored input.
    fs::write(project.join("index.panta.md"), "not parseable as a project").unwrap();
    let before = operator_snapshot(&project);
    for input in [&project, &project.join("index.panta.md"), &root, &root.join("work.md"), &project.join("other.rhei.md")] {
        let error = load_plan(input).err().unwrap().to_string();
        assert!(error.contains("basin.1.1 work -> done"), "{error}");
        assert_eq!(error.matches("rhei recover ").count(), 1, "{error}");
    }
    assert!(load_plan_leniently(&project).is_err());
    assert!(LockedPlanFile::open(&project.join("index.panta.md")).err().unwrap().to_string().contains("forced recovery pending"));
    assert!(LockedPlanFile::open(&root.join("work.md")).err().unwrap().to_string().contains("forced recovery pending"));
    assert!(reset_command(&project, Some(&machine), &[], false, true).unwrap_err().to_string().contains("forced recovery pending"));
    assert!(init_command(Some(&project), Some("Replacement"), true, true, true).unwrap_err().to_string().contains("forced recovery pending"));
    assert!(rhei_core::source::read_to_string(&project.join("index.panta.md")).unwrap_err().to_string().contains("forced recovery pending"));
    assert_eq!(operator_snapshot(&project), before);
}

/// Invalid owner capabilities cannot gain write authority outside the basin. §FS-rhei-recover.2
#[test]
fn operator_basin_marker_rejects_unknown_owners_paths_and_versions() {
    let (_dir, project, root, machine) = operator_basin_fixture();
    let request = operator_basin_request(&project, &machine);
    interrupt_force_at("marker-after");
    assert!(commit_confirmed_force(&request, prepare_forced_transition(&request).unwrap(), "test").is_err());
    clear_force_interrupt();
    let marker = recorded_marker(&root);
    for mode in ["unknown-owner", "parent", "absolute", "filename", "roles", "absent", "v1-owner", "missing-owner", "duplicate", "task-owner"] {
        let mut value = serde_json::to_value(&marker).unwrap();
        match mode {
            "unknown-owner" => value["files"][0]["owner"] = "project".into(),
            "parent" => value["files"][0]["path"] = "../index.panta.md".into(),
            "absolute" => value["files"][0]["path"] = project.join("index.panta.md").display().to_string().into(),
            "filename" => value["files"][0]["path"] = "other.rhei.md".into(),
            "roles" => value["files"][0]["roles"] = serde_json::json!(["task"]),
            "absent" => value["files"][0]["before"] = serde_json::json!({"kind":"absent"}),
            "v1-owner" => value["version"] = 1.into(),
            "missing-owner" => { value["files"][1].as_object_mut().unwrap().remove("owner"); }
            "duplicate" => { let file = value["files"][0].clone(); value["files"].as_array_mut().unwrap().insert(0, file); }
            _ => value["files"][1]["path"] = "../other.rhei.md".into(),
        }
        let mut bytes = rhei_core::transition_history::canonical_json(&value).unwrap(); bytes.push(b'\n');
        let before = operator_snapshot(&project);
        assert!(ForcedMarker::parse(&root, &bytes).is_err(), "{mode}");
        assert_eq!(operator_snapshot(&project), before);
    }
    fs::write(root.join("index.rhei.md"), "# Rhei: Ambiguous\n").unwrap();
    assert!(ForcedMarker::parse(&root, &marker.bytes().unwrap()).is_err());
}

/// Neither the parent manifest nor a contained image can escape via a symlink. §FS-rhei-recover.2
#[cfg(unix)]
#[test]
fn operator_basin_marker_rejects_symlink_owner_and_image() {
    for manifest in [true, false] {
        let (_dir, project, root, machine) = operator_basin_fixture();
        let request = operator_basin_request(&project, &machine);
        interrupt_force_at("marker-after");
        assert!(commit_confirmed_force(&request, prepare_forced_transition(&request).unwrap(), "test").is_err());
        clear_force_interrupt();
        let marker = recorded_marker(&root);
        let path = if manifest { project.join("index.panta.md") } else { root.join("work.md") };
        let original = path.with_extension("saved");
        fs::rename(&path, &original).unwrap();
        std::os::unix::fs::symlink(&original, &path).unwrap();
        let before = fs::read(&original).unwrap();
        assert!(ForcedMarker::parse(&root, &marker.bytes().unwrap()).is_err());
        assert_eq!(fs::read(&original).unwrap(), before);
    }
}
