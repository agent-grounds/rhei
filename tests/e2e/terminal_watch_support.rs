//! The terminal side of the attended operator scenarios: one thread reads the
//! terminal, and the scenario waits on what it shows. The scenarios themselves
//! are black-box proofs of §FS-rhei-recover.5; what a wait that runs out reports
//! is §REQ-cross-platform.7.

use std::io::Read;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// What one reader thread has seen of a terminal: a signal when the prompt
/// appears, a signal when the terminal ends, and the transcript so far.
///
/// The transcript is published on every read rather than at the end, because a
/// ConPTY reader sees no end while the harness still holds the pseudo-console,
/// and a failure must still say what was printed. §REQ-cross-platform.7
pub(super) struct TerminalWatch {
    prompt_rx: mpsc::Receiver<()>,
    ended_rx: mpsc::Receiver<()>,
    read: Arc<Mutex<Vec<u8>>>,
}

impl TerminalWatch {
    /// Reads `reader` on a thread of its own until it ends, watching for `prompt`.
    pub(super) fn start(mut reader: impl Read + Send + 'static, prompt: &'static str) -> Self {
        let (prompt_tx, prompt_rx) = mpsc::channel();
        let (ended_tx, ended_rx) = mpsc::channel();
        let read = Arc::new(Mutex::new(Vec::new()));
        let published = Arc::clone(&read);
        std::thread::spawn(move || {
            let mut buffer = [0; 4096];
            let mut prompted = false;
            while let Ok(count) = reader.read(&mut buffer) {
                if count == 0 {
                    break;
                }
                let mut all = published.lock().unwrap();
                all.extend_from_slice(&buffer[..count]);
                if !prompted && String::from_utf8_lossy(&all).contains(prompt) {
                    prompted = true;
                    let _ = prompt_tx.send(());
                }
            }
            let _ = ended_tx.send(());
        });
        Self { prompt_rx, ended_rx, read }
    }

    /// Whether the prompt appeared within `timeout`.
    pub(super) fn prompted_within(&self, timeout: Duration) -> bool {
        self.prompt_rx.recv_timeout(timeout).is_ok()
    }

    /// The transcript a failure reports, taking at most `timeout`: whatever the
    /// terminal printed before it ended or the bound ran out, whichever is first.
    /// §REQ-cross-platform.7
    pub(super) fn transcript_for_failure(&self, timeout: Duration) -> String {
        let _ = self.ended_rx.recv_timeout(timeout);
        self.read_so_far()
    }

    /// The whole transcript once the terminal has ended, within `timeout`.
    pub(super) fn transcript(&self, timeout: Duration) -> String {
        if self.ended_rx.recv_timeout(timeout).is_err() {
            panic!("terminal did not end within {timeout:?}; it printed: {}", self.read_so_far());
        }
        self.read_so_far()
    }

    fn read_so_far(&self) -> String {
        String::from_utf8_lossy(&self.read.lock().unwrap()).into_owned()
    }
}
