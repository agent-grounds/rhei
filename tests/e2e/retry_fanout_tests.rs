// One retry command recovers each exhausted active invocation without stockpiling.
// §FS-rhei-retry.2 §FS-rhei-retry.3
use super::supervision_retry_support::{cli, run, text};
use super::*;

const MACHINE: &str = r#"name: fanout-retry
version: 1
states:
  work:
    initial: true
    description: Work
    attempts: 2
    all_targets: ["mock:openai:model-a", "mock:openai:model-b"]
    agent_timeout: 10s
    outputs:
      - name: digest
        path: runtime/digest-{model}.md
  verifying:
    description: Inspect digests
    gating: true
  completed:
    description: Done
    final: true
transitions:
  - {from: work, to: verifying}
  - {from: verifying, to: completed}
"#;

fn fixture() -> (TestDir, PathBuf, PathBuf) {
    super::agent_reentry_support::setup(
        "fanout-retry",
        MACHINE,
        r#"root = pathlib.Path(env('RHEI_ROOT'))
model = env('RHEI_MODEL')
attempt = env('RHEI_ATTEMPT')
visit = env('RHEI_VISIT_COUNT')
append(root / ('spawns-' + model + '.log'), visit + ':' + attempt + '\n')
if (root / 'recover').exists() or (model == 'model-b' and (root / 'finish-b').exists()):
    write(root / 'runtime' / ('digest-' + model + '.md'), 'done\n')
    sys.exit(0)
sys.exit(1)
"#,
    )
}

fn record_path(dir: &Path, model: &str) -> PathBuf {
    dir.join(format!("runtime/spawns/task-plan.1-work-mock-openai-{model}.json"))
}

fn record(dir: &Path, model: &str) -> serde_json::Value {
    serde_json::from_slice(&fs::read(record_path(dir, model)).unwrap()).unwrap()
}

fn failed_run(dir: &Path) {
    let out = run(dir);
    assert!(!out.status.success(), "{}", text(&out));
}

fn assert_grant(dir: &Path, model: &str, attempt: u64, grants: usize) {
    let value = record(dir, model);
    assert_eq!(value["moves"], 0);
    assert_eq!(value["attempt"], attempt);
    let audit = value["grants"].as_array().unwrap();
    assert_eq!(audit.len(), grants);
    for grant in audit {
        assert_eq!(grant["attempts"], 2);
        for field in ["at", "os_user"] {
            assert!(!grant[field].as_str().unwrap().is_empty());
        }
    }
}

#[test]
fn retry_recovers_both_exhausted_fanout_arms_repeatedly_in_the_same_visit() {
    let (dir, plan, _) = fixture();
    failed_run(&dir);
    failed_run(&dir);
    let plan_before = fs::read(&plan).unwrap();
    for total in [2, 4] {
        let granted = cli(&dir, "retry", "plan.1", &[], false);
        assert_success(&granted);
        for model in ["model-a", "model-b"] {
            assert_grant(&dir, model, total, (total / 2) as usize);
            assert!(text(&granted).contains(&format!("mock-openai-{model}")));
            assert!(text(&granted).contains(&format!("attempt {}", total + 1)));
        }
        let bytes = ["model-a", "model-b"].map(|model| fs::read(record_path(&dir, model)).unwrap());
        let premature = cli(&dir, "retry", "plan.1", &[], false);
        assert!(!premature.status.success(), "no stockpiling");
        assert!(text(&premature).contains("attempts left"), "{}", text(&premature));
        for (model, before) in ["model-a", "model-b"].iter().zip(bytes) {
            assert_eq!(fs::read(record_path(&dir, model)).unwrap(), before);
        }
        failed_run(&dir);
        failed_run(&dir);
        for model in ["model-a", "model-b"] {
            assert_grant(&dir, model, total + 2, (total / 2) as usize);
            assert_eq!(record(&dir, model)["charged"], total + 2);
        }
    }
    assert_eq!(fs::read(&plan).unwrap(), plan_before);
    assert!(!dir.join("runtime/state-transitions.log").exists());
    assert_success(&cli(&dir, "retry", "plan.1", &[], false));
    fs::write(dir.join("recover"), "yes").unwrap();
    assert_success(&run(&dir));
    for model in ["model-a", "model-b"] {
        assert_grant(&dir, model, 7, 3);
        assert_eq!(
            fs::read_to_string(dir.join(format!("spawns-{model}.log"))).unwrap(),
            "1:1\n1:2\n1:3\n1:4\n1:5\n1:6\n1:7\n"
        );
        assert!(dir
            .join(format!("runtime/logs/task-plan.1-work-mock-openai-{model}-attempt7.log"))
            .exists());
    }
}

#[test]
fn retry_grants_only_the_exhausted_arm_when_its_sibling_has_capacity() {
    let (dir, _, _) = fixture();
    failed_run(&dir);
    failed_run(&dir);
    // A prior non-charge leaves B an attempt while A is exhausted.
    let mut b = record(&dir, "model-b");
    b["charged"] = 1.into();
    b["attempt_charged"] = false.into();
    b["ending"] = "provider_limited".into();
    fs::write(record_path(&dir, "model-b"), serde_json::to_vec_pretty(&b).unwrap()).unwrap();
    let before_b = fs::read(record_path(&dir, "model-b")).unwrap();
    let granted_a = cli(&dir, "retry", "plan.1", &[], false);
    assert_success(&granted_a);
    assert!(text(&granted_a).contains("mock-openai-model-a"));
    assert!(!text(&granted_a).contains("mock-openai-model-b"));
    assert_eq!(fs::read(record_path(&dir, "model-b")).unwrap(), before_b);
    assert_grant(&dir, "model-a", 2, 1);
    failed_run(&dir);
    assert_eq!(record(&dir, "model-b")["charged"], 2);
    let before_a = fs::read(record_path(&dir, "model-a")).unwrap();
    let granted_b = cli(&dir, "retry", "plan.1", &[], false);
    assert_success(&granted_b);
    assert!(text(&granted_b).contains("mock-openai-model-b"));
    assert!(!text(&granted_b).contains("mock-openai-model-a"));
    assert_eq!(fs::read(record_path(&dir, "model-a")).unwrap(), before_a);
    assert_grant(&dir, "model-b", 3, 1);
}

#[test]
fn retry_keeps_completed_fanout_work_without_granting_or_spawning_it() {
    let (dir, _, _) = fixture();
    failed_run(&dir);
    fs::write(dir.join("finish-b"), "yes").unwrap();
    failed_run(&dir);
    assert_eq!(record(&dir, "model-b")["charged"], 2);
    let before_b = fs::read(record_path(&dir, "model-b")).unwrap();
    let granted = cli(&dir, "retry", "plan.1", &[], false);
    assert_success(&granted);
    assert!(text(&granted).contains("mock-openai-model-a"));
    assert!(!text(&granted).contains("mock-openai-model-b"));
    assert_eq!(fs::read(record_path(&dir, "model-b")).unwrap(), before_b);
    fs::write(dir.join("recover"), "yes").unwrap();
    assert_success(&run(&dir));
    assert_eq!(fs::read_to_string(dir.join("spawns-model-a.log")).unwrap(), "1:1\n1:2\n1:3\n");
    assert_eq!(fs::read_to_string(dir.join("spawns-model-b.log")).unwrap(), "1:1\n1:2\n");
}

#[test]
fn retry_does_not_grant_an_exhausted_target_removed_from_the_active_fanout() {
    let (dir, _, machine) = fixture();
    failed_run(&dir);
    failed_run(&dir);
    fs::write(&machine, MACHINE.replace(", \"mock:openai:model-b\"", "")).unwrap();
    let before_b = fs::read(record_path(&dir, "model-b")).unwrap();
    let granted = cli(&dir, "retry", "plan.1", &[], false);
    assert_success(&granted);
    assert!(text(&granted).contains("mock-openai-model-a"));
    assert!(!text(&granted).contains("mock-openai-model-b"));
    assert_eq!(fs::read(record_path(&dir, "model-b")).unwrap(), before_b);
}
