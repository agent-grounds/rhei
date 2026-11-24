// Client half of the run's loopback control server.
//
// `rhei intervene` and an attached surface are the second and third clients of
// the boundaries the browser dashboard already uses — not second
// implementations of them. Everything here posts JSON to a route the server
// already serves and reports what came back.

// §AR-rhei-viz-flow.7 §FS-rhei-run-headless.4 §FS-rhei-run-headless.5

/// A reply from a control route: `{ "ok": bool, ... }`. A non-`ok` reply is a
/// successful round trip carrying a refusal, so it is `Ok` here; only a
/// transport failure is `Err`.
pub(crate) struct ControlReply {
    pub(crate) ok: bool,
    pub(crate) error: Option<String>,
    pub(crate) body: serde_json::Value,
}

/// POST a JSON body to one of the run's control routes.
pub(crate) fn post_control(url: &str, route: &str, body: &str) -> Result<ControlReply, String> {
    let addr = url.strip_prefix("http://").unwrap_or(url);
    let mut stream = std::net::TcpStream::connect(addr).map_err(|err| err.to_string())?;
    // A wedged server must not hang the caller's terminal forever; the render
    // thread is waiting on this call when a composer submits.
    let timeout = Duration::from_secs(15);
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));
    let request = format!(
        "POST {route} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).map_err(|err| err.to_string())?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response).map_err(|err| err.to_string())?;
    let split = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| "malformed HTTP response".to_string())?;
    let parsed: serde_json::Value = serde_json::from_slice(&response[split + 4..])
        .map_err(|err| format!("invalid response body: {err}"))?;
    Ok(ControlReply {
        ok: parsed.get("ok").and_then(serde_json::Value::as_bool).unwrap_or(false),
        error: parsed
            .get("error")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        body: parsed,
    })
}

/// What a control action says when the run serves no control URL.
///
/// Reported up front rather than after the operator has typed: a composer that
/// accepts a message it cannot deliver is worse than one that is not offered.
// §FS-rhei-run-headless.5
fn no_control_server() -> String {
    "this run serves no control endpoint — restart it with `--dashboard` to intervene, or to \
     release or answer gates, from an attached surface"
        .to_string()
}

/// Deliver intervene messages over the control server. §FS-rhei-viz.5
pub(crate) struct ControlInterveneSink {
    control_url: Option<String>,
}

impl ControlInterveneSink {
    pub(crate) fn new(control_url: Option<String>) -> Self {
        Self { control_url }
    }
}

impl rhei_tui::InterveneSink for ControlInterveneSink {
    fn deliver(
        &self,
        task_id: Option<&str>,
        slot: Option<rhei_tui::Slot>,
        message: &str,
    ) -> Result<(), String> {
        let url = self.control_url.as_deref().ok_or_else(no_control_server)?;
        let body =
            serde_json::json!({ "task_id": task_id, "slot": slot, "message": message }).to_string();
        let reply = post_control(url, "/intervene", &body)
            .map_err(|err| format!("could not reach the run at {url}: {err}"))?;
        if reply.ok {
            Ok(())
        } else {
            Err(reply.error.unwrap_or_else(|| "intervention not delivered".to_string()))
        }
    }

    /// An attached surface cannot see the run's stdin registry, so it asks the
    /// run. Answering `true` optimistically would offer a composer whose
    /// messages fail after they are typed; answering from the snapshot the run
    /// already publishes keeps the capability gate where it belongs.
    // §FS-rhei-viz.5
    fn reachable(&self, task_id: &str, slot: Option<rhei_tui::Slot>) -> bool {
        let Some(url) = self.control_url.as_deref() else {
            return false;
        };
        let Some(snapshot) = fetch_control_snapshot(url) else {
            return false;
        };
        let Some(runtime) = snapshot.get("task_runtime").and_then(|r| r.get(task_id)) else {
            return false;
        };
        let slot_matches = slot.is_none_or(|wanted| {
            runtime.get("in_slot").and_then(serde_json::Value::as_u64) == Some(u64::from(wanted))
        });
        slot_matches
            && runtime.get("intervene").and_then(serde_json::Value::as_bool).unwrap_or(false)
    }
}

/// Release and answer human gates over the control server. §FS-rhei-viz.5.1
pub(crate) struct ControlGateSink {
    control_url: Option<String>,
}

impl ControlGateSink {
    pub(crate) fn new(control_url: Option<String>) -> Self {
        Self { control_url }
    }
}

impl rhei_tui::GateTransitionSink for ControlGateSink {
    fn transition_gate(
        &self,
        task_id: &str,
        from: &str,
        to: &str,
        result: Option<&str>,
    ) -> Result<String, String> {
        let url = self.control_url.as_deref().ok_or_else(no_control_server)?;
        let body =
            serde_json::json!({ "task_id": task_id, "from": from, "to": to, "result": result })
                .to_string();
        let reply = post_control(url, "/transition-gate", &body)
            .map_err(|err| format!("could not reach the run at {url}: {err}"))?;
        if reply.ok {
            Ok(reply
                .body
                .get("to")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(to)
                .to_string())
        } else {
            Err(reply.error.unwrap_or_else(|| "the run refused the gate release".to_string()))
        }
    }

    fn answers_gates(&self) -> bool {
        true
    }

    /// Post to the run's `/answer-gate`. A reply carrying `errors` is the
    /// answer document, refusals included; any other is the server saying it
    /// could not answer at all. §FS-rhei-viz.5.1.2 §FS-rhei-run-headless.5
    fn answer_gate(
        &self,
        task_id: &str,
        from: &str,
        answers: &str,
        dry_run: bool,
    ) -> Result<serde_json::Value, String> {
        let url = self.control_url.as_deref().ok_or_else(no_control_server)?;
        let answers: serde_json::Value = serde_json::from_str(answers)
            .map_err(|err| format!("the answer is not a JSON document: {err}"))?;
        let body = serde_json::json!({
            "task_id": task_id, "from": from, "answers": answers, "dry_run": dry_run,
        })
        .to_string();
        let reply = post_control(url, "/answer-gate", &body)
            .map_err(|err| format!("could not reach the run at {url}: {err}"))?;
        if reply.body.get("errors").is_some() {
            Ok(reply.body)
        } else {
            Err(reply.error.unwrap_or_else(|| "the run did not answer the gate".to_string()))
        }
    }
}

/// GET the run's `/snapshot`. Used only for capability questions an attached
/// surface cannot answer from files.
fn fetch_control_snapshot(url: &str) -> Option<serde_json::Value> {
    let addr = url.strip_prefix("http://").unwrap_or(url);
    let mut stream = std::net::TcpStream::connect(addr).ok()?;
    let timeout = Duration::from_secs(5);
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));
    stream
        .write_all(b"GET /snapshot HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .ok()?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response).ok()?;
    let split = response.windows(4).position(|window| window == b"\r\n\r\n")?;
    serde_json::from_slice(&response[split + 4..]).ok()
}
