// Actual-writer interruption evidence; no CLI authority bypass. §FS-rhei-recover.5

const OPERATOR_MACHINE: &str = "name: recovery\nversion: 1\nstates:\n  gate:\n    initial: true\n    gating: true\n  work:\n    visits: 3\n  done:\n    final: true\n  cancelled:\n    final: true\ntransitions:\n  - {from: gate, to: done}\n  - {from: work, to: cancelled}\n";

fn operator_fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let plan = dir.path().join("plan.rhei.md");
    let machine = dir.path().join("states.yaml");
    fs::write(&plan, "# Rhei: Recovery\n\n## Tasks\n\n### Task 1: Fix\n**State:** gate\n").unwrap();
    fs::write(&machine, OPERATOR_MACHINE).unwrap();
    (dir, plan, machine)
}

fn operator_request<'a>(plan: &'a Path, machine: &'a Path) -> ForcedRequest<'a> {
    ForcedRequest { input: plan, scope: &[], machine_path: Some(machine), task: "1", from: "gate", to: "work",
        reason: "repair route", result: Some("operator context") }
}

fn interrupt_force_at(boundary: &str) {
    let boundary = boundary.to_string();
    FORCED_BOUNDARY.with(|hook| *hook.borrow_mut() = Some(Box::new(move |point| {
        if point == boundary { Err(miette!("interrupted at {point}")) } else { Ok(()) }
    })));
}

fn clear_force_interrupt() { FORCED_BOUNDARY.with(|hook| hook.borrow_mut().take()); }

fn recorded_marker(root: &Path) -> ForcedMarker {
    ForcedMarker::parse(root, &fs::read(root.join(rhei_core::root_access::MARKER)).unwrap()).unwrap()
}

/// Every boundary is reached by the production writer with computed visits and result images.
/// §FS-rhei-recover.5 §FS-rhei-transition-cmd.6.1
#[test]
fn operator_writer_recovers_every_boundary_without_duplicate_effects() {
    let points = ["marker-before", "marker-after", "image-0-before", "image-0-after", "image-1-before",
        "image-1-after", "pair-before", "pair-between", "ledger-sync-before", "ledger-sync-after",
        "marker-remove-before", "marker-remove-after"];
    for point in points {
        let (dir, plan, machine) = operator_fixture();
        let request = operator_request(&plan, &machine);
        let preview = prepare_forced_transition(&request).unwrap();
        let expected = preview.files.clone();
        interrupt_force_at(point);
        let error = commit_confirmed_force(&request, preview, "test-operator").unwrap_err();
        clear_force_interrupt();
        assert!(error.to_string().contains(point), "{error}");
        if point == "marker-before" {
            assert!(!dir.path().join(rhei_core::root_access::MARKER).exists());
            for file in expected { assert_eq!(ForcedImage::read(&dir.path().join(file.path)).unwrap(), file.before); }
            continue;
        }
        if point == "marker-remove-after" {
            for file in expected { assert_eq!(ForcedImage::read(&dir.path().join(file.path)).unwrap(), file.after); }
            continue;
        }
        let marker = recorded_marker(dir.path());
        let decision = forced_decision(dir.path(), &marker).unwrap();
        // Repeat each recovery with a second interruption after an image replacement.
        interrupt_force_at("image-0-after");
        assert!(forced_replay(dir.path(), &marker, decision, "test").is_err());
        clear_force_interrupt();
        assert!(dir.path().join(rhei_core::root_access::MARKER).exists());
        assert_eq!(forced_decision(dir.path(), &marker).unwrap(), decision);
        forced_replay(dir.path(), &marker, decision, "test").unwrap();
        for file in &marker.files {
            let expected = if decision == ForcedDecision::Forward { &file.after } else { &file.before };
            assert_eq!(&ForcedImage::read(&dir.path().join(&file.path)).unwrap(), expected, "{point}: {}", file.path);
        }
        let ledger = fs::read_to_string(dir.path().join("runtime/state-transitions.log")).unwrap_or_default();
        let movements = rhei_core::transition_history::parse(&ledger).unwrap();
        assert_eq!(movements.len(), usize::from(decision == ForcedDecision::Forward));
        assert!(!dir.path().join(rhei_core::root_access::MARKER).exists());
        assert!(!dir.path().join("runtime/transitions.log").exists());
    }
}

/// Third values and ambiguous ledger tails are refused before the first restoration. §FS-rhei-recover.3
#[test]
fn operator_recovery_retains_third_values_and_ambiguous_evidence() {
    for mode in ["image", "prefix", "foreign-tail", "duplicate", "later-row"] {
        let (dir, plan, machine) = operator_fixture();
        let request = operator_request(&plan, &machine);
        interrupt_force_at("marker-after");
        assert!(commit_confirmed_force(&request, prepare_forced_transition(&request).unwrap(), "test").is_err());
        clear_force_interrupt();
        let marker = recorded_marker(dir.path());
        fs::create_dir_all(dir.path().join("runtime")).unwrap();
        let pair = format!("{}{}", marker.ledger.metadata_line, marker.ledger.movement_line);
        let mut candidate = marker.clone();
        match mode {
            "image" => fs::write(&plan, "out-of-band edit").unwrap(),
            "prefix" => candidate.ledger.prefix_sha256 = "0".repeat(64),
            "foreign-tail" => fs::write(dir.path().join(&marker.ledger.path), "foreign").unwrap(),
            "duplicate" => fs::write(dir.path().join(&marker.ledger.path), format!("{pair}{pair}")).unwrap(),
            _ => fs::write(dir.path().join(&marker.ledger.path), format!("{pair}other a@b\n")).unwrap(),
        }
        let before = fs::read(&plan).unwrap();
        assert!(forced_decision(dir.path(), &candidate).is_err(), "{mode}");
        assert_eq!(fs::read(&plan).unwrap(), before);
        assert!(dir.path().join(rhei_core::root_access::MARKER).exists());
    }
}

/// Revalidation observes a state change after confirmation, before publishing a marker. §FS-rhei-transition-cmd.6
#[test]
fn operator_confirmation_to_lock_race_rechecks_cas() {
    let (dir, plan, machine) = operator_fixture();
    let request = operator_request(&plan, &machine);
    let preview = prepare_forced_transition(&request).unwrap();
    let path = plan.clone();
    FORCED_BOUNDARY.with(|hook| *hook.borrow_mut() = Some(Box::new(move |point| {
        if point == "confirmed-before-locks" {
            let raw = fs::read_to_string(&path).unwrap();
            fs::write(&path, raw.replace("**State:** gate", "**State:** work")).unwrap();
        }
        Ok(())
    })));
    let error = commit_confirmed_force(&request, preview, "test").unwrap_err();
    clear_force_interrupt();
    assert!(error.to_string().contains("conflict: Task plan.1 is in state 'work', expected 'gate'"), "{error}");
    assert!(!dir.path().join(rhei_core::root_access::MARKER).exists());
    assert!(!dir.path().join("runtime/state-transitions.log").exists());
}

/// A live run prevents all exceptional effects and identifies the owner. §FS-rhei-transition-cmd.6
#[test]
fn operator_held_run_lock_refuses_before_marker() {
    let (dir, plan, machine) = operator_fixture();
    let request = operator_request(&plan, &machine);
    let preview = prepare_forced_transition(&request).unwrap();
    let _held = operator_run_locks(&preview.roots, "fixture-owner").unwrap();
    let error = commit_confirmed_force(&request, preview, "other").unwrap_err();
    assert!(error.to_string().contains("fixture-owner"), "{error}");
    assert!(!dir.path().join(rhei_core::root_access::MARKER).exists());
}
