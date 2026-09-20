//! Portable, provider-free fixtures for §FS-rhei-budgets. Synthetic financial
//! history includes its separate witness. Positive launch cases still need a
//! fixture driver; fixture qualification never grants release authority.

use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::*;

include!("budget_process_support.rs");

pub const PROJECT_UUID: &str = "550e8400-e29b-41d4-a716-446655440107";
pub const TICKET_UUID: &str = "550e8400-e29b-41d4-a716-446655440001";

pub struct BudgetCase {
    pub _dir: TestDir,
    pub root: PathBuf,
    pub plan: PathBuf,
    pub machine: PathBuf,
}

pub struct Limits {
    pub invocations: u64,
    pub spend_micro: u64,
    pub transition_limit: u64,
    pub threshold_micro: u64,
    pub residual_micro: u64,
}

pub fn cycle_case(prefix: &str, limits: Limits, guard_after: u64) -> BudgetCase {
    build_cycle_case(prefix, limits, guard_after, true)
}

pub fn fresh_case(prefix: &str, limits: Limits) -> BudgetCase {
    build_cycle_case(prefix, limits, 4, false)
}

fn build_cycle_case(
    prefix: &str,
    limits: Limits,
    guard_after: u64,
    initialized: bool,
) -> BudgetCase {
    let dir = unique_temp_dir(prefix);
    let root = dir.join("project");
    fs::create_dir_all(&root).expect("create project");
    let identities = if initialized {
        format!("---\nbudgetProjectId: {PROJECT_UUID}\nmetadata:\n  tasks:\n    1:\n      budgetTicketId: {TICKET_UUID}\n---")
    } else {
        String::new()
    };
    let plan = write_fixture_file(
        &root,
        "plan.rhei.md",
        &format!(
            r#"# Rhei: Bounded cycle

{identities}

## Tasks

### Task 1: Cycle through two states
**State:** a
"#
        ),
    );
    let machine = write_fixture_file(
        &dir,
        "states.yaml",
        &format!(
            r#"name: bounded-cycle
version: 1
models: [fixture]
states:
  a:
    description: First side of the cycle
    agent: codex
    model: fixture
    agent_timeout: 5s
    budget_threshold:
      currency: USD
      amount_micro: {}
  b:
    description: Second side of the cycle
    agent: codex
    model: fixture
    agent_timeout: 5s
    budget_threshold:
      currency: USD
      amount_micro: {}
  cancelled:
    final: true
    description: Finite escape used only for validation
profiles:
  work:
    initial: a
    allowed: [a, b, cancelled]
    transition_limit: {}
node_policy:
  root: work
  default: work
transitions:
  - from: a
    to: b
  - from: b
    to: a
  - from: a
    to: cancelled
  - from: b
    to: cancelled
  - from: "*"
    to: cancelled
"#,
            limits.threshold_micro, limits.threshold_micro, limits.transition_limit
        ),
    );

    let agent = write_python_agent(
        &root,
        "bounded-agent.py",
        &format!(
            r#"root = pathlib.Path(env('RHEI_ROOT'))
counter = root / 'fixture-invocations.txt'
count = int(counter.read_text().strip()) + 1 if counter.exists() else 1
write(counter, str(count) + '\n')
fixture_provider_call()
import json
sys.stdout.write(json.dumps({{
    'type': 'turn.completed',
    'usage': {{
        'input_tokens': 1000,
        'cached_input_tokens': 0,
        'cache_creation_input_tokens': 0,
        'output_tokens': 1000
    }}
}}) + '\n')
if count > {guard_after}:
    sys.stderr.write('FINITE-GUARD: bounded cycle exceeded its external guard\n')
    sys.exit(1)
"#
        ),
    );
    let settings = root.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings).expect("create settings directory");
    fs::write(
        settings.join("settings.json"),
        format!(
            r#"{{
  "defaults": {{
    "agent": "codex",
    "model": "fixture",
    "agent_timeout": "5s",
    "budget_threshold": {{"currency": "USD", "amount_micro": {threshold}}}
  }},
  "agents": {{
    "codex": {{
      "command": {command},
      "stdin_prompt": true,
      "timeout": "5s"
    }}
  }},
  "models": {{
    "fixture": {{
      "provider": "rhei-test",
      "model": "fixed-2000",
      "default_agent": "codex"
    }}
  }}
}}"#,
            threshold = limits.threshold_micro,
            command = fixture_command(&agent),
        ),
    )
    .expect("write settings");

    if initialized {
        seed_allowance(&root, &limits);
        write_fixture_qualification(&root, [true, true, true, true]);
    }
    BudgetCase { _dir: dir, root, plan, machine }
}

pub fn seed_allowance(root: &Path, limits: &Limits) {
    let budget_dir = root.join(".agent-grounds/rhei/budgets").join(PROJECT_UUID);
    fs::create_dir_all(&budget_dir).expect("create budget directory");
    fs::write(
        budget_dir.join("journal.jsonl"),
        format!(
            concat!(
                "{{\"schema\":\"rhei.budget.receipt.v1\",",
                "\"project_id\":\"panta:{project}\",",
                "\"sequence\":1,",
                "\"receipt_id\":\"receipt:00000000-0000-0000-0000-000000000001\",",
                "\"previous_hash\":null,",
                "\"written_at\":\"2026-09-18T12:00:00Z\",",
                "\"actor\":\"rhei-e2e-fixture\",",
                "\"kind\":\"initialize\",",
                "\"payload\":{{",
                "\"allowance\":{{\"invocations\":{invocations},",
                "\"spend\":{{\"currency\":\"USD\",\"amount_micro\":{spend}}}}},",
                "\"history\":{{\"status\":\"complete\",\"records\":0}},",
                "\"fixture_residual_micro\":{residual}",
                "}}}}\n"
            ),
            project = PROJECT_UUID,
            invocations = limits.invocations,
            spend = limits.spend_micro,
            residual = limits.residual_micro,
        ),
    )
    .expect("write budget journal");
    mirror_fixture_authority(root);
}

pub fn fixture_home(root: &Path) -> PathBuf {
    root.parent().expect("fixture parent").join(".home")
}

// Only fixture construction writes synthetic financial history; this never
// grants provider qualification to the production binary. §AR-neural-admission.8
fn mirror_fixture_authority(root: &Path) {
    let authority = fixture_home(root).join("state/rhei/budget-authority").join(PROJECT_UUID);
    fs::create_dir_all(&authority).expect("create fixture authority");
    fs::write(authority.join("history.lock"), "").expect("create fixture authority lock");
    fs::copy(
        root.join(".agent-grounds/rhei/budgets").join(PROJECT_UUID).join("journal.jsonl"),
        authority.join("history.jsonl"),
    )
    .expect("mirror fixture receipts");
}

/// Append one hash-linked public receipt to a fixture journal. Recovery tests
/// use the production format rather than a test-only side channel.
pub fn append_receipt(
    root: &Path,
    sequence: u64,
    receipt_id: &str,
    kind: &str,
    payload: serde_json::Value,
) {
    let journal = root.join(".agent-grounds/rhei/budgets").join(PROJECT_UUID).join("journal.jsonl");
    let mut contents = fs::read_to_string(&journal).expect("read fixture journal");
    let preceding = contents.lines().last().expect("journal has an initialization receipt");
    let previous_hash = format!("sha256:{:x}", Sha256::digest(preceding.as_bytes()));
    let receipt = serde_json::json!({
        "schema": "rhei.budget.receipt.v1",
        "project_id": format!("panta:{PROJECT_UUID}"),
        "sequence": sequence,
        "receipt_id": receipt_id,
        "previous_hash": previous_hash,
        "written_at": "2026-09-18T12:00:01Z",
        "actor": "rhei-e2e-fixture",
        "kind": kind,
        "payload": payload,
    });
    contents.push_str(&serde_json::to_string(&receipt).expect("serialize fixture receipt"));
    contents.push('\n');
    fs::write(journal, contents).expect("append fixture receipt");
    mirror_fixture_authority(root);
}

pub fn write_fixture_qualification(root: &Path, obligations: [bool; 4]) {
    let dir = root.join(".agent-grounds/rhei/qualifications");
    fs::create_dir_all(&dir).expect("create qualification fixture directory");
    fs::write(
        dir.join("rhei-test-fixed-2000.json"),
        format!(
            r#"{{
  "schema": "rhei.test-qualification.v1",
  "tuple": {{
    "agent": "codex",
    "provider": "rhei-test",
    "model": "fixed-2000",
    "billing": "synthetic-fixed"
  }},
  "grade": "contained",
  "fixture_only": true,
  "obligations": {{
    "capture_barrier": {},
    "committed_in_flight_maximum": {},
    "post_kill_exposure": {},
    "nested_fallback_bound": {}
  }},
  "residual_micro": 0
}}"#,
            obligations[0], obligations[1], obligations[2], obligations[3]
        ),
    )
    .expect("write qualification fixture");
}

pub fn run_case(case: &BudgetCase, extra: &[&str]) -> CliRun {
    run_case_with_binary(case, extra, budget_fixture_binary(), false)
}

pub fn run_release_case(case: &BudgetCase, extra: &[&str]) -> CliRun {
    run_case_with_binary(case, extra, rhei_binary(), true)
}

fn run_case_with_binary(
    case: &BudgetCase,
    extra: &[&str],
    binary: PathBuf,
    old_switch: bool,
) -> CliRun {
    let mut cmd = rhei_process_at(binary);
    if old_switch {
        cmd.env("RHEI_E2E_FIXTURE_QUALIFICATIONS", "1");
    }
    cmd.env("HOME", fixture_home(&case.root))
        .env("XDG_STATE_HOME", fixture_home(&case.root).join("state"))
        .arg("--state-machine")
        .arg(&case.machine)
        .arg("run")
        .arg(&case.plan)
        .arg("--no-tui")
        .arg("--no-callbacks");
    for arg in extra {
        cmd.arg(arg);
    }
    let output = bounded_budget_output(&mut cmd);
    CliRun::from(&output)
}

pub fn spawn_count(case: &BudgetCase) -> u64 {
    if let Ok(raw) = fs::read_to_string(case.root.join("fixture-invocations.txt")) {
        return raw.trim().parse().unwrap_or(0);
    }
    fs::read_dir(&case.root)
        .expect("read fixture root")
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("spawn-"))
        .count() as u64
}

pub fn combined(run: &CliRun) -> String {
    format!("{}{}", run.stdout, run.stderr)
}

pub fn parallel_case(prefix: &str, fanout: bool) -> BudgetCase {
    let dir = unique_temp_dir(prefix);
    let (root, plan) = if fanout {
        let root = dir.join("project");
        fs::create_dir_all(&root).expect("create fanout project");
        let plan = write_fixture_file(
            &root,
            "plan.rhei.md",
            &format!(
                r#"# Rhei: Atomic admission

---
budgetProjectId: {PROJECT_UUID}
metadata:
  tasks:
    1:
      budgetTicketId: {TICKET_UUID}
---

## Tasks

### Task 1: Atomic fanout
**State:** work
"#
            ),
        );
        (root, plan)
    } else {
        let root = dir.join("workspace");
        let tasks = root.join("tasks");
        fs::create_dir_all(&tasks).expect("create workspace task directory");
        fs::write(
            root.join("index.rhei.md"),
            format!(
                r#"# Rhei: Atomic admission

---
budgetProjectId: {PROJECT_UUID}
metadata:
  tasks:
    1:
      budgetTicketId: {TICKET_UUID}
    2:
      budgetTicketId: 550e8400-e29b-41d4-a716-446655440002
---
"#
            ),
        )
        .expect("write workspace index");
        fs::write(tasks.join("01-first.md"), "### Task 1: First competitor\n**State:** work\n")
            .expect("write first task");
        fs::write(tasks.join("02-second.md"), "### Task 2: Second competitor\n**State:** work\n")
            .expect("write second task");
        (root.clone(), root)
    };
    let execution = if fanout {
        r#"    all_targets:
      - "codex:rhei-test:fixed-2000-a"
      - "codex:rhei-test:fixed-2000-b""#
    } else {
        "    agent: codex\n    model: fixture"
    };
    let machine = write_fixture_file(
        &dir,
        "states.yaml",
        &format!(
            r#"name: atomic-admission
version: 1
models: [fixture, fixed-2000-a, fixed-2000-b]
states:
  work:
    description: Competing bounded work
    concurrent: true
{execution}
    agent_timeout: 5s
    budget_threshold:
      currency: USD
      amount_micro: 2000
  completed:
    final: true
    description: Done
profiles:
  work:
    initial: work
    allowed: [work, completed]
    transition_limit: 5
node_policy:
  root: work
  default: work
transitions:
  - from: work
    to: completed
"#
        ),
    );
    let agent = write_python_agent(
        &root,
        "atomic-agent.py",
        r#"root = pathlib.Path(env('RHEI_ROOT'))
marker = root / ('spawn-' + env('RHEI_TASK_ID') + '-' + env('RHEI_MODEL') + '.txt')
write(marker, 'started\n')
fixture_provider_call()
time.sleep(0.2)
result('## Result\n\nSynthetic bounded work completed.\n')
"#,
    );
    let settings = root.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings).expect("create settings directory");
    fs::write(
        settings.join("settings.json"),
        format!(
            r#"{{
  "agents": {{
    "codex": {{"command": {}, "stdin_prompt": true, "timeout": "5s"}}
  }},
  "models": {{
    "fixture": {{"provider": "rhei-test", "model": "fixed-2000", "default_agent": "codex"}},
    "fixed-2000-a": {{"provider": "rhei-test", "model": "fixed-2000-a", "default_agent": "codex"}},
    "fixed-2000-b": {{"provider": "rhei-test", "model": "fixed-2000-b", "default_agent": "codex"}}
  }}
}}"#,
            fixture_command(&agent)
        ),
    )
    .expect("write settings");

    let invocations = 1;
    seed_allowance(
        &root,
        &Limits {
            invocations,
            spend_micro: 10_000,
            transition_limit: 5,
            threshold_micro: 2_000,
            residual_micro: 0,
        },
    );
    write_fixture_qualification(&root, [true, true, true, true]);
    BudgetCase { _dir: dir, root, plan, machine }
}
