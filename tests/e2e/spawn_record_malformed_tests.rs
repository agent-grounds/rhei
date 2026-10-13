// A completed worker must still account for its log when its prior JSON was malformed.
// §FS-rhei-agents.8.4
use super::supervision_retry_support::{cli, run, text};
use super::*;

#[test]
fn malformed_prior_record_is_replaced_by_a_completed_program_charge() {
    let dir = unique_temp_dir("malformed-spawn-record");
    write_fixture_file(&dir, "plan.rhei.md", super::agent_reentry_support::PLAN);
    let program = write_python_agent(
        &dir,
        "program.py",
        r#"root = pathlib.Path(env('RHEI_ROOT'))
append(root / 'spawns.log', env('RHEI_ATTEMPT') + '\n')
sys.exit(1)
"#,
    );
    write_fixture_file(
        &dir,
        "states.yaml",
        &format!(
            r#"name: malformed-spawn-record
version: 1
states:
  work:
    initial: true
    description: Work
    attempts: 2
    program:
      command: {}
    program_timeout: 10s
  completed:
    description: Done
    final: true
transitions:
  - {{from: work, to: completed}}
"#,
            fixture_command(&program)
        ),
    );
    let record_path = dir.join("runtime/spawns/task-plan.1-work.json");
    fs::create_dir_all(record_path.parent().unwrap()).unwrap();
    fs::write(&record_path, "{broken").unwrap();
    for attempt in [1, 2] {
        let out = run(&dir);
        assert!(!out.status.success(), "{}", text(&out));
        assert!(!text(&out).contains("no spawn record accounts for it"), "{}", text(&out));
        let stored: serde_json::Value =
            serde_json::from_slice(&fs::read(&record_path).unwrap()).unwrap();
        assert_eq!(stored["moves"], 0);
        assert_eq!(stored["attempt"], attempt);
        assert_eq!(stored["charged"], attempt);
        assert_eq!(stored["kind"], "program");
    }
    let halt = run(&dir);
    assert!(!halt.status.success());
    assert!(text(&halt).contains("2 attempts spent"), "{}", text(&halt));
    assert_eq!(fs::read_to_string(dir.join("spawns.log")).unwrap(), "1\n2\n");
    assert_success(&cli(&dir, "retry", "plan.1", &[], false));
    let resumed = run(&dir);
    assert!(!resumed.status.success(), "the program still fails");
    assert!(!text(&resumed).contains("no spawn record accounts for it"));
    let stored: serde_json::Value =
        serde_json::from_slice(&fs::read(&record_path).unwrap()).unwrap();
    assert_eq!(stored["attempt"], 3);
    assert_eq!(stored["charged"], 3);
    assert_eq!(stored["grants"][0]["attempts"], 2);
    assert_eq!(fs::read_to_string(dir.join("spawns.log")).unwrap(), "1\n2\n3\n");
    assert!(dir.join("runtime/logs/task-plan.1-work-attempt3.log").exists());
}
