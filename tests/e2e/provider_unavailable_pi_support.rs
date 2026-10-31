//! A fake `pi` whose provider stops answering, and the runs that watch rhei
//! judge how its session ended.
//!
//! The streams are pi 0.83.0's `--mode json` events, cut down to what the
//! verdict reads: an assistant `message_end` per turn, an `auto_retry_start`
//! per retry, and the `auto_retry_end` pi emits once it stops retrying. pi
//! exits `0` in that mode however the session ended, so every fixture does.
// §FS-rhei-agents.2.5 §FS-rhei-agents.3.2.3

use super::provider_limit_support::*;
use super::*;
use serde_json::{json, Value};

/// pi's `finalError` when the transport, not the model, failed.
pub(super) const SIGNAL: &str = "fetch failed";

/// The qualified id of `SINGLE_TASK`'s one ticket in a fixture workspace.
pub(super) const TASK: &str = "workspace.1";

/// A custom profile of the `pi` family with a resolved provider, so the wait
/// has the identity it is keyed on (§FS-rhei-run.3.3).
pub(super) const TARGET: &str = "pi-local:openai-codex:gpt-6.1-sol";

/// The reporter's spelling: the provider lives only inside pi's model id.
pub(super) const TARGET_WITHOUT_PROVIDER: &str = "pi-local:openai-codex/gpt-6.1-sol";

/// pi's default `maxRetries`, which every `auto_retry_start` carries as
/// `maxAttempts`.
const MAX_ATTEMPTS: u64 = 3;

/// The marker the test writes when the provider is back: the next session
/// answers, writes the state's outputs and its result, and exits.
pub(super) const PROVIDER_BACK: &str = "runtime/provider-back";

pub(super) fn assistant(stop_reason: &str, error: Option<&str>) -> Value {
    let mut message = json!({
        "role": "assistant",
        "content": [],
        "api": "openai-codex-responses",
        "provider": "openai-codex",
        "model": "gpt-6.1-sol",
        "usage": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "totalTokens": 0,
                  "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0}},
        "stopReason": stop_reason,
        "timestamp": 0
    });
    if let Some(error) = error {
        message["errorMessage"] = error.into();
    }
    json!({"type": "message_end", "message": message})
}

pub(super) fn failed_turn(error: &str) -> Value {
    assistant("error", Some(error))
}

pub(super) fn retry_start(attempt: u64, error: &str) -> Value {
    json!({"type": "auto_retry_start", "attempt": attempt, "maxAttempts": MAX_ATTEMPTS,
           "delayMs": 2000 << attempt, "errorMessage": error})
}

pub(super) fn retry_end(success: bool, attempt: u64, final_error: Option<&str>) -> Value {
    let mut event = json!({"type": "auto_retry_end", "success": success, "attempt": attempt});
    if let Some(error) = final_error {
        event["finalError"] = error.into();
    }
    event
}

/// One session: pi's header, `events`, and the end of the agent run.
pub(super) fn session(events: &[Value]) -> String {
    let head = [
        json!({"type": "session", "version": 3, "id": "fake", "timestamp": "2026-10-29T00:00:00Z",
               "cwd": "/x"}),
        json!({"type": "agent_start"}),
    ];
    let tail = json!({"type": "agent_end", "messages": []});
    let mut stream = String::new();
    for event in head.iter().chain(events).chain([&tail]) {
        stream.push_str(&event.to_string());
        stream.push('\n');
    }
    stream
}

/// Every retry pi allows spent on `error`, after the `answered` turns that
/// ended without one.
pub(super) fn gave_up_after(answered: &[Value], error: &str) -> String {
    let mut events = answered.to_vec();
    for attempt in 1..=MAX_ATTEMPTS {
        events.push(failed_turn(error));
        events.push(retry_start(attempt, error));
    }
    events.push(failed_turn(error));
    events.push(retry_end(false, MAX_ATTEMPTS, Some(error)));
    session(&events)
}

/// The incident: the transport fails on the first turn and on every retry.
pub(super) fn gave_up() -> String {
    gave_up_after(&[], SIGNAL)
}

/// What one fixture `pi` does each time it is spawned, until the test writes
/// `PROVIDER_BACK`.
pub(super) struct Outage {
    pub family: &'static str,
    pub target: &'static str,
    pub stream: String,
    /// The state's `agent_timeout`, when the session must outlive it.
    pub agent_timeout: Option<&'static str>,
    /// Seconds the session stays alive after its stream.
    pub linger: u64,
    /// Whether the session writes the state's outputs whatever its stream says.
    pub writes_outputs: bool,
}

impl Outage {
    pub fn pi(stream: String) -> Self {
        Self {
            family: "pi",
            target: TARGET,
            stream,
            agent_timeout: None,
            linger: 0,
            writes_outputs: false,
        }
    }

    pub fn fixture(self, name: &str) -> ProviderFixture {
        let timeout = self
            .agent_timeout
            .map(|timeout| format!("    agent_timeout: {timeout}\n"))
            .unwrap_or_default();
        let machine = format!(
            "name: pi-outage\nversion: 1\nstates:\n  working:\n    initial: true\n    \
             target: {target}\n    attempts: 2\n{timeout}    outputs:\n      - name: finding\n        \
             path: runtime/exports/{{task_id}}/finding.md\n  completed:\n    final: true\n\
             transitions:\n  - from: working\n    to: completed\n",
            target = self.target
        );
        let answered = session(&[assistant("stop", None)]);
        let body = format!(
            r#"root = pathlib.Path(env('RHEI_ROOT'))
runtime = root / 'runtime'
append(runtime / 'starts.txt', env('RHEI_ATTEMPT') + '\n')
append(runtime / 'argv.txt', ' '.join(a for a in sys.argv[1:] if '\n' not in a) + '\n')
back = (root / {back:?}).exists()
if back or {writes_outputs}:
    write(runtime / 'exports' / env('RHEI_TASK_ID') / 'finding.md', 'found\n')
    result('done\n')
sys.stdout.write({answered:?} if back else {stream:?})
sys.stdout.flush()
time.sleep({linger})
raise SystemExit(0)
"#,
            back = PROVIDER_BACK,
            writes_outputs = if self.writes_outputs { "True" } else { "False" },
            stream = self.stream,
            linger = self.linger,
        );
        let fixture = ProviderFixture::new(name, SINGLE_TASK, &machine, &body);
        let command: Value = serde_json::from_str(&fixture_command(&fixture.agent)).unwrap();
        fs::write(
            fixture.root.join(".agent-grounds/rhei/settings.json"),
            json!({"agents": {"pi-local": {
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

/// One `--until-idle` pass, its stdout and stderr kept as `<name>.stdout` and
/// `<name>.stderr` in the test's directory.
pub(super) fn run_once(fixture: &ProviderFixture, name: &str) -> Pass {
    let mut command = rhei_command(fixture.root.join(".home"));
    // A nested fixture owns its admission and accounting context. §FS-rhei-budgets.5
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("RHEI_") {
            command.env_remove(key);
        }
    }
    command.arg("--state-machine").arg(&fixture.machine).arg("run").arg(&fixture.root);
    command.args(["--no-tui", "--no-dashboard", "--json", "--continue-on-error", "--until-idle"]);
    let mut run = RunningChild::spawn(&mut command, &fixture.dir, name);
    let status = run.wait_for_exit(name);
    let events = fs::read_to_string(fixture.dir.join(format!("{name}.stdout")))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    Pass { code: status.code(), output: run.output(), events }
}

/// What one pass of `rhei run` left: its exit code, its output for messages,
/// and its stdout events.
pub(super) struct Pass {
    pub code: Option<i32>,
    pub output: String,
    pub events: Vec<Value>,
}

impl Pass {
    pub fn event(&self, name: &str) -> Value {
        self.events
            .iter()
            .find(|event| event["event"] == name)
            .unwrap_or_else(|| panic!("the stream should carry {name}; {}", self.output))
            .clone()
    }

    pub fn stop(&self) -> Value {
        self.event("run_finished")["summary"]["stop"].clone()
    }

    /// The run's own lines - its warnings, notes and halts - which `--json`
    /// carries as `message` events, and whatever it wrote to stderr besides.
    pub fn said(&self) -> String {
        let messages = self.events.iter().filter(|event| event["event"] == "message");
        let stderr = self.output.lines().filter(|line| {
            serde_json::from_str::<Value>(line).is_err() && !line.starts_with("its std")
        });
        messages
            .filter_map(|event| event["text"].as_str())
            .chain(stderr)
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn assert_says(&self, line: &str) {
        let said = self.said();
        assert!(said.contains(line), "the run should say {line:?}; it said:\n{said}");
    }

    pub fn assert_never_says(&self, text: &str) {
        let said = self.said();
        assert!(!said.contains(text), "the run should not say {text:?}; it said:\n{said}");
    }
}

/// The one spawn record the fixture's one invocation leaves.
pub(super) fn only_record(fixture: &ProviderFixture, pass: &Pass) -> Value {
    let records = fixture.records();
    assert_eq!(records.len(), 1, "one invocation, one record: {records:#?}; {}", pass.said());
    records.into_iter().next().unwrap()
}

/// The wait the ticket holds in its state, or `Null` when it holds none.
pub(super) fn wait(fixture: &ProviderFixture) -> Value {
    metadata(&fixture.root)["metadata"]["tasks"]["1"]["providerLimits"]["working"].clone()
}

/// Seconds from `observedAt` to `nextAttemptAt`: the backoff the wait chose.
/// Both are read to the second, so a fractional `observedAt` may cost one.
pub(super) fn backoff(wait: &Value) -> i64 {
    let at = |field: &str| {
        let text = wait[field].as_str().unwrap_or_else(|| panic!("{field} in {wait:#}"));
        let n = |from: usize, to: usize| -> u8 {
            text.get(from..to)
                .and_then(|digits| digits.parse::<u16>().ok())
                .map(|value| value as u8)
                .unwrap_or_else(|| panic!("{field} {text:?} is not an RFC 3339 instant"))
        };
        let year = text[0..4].parse::<i32>().unwrap();
        let month = time::Month::try_from(n(5, 7)).unwrap();
        let date = time::Date::from_calendar_date(year, month, n(8, 10)).unwrap();
        let clock = time::Time::from_hms(n(11, 13), n(14, 16), n(17, 19)).unwrap();
        time::PrimitiveDateTime::new(date, clock).assume_utc().unix_timestamp()
    };
    at("nextAttemptAt") - at("observedAt")
}

/// The wait chose `minutes` of backoff, to the second.
pub(super) fn assert_backoff(wait: &Value, minutes: i64) {
    let seconds = backoff(wait);
    assert!((minutes * 60 - seconds).abs() <= 1, "{minutes}m backoff, got {seconds}s: {wait:#}");
}

pub(super) fn starts(fixture: &ProviderFixture) -> usize {
    fs::read_to_string(fixture.root.join("runtime/starts.txt")).unwrap_or_default().lines().count()
}

/// The ending's warning up to its charge clause. §FS-rhei-agents.3.2.1
pub(super) fn unavailable_warning(charge: &str) -> String {
    format!(
        "warning: the provider was unavailable for task {TASK} in state 'working': \
         pi gave up retrying ({SIGNAL}); {charge}"
    )
}

/// What every ending that is not recognized leaves, and what it is today: one
/// charged `exited` record, no wait, the ticket where it was. §FS-rhei-agents.2.5
pub(super) fn assert_charged_as_today(fixture: &ProviderFixture, pass: &Pass, ending: &str) {
    let record = only_record(fixture, pass);
    assert_eq!(record["ending"], ending, "{record:#}; {}", pass.said());
    assert_eq!(record["attempt_charged"], true, "{record:#}");
    assert_eq!(record["charged"], 1, "{record:#}");
    assert_eq!(record["unavailable"].as_u64().unwrap_or(0), 0, "{record:#}");
    let markdown = markdown_text(&fixture.root);
    assert!(!markdown.contains("providerLimits:"), "no wait is written: {markdown}");
    pass.assert_never_says("the provider was unavailable");
    assert!(!pass.events.iter().any(|event| event["outcome"] == "provider_unavailable"));
}
