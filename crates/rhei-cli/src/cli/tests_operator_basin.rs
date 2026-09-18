// The real basin writer has three images and one ledger witness. §FS-rhei-recover.5

fn operator_basin_fixture() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let project = rhei_core::platform::canonical_path(dir.path()).unwrap();
    let root = project.join("basin");
    fs::create_dir(&root).unwrap();
    fs::write(project.join("index.panta.md"), "# Panta: Recovery\n---\nowner: keep-me\nmetadata:\n  tasks:\n    basin.9:\n      stateVisits:\n        work: 2\n---\n\nProject prose stays byte-for-byte.\n").unwrap();
    fs::write(root.join("work.md"), "### Task 1: Parent\n**State:** supervising\n\n#### Task 1.1: Fix\n**State:** work\n").unwrap();
    fs::write(root.join("sibling.md"), "### Task 9: Sibling\n**State:** gate\n").unwrap();
    fs::write(project.join("other.rhei.md"), "# Rhei: Other\n\n## Tasks\n\n### Task 1: Independent\n**State:** gate\n").unwrap();
    let machine = project.join("states.yaml");
    fs::write(&machine, OPERATOR_MACHINE.replace("  work:\n", "  supervising:\n    agent: pi\n    execute_on: descendant-terminal\n    visits: 3\n  work:\n")
        + "  - {from: supervising, to: supervising}\n  - {from: supervising, to: done}\n").unwrap();
    (dir, project, root, machine)
}

fn operator_basin_request<'a>(project: &'a Path, machine: &'a Path) -> ForcedRequest<'a> {
    ForcedRequest { input: project, scope: &[], machine_path: Some(machine), task: "basin.1.1",
        from: "work", to: "done", reason: "repair basin route", result: Some("fresh basin result") }
}

fn assert_basin_images(root: &Path, files: &[ForcedFile], forward: bool) {
    for file in files {
        assert_eq!(ForcedImage::read(&forced_file_path(root, file).unwrap()).unwrap(),
            if forward { file.after.clone() } else { file.before.clone() }, "{}", file.path);
    }
}

fn assert_basin_effects(project: &Path, root: &Path, forward: bool) {
    let manifest = fs::read_to_string(project.join("index.panta.md")).unwrap();
    assert!(manifest.contains("owner: keep-me\n"));
    assert!(manifest.ends_with("Project prose stays byte-for-byte.\n"));
    let metadata = parse_metadata_from_raw(&project.join("index.panta.md"), &manifest).unwrap().unwrap();
    assert_eq!(task_visit_count(Some(&metadata), &parse_task_id("basin.9"), "work"), 2);
    if forward {
        assert_eq!(task_visit_count(Some(&metadata), &parse_task_id("basin.1.1"), "work"), 1);
        assert!(manifest.contains("basin.1:"), "{manifest}");
        assert_eq!(manifest.matches("checkpoints:").count(), 1, "{manifest}");
        let result = fs::read_to_string(root.join("runtime/results/basin.1.1.md")).unwrap();
        assert_eq!(result, "## Result\n\nfresh basin result\n\n");
        assert_eq!(fs::read_to_string(root.join("work.md")).unwrap().matches("> **Result:**").count(), 1);
    }
    let ledger = fs::read_to_string(root.join("runtime/state-transitions.log")).unwrap_or_default();
    let movements = rhei_core::transition_history::parse(&ledger).unwrap();
    assert_eq!(movements.len(), usize::from(forward));
    if forward {
        assert_eq!(ledger.lines().count(), 2);
        assert_eq!(movements[0].task_id, "basin.1.1");
        assert_eq!(movements[0].audit.as_ref().unwrap().result_sha256, Some(forced_digest(b"fresh basin result")));
    }
    assert!(!project.join("runtime/state-transitions.log").exists());
    assert!(!project.join(rhei_core::root_access::MARKER).exists());
    assert!(!root.join("runtime/transitions.log").exists());
    assert_eq!(fs::read(root.join("sibling.md")).unwrap(), b"### Task 9: Sibling\n**State:** gate\n");
    assert_eq!(fs::read(project.join("other.rhei.md")).unwrap(), b"# Rhei: Other\n\n## Tasks\n\n### Task 1: Independent\n**State:** gate\n");
}

/// Real final entry includes a typed manifest, fresh result and qualified supervision effects. §FS-rhei-transition-cmd.6.1
#[test]
fn operator_basin_force_commits_all_three_images() {
    let (_dir, project, root, machine) = operator_basin_fixture();
    let request = operator_basin_request(&project, &machine);
    let prepared = prepare_forced_transition(&request).unwrap();
    assert_eq!(prepared.roots, vec![project.clone(), root.clone()]);
    assert_eq!(prepared.files.len(), 3);
    let manifest = &prepared.files[0];
    assert_eq!(manifest.owner, Some(ForcedOwner::BasinProjectMetadata));
    assert_eq!(manifest.path, "index.panta.md");
    let images = prepared.files.clone();
    commit_confirmed_force(&request, prepared, "test").unwrap();
    assert_basin_images(&root, &images, true);
    assert_basin_effects(&project, &root, true);
    assert!(!root.join(rhei_core::root_access::MARKER).exists());
}

/// Every initial write boundary and every replay boundary, including a repeated interruption. §FS-rhei-recover.5
#[test]
fn operator_basin_recovers_every_writer_and_replay_boundary() {
    let writer_points = ["marker-before", "marker-after", "image-0-before", "image-0-after",
        "image-1-before", "image-1-after", "image-2-before", "image-2-after", "pair-before",
        "pair-between", "ledger-sync-before", "ledger-sync-after", "marker-remove-before", "marker-remove-after"];
    let replay_points = ["recovery-ledger-before", "recovery-ledger-truncate-before", "recovery-ledger-truncate-after",
        "recovery-ledger-sync-before", "recovery-ledger-sync-after", "image-0-before", "image-0-after",
        "image-1-before", "image-1-after", "image-2-before", "image-2-after", "marker-remove-before", "marker-remove-after"];
    for writer_point in writer_points {
        for replay_point in replay_points {
            let (_dir, project, root, machine) = operator_basin_fixture();
            let request = operator_basin_request(&project, &machine);
            let prepared = prepare_forced_transition(&request).unwrap();
            let images = prepared.files.clone();
            interrupt_force_at(writer_point);
            let error = commit_confirmed_force(&request, prepared, "test").unwrap_err();
            clear_force_interrupt();
            assert!(error.to_string().contains(writer_point), "{error}");
            if writer_point == "marker-before" || writer_point == "marker-remove-after" {
                let forward = writer_point == "marker-remove-after";
                assert_basin_images(&root, &images, forward);
                assert_basin_effects(&project, &root, forward);
                break;
            }
            let marker = recorded_marker(&root);
            assert_eq!(marker.version, 2);
            let decision = forced_decision(&root, &marker).unwrap();
            // Ledger replay boundaries require an existing ledger; truncation requires rollback.
            if replay_point.starts_with("recovery-ledger") && !root.join(&marker.ledger.path).exists() { continue; }
            if replay_point.contains("truncate") && decision == ForcedDecision::Forward { continue; }
            interrupt_force_at(replay_point);
            let error = forced_replay(&root, &marker, decision, "test").unwrap_err();
            clear_force_interrupt();
            assert!(error.to_string().contains(replay_point), "{error}");
            if replay_point != "marker-remove-after" {
                assert_eq!(forced_decision(&root, &marker).unwrap(), decision);
                // Interrupt the second recovery too, then finish a third explicit invocation.
                interrupt_force_at("image-1-after");
                assert!(forced_replay(&root, &marker, decision, "test").is_err());
                clear_force_interrupt();
                assert_eq!(forced_decision(&root, &marker).unwrap(), decision);
                forced_replay(&root, &marker, decision, "test").unwrap();
            }
            assert_basin_images(&root, &images, decision == ForcedDecision::Forward);
            assert_basin_effects(&project, &root, decision == ForcedDecision::Forward);
            assert!(!root.join(rhei_core::root_access::MARKER).exists());
        }
    }
}

/// The basin ledger alone witnesses commitment, even when the project has unrelated history. §FS-rhei-recover.3
#[test]
fn operator_basin_ignores_parent_ledger_and_refuses_third_values_and_ambiguous_tails() {
    for mode in ["parent-pair", "manifest", "task", "result", "prefix", "torn", "foreign", "duplicate", "later-row", "reversed"] {
        let (_dir, project, root, machine) = operator_basin_fixture();
        let request = operator_basin_request(&project, &machine);
        interrupt_force_at("marker-after");
        assert!(commit_confirmed_force(&request, prepare_forced_transition(&request).unwrap(), "test").is_err());
        clear_force_interrupt();
        let marker = recorded_marker(&root);
        let pair = format!("{}{}", marker.ledger.metadata_line, marker.ledger.movement_line);
        fs::create_dir_all(project.join("runtime")).unwrap();
        fs::write(project.join(&marker.ledger.path), &pair).unwrap();
        fs::create_dir_all(root.join("runtime/results")).unwrap();
        let ledger = root.join(&marker.ledger.path);
        match mode {
            "manifest" => fs::write(project.join("index.panta.md"), "foreign manifest").unwrap(),
            "task" => fs::write(root.join("work.md"), "foreign task").unwrap(),
            "result" => fs::write(root.join("runtime/results/basin.1.1.md"), "foreign result").unwrap(),
            "prefix" => { fs::write(&ledger, "prefix changed\n").unwrap(); }
            "torn" => fs::write(&ledger, &marker.ledger.metadata_line).unwrap(),
            "foreign" => fs::write(&ledger, "foreign tail").unwrap(),
            "duplicate" => fs::write(&ledger, format!("{pair}{pair}")).unwrap(),
            "later-row" => fs::write(&ledger, format!("{pair}basin.9 gate@work\n")).unwrap(),
            "reversed" => fs::write(&ledger, format!("{}{}", marker.ledger.movement_line, marker.ledger.metadata_line)).unwrap(),
            _ => (),
        }
        let snapshot = operator_snapshot(&project);
        if ["parent-pair", "torn"].contains(&mode) {
            assert_eq!(forced_decision(&root, &marker).unwrap(), ForcedDecision::Rollback);
            forced_replay(&root, &marker, ForcedDecision::Rollback, "test").unwrap();
            assert_basin_images(&root, &marker.files, false);
            assert_eq!(fs::read_to_string(project.join(&marker.ledger.path)).unwrap(), pair);
        } else {
            assert!(forced_decision(&root, &marker).is_err(), "{mode}");
            assert_eq!(operator_snapshot(&project), snapshot);
        }
    }
}
