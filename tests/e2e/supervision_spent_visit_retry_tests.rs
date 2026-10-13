// §FS-rhei-retry.2 §FS-rhei-retry.3 §FS-rhei-supervision.3.5
use super::supervision_retry_support::*;
use super::*;

#[test]
fn spent_supervising_visit_halt_names_recovery_that_keeps_the_hold() {
    let (dir, plan, _) = fixture();
    let output = text(&exhaust(&dir, &plan));
    assert!(output.contains("The ticket stays in 'supervising'."), "{output}");
    for expected in
        ["rhei retry plan.1", "still held", "1 unread checkpoint", "self-loop", "release"]
    {
        assert!(output.contains(expected), "halt must name {expected:?}:\n{output}");
    }
}

#[test]
fn retry_recovers_as_attempt_three_of_the_same_held_visit_with_its_checkpoint() {
    let (dir, plan, machine) = fixture();
    exhaust(&dir, &plan);
    let before = fs::read(&plan).expect("plan bytes");
    let ledger = fs::read(dir.join("runtime/state-transitions.log")).expect("ledger");
    let held = supervision(&plan);
    let (record_path, old_record) = record(&dir);

    let grant = cli(&dir, "retry", "plan.1", &[], false);
    assert!(grant.status.success(), "grant exhausted visit: {}", text(&grant));
    for expected in ["granted 2", "4 in total", "attempt 3", "next", "held", "1 checkpoint"] {
        assert!(text(&grant).contains(expected), "grant must say {expected:?}: {}", text(&grant));
    }
    assert_eq!(fs::read(&plan).unwrap(), before, "no state, visit, hold or metadata mutation");
    assert_eq!(fs::read(dir.join("runtime/state-transitions.log")).unwrap(), ledger);
    assert_eq!(supervision(&plan), held);
    let granted: serde_json::Value =
        serde_json::from_slice(&fs::read(&record_path).unwrap()).unwrap();
    assert_eq!(granted["moves"], old_record["moves"]);
    assert_eq!(granted["attempt"], old_record["attempt"]);
    assert_eq!(granted["charged"], old_record["charged"]);
    assert_eq!(granted["grants"].as_array().unwrap().len(), 1);
    assert_eq!(granted["grants"][0]["attempts"], 2);
    for field in ["at", "os_user"] {
        assert!(!granted["grants"][0][field].as_str().unwrap().is_empty());
    }
    let bytes = fs::read(&record_path).unwrap();
    let premature = cli(&dir, "retry", "plan.1", &[], false);
    assert!(!premature.status.success(), "cannot stockpile grants");
    assert!(text(&premature).contains("attempts left"), "{}", text(&premature));
    assert_eq!(fs::read(&record_path).unwrap(), bytes);
    assert_eq!(fs::read(&plan).unwrap(), before);
    assert_eq!(fs::read(dir.join("runtime/state-transitions.log")).unwrap(), ledger);

    fs::write(dir.join("recover"), "allowed").unwrap();
    let recovered = run(&dir);
    assert!(recovered.status.success(), "{}", text(&recovered));
    assert!(fs::read_to_string(dir.join("spawns.log"))
        .unwrap()
        .contains("plan.1 supervising visit=2 attempt=3"));
    let prompt = fs::read_to_string(dir.join("prompts/plan.1-2-3.md")).unwrap();
    assert!(prompt.contains("## Checkpoints"), "{prompt}");
    assert!(prompt.contains("plan.1.1") && prompt.contains("work") && prompt.contains("completed"));
    assert!(dir.join("runtime/supervise/plan.1.2.md").exists());
    assert!(dir.join("runtime/logs/task-plan.1-supervising-2-attempt3.log").exists());
    super::supervision_tests::assert_state_anywhere(&plan, &machine, "1.2", "completed");
}
