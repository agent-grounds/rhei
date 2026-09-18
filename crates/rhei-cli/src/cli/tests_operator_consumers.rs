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
