//! The one route that reaches a running agent: `/intervene` hands a message
//! to the host's [`InterveneSink`], which delivers it to the agent's stdin and
//! keeps the audit trail. §AR-rhei-viz-flow.7

use std::net::TcpStream;
use std::sync::Arc;

use super::http::{write_response, write_status};

/// The single inbound, state-changing-adjacent path (§AR-rhei-viz-flow.7): deliver a message
/// to the stdin of the agent running one task, and **nothing else**. There is
/// no path from the loopback server to plan-state mutation. The host
/// (`rhei-cli`) implements delivery, the agent-capability gate, and the durable
/// audit trail; the server is a thin pass-through.
pub trait InterveneSink: Send + Sync {
    /// Deliver `message` to the selected running agent. `slot` disambiguates
    /// concurrent fanout invocations of the same task; `task_id` remains a
    /// fallback for single-invocation callers. `Ok(())` on delivery;
    /// `Err(reason)` when the agent is not interactively reachable (one-shot
    /// agent, closed stdin, or no matching running task).
    fn deliver(
        &self,
        task_id: Option<&str>,
        slot: Option<crate::rhei_tui::event::Slot>,
        message: &str,
    ) -> Result<(), String>;

    /// Whether the named running task — optionally a specific fanout `slot` — has
    /// a writable agent stdin registered, so a `/intervene` message can be
    /// delivered now. Gates the live composer; defaults to `false`. §FS-rhei-viz.5
    fn reachable(&self, _task_id: &str, _slot: Option<crate::rhei_tui::event::Slot>) -> bool {
        false
    }
}

/// §AR-rhei-viz-flow.7: deliver a `POST /intervene` message to one agent's stdin via the
/// host's [`InterveneSink`]. This route never transitions a task, writes the
/// plan, or mutates task metadata — it only hands bytes to the sink, which owns
/// delivery and the durable audit trail.
pub(super) fn handle_intervene(
    stream: &mut TcpStream,
    intervene: Option<&Arc<dyn InterveneSink>>,
    body: &[u8],
) {
    #[derive(serde::Deserialize)]
    struct Req {
        #[serde(default)]
        task_id: String,
        #[serde(default)]
        slot: Option<crate::rhei_tui::event::Slot>,
        #[serde(default)]
        message: String,
    }
    let Ok(req) = serde_json::from_slice::<Req>(body) else {
        return write_status(stream, "400 Bad Request", b"invalid intervene body");
    };
    if req.message.trim().is_empty() {
        return reply_intervene(stream, false, "empty message");
    }
    if req.task_id.is_empty() && req.slot.is_none() {
        return reply_intervene(stream, false, "missing task_id");
    }
    let Some(sink) = intervene else {
        return reply_intervene(stream, false, "intervene is not available on this surface");
    };
    let task_id = (!req.task_id.is_empty()).then_some(req.task_id.as_str());
    match sink.deliver(task_id, req.slot, &req.message) {
        Ok(()) => reply_intervene(stream, true, ""),
        Err(reason) => reply_intervene(stream, false, &reason),
    }
}

pub(super) fn reply_intervene(stream: &mut TcpStream, ok: bool, error: &str) {
    let body = if ok {
        serde_json::json!({ "ok": true })
    } else {
        serde_json::json!({ "ok": false, "error": error })
    };
    match serde_json::to_vec(&body) {
        Ok(bytes) => write_response(stream, "application/json", &bytes),
        Err(_) => write_response(stream, "application/json", br#"{"ok":false}"#),
    }
}
