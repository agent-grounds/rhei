//! The workspaces a *wrapped* agent runs in.
//!
//! A wrapper — a second account, a proxy, a `nix run`, a sandbox — is an
//! `agents.<id>` profile under an id of its own that names the built-in family
//! it belongs to. Every capability rhei selects per built-in agent is then
//! selected for it, and every one of them is observable with a fake script:
//! nothing here needs a real Claude Code or Codex on the machine.
//! §FS-rhei-agents.1.1.2

use std::fs;
use std::path::{Path, PathBuf};

use super::{fixture_command, unique_temp_dir, write_python_agent, TestDir};

/// The wrapper's own id. Deliberately not a built-in id: the point of the
/// ticket is that the extractor, the transport, and the recognition follow the
/// *declared family* rather than this name, while everything that names the
/// agent keeps naming this. §FS-rhei-agents.1.1.2
pub const WRAPPED_AGENT: &str = "cld";

/// One ticket with work left, so exactly one agent runs.
pub const WORKING_PLAN: &str = r#"# Rhei: Wrapped Agent

## Tasks

### Task 1: Do the work
**State:** work
"#;

/// A machine whose one working state targets the wrapper by a literal target,
/// so no model registry has to exist for the run to resolve a provider.
pub fn wrapped_machine(provider: &str, model: &str) -> String {
    format!(
        r#"name: agent-family
version: 1
states:
  work:
    initial: true
    description: Work to do
    target: {WRAPPED_AGENT}:{provider}:{model}
    agent_timeout: 60s
    attempts: 1
  completed:
    final: true
    description: Done
transitions:
  - from: work
    to: completed
"#
    )
}

/// A workspace whose only agent is the wrapper.
pub struct WrappedWorkspace {
    /// Held so the tree outlives the test that built it.
    pub _dir: TestDir,
    pub root: PathBuf,
    pub plan: PathBuf,
    pub machine: PathBuf,
}

impl WrappedWorkspace {
    /// Every invocation record the run wrote, parsed. An absent accounting root
    /// is no records rather than an error: that is what an unmeasured run
    /// leaves, and it is the floor this ticket must not move.
    /// §FS-rhei-cost-accounting.3.2
    pub fn records(&self) -> Vec<serde_json::Value> {
        self.runtime_files("runtime/accounting/invocations", "json")
            .into_iter()
            .map(|path| {
                let text = fs::read_to_string(&path).expect("read invocation record");
                serde_json::from_str(&text).expect("invocation record parses")
            })
            .collect()
    }

    pub fn accounting_root(&self) -> PathBuf {
        self.root.join("runtime/accounting")
    }

    /// The one agent log the run captured. §FS-rhei-agents.8.2
    pub fn agent_log(&self) -> String {
        let dir = self.root.join("runtime/logs");
        let mut logs = fs::read_dir(&dir)
            .unwrap_or_else(|err| panic!("read {}: {err}", dir.display()))
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("log"))
            .collect::<Vec<_>>();
        logs.sort();
        assert_eq!(logs.len(), 1, "expected one agent log under {}", dir.display());
        fs::read_to_string(&logs[0]).expect("read agent log")
    }

    /// Files of one extension under a runtime subdirectory, sorted; empty when
    /// the directory was never created.
    pub fn runtime_files(&self, relative: &str, extension: &str) -> Vec<PathBuf> {
        let dir = self.root.join(relative);
        let Ok(entries) = fs::read_dir(&dir) else { return Vec::new() };
        let mut paths = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|value| value.to_str()) == Some(extension))
            .collect::<Vec<_>>();
        paths.sort();
        paths
    }
}

/// Stand up a workspace whose sole agent is `WRAPPED_AGENT` over `script`.
///
/// `profile_extras` is merged into the profile object, so a caller writes
/// `{"family": "codex"}` to declare a family and `{}` to declare none — the
/// compatibility floor every other case is measured against.
pub fn wrapped_workspace(
    prefix: &str,
    script: &str,
    profile_extras: serde_json::Value,
    provider: &str,
    model: &str,
) -> WrappedWorkspace {
    let dir = unique_temp_dir(prefix);
    let root = dir.to_path_buf();
    let script_path = write_python_agent(&dir, "wrapped-agent.py", script);
    let mut profile = serde_json::json!({
        "command": serde_json::from_str::<serde_json::Value>(&fixture_command(&script_path))
            .expect("fixture command is JSON"),
        "stdin_prompt": true,
        "timeout": "60s",
    });
    let extras = profile_extras.as_object().expect("profile extras are an object").clone();
    profile.as_object_mut().expect("profile object").extend(extras);

    let settings_dir = root.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    fs::write(
        settings_dir.join("settings.json"),
        serde_json::json!({ "agents": { WRAPPED_AGENT: profile } }).to_string(),
    )
    .expect("write settings");

    let plan = root.join("plan.rhei.md");
    fs::write(&plan, WORKING_PLAN).expect("write plan");
    let machine = root.join("states.yaml");
    fs::write(&machine, wrapped_machine(provider, model)).expect("write machine");
    WrappedWorkspace { _dir: dir, root, plan, machine }
}

/// The one invocation record a measured run wrote.
pub fn one_record(workspace: &WrappedWorkspace) -> serde_json::Value {
    let records = workspace.records();
    assert_eq!(
        records.len(),
        1,
        "expected exactly one invocation record under {}; got {records:#?}",
        workspace.accounting_root().join("invocations").display()
    );
    records.into_iter().next().expect("one record")
}

/// Where a fake agent records the argument vector it was handed, so a test can
/// assert on the launch arguments the family's extractor requires rhei to add.
/// §FS-rhei-cost-accounting.4
pub const ARGV_FILE: &str = "wrapped-argv.json";

/// The argument vector the wrapped agent was launched with.
pub fn wrapped_argv(workspace: &WrappedWorkspace) -> Vec<String> {
    let path = workspace.root.join("runtime").join(ARGV_FILE);
    let text =
        fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    serde_json::from_str(&text).expect("recorded argv parses")
}

/// A fake Codex: the JSONL its extractor reads, and nothing else.
/// §FS-rhei-cost-accounting.4
pub const CODEX_DIALECT_AGENT: &str = r#"import json

write(pathlib.Path(env('RHEI_ROOT')) / 'runtime' / 'wrapped-argv.json', json.dumps(sys.argv[1:]))
print(json.dumps({'type': 'thread.started', 'thread_id': 'thread-family-332'}), flush=True)
print(json.dumps({
    'type': 'turn.completed',
    'usage': {
        'input_tokens': 120,
        'cached_input_tokens': 20,
        'cache_creation_input_tokens': 5,
        'output_tokens': 30,
    },
}), flush=True)
result('## Result\n\nWrapped agent finished.\n')
"#;

/// A fake Claude Code: the `stream-json` events its extractor and its session
/// report read. §FS-rhei-cost-accounting.4 §FS-rhei-session-reports.6.2
pub const CLAUDE_DIALECT_AGENT: &str = r#"import json

write(pathlib.Path(env('RHEI_ROOT')) / 'runtime' / 'wrapped-argv.json', json.dumps(sys.argv[1:]))
print(json.dumps({'type': 'system', 'subtype': 'init', 'session_id': 'session-family-332'}), flush=True)
print(json.dumps({
    'type': 'assistant',
    'message': {'content': [{'type': 'text', 'text': 'The wrapper spoke Claude.'}]},
}), flush=True)
print(json.dumps({
    'type': 'result',
    'subtype': 'success',
    'is_error': False,
    'result': 'The wrapper spoke Claude.',
    'usage': {
        'input_tokens': 123,
        'cache_read_input_tokens': 456,
        'cache_creation_input_tokens': 78,
        'output_tokens': 90,
    },
}), flush=True)
result('## Result\n\nWrapped Claude finished.\n')
"#;

/// A fake Claude Code that emits no envelope but leaves usage-shaped text in
/// its log — the one thing the Claude row forbids reading.
/// §FS-rhei-cost-accounting.4
pub const CLAUDE_LOG_TEXT_ONLY_AGENT: &str = r#"print('tokens used', flush=True)
print('12,345', flush=True)
result('## Result\n\nNo envelope, only prose.\n')
"#;

/// `path` exists, said with the path in the message.
pub fn assert_missing(path: &Path, why: &str) {
    assert!(!path.exists(), "{why}: {} exists", path.display());
}
