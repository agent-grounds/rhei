/// Exit cancels an unacknowledged attempt, without blocking a sibling slot;
/// even a later successful OS write cannot publish success or evidence.
/// §FS-rhei-evidence.5.4 §FS-rhei-agents.3.2.1
#[test]
fn exit_cancels_pending_write_and_rejects_late_acknowledgement() {
    let sink = Arc::new(RunInterveneSink::new(std::env::temp_dir()));
    let (target, rx) = fake_target("1", 0, "cancelled-write");
    let (sibling, sibling_rx) = fake_target("1", 1, "cancelled-write-sibling");
    {
        let mut targets = sink.targets.lock().unwrap();
        targets.insert(InterveneKey { task_id: "1".into(), slot: 0 }, target);
        targets.insert(InterveneKey { task_id: "1".into(), slot: 1 }, sibling);
    }
    let sending = sink.clone();
    let (sent_tx, sent_rx) = channel();
    let delivery = std::thread::spawn(move || {
        let _ = sent_tx.send(rhei_tui::InterveneSink::deliver(
            &*sending, Some("1"), Some(0), "cancel me",
        ));
    });
    let pending = rx.recv_timeout(Duration::from_secs(60)).expect("pending write");
    assert!(pending.ack.pending());

    // A blocked invocation does not keep the registry or another slot locked.
    let sibling_writer = std::thread::spawn(move || {
        let write = sibling_rx.recv_timeout(Duration::from_secs(60)).expect("sibling write");
        write.ack.send(Ok(())).expect("sibling acknowledgement");
    });
    rhei_tui::InterveneSink::deliver(&*sink, Some("1"), Some(1), "sibling succeeds")
        .expect("independent delivery");
    sibling_writer.join().unwrap();

    let exiting = sink.clone();
    let (end_tx, end_rx) = channel();
    let finisher = std::thread::spawn(move || {
        let _ = end_tx.send(exiting.unregister("1", 0));
    });
    let captured = end_rx.recv_timeout(Duration::from_secs(60));
    // Always settle the fake writer before assertions, including on failure.
    let still_pending = pending.ack.pending();
    let late = pending.ack.send(Ok(()));
    let sent = sent_rx.recv_timeout(Duration::from_secs(60)).expect("cancelled delivery returns");
    delivery.join().unwrap();
    finisher.join().unwrap();
    assert!(captured.expect("exit must not wait for acknowledgement").is_empty());
    assert!(!still_pending, "cancelled queued writes must not start");
    assert!(late.is_err(), "late acknowledgement cannot report success");
    assert!(sent.is_err(), "cancelled delivery cannot report success");
    let sibling = sink.unregister("1", 1);
    assert_eq!(sibling.len(), 1);
    assert_eq!(sibling[0].message, "sibling succeeds");
}
