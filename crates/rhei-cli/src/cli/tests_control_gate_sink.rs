// An attached surface's gate sink: what it says when the run serves no control
// URL, and how it learns whether the run answers gates.

// §FS-rhei-run-headless.5

/// A control server that answers every `GET /snapshot` with `snapshot` and
/// counts the requests it served.
fn snapshot_server(snapshot: serde_json::Value) -> (String, Arc<std::sync::atomic::AtomicUsize>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let url = format!("http://{}", listener.local_addr().expect("address"));
    let served = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = served.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let mut request = [0u8; 1024];
            let _ = stream.read(&mut request);
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let body = snapshot.to_string();
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
                 Connection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    (url, served)
}

/// Without a control URL every gate action is unavailable up front, with the
/// remedy, and nothing pretends it could answer. §FS-rhei-run-headless.5
#[test]
fn a_run_without_a_control_url_offers_no_gate_action() {
    let sink = ControlGateSink::new(None);
    let reason = rhei_tui::GateTransitionSink::unavailable(&sink).expect("unavailable");
    assert!(reason.contains("serves no control endpoint"), "{reason}");
    assert!(reason.contains("--dashboard"), "{reason}");
    assert!(!rhei_tui::GateTransitionSink::answers_gates(&sink));
}

/// Whether the run answers gates is its snapshot's `capabilities.gate_answer`,
/// asked once for the attachment rather than on every frame; a run that does
/// not say so, an older one included, is not offered the form.
/// §FS-rhei-run-headless.5 §FS-rhei-viz.8
#[test]
fn the_run_says_whether_it_answers_gates() {
    let (url, served) = snapshot_server(serde_json::json!({
        "capabilities": {"gate_transition": true, "gate_answer": true}
    }));
    let sink = ControlGateSink::new(Some(url));
    assert!(rhei_tui::GateTransitionSink::unavailable(&sink).is_none());
    assert!(rhei_tui::GateTransitionSink::answers_gates(&sink));
    assert!(rhei_tui::GateTransitionSink::answers_gates(&sink));
    assert_eq!(served.load(std::sync::atomic::Ordering::SeqCst), 1, "asked once");

    for snapshot in [
        serde_json::json!({"capabilities": {"gate_transition": true, "gate_answer": false}}),
        serde_json::json!({"capabilities": {"gate_transition": true}}),
    ] {
        let (url, _) = snapshot_server(snapshot);
        assert!(!rhei_tui::GateTransitionSink::answers_gates(&ControlGateSink::new(Some(url))));
    }
}
