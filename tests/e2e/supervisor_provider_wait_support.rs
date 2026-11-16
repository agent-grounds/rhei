//! Mock-only port of the #727 reproducer's supervisor and open child.
//! §FS-rhei-run.5.1 §FS-rhei-run.3.3 §FS-rhei-supervision.3.2

use super::provider_limit_support::*;
use super::provider_wait_quiet_support::far_limit_signal;
use super::*;

pub(super) const MACHINE: &str = r#"name: provider-wait-supervised
version: 1
states:
  supervising:
    initial: true
    execute_on: child-terminal
    target: pi:anthropic:claude-opus-5-5
    agent_timeout: 60s
    visits: 6
  work:
    target: pi:openai:gpt-6
    agent_timeout: 60s
  completed:
    final: true
transitions:
  - { from: supervising, to: completed, condition: openDescendants < 1 }
  - { from: supervising, to: supervising }
  - { from: work, to: completed }
"#;

pub(super) fn fixture() -> ProviderFixture {
    let fixture = ProviderFixture::new(
        "supervisor-provider-wait-727",
        &[(
            "01-supervisor.md",
            "### Task 1: Supervise\n**State:** supervising\n\n\
             #### Task 1.1: Work\n**State:** work\n",
        )],
        MACHINE,
        &format!(
            "root = pathlib.Path(env('RHEI_ROOT'))\n\
             append(root / 'starts', env('RHEI_STATE') + '\\n')\n\
             print({:?}, file=sys.stderr, flush=True)\n\
             raise SystemExit(1)\n",
            far_limit_signal()
        ),
    );
    // Override pi as well as the default profiles: neither a probe nor a spawn
    // can reach an installed agent. The original fixture used a PATH stand-in.
    let path = fixture.root.join(".agent-grounds/rhei/settings.json");
    let mut settings: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    settings["agents"]["pi"] = settings["agents"]["codex"].clone();
    fs::write(path, settings.to_string()).unwrap();
    fixture
}

pub(super) fn seed_wait(fixture: &ProviderFixture) -> String {
    let deadline = epoch_now() + 7200;
    let mut record = limit_record(deadline);
    record["identity"] = serde_json::json!({"agent": "pi", "provider": "anthropic"});
    edit_metadata(&fixture.root, |m| {
        m["metadata"] = serde_json::json!({"tasks": {"1": {
            "providerLimits": {"supervising": record}
        }}});
    });
    utc_at(deadline)
}

pub(super) fn assert_held(fixture: &ProviderFixture, expected_starts: usize) {
    let starts = fs::read_to_string(fixture.root.join("starts")).unwrap_or_default();
    assert_eq!(starts.lines().count(), expected_starts, "spawns:\n{starts}");
    assert!(starts.lines().all(|state| state == "supervising"), "child ran: {starts}");
    assert_task_state(&fixture.root, &fixture.machine, "1", "supervising");
    assert_task_state(&fixture.root, &fixture.machine, "1.1", "work");
    let frontmatter = metadata(&fixture.root);
    let m = &frontmatter["metadata"];
    let limit = &m["tasks"]["1"]["providerLimits"]["supervising"];
    assert_eq!(limit["identity"]["agent"], "pi");
    assert_eq!(limit["identity"]["provider"], "anthropic");
    assert!(limit["nextAttemptAt"].as_str().is_some());
    // No block is the authored-initial held phase; a refusal must not release it.
    assert_ne!(m["tasks"]["1"]["supervision"]["phase"], "released");
}
