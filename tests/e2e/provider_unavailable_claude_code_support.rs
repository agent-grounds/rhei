//! A fake `claude-code` whose provider stops answering, and the runs that watch
//! rhei judge how its session ended.
//!
//! The streams are Claude Code 2.1.328's own `--output-format stream-json`
//! output, captured against a closed local port and a stub API answering one
//! status, with the retry count lowered to 3 and the `init` event cut down. Every
//! captured give-up exits `1`, so the fixture does unless a case says otherwise.
// §FS-rhei-provider-unavailability.2 §FS-rhei-agents.3.2.3

use super::provider_limit_support::*;
use super::provider_unavailable_pi_support::Pass;
use super::*;
use serde_json::{json, Value};

/// Connection refused on every try: `error_status: null`, 3 of 3.
pub(super) const REFUSED: &str =
    include_str!("fixtures/provider_unavailable_claude_code/refused.jsonl");
/// A stub that never answers, with `API_TIMEOUT_MS` lowered: `null`, 2 of 2.
pub(super) const HANG: &str = include_str!("fixtures/provider_unavailable_claude_code/hang.jsonl");
pub(super) const STATUS_429: &str =
    include_str!("fixtures/provider_unavailable_claude_code/status-429.jsonl");
pub(super) const STATUS_500: &str =
    include_str!("fixtures/provider_unavailable_claude_code/status-500.jsonl");
pub(super) const STATUS_529: &str =
    include_str!("fixtures/provider_unavailable_claude_code/status-529.jsonl");
/// An expired or revoked key: Claude Code retries it to the count all the same.
pub(super) const STATUS_401: &str =
    include_str!("fixtures/provider_unavailable_claude_code/status-401.jsonl");
/// An error Claude Code does not retry: no `api_retry` event at all.
pub(super) const STATUS_400: &str =
    include_str!("fixtures/provider_unavailable_claude_code/status-400.jsonl");

/// The text of [`REFUSED`]'s `is_api_error_message` event.
pub(super) const SIGNAL: &str =
    "API Error: Connection refused — a firewall or proxy may be blocking it (ECONNREFUSED)";

/// The qualified id of `SINGLE_TASK`'s one ticket in a fixture workspace.
pub(super) const TASK: &str = "workspace.1";

/// A custom `claude-code` profile with a resolved provider and a model the
/// built-in price book prices. §FS-rhei-run.3.3 §FS-rhei-cost-accounting.5.1
pub(super) const TARGET: &str = "cld:anthropic:claude-sonnet-4-6";

/// The same, on a model the price book has no entry for. §FS-rhei-budgets.6.2
pub(super) const UNPRICED_TARGET: &str = "cld:anthropic:claude-opus-9";

/// A bare model: Claude Code has no `<provider>/<model>` spelling, so nothing
/// keys a wait. §FS-rhei-provider-unavailability.2
pub(super) const TARGET_WITH_NO_PROVIDER: &str = "cld:claude-sonnet-4-6";

/// The marker the test writes when the provider is back: the next session
/// answers, writes the state's outputs and its result, and exits `0`.
pub(super) const PROVIDER_BACK: &str = "runtime/provider-back";

/// The marker a session writes once its stream is out, before it lingers.
pub(super) const STREAM_OUT: &str = "runtime/stream-out";

/// The stream's lines, each parsed.
fn events(stream: &str) -> Vec<Value> {
    stream.lines().map(|line| serde_json::from_str(line).unwrap()).collect()
}

fn join(events: &[Value]) -> String {
    events.iter().fold(String::new(), |mut stream, event| {
        stream.push_str(&event.to_string());
        stream.push('\n');
        stream
    })
}

fn is_retry(event: &Value) -> bool {
    event["type"] == "system" && event["subtype"] == "api_retry"
}

/// `stream` with every event `drop` selects left out.
pub(super) fn without(stream: &str, drop: impl Fn(&Value) -> bool) -> String {
    join(&events(stream).into_iter().filter(|event| !drop(event)).collect::<Vec<_>>())
}

/// `stream` with its last `api_retry` left out, so the count stops one short.
pub(super) fn under_the_count(stream: &str) -> String {
    let mut events = events(stream);
    let last = events.iter().rposition(is_retry).expect("a retry to drop");
    events.remove(last);
    join(&events)
}

/// The synthetic give-up event, `is_api_error_message: true`.
pub(super) fn is_give_up(event: &Value) -> bool {
    event["is_api_error_message"] == true
}

pub(super) fn is_result(event: &Value) -> bool {
    event["type"] == "result"
}

/// A real assistant turn from the model: the provider answered.
pub(super) fn answer() -> Value {
    json!({"type": "assistant", "message": {
        "id": "msg_answer", "model": "claude-sonnet-4-6", "role": "assistant", "type": "message",
        "content": [{"type": "text", "text": "Reading the ticket."}],
        "stop_reason": "end_turn",
        "usage": {"input_tokens": 0, "output_tokens": 0}
    }, "session_id": "fake"})
}

/// `stream` with a real answer before its first `api_retry`.
pub(super) fn answered_first(stream: &str) -> String {
    let mut events = events(stream);
    let first = events.iter().position(is_retry).expect("a retry");
    events.insert(first, answer());
    join(&events)
}

/// `stream` with its `result` text replaced by a recognized session limit.
pub(super) fn with_refusal(stream: &str) -> String {
    let mut events = events(stream);
    let result = events.iter_mut().find(|event| is_result(event)).expect("a result event");
    result["result"] = LIMIT_SIGNAL.into();
    join(&events)
}

/// The session a back provider gives: one answer and a successful result.
fn back_stream() -> String {
    let result = json!({"type": "result", "subtype": "success", "is_error": false,
        "result": "done", "session_id": "fake", "total_cost_usd": 0,
        "usage": {"input_tokens": 0, "output_tokens": 0,
                  "cache_creation_input_tokens": 0, "cache_read_input_tokens": 0}});
    join(&[answer(), result])
}

/// What one fixture `claude-code` does each time it is spawned, until the test
/// writes `PROVIDER_BACK`.
pub(super) struct Outage {
    pub family: &'static str,
    pub target: &'static str,
    pub stream: String,
    pub code: i32,
    /// The state's `agent_timeout`, when the session must outlive it.
    pub agent_timeout: Option<&'static str>,
    /// Seconds the session stays alive after its stream.
    pub linger: u64,
    /// The plan's tickets, every one in `working`.
    pub tasks: &'static [(&'static str, &'static str)],
    /// Whether `working` lets its tickets run at once, which the pool needs.
    pub concurrent: bool,
}

impl Outage {
    pub fn claude(stream: &str) -> Self {
        Self {
            family: "claude-code",
            target: TARGET,
            stream: stream.to_string(),
            code: 1,
            agent_timeout: None,
            linger: 0,
            tasks: SINGLE_TASK,
            concurrent: false,
        }
    }

    pub fn fixture(self, name: &str) -> ProviderFixture {
        let timeout = self
            .agent_timeout
            .map(|timeout| format!("    agent_timeout: {timeout}\n"))
            .unwrap_or_default();
        let concurrent = if self.concurrent { "    concurrent: true\n" } else { "" };
        let machine = format!(
            "name: claude-outage\nversion: 1\nstates:\n  working:\n    initial: true\n{concurrent}    \
             target: {target}\n    attempts: 2\n{timeout}    outputs:\n      - name: finding\n        \
             path: runtime/exports/{{task_id}}/finding.md\n  completed:\n    final: true\n\
             transitions:\n  - from: working\n    to: completed\n",
            target = self.target
        );
        let body = format!(
            r#"root = pathlib.Path(env('RHEI_ROOT'))
runtime = root / 'runtime'
append(runtime / 'starts.txt', env('RHEI_ATTEMPT') + '\n')
back = (root / {back:?}).exists()
if back:
    write(runtime / 'exports' / env('RHEI_TASK_ID') / 'finding.md', 'found\n')
    result('done\n')
sys.stdout.write({answered:?} if back else {stream:?})
sys.stdout.flush()
write(root / {out:?}, 'out\n')
time.sleep(0 if back else {linger})
raise SystemExit(0 if back else {code})
"#,
            back = PROVIDER_BACK,
            answered = back_stream(),
            stream = self.stream,
            out = STREAM_OUT,
            linger = self.linger,
            code = self.code,
        );
        let fixture = ProviderFixture::new(name, self.tasks, &machine, &body);
        let command: Value = serde_json::from_str(&fixture_command(&fixture.agent)).unwrap();
        fs::write(
            fixture.root.join(".agent-grounds/rhei/settings.json"),
            json!({"agents": {"cld": {
                "family": self.family,
                "command": command,
                "stdin_prompt": true,
                "timeout": "30s"
            }}})
            .to_string(),
        )
        .unwrap();
        fixture
    }
}

/// How a pass is run: through the single worker or the pool of two, and with or
/// without `--continue-on-error`. Every pass is `--until-idle`.
#[derive(Clone, Copy, Debug)]
pub(super) struct Mode {
    pub pool: bool,
    pub continue_on_error: bool,
}

/// The four modes, so the two paths and the flag cannot drift apart.
pub(super) const MODES: [Mode; 4] = [
    Mode { pool: false, continue_on_error: true },
    Mode { pool: false, continue_on_error: false },
    Mode { pool: true, continue_on_error: true },
    Mode { pool: true, continue_on_error: false },
];

impl Mode {
    /// The plan a mode runs: one ticket, or two that fill both pool slots.
    pub fn outage(self, outage: Outage) -> Outage {
        if self.pool {
            Outage { tasks: TWO_TASKS, concurrent: true, ..outage }
        } else {
            outage
        }
    }

    /// The tickets the mode's plan holds, local id and qualified id.
    pub fn tasks(self) -> &'static [(&'static str, &'static str)] {
        if self.pool {
            &[("1", "workspace.1"), ("2", "workspace.2")]
        } else {
            &[("1", "workspace.1")]
        }
    }

    pub fn name(self, case: &str) -> String {
        let pool = if self.pool { "pool" } else { "single" };
        let flag = if self.continue_on_error { "coe" } else { "strict" };
        format!("claude-{case}-{pool}-{flag}")
    }

    pub fn args(self) -> Vec<&'static str> {
        let mut args = vec![];
        if self.continue_on_error {
            args.push("--continue-on-error");
        }
        if self.pool {
            args.extend(["--parallel", "2"]);
        }
        args
    }
}

/// Two tickets, so a concurrent state fills two slots of the worker pool.
pub(super) const TWO_TASKS: &[(&str, &str)] = &[
    ("01.md", "### Task 1: Provider worker\n**State:** working\n"),
    ("02.md", "### Task 2: Its sibling\n**State:** working\n"),
];

/// Start one `--until-idle` pass, its stdout and stderr kept as `<name>.stdout`
/// and `<name>.stderr` in the test's directory.
pub(super) fn start(fixture: &ProviderFixture, name: &str, extra: &[&str]) -> RunningChild {
    let mut command = rhei_command(fixture.root.join(".home"));
    command.arg("--state-machine").arg(&fixture.machine).arg("run").arg(&fixture.root);
    command.args(["--no-tui", "--no-dashboard", "--json", "--until-idle"]);
    command.args(extra);
    RunningChild::spawn(&mut command, &fixture.dir, name)
}

/// What a started pass left once it exited.
pub(super) fn finish(fixture: &ProviderFixture, name: &str, mut run: RunningChild) -> Pass {
    let status = run.wait_for_exit(name);
    let events = fs::read_to_string(fixture.dir.join(format!("{name}.stdout")))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    Pass { code: status.code(), output: run.output(), events }
}

/// One pass in `mode`.
pub(super) fn run_in(fixture: &ProviderFixture, name: &str, mode: Mode) -> Pass {
    finish(fixture, name, start(fixture, name, &mode.args()))
}

/// One single-worker pass under `--continue-on-error`.
pub(super) fn run_once(fixture: &ProviderFixture, name: &str) -> Pass {
    run_in(fixture, name, MODES[0])
}

/// The spawn record `task` left, which must be exactly one.
pub(super) fn record_of(fixture: &ProviderFixture, pass: &Pass, task: &str) -> Value {
    let records: Vec<_> =
        fixture.records().into_iter().filter(|record| record["task"] == task).collect();
    assert_eq!(
        records.len(),
        1,
        "{task}: one invocation, one record: {records:#?}; {}",
        pass.said()
    );
    records.into_iter().next().unwrap()
}

/// The wait the ticket with local id `local` holds in `working`, or `Null`.
pub(super) fn task_wait(fixture: &ProviderFixture, local: &str) -> Value {
    metadata(&fixture.root)["metadata"]["tasks"][local]["providerLimits"]["working"].clone()
}

pub(super) fn starts(fixture: &ProviderFixture) -> usize {
    fs::read_to_string(fixture.root.join("runtime/starts.txt")).unwrap_or_default().lines().count()
}

/// The ending's warning for `task`, up to its charge clause. §FS-rhei-agents.3.2.1
pub(super) fn unavailable_warning(task: &str, signal: &str, charge: &str) -> String {
    format!(
        "warning: the provider was unavailable for task {task} in state 'working': \
         claude-code gave up retrying ({signal}); {charge}"
    )
}

/// An ending that is not recognized, as it is today: one record per ticket
/// with `ending`, charged as `charged` says, no wait, and every ticket where it
/// was. §FS-rhei-provider-unavailability.2
pub(super) fn assert_not_recognized(
    fixture: &ProviderFixture,
    pass: &Pass,
    mode: Mode,
    ending: &str,
    charged: bool,
) {
    for (_, task) in mode.tasks() {
        let record = record_of(fixture, pass, task);
        assert_eq!(record["ending"], ending, "{mode:?} {record:#}; {}", pass.said());
        assert_eq!(record["attempt_charged"], charged, "{mode:?} {record:#}");
        assert_eq!(record["charged"], u64::from(charged), "{mode:?} {record:#}");
        assert_eq!(record["unavailable"].as_u64().unwrap_or(0), 0, "{mode:?} {record:#}");
        assert!(record.get("signal").is_none(), "{mode:?} {record:#}");
    }
    let markdown = markdown_text(&fixture.root);
    assert!(!markdown.contains("providerLimits:"), "{mode:?}: no wait is written: {markdown}");
    pass.assert_never_says("the provider was unavailable");
    assert!(!pass.events.iter().any(|event| event["outcome"] == "provider_unavailable"));
    assert_all_tasks_in_state(&fixture.root, &fixture.machine, "working");
}

/// `rhei budget show` for the fixture's workspace, today.
pub(super) fn budget_show(fixture: &ProviderFixture) -> String {
    let output = rhei_command(fixture.root.join(".home"))
        .arg("budget")
        .arg("show")
        .arg(&fixture.root)
        .output()
        .expect("rhei budget show should run");
    assert!(output.status.success(), "{}", stderr(&output));
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// The record a visit's sixth unavailable ending leaves, `charged` of the
/// attempts already spent. Seeded rather than reached, as the pi cap test does.
pub(super) fn seed_ending(fixture: &ProviderFixture, unavailable: u64, charged: u64) {
    let spawns = fixture.root.join("runtime/spawns");
    fs::create_dir_all(&spawns).unwrap();
    let stem = format!("task-{TASK}-working-cld-anthropic-claude-sonnet-4-6");
    let log = fixture.root.join("runtime/logs").join(format!("{stem}-attempt{unavailable}.log"));
    let record = json!({
        "task": TASK, "state": "working", "moves": 0, "attempt": unavailable,
        "charged": charged, "attempt_charged": false, "kind": "agent", "worker": "cld",
        "log": log, "started": "2026-11-17T02:00:00Z", "ended": "2026-11-17T02:03:00Z",
        "duration": "3m", "code": 1, "ending": "provider_unavailable", "signal": SIGNAL,
        "unavailable": unavailable
    });
    fs::write(spawns.join(format!("{stem}.json")), format!("{record:#}\n")).unwrap();
}
