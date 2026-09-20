// §REQ-cross-platform.4

// The mock agents, programs, callbacks, and redactors the CLI's test harnesses
// stand up.
//
// They were `#!/bin/sh` scripts, which is the single reason the e2e target ran
// on Unix only: a fixture that writes a result file and exits with a code has
// nothing Unix about it. Python is installed on every GitHub runner and this
// repository already depends on it (`scripts/*.py`, the pre-commit hook), so a
// fixture written in Python needs no shell, no `chmod`, and no `.exe`.
//
// Shared by both harnesses; the comments are `//` because one of them pulls
// this file in with `include!`, where an inner doc comment cannot open a file.

/// The interpreter every fixture runs under: `python3`, or `python` where that
/// is the only name (which is how Windows spells it). Probed once — a probe per
/// fixture would spend a process per test.
pub fn python_command() -> &'static str {
    static PYTHON: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();
    PYTHON.get_or_init(|| {
        // `python` first on Windows: `python3` there is often the Microsoft
        // Store's launcher stub rather than an interpreter.
        let candidates = if cfg!(windows) { ["python", "python3"] } else { ["python3", "python"] };
        for candidate in candidates {
            let runs = std::process::Command::new(candidate)
                .arg("-c")
                .arg("pass")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|status| status.success())
                .unwrap_or(false);
            if runs {
                return candidate;
            }
        }
        panic!("these tests run their fixtures under Python: put `python3` or `python` on PATH")
    })
}

/// What a fixture body may assume without importing anything.
///
/// `env` reads the process environment with `${VAR:-default}` semantics. For
/// fake autonomous agents only, its five historical identity lookups resolve
/// the same values from the authoritative prompt; the actual process
/// environment stays empty, which tests that exercise isolation inspect with
/// `os.environ` directly. `write`, `append`, and `result` create parents as
/// `mkdir -p` did.

// §FS-rhei-agents.4
const FIXTURE_PRELUDE: &str = r#"import io
import os
import pathlib
import re
import sys
import time

# Only the isolated budget driver supplies this synthetic endpoint. Ordinary
# fixture execution and the release binary never enable it.
def fixture_provider_call():
    if not os.environ.get('RHEI_FIXTURE_BROKER_URL'):
        return
    import json
    import urllib.request
    _budget_body = json.dumps({
        'model': os.environ['RHEI_FIXTURE_MODEL'], 'input': 'deterministic fixture',
        'max_output_tokens': 1000, 'stream': False, 'store': False,
        'background': False, 'service_tier': 'default',
    }).encode('utf-8')
    _budget_request = urllib.request.Request(os.environ['RHEI_FIXTURE_BROKER_URL'],
        data=_budget_body, headers={
            'Content-Type': 'application/json',
            'Authorization': 'Bearer ' + os.environ['RHEI_FIXTURE_BROKER_TOKEN'],
        })
    with urllib.request.build_opener(urllib.request.ProxyHandler({})).open(_budget_request, timeout=3) as _response:
        assert json.load(_response)['status'] == 'completed'

# Rhei speaks UTF-8 with one `\n` per line on every platform, and Python does
# not: on Windows it decodes stdin in the host's code page — a prompt carrying
# an em dash comes back as lone surrogates that will not re-encode — and
# translates every `\n` it writes into `\r\n`. Both streams are pinned here so a
# fixture reads what the engine sent and writes bytes the engine can compare.
for _stream in (sys.stdin, sys.stdout, sys.stderr):
    if hasattr(_stream, 'reconfigure'):
        _stream.reconfigure(encoding='utf-8', newline='')


_agent_prompt = None


def agent_prompt():
    """Capture and replay the autonomous prompt, whether passed by flag or stdin."""
    global _agent_prompt
    if not os.environ.get('RHEI_ATTEMPT'):
        return ''
    if _agent_prompt is not None:
        return _agent_prompt
    for arg in sys.argv[1:]:
        if arg.startswith('# Task ') and '\n## State: ' in arg:
            _agent_prompt = arg
            return _agent_prompt
    _agent_prompt = sys.stdin.read()
    # Fixture bodies that inspect stdin must still see the prompt. More
    # importantly, child commands they launch cannot consume the only copy
    # before a later identity lookup needs it. §FS-rhei-agents.4
    sys.stdin = io.StringIO(_agent_prompt)
    return _agent_prompt


def agent_context(name, default=''):
    """Resolve removed autonomous identity fields from their prompt text."""
    prompt = agent_prompt()
    if not prompt:
        return default
    task = re.search(r'^# Task ([^:]+):', prompt, re.MULTILINE)
    plan = re.search(
        r'^You are working in a rhei-managed plan at `([^`]+)`\.$',
        prompt,
        re.MULTILINE,
    )
    root = re.search(r'^- This rhei: `([^`]+)`', prompt, re.MULTILINE)
    result_section = prompt.split('\n## Result\n', 1)
    result_path = None
    if len(result_section) == 2:
        result_path = re.search(r'^- `([^`]+)`$', result_section[1], re.MULTILINE)
    values = {
        'RHEI_TASK_ID': task.group(1) if task else '',
        'RHEI_PLAN_PATH': plan.group(1) if plan else '',
        'RHEI_ROOT': root.group(1) if root else '',
        'RHEI_RESULT_PATH': result_path.group(1) if result_path else '',
    }
    # Only one base is ever needed: the artifact root, which the prompt names
    # itself. A relative root is the run's way of saying the root is also the
    # working directory, so it is left as written. §FS-rhei-agents.4.1
    if not values['RHEI_ROOT'] and values['RHEI_PLAN_PATH']:
        plan_path = pathlib.Path(values['RHEI_PLAN_PATH'])
        values['RHEI_ROOT'] = str(plan_path.parent if plan_path.suffix else plan_path)
    if values['RHEI_RESULT_PATH']:
        result = pathlib.Path(values['RHEI_RESULT_PATH'])
        if not result.is_absolute():
            result = pathlib.Path(values['RHEI_ROOT']) / result
        values['RHEI_RESULT_PATH'] = str(result)
    task_id = values['RHEI_TASK_ID']
    values['RHEI_TASK_ID_LOCAL'] = task_id.split('.', 1)[-1] if '.' in task_id else task_id
    return values.get(name) or default


def env(name, default=''):
    """`${NAME:-default}`: an empty value counts as unset, as it does in sh."""
    value = os.environ.get(name)
    if value:
        return value
    if name in {
        'RHEI_ROOT',
        'RHEI_PLAN_PATH',
        'RHEI_RESULT_PATH',
        'RHEI_TASK_ID',
        'RHEI_TASK_ID_LOCAL',
    }:
        return agent_context(name, default)
    return default


def write(path, text):
    target = pathlib.Path(path)
    target.parent.mkdir(parents=True, exist_ok=True)
    with target.open('w', encoding='utf-8', newline='') as handle:
        handle.write(text)


def append(path, text):
    target = pathlib.Path(path)
    target.parent.mkdir(parents=True, exist_ok=True)
    with target.open('a', encoding='utf-8', newline='') as handle:
        handle.write(text)


def result(text):
    """The ticket's own account, at the prompt path for an agent. §FS-rhei-states.3.3

    Programs still receive the path in their environment. Not every invocation
    is given one, so an absent path remains nothing to write rather than an
    error.
    """
    path = env('RHEI_RESULT_PATH')
    if path:
        write(path, text)


# Preserve stdin-delivered context before the fixture body can launch a child
# process that inherits and consumes stdin. §FS-rhei-agents.4
agent_prompt()


"#;

/// Write `body` as a fixture script called `name` under `dir`, and return its
/// path. Every fixture in the suite goes through here, so the prelude and the
/// interpreter are decided in one place.
///
/// Quote the strings inside `body` with `'…'`. A fixture writes markdown, so a
/// `"` followed by `#` is common — and that sequence closes the Rust raw
/// literal the body is written in.
pub fn write_python_agent(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, format!("{FIXTURE_PRELUDE}{body}"))
        .expect("fixture script should be written");
    path
}

/// The `["python3", "<script>"]` command array that runs `script`, already
/// encoded.
///
/// Both elements go through `serde_json`, because a Windows path is full of
/// backslashes and a settings file that spells one literally reads them as
/// escapes. JSON is a subset of YAML, so a state machine's `program.command:`
/// takes the same string unchanged.
pub fn fixture_command(script: &std::path::Path) -> String {
    fixture_command_with_args(script, &[])
}

/// [`fixture_command`] with fixed arguments after the script.
pub fn fixture_command_with_args(script: &std::path::Path, args: &[&str]) -> String {
    let mut command = vec![python_command().to_string(), script.display().to_string()];
    command.extend(args.iter().map(|arg| (*arg).to_string()));
    serde_json::to_string(&command).expect("fixture command should serialize")
}

/// The command *line* that runs `script`, rather than the argument vector.
///
/// Two places take one: a `cli:` callback and a string-form `program:`. Both go
/// to the platform's own shell. A caller embedding this in YAML must
/// single-quote it: a Windows path is full of backslashes, and a double-quoted
/// YAML scalar reads those as escapes.
///
/// The script path is quoted the way the product quotes a path it prints for a
/// shell, because a line is re-parsed by that shell and a temporary directory
/// with a space in it would otherwise arrive as two arguments — which is a real
/// shape on Windows, where the per-user temp directory lives under a profile
/// name the user chose.

// §FS-rhei-programs.1.1 §FS-rhei-errors.2
pub fn fixture_command_line(script: &std::path::Path) -> String {
    format!(
        "{} {}",
        python_command(),
        rhei_core::platform::shell_quote(&script.display().to_string())
    )
}
