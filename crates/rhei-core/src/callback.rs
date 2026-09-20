//! Callback execution interface for state transition rules.
//!
//! When a transition rule declares `on_leave` or `on_enter` callbacks, a
//! [`CallbackExecutor`] is responsible for resolving and invoking them.
//!
//! The [`ShellCallbackExecutor`] implementation runs callbacks as shell
//! commands, passing transition context via environment variables and
//! JSON on stdin, and parsing a JSON `TransitionResult` on stdout per the
//! transitions specification.

use crate::ast::CallbackRef;
use crate::platform::{plain_path, system_shell_command};
use serde_json::Value as JsonValue;
use std::io::Write;
use std::path::Path;
use std::process::Stdio;

/// Context provided to a callback during a state transition.
///
/// `context_json`, when present, is serialized and piped to the callback
/// process as its stdin payload — matching the cross-language
/// `TransitionContext` shape documented in the transitions specification.
/// Callers that do not have the full context (e.g., unit tests) may pass
/// `None` to skip stdin delivery; callbacks then rely solely on environment
/// variables.
#[derive(Debug, Clone)]
pub struct CallbackContext<'a> {
    /// Project-qualified identifier of the task being transitioned
    /// (`auth.1`). §FS-rhei-panta.6
    pub task_id: &'a str,
    /// The same ticket's id as written in its rhei file's heading (`1`).
    /// Matches what scripts that edit or grep the plan file need.
    pub task_id_local: &'a str,
    /// State the task is leaving.
    pub from_state: &'a str,
    /// State the task is entering.
    pub to_state: &'a str,
    /// Opaque identity of this callback-bearing transition firing, when the
    /// caller is Rhei's shared transition executor. §FS-rhei-transitions.1.2
    pub firing_id: Option<&'a str>,
    /// Central transition-ledger status for this firing.
    /// §FS-rhei-transitions.1.2
    pub ledger_status: Option<TransitionLedgerStatus>,
    /// Path to the plan file.
    pub plan_path: &'a Path,
    /// Working directory used to execute shell callbacks.
    pub callback_cwd: &'a Path,
    /// Current model identifier when a state declares `all_models` or `model`, or `None`.
    pub model: Option<&'a str>,
    /// Current agent identifier when a state declares `agent`, or `None`.
    pub agent: Option<&'a str>,
    /// Full `TransitionContext` payload to deliver on stdin, if available.
    pub context_json: Option<&'a JsonValue>,
}

/// Callback-visible status of a transition firing in the central ledger.
/// §FS-rhei-transitions.1.2
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionLedgerStatus {
    /// The firing's row has not been appended yet.
    Pending,
}

impl TransitionLedgerStatus {
    /// Canonical callback representation of this status.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
        }
    }
}

/// Outcome of a callback invocation.
///
/// Mirrors the cross-language `TransitionResult` shape: `success`, optional
/// `error` message, optional `next_state` redirect, and optional `data`
/// blob to forward as `transitionData` to downstream callbacks.
#[derive(Debug, Clone)]
pub struct CallbackResult {
    /// Whether the callback approves the transition proceeding.
    pub success: bool,
    /// Error message explaining why the transition was rejected.
    /// Populated when `success` is false, or when the callback crashed
    /// (non-zero exit). `None` on success.
    pub error: Option<String>,
    /// Redirected target state, only valid when `success` is true.
    /// `None` means "proceed to the originally requested target".
    pub next_state: Option<String>,
    /// Data emitted by the callback to flow into the next callback's
    /// `transitionData` field.
    pub data: Option<JsonValue>,
    /// Stdout captured from the callback (if any).
    pub stdout: String,
    /// Stderr captured from the callback (if any).
    pub stderr: String,
}

impl CallbackResult {
    /// Implicit success — used when a callback emits no parseable payload.
    fn implicit_success(stdout: String, stderr: String) -> Self {
        Self { success: true, error: None, next_state: None, data: None, stdout, stderr }
    }
}

/// Error returned when a callback cannot be executed.
#[derive(Debug)]
pub enum CallbackError {
    /// A callback had no qualified admission capability. §FS-rhei-budgets.9
    BudgetRefused(String),
    /// The callback identifier has no recognized platform prefix.
    UnknownPlatform(String),
    /// The callback command could not be started.
    SpawnFailed(String, std::io::Error),
    /// The callback command was started but writing to its stdin failed.
    StdinWriteFailed(String, std::io::Error),
}

impl std::fmt::Display for CallbackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallbackError::BudgetRefused(reason) => write!(f, "budget admission halted: {reason}"),
            CallbackError::UnknownPlatform(id) => {
                write!(f, "unknown callback platform in '{id}' (expected prefix like 'cli:')")
            }
            CallbackError::SpawnFailed(cmd, err) => {
                write!(f, "failed to execute callback '{cmd}': {err}")
            }
            CallbackError::StdinWriteFailed(cmd, err) => {
                write!(f, "failed to write context to callback '{cmd}' stdin: {err}")
            }
        }
    }
}

impl std::error::Error for CallbackError {}

/// Trait for executing transition callbacks.
///
/// Implementations resolve a [`CallbackRef`] identifier and invoke the
/// corresponding callback, passing the [`CallbackContext`] as input.
pub trait CallbackExecutor {
    /// Execute a callback and return its result.
    fn execute(
        &self,
        callback: &CallbackRef,
        context: &CallbackContext<'_>,
    ) -> Result<CallbackResult, CallbackError>;
}

/// Executes `cli:`-prefixed callbacks as shell commands.
///
/// The command portion after `cli:` is run via the platform's shell from
/// [`CallbackContext::callback_cwd`]. The runtime delivers two channels:
///
/// - **Environment variables** (always present):
///   - `RHEI_TASK_ID` — the project-qualified task identifier (`auth.1`)
///   - `RHEI_TASK_ID_LOCAL` — the task id as written in its rhei file (`1`)
///   - `RHEI_FROM_STATE` — the state being left
///   - `RHEI_TO_STATE` — the state being entered
///   - `RHEI_TRANSITION_FIRING_ID` — opaque transition firing identity
///   - `RHEI_TRANSITION_LEDGER_STATUS` — central-ledger status (`pending`)
///   - `RHEI_PLAN_PATH` — path to the plan file
///   - `RHEI_MODEL` — model identifier when the state declares one
///   - `RHEI_AGENT` — agent identifier when the state declares one
///
/// - **Stdin JSON**: when `context.context_json` is `Some`, the full
///   `TransitionContext` is serialized and written to the child's stdin.
///
/// The callback is expected to emit a `TransitionResult` JSON object on
/// stdout (`{ success, error?, nextState?, data? }`). Exit-code semantics:
///
/// - **Zero** + parseable JSON → the parsed result is returned as-is,
///   except that `success: false` combined with `nextState` is downgraded
///   to rejection per the spec.
/// - **Zero** + empty stdout → implicit success (compatible with callbacks
///   that don't yet speak the TransitionResult protocol).
/// - **Zero** + non-empty stdout that does not parse as JSON → implicit
///   success; the raw stdout is preserved for logging but not interpreted.
/// - **Non-zero** → rejection with an error synthesized from stderr. This
///   matches the spec: "exit code non-zero = callback crashed".
pub struct ShellCallbackExecutor;

impl CallbackExecutor for ShellCallbackExecutor {
    fn execute(
        &self,
        callback: &CallbackRef,
        context: &CallbackContext<'_>,
    ) -> Result<CallbackResult, CallbackError> {
        let id = &callback.0;

        let command =
            id.strip_prefix("cli:").ok_or_else(|| CallbackError::UnknownPlatform(id.clone()))?;

        // §FS-rhei-programs.1.1: the platform's shell, not always `sh`.
        let mut cmd = system_shell_command(command);
        // `cmd.exe` refuses a verbatim working directory. §REQ-cross-platform.5
        cmd.current_dir(plain_path(context.callback_cwd.to_path_buf()))
            .env("RHEI_TASK_ID", context.task_id)
            .env("RHEI_TASK_ID_LOCAL", context.task_id_local)
            .env("RHEI_FROM_STATE", context.from_state)
            .env("RHEI_TO_STATE", context.to_state)
            .env("RHEI_PLAN_PATH", context.plan_path.as_os_str())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(model) = context.model {
            cmd.env("RHEI_MODEL", model);
        }
        if let Some(agent) = context.agent {
            cmd.env("RHEI_AGENT", agent);
        }
        // Callback-only delivery mirrors the canonical JSON transition fields.
        // §FS-rhei-transitions.4.7
        if let Some(firing_id) = context.firing_id {
            cmd.env("RHEI_TRANSITION_FIRING_ID", firing_id);
        }
        if let Some(ledger_status) = context.ledger_status {
            cmd.env("RHEI_TRANSITION_LEDGER_STATUS", ledger_status.as_str());
        }

        // Shell callbacks are neural-capable, including through embedded
        // callers. A raw shell has no qualified launch capability.
        // §AR-neural-admission.1 §FS-rhei-budgets.4
        crate::budget::refuse_unqualified_spawn()
            .map_err(|e| CallbackError::BudgetRefused(e.to_string()))?;
        let mut child =
            cmd.spawn().map_err(|e| CallbackError::SpawnFailed(command.to_string(), e))?;

        if let Some(ctx_json) = context.context_json {
            let payload = serde_json::to_vec(ctx_json).unwrap_or_else(|_| b"{}".to_vec());
            if let Some(mut stdin) = child.stdin.take() {
                match stdin.write_all(&payload) {
                    Ok(()) => {}
                    // `BrokenPipe` means the callback closed its stdin
                    // before we finished writing (e.g. `exit 1` / scripts
                    // that don't consume stdin). That's not an executor
                    // failure — the child still ran and its exit status /
                    // stdout / stderr are the truth of the matter.
                    Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {}
                    Err(e) => {
                        return Err(CallbackError::StdinWriteFailed(command.to_string(), e));
                    }
                }
                // Dropping stdin here closes the pipe so the callback sees EOF.
            }
        } else {
            // No context payload — still close the callback's stdin so reads
            // (e.g. `cat`) return immediately instead of blocking.
            drop(child.stdin.take());
        }

        let output = child
            .wait_with_output()
            .map_err(|e| CallbackError::SpawnFailed(command.to_string(), e))?;

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

        if !output.status.success() {
            let code = output.status.code();
            let tail = stderr.trim();
            let error = match (code, tail.is_empty()) {
                (Some(c), true) => format!("callback crashed (exit {c})"),
                (Some(c), false) => format!("callback crashed (exit {c}): {tail}"),
                (None, true) => "callback terminated by signal".to_string(),
                (None, false) => format!("callback terminated by signal: {tail}"),
            };
            return Ok(CallbackResult {
                success: false,
                error: Some(error),
                next_state: None,
                data: None,
                stdout,
                stderr,
            });
        }

        Ok(parse_callback_stdout(&stdout, stderr.clone())
            .unwrap_or_else(|| CallbackResult::implicit_success(stdout.clone(), stderr)))
    }
}

/// Parse the stdout of a CLI callback as a `TransitionResult`.
///
/// Returns `None` when the payload is empty or not a recognizable
/// `TransitionResult` JSON object, signalling to the caller that it
/// should fall back to the implicit-success behavior.
fn parse_callback_stdout(stdout: &str, stderr: String) -> Option<CallbackResult> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return None;
    }
    let value: JsonValue = serde_json::from_str(trimmed).ok()?;
    let object = value.as_object()?;
    let success = object.get("success")?.as_bool()?;

    let error = object.get("error").and_then(|v| v.as_str()).map(str::to_string);
    let next_state = object.get("nextState").and_then(|v| v.as_str()).map(str::to_string);
    let data = object.get("data").cloned();

    // Spec invariant: `success: false` with `nextState` is invalid and is
    // downgraded to rejection. Never forward a redirect from a rejection.
    let next_state = if success { next_state } else { None };
    let error = if success {
        error
    } else {
        error.or_else(|| Some("transition rejected by callback".to_string()))
    };

    Some(CallbackResult { success, error, next_state, data, stdout: stdout.to_string(), stderr })
}

/// A no-op executor that skips all callbacks (used with `--no-callbacks`).
pub struct NoopCallbackExecutor;

impl CallbackExecutor for NoopCallbackExecutor {
    fn execute(
        &self,
        _callback: &CallbackRef,
        _context: &CallbackContext<'_>,
    ) -> Result<CallbackResult, CallbackError> {
        Ok(CallbackResult {
            success: true,
            error: None,
            next_state: None,
            data: None,
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::process::Command;

    /// The interpreter these callbacks are written in.
    ///
    /// A callback is a command line for the platform's own shell, and the two
    /// shells share almost no vocabulary: `printf`, `true`, and `$VAR` are `sh`,
    /// not `cmd`. Python is on both, so what a callback *does* can be pinned
    /// once instead of twice. Every command below stays inside one pair of
    /// double quotes and uses `'…'` for its own strings, which both shells hand
    /// through unchanged.
    fn python() -> &'static str {
        static PYTHON: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();
        PYTHON.get_or_init(|| {
            let candidates =
                if cfg!(windows) { ["python", "python3"] } else { ["python3", "python"] };
            for candidate in candidates {
                let runs = Command::new(candidate)
                    .arg("-c")
                    .arg("pass")
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .map(|status| status.success())
                    .unwrap_or(false);
                if runs {
                    return candidate;
                }
            }
            panic!(
                "these tests run their callbacks under Python: put `python3` or `python` on PATH"
            )
        })
    }

    /// `cli:<python> -c "<code>"`.
    fn python_callback(code: &str) -> CallbackRef {
        CallbackRef(format!("cli:{} -c \"{code}\"", python()))
    }

    fn ctx<'a>(plan_path: &'a Path, cwd: &'a Path) -> CallbackContext<'a> {
        CallbackContext {
            task_id: "1",
            task_id_local: "1",
            from_state: "pending",
            to_state: "in-progress",
            firing_id: None,
            ledger_status: None,
            plan_path,
            callback_cwd: cwd,
            model: None,
            agent: None,
            context_json: None,
        }
    }

    #[test]
    fn shell_executor_rejects_non_cli_prefix() {
        let executor = ShellCallbackExecutor;
        let callback = CallbackRef("js:someFunction".to_string());
        let context = ctx(Path::new("plan.rhei.md"), Path::new("."));

        let err = executor.execute(&callback, &context).unwrap_err();
        assert!(matches!(err, CallbackError::UnknownPlatform(_)));
        assert!(err.to_string().contains("js:someFunction"));
    }

    #[test]
    fn shell_executor_runs_successful_command_with_empty_stdout() {
        let executor = ShellCallbackExecutor;
        let callback = python_callback("pass");
        let context = ctx(Path::new("plan.rhei.md"), Path::new("."));

        let result = executor.execute(&callback, &context).unwrap();
        assert!(result.success);
        assert!(result.error.is_none());
        assert!(result.next_state.is_none());
        assert!(result.data.is_none());
    }

    #[test]
    fn shell_executor_treats_nonjson_stdout_as_implicit_success() {
        let executor = ShellCallbackExecutor;
        let callback = CallbackRef("cli:echo hello".to_string());
        let context = ctx(Path::new("plan.rhei.md"), Path::new("."));

        let result = executor.execute(&callback, &context).unwrap();
        assert!(result.success);
        assert!(result.error.is_none());
        assert_eq!(result.stdout.trim(), "hello");
    }

    #[test]
    fn shell_executor_reports_failure_on_nonzero_exit() {
        let executor = ShellCallbackExecutor;
        let callback = CallbackRef("cli:exit 1".to_string());
        let context = ctx(Path::new("plan.rhei.md"), Path::new("."));

        let result = executor.execute(&callback, &context).unwrap();
        assert!(!result.success);
        let error = result.error.as_deref().unwrap_or("");
        assert!(error.contains("exit 1"), "unexpected error: {error}");
    }

    #[test]
    fn shell_executor_parses_success_json_result() {
        let executor = ShellCallbackExecutor;
        let callback = python_callback(
            "import json,sys;sys.stdout.write(json.dumps({'success': True, 'data': {'k': 'v'}}))",
        );
        let context = ctx(Path::new("plan.rhei.md"), Path::new("."));

        let result = executor.execute(&callback, &context).unwrap();
        assert!(result.success);
        assert_eq!(result.data.unwrap(), json!({"k": "v"}));
        assert!(result.next_state.is_none());
    }

    #[test]
    fn shell_executor_parses_rejection_with_error_message() {
        let executor = ShellCallbackExecutor;
        let callback = python_callback(
            "import json,sys;sys.stdout.write(json.dumps({'success': False, 'error': 'dep missing'}))",
        );
        let context = ctx(Path::new("plan.rhei.md"), Path::new("."));

        let result = executor.execute(&callback, &context).unwrap();
        assert!(!result.success);
        assert_eq!(result.error.as_deref(), Some("dep missing"));
        assert!(result.next_state.is_none());
    }

    #[test]
    fn shell_executor_parses_next_state_redirect() {
        let executor = ShellCallbackExecutor;
        let callback = python_callback(
            "import json,sys;sys.stdout.write(json.dumps({'success': True, 'nextState': 'rejected'}))",
        );
        let context = ctx(Path::new("plan.rhei.md"), Path::new("."));

        let result = executor.execute(&callback, &context).unwrap();
        assert!(result.success);
        assert_eq!(result.next_state.as_deref(), Some("rejected"));
    }

    #[test]
    fn shell_executor_downgrades_rejection_with_next_state() {
        let executor = ShellCallbackExecutor;
        let callback = python_callback(
            "import json,sys;sys.stdout.write(json.dumps({'success': False, 'nextState': 'somewhere'}))",
        );
        let context = ctx(Path::new("plan.rhei.md"), Path::new("."));

        let result = executor.execute(&callback, &context).unwrap();
        assert!(!result.success);
        assert!(result.next_state.is_none());
    }

    #[test]
    fn shell_executor_passes_env_vars() {
        let executor = ShellCallbackExecutor;
        let callback = python_callback(
            "import os;print(os.environ['RHEI_TASK_ID'], os.environ['RHEI_FROM_STATE'], os.environ['RHEI_TO_STATE'])",
        );
        let mut context = ctx(Path::new("my-plan.rhei.md"), Path::new("."));
        context.task_id = "42";

        let result = executor.execute(&callback, &context).unwrap();
        assert!(result.success);
        assert_eq!(result.stdout.trim(), "42 pending in-progress");
    }

    #[test]
    fn shell_executor_delivers_context_json_on_stdin() {
        let executor = ShellCallbackExecutor;
        let callback =
            python_callback("import json,sys;sys.stdout.write(json.load(sys.stdin)['task']['id'])");
        let payload = json!({
            "task": { "id": "99", "title": "demo" },
            "transition": { "from": "pending", "to": "in-progress" },
        });
        let mut context = ctx(Path::new("plan.rhei.md"), Path::new("."));
        context.context_json = Some(&payload);

        let result = executor.execute(&callback, &context).unwrap();
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.stdout.trim_end(), "99");
    }

    #[test]
    fn noop_executor_always_succeeds() {
        let executor = NoopCallbackExecutor;
        let callback = CallbackRef("cli:anything".to_string());
        let context = ctx(Path::new("plan.rhei.md"), Path::new("."));

        let result = executor.execute(&callback, &context).unwrap();
        assert!(result.success);
        assert!(result.error.is_none());
    }
}
