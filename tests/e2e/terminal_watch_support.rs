//! The terminal side of the attended operator scenarios: one thread reads the
//! terminal, and the scenario waits on what it shows. The scenarios themselves
//! are black-box proofs of §FS-rhei-recover.5; what a wait that runs out reports
//! is §REQ-cross-platform.7.

use std::io::Read;
use std::sync::mpsc;
use std::time::Duration;

/// What one reader thread has seen of a terminal: a signal when the prompt
/// appears, and the transcript.
pub(super) struct TerminalWatch {
    prompt_rx: mpsc::Receiver<()>,
    output_rx: mpsc::Receiver<String>,
}

impl TerminalWatch {
    /// Reads `reader` on a thread of its own until it ends, watching for `prompt`.
    pub(super) fn start(mut reader: impl Read + Send + 'static, prompt: &'static str) -> Self {
        let (prompt_tx, prompt_rx) = mpsc::channel();
        let (output_tx, output_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut all = Vec::new();
            let mut buffer = [0; 4096];
            let mut prompted = false;
            while let Ok(count) = reader.read(&mut buffer) {
                if count == 0 {
                    break;
                }
                all.extend_from_slice(&buffer[..count]);
                if !prompted && String::from_utf8_lossy(&all).contains(prompt) {
                    prompted = true;
                    let _ = prompt_tx.send(());
                }
            }
            let _ = output_tx.send(String::from_utf8_lossy(&all).into_owned());
        });
        Self { prompt_rx, output_rx }
    }

    /// Whether the prompt appeared within `timeout`.
    pub(super) fn prompted_within(&self, timeout: Duration) -> bool {
        self.prompt_rx.recv_timeout(timeout).is_ok()
    }

    /// The transcript a failure reports, taking at most `timeout`.
    pub(super) fn transcript_for_failure(&self, timeout: Duration) -> String {
        self.output_rx.recv_timeout(timeout).unwrap_or_default()
    }

    /// The whole transcript once the terminal has ended, within `timeout`.
    pub(super) fn transcript(&self, timeout: Duration) -> String {
        self.output_rx.recv_timeout(timeout).expect("terminal transcript drained")
    }
}
