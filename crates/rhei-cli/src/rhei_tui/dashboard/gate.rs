//! The loopback server's two gate routes: `/transition-gate` releases a plain
//! gate and `/answer-gate` answers a briefed one. Both only transport the
//! request to the host's [`GateTransitionSink`], which owns validation,
//! callbacks, compare-and-swap writes, and audit entries. §FS-rhei-viz.5.1

use std::net::TcpStream;
use std::sync::Arc;

use super::http::{write_response, write_status};

/// Live dashboard transport for human gates: the release of a plain gate and
/// the answer to a briefed one. §FS-rhei-viz.5.1
/// The host (`rhei-cli`) owns validation, callbacks, compare-and-swap writes,
/// and audit entries; the loopback server only transports the request.
pub trait GateTransitionSink: Send + Sync {
    /// Transition `task_id` from `from` to `to`, returning the effective target
    /// state after callbacks, or a human-readable rejection reason.
    ///
    /// `result` is the operator's own account of the decision, carried through
    /// the move exactly as `rhei transition --result` carries it. It is
    /// optional on every hop and blank-is-none: the host refuses a terminal
    /// release that has neither a message nor a result already on disk, and the
    /// surfaces render that refusal rather than inventing one for the human.
    // §FS-rhei-viz.5.1.3 §FS-rhei-states.3.3
    fn transition_gate(
        &self,
        task_id: &str,
        from: &str,
        to: &str,
        result: Option<&str>,
    ) -> Result<String, String>;

    /// Why this sink can take no gate action at all, when it cannot: an
    /// attached surface whose run serves no control URL. A surface then opens
    /// no gate block and says this instead, before the operator picks or types
    /// anything. §FS-rhei-run-tui.1.5.5 §FS-rhei-run-headless.5
    fn unavailable(&self) -> Option<String> {
        None
    }

    /// Whether this sink answers briefed gates. A sink that does not leaves
    /// every gate on the release, and the release's refusal at a briefed gate
    /// stands. Surfaces ask on every frame, so an answer must be cheap.
    /// §FS-rhei-viz.5.1.2
    fn answers_gates(&self) -> bool {
        false
    }

    /// Answer `task_id`'s brief at `from` with `answers`, an answer document
    /// in JSON, as a dry run or for real. `Ok` is the document
    /// `rhei gate answer --json` prints, refusals included; `Err` is a reason
    /// the answer could not be attempted at all. §FS-rhei-viz.5.1.2
    fn answer_gate(
        &self,
        _task_id: &str,
        _from: &str,
        _answers: &str,
        _dry_run: bool,
    ) -> Result<serde_json::Value, String> {
        Err("gate answers are not available on this surface".to_string())
    }
}

/// Transport one explicit human gate transition to the host. The host owns
/// validation and plan writes; this route never rewrites the plan directly.
/// §FS-rhei-viz.5.1.3
pub(super) fn handle_gate_transition(
    stream: &mut TcpStream,
    gate_transition: Option<&Arc<dyn GateTransitionSink>>,
    body: &[u8],
) {
    #[derive(serde::Deserialize)]
    struct Req {
        #[serde(default)]
        task_id: String,
        #[serde(default)]
        from: String,
        #[serde(default)]
        to: String,
        /// The operator's account of the decision. Optional so an older page
        /// (or a non-terminal release with nothing to say) still posts a valid
        /// body. §FS-rhei-viz.5.1.3
        #[serde(default)]
        result: Option<String>,
    }
    let Ok(req) = serde_json::from_slice::<Req>(body) else {
        return write_status(stream, "400 Bad Request", b"invalid gate transition body");
    };
    if req.task_id.trim().is_empty() {
        return reply_gate_transition(stream, false, "", "missing task_id");
    }
    if req.from.trim().is_empty() {
        return reply_gate_transition(stream, false, "", "missing from");
    }
    if req.to.trim().is_empty() {
        return reply_gate_transition(stream, false, "", "missing to");
    }
    let Some(sink) = gate_transition else {
        return reply_gate_transition(
            stream,
            false,
            "",
            "gate transitions are not available on this surface",
        );
    };
    // Blank is none: a field the operator left empty must not become a result
    // entry that says nothing. §FS-rhei-viz.5.1.3 §FS-rhei-states.3.3
    let result = req.result.as_deref().map(str::trim).filter(|message| !message.is_empty());
    match sink.transition_gate(req.task_id.trim(), req.from.trim(), req.to.trim(), result) {
        Ok(effective_to) => reply_gate_transition(stream, true, &effective_to, ""),
        Err(reason) => reply_gate_transition(stream, false, "", &reason),
    }
}

fn reply_gate_transition(stream: &mut TcpStream, ok: bool, to: &str, error: &str) {
    let body = if ok {
        serde_json::json!({ "ok": true, "to": to })
    } else {
        serde_json::json!({ "ok": false, "error": error })
    };
    reply(stream, &body);
}

/// Transport one answer to a briefed gate. The reply is the document
/// `rhei gate answer --json` prints, refusals included, or `{ok: false,
/// error}` when the host cannot answer at all. §FS-rhei-viz.5.1.2
pub(super) fn handle_gate_answer(
    stream: &mut TcpStream,
    gate_transition: Option<&Arc<dyn GateTransitionSink>>,
    body: &[u8],
) {
    #[derive(serde::Deserialize)]
    struct Req {
        #[serde(default)]
        task_id: String,
        #[serde(default)]
        from: String,
        /// The answer document itself: `{version, request_digest, answers}`.
        #[serde(default)]
        answers: serde_json::Value,
        #[serde(default)]
        dry_run: bool,
    }
    let Ok(req) = serde_json::from_slice::<Req>(body) else {
        return write_status(stream, "400 Bad Request", b"invalid gate answer body");
    };
    let refused = |stream: &mut TcpStream, error: &str| {
        reply(stream, &serde_json::json!({ "ok": false, "error": error }));
    };
    if req.task_id.trim().is_empty() {
        return refused(stream, "missing task_id");
    }
    if req.from.trim().is_empty() {
        return refused(stream, "missing from");
    }
    if !req.answers.is_object() {
        return refused(stream, "missing answers: post the answer document as an object");
    }
    let Some(sink) = gate_transition.filter(|sink| sink.answers_gates()) else {
        return refused(stream, "gate answers are not available on this surface");
    };
    match sink.answer_gate(
        req.task_id.trim(),
        req.from.trim(),
        &req.answers.to_string(),
        req.dry_run,
    ) {
        Ok(document) => reply(stream, &document),
        Err(reason) => refused(stream, &reason),
    }
}

fn reply(stream: &mut TcpStream, body: &serde_json::Value) {
    match serde_json::to_vec(body) {
        Ok(bytes) => write_response(stream, "application/json", &bytes),
        Err(_) => write_response(stream, "application/json", br#"{"ok":false}"#),
    }
}
