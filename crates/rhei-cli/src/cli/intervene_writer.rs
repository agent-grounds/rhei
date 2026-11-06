/// Acknowledgement and exit cancellation share this short critical section;
/// blocking pipe I/O never holds it. There is at most one pending write because
/// the invocation's delivered guard serializes admission. §FS-rhei-evidence.5.4
#[derive(Default)]
struct InterveneWriterState {
    closed: bool,
    pending: Option<Sender<Result<(), String>>>,
}

/// The writer publishes its result through the same boundary exit closes.
/// A cancelled write may finish later, but cannot acknowledge success or touch
/// the final delivery snapshot. §FS-rhei-evidence.5.4
struct InterveneAck {
    writer: Arc<Mutex<InterveneWriterState>>,
}

impl InterveneAck {
    fn pending(&self) -> bool {
        self.writer.lock().is_ok_and(|writer| !writer.closed && writer.pending.is_some())
    }

    fn send(self, result: Result<(), String>) -> Result<(), String> {
        let mut writer = self.writer.lock().map_err(|_| "intervene writer poisoned".to_string())?;
        let ack = writer.pending.take().ok_or_else(|| "agent stdin is closed".to_string())?;
        // Send while locked: exit cannot cancel between taking the sender and
        // publishing the successful acknowledgement. §FS-rhei-evidence.5.4
        ack.send(result).map_err(|_| "intervention receiver is closed".to_string())
    }
}
