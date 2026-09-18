// `rhei intervene` — the headless sibling of the dashboard's intervene composer.
// It reads a live run's loopback URL from the discovery file the dashboard
// publishes (`runtime/dashboard.json`) and POSTs to `/intervene`, which delivers
// the message to one agent's stdin and nothing else — like the composer, it only
// reaches agents that keep stdin open and never transitions or edits the plan.

/// Send a message to a running agent's stdin via the live dashboard's
/// `/intervene` route, the headless form of the Flow composer. Resolves the
/// run's workspace from `plan` and reports the delivery outcome. §FS-rhei-viz.5
fn intervene_command(plan: &Path, task: &str, slot: Option<u16>, message: &str) -> MietteResult<()> {
    if message.trim().is_empty() {
        return Err(miette!(
            help = "pass the message to deliver: rhei intervene <plan> <task> --message <text>",
            "refusing to send an empty intervention message"
        ));
    }
    let workspace = execution_workspace_root(plan);
    // Discovery and delivery are one guarded dependent operation. §FS-rhei-recover.4
    let _root_guards = rhei_core::root_access::for_input(plan).map_err(|err| diagnostic!("{err}"))?;
    let addr_file = workspace.join("runtime").join("dashboard.json");
    let raw = std::fs::read_to_string(&addr_file).map_err(|_| {
        miette!(
            help = dashboard_required_help(),
            "no live dashboard found at {} — start one with `rhei run {} --dashboard`",
            addr_file.display(),
            plan.display()
        )
    })?;
    let url = parse_dashboard_url(&raw)
        .ok_or_else(|| miette!(
            help = dashboard_required_help(),
            "could not read the dashboard URL from {}", addr_file.display()
        ))?;

    let body = serde_json::json!({ "task_id": task, "slot": slot, "message": message }).to_string();
    let reply = post_intervene(&url, &body).map_err(|err| {
        miette!(
            help = dashboard_required_help(),
            "could not reach the live dashboard at {url}: {err}\n\
             The run may have ended; `rhei intervene` only works while `rhei run --dashboard` is live."
        )
    })?;

    if reply.ok {
        match slot {
            Some(s) => println!("Delivered intervention to task {task} (slot {s})."),
            None => println!("Delivered intervention to task {task}."),
        }
        Ok(())
    } else {
        Err(miette!(
            help = "the dashboard rejected the message. Check the task is still running: rhei list <plan>",
            "intervention not delivered: {}",
            reply.error.unwrap_or_else(|| "unknown error".to_string())
        ))
    }
}

/// The `/intervene` JSON reply: `{ "ok": bool, "error"?: string }`.
struct InterveneReply {
    ok: bool,
    error: Option<String>,
}

/// Pull the `url` field out of `runtime/dashboard.json`.
fn parse_dashboard_url(raw: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    value.get("url")?.as_str().map(str::to_string)
}

/// POST the intervene body to the loopback dashboard and parse its JSON reply.
/// A non-`ok` reply (unreachable agent, ambiguous fanout, closed stdin) is a
/// successful round trip carrying a failure reason, so it returns `Ok(reply)`;
/// only a transport failure (no live server) returns `Err`. The CLI is the
/// second client of the one mutation boundary, after the composer.
fn post_intervene(url: &str, body: &str) -> Result<InterveneReply, String> {
    use std::io::{Read as _, Write as _};
    let addr = url.strip_prefix("http://").unwrap_or(url);
    let mut stream = std::net::TcpStream::connect(addr).map_err(|err| err.to_string())?;
    let request = format!(
        "POST /intervene HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\n\
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
    Ok(InterveneReply {
        ok: parsed.get("ok").and_then(serde_json::Value::as_bool).unwrap_or(false),
        error: parsed.get("error").and_then(serde_json::Value::as_str).map(str::to_string),
    })
}

#[cfg(test)]
mod intervene_command_tests {
    use super::*;

    #[test]
    fn parse_dashboard_url_extracts_url() {
        let raw = r#"{"url":"http://127.0.0.1:54321","pid":42}"#;
        assert_eq!(parse_dashboard_url(raw).as_deref(), Some("http://127.0.0.1:54321"));
    }

    #[test]
    fn parse_dashboard_url_rejects_garbage() {
        assert_eq!(parse_dashboard_url("not json"), None);
        assert_eq!(parse_dashboard_url(r#"{"pid":42}"#), None);
    }

    #[test]
    fn missing_discovery_file_is_a_clear_error() {
        let dir = std::env::temp_dir().join(format!("rhei-intervene-cmd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let plan = dir.join("plan.rhei.md");
        let err = intervene_command(&plan, "1", None, "hello").expect_err("no dashboard");
        assert!(err.to_string().contains("no live dashboard found"));
    }

    #[test]
    fn empty_message_is_rejected_before_any_io() {
        let err = intervene_command(Path::new("."), "1", None, "   ").expect_err("empty message");
        assert!(err.to_string().contains("empty intervention message"));
    }

    /// A recording [`rhei_tui::InterveneSink`] that captures deliveries so the
    /// CLI command can be exercised against a real loopback dashboard.
    struct RecordingSink {
        got: std::sync::Mutex<Vec<(Option<String>, String)>>,
    }
    impl rhei_tui::InterveneSink for RecordingSink {
        fn deliver(
            &self,
            task_id: Option<&str>,
            _slot: Option<rhei_tui::Slot>,
            message: &str,
        ) -> Result<(), String> {
            self.got.lock().unwrap().push((task_id.map(str::to_string), message.to_string()));
            Ok(())
        }
    }

    // End to end: `rhei intervene` discovers the published URL from
    // runtime/dashboard.json and delivers the message through the live server's
    // /intervene route to the host sink. §AR-rhei-viz-flow.7
    #[test]
    fn intervene_command_delivers_through_live_dashboard() {
        use std::sync::Arc;
        let workspace = std::env::temp_dir()
            .join(format!("rhei-intervene-e2e-{}-{:?}", std::process::id(), std::thread::current().id()));
        let _ = std::fs::remove_dir_all(&workspace);
        std::fs::create_dir_all(&workspace).expect("create workspace");

        let sink = Arc::new(RecordingSink { got: std::sync::Mutex::new(Vec::new()) });
        let dashboard = rhei_tui::DashboardSink::start_with_plan_and_intervene(
            workspace.clone(),
            1,
            1,
            None,
            Some(sink.clone() as Arc<dyn rhei_tui::InterveneSink>),
        )
        .expect("start dashboard");

        intervene_command(&workspace, "3.1", None, "prefer integration tests")
            .expect("delivered through live dashboard");
        assert_eq!(
            sink.got.lock().unwrap().as_slice(),
            &[(Some("3.1".to_string()), "prefer integration tests".to_string())]
        );

        dashboard.finish();
        let _ = std::fs::remove_dir_all(&workspace);
    }
}
