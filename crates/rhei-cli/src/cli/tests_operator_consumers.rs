// One central parser governs all four history consumers. §FS-rhei-complete.3.1

/// Real writer output survives prompt/handoff/viz reads and a narrowed reset. §FS-rhei-complete.3.1
#[test]
fn operator_pair_consumers_count_one_movement_and_reset_prunes_the_pair() {
    let (dir, plan, machine_path) = operator_fixture();
    let request = operator_request(&plan, &machine_path);
    commit_confirmed_force(&request, prepare_forced_transition(&request).unwrap(), "test").unwrap();
    let machine = rhei_validator::StateMachine::from_yaml_file(&machine_path).unwrap();
    let ledger = read_ledger(dir.path()).unwrap();
    assert_eq!(ledger, vec![("plan.1".into(), "gate".into(), "work".into())]);
    assert_eq!(ledger_first_departures(dir.path()).get("plan.1").unwrap(), "gate");
    assert_eq!(last_recorded_source_state_for_current(dir.path(), &parse_task_id("plan.1"), "work", &machine).unwrap().as_deref(), Some("gate"));
    let model = rhei_viz::collect_plans(&plan, "plan", Some(&machine_path)).unwrap();
    let history = &model["plan"].tasks[0].history;
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].forced_reason.as_deref(), Some("repair route"));
    {
        let mut ledger = LockedTransitionLedger::open(dir.path()).unwrap();
        ledger.append("other.1", "draft", "done").unwrap();
        ledger.prune(&BTreeSet::from(["plan.1".to_string()])).unwrap();
    }
    assert_eq!(fs::read_to_string(dir.path().join("runtime/state-transitions.log")).unwrap(), "other.1 draft@done\n");
}

/// Known-pair corruption cannot be quietly consumed as ordinary history. §FS-rhei-complete.3.1
#[test]
fn operator_all_history_consumers_refuse_a_broken_pair() {
    let (dir, plan, machine_path) = operator_fixture();
    let request = operator_request(&plan, &machine_path);
    commit_confirmed_force(&request, prepare_forced_transition(&request).unwrap(), "test").unwrap();
    let path = dir.path().join("runtime/state-transitions.log");
    let raw = fs::read_to_string(&path).unwrap();
    fs::write(&path, raw.replace("gate@work", "gate@done")).unwrap();
    let machine = rhei_validator::StateMachine::from_yaml_file(&machine_path).unwrap();
    assert!(read_ledger(dir.path()).is_err());
    assert!(last_recorded_source_state_for_current(dir.path(), &parse_task_id("plan.1"), "done", &machine).is_err());
    assert!(rhei_viz::collect_plans(&plan, "plan", Some(&machine_path)).is_err());
    let before = operator_snapshot(dir.path());
    assert!(reset_command(&plan, Some(&machine_path), &[], true, false).is_err());
    assert_eq!(operator_snapshot(dir.path()), before);
}

/// Exercise pruning directly so reset's earlier memory read cannot mask a
/// competing decoder. Both lock-only and append handles preserve the evidence.
/// §FS-rhei-errors.1.2 §FS-rhei-errors.6
#[test]
fn diagnostic_guidance_prune_invalid_utf8_ledger() {
    for append_handle in [false, true] {
        let (dir, plan, _) = operator_fixture();
        let before = fs::read(&plan).unwrap();
        // The ledger names the canonical root it locked, in the platform's separators.
        let path = rhei_core::platform::canonical_path(dir.path())
            .unwrap()
            .join("runtime")
            .join("state-transitions.log");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let damaged = b"\xff\n";
        fs::write(&path, damaged).unwrap();
        let mut ledger = if append_handle {
            LockedTransitionLedger::open(dir.path()).unwrap()
        } else {
            LockedTransitionLedger::lock(dir.path()).unwrap()
        };
        let error = ledger.prune(&BTreeSet::from(["plan.1".to_string()]))
            .expect_err("pruning must refuse decoding damage");
        let diagnostic = error.chain().map(ToString::to_string).collect::<Vec<_>>().join("\n");
        for expected in [
            "failed to read state transition log",
            "invalid transition ledger",
            &path.display().to_string(),
            "invalid utf-8 sequence of 1 bytes from index 0",
        ] {
            assert!(diagnostic.contains(expected), "{diagnostic}");
        }
        let guidance = error.help().expect("ledger repair help").to_string();
        for expected in ["preserve", "restore", "valid UTF-8", "trusted backup", "Do not delete history"] {
            assert!(guidance.contains(expected), "{guidance}");
        }
        for unrelated in ["writable", "free space", "retry the command", "try again"] {
            assert!(!guidance.contains(unrelated), "{guidance}");
        }
        assert_eq!(fs::read(&path).unwrap(), damaged, "refusal changed ledger");
        assert_eq!(fs::read(&plan).unwrap(), before, "refusal changed plan");
    }
}

/// Shared decoding must retain memory/pruning absence behavior and classify
/// actual OS failures as filesystem failures. §FS-rhei-errors.6
#[test]
fn diagnostic_guidance_ledger_absence_and_io_remain_distinct() {
    let (dir, _, _) = operator_fixture();
    // `read_ledger` names the root as given, in the platform's separators.
    let path = dir.path().join("runtime").join("state-transitions.log");
    let absent = read_transition_ledger(&path).unwrap_err();
    assert!(absent.is_not_found());
    assert!(read_ledger(dir.path()).unwrap().is_empty());
    let mut ledger = LockedTransitionLedger::lock(dir.path()).unwrap();
    assert!(!ledger.prune(&BTreeSet::from(["plan.1".to_string()])).unwrap());
    assert!(!path.exists(), "an absent ledger stays absent");

    fs::create_dir_all(&path).unwrap();
    let cause = fs::read(&path).unwrap_err();
    let error = read_ledger(dir.path()).expect_err("a directory cannot be read as history");
    let diagnostic = error.to_string();
    assert!(diagnostic.contains(&path.display().to_string()), "{diagnostic}");
    assert!(diagnostic.contains(&cause.to_string()), "{diagnostic}");
    let guidance = error.help().expect("I/O help").to_string();
    assert_eq!(guidance, io_error_help(&path, cause.kind()));
    assert!(!guidance.contains("trusted backup"), "{guidance}");
    assert!(!ledger.prune(&BTreeSet::from(["plan.1".to_string()])).unwrap());
    assert!(path.is_dir(), "pruning still ignores a non-file ledger path");
}
