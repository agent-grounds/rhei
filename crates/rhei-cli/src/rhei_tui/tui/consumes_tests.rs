use std::path::PathBuf;

use super::state::UiState;
use crate::rhei_tui::event::{MessageLevel, RunEvent};

/// The TUI retains the same validation advisory event its other frontends
/// render, including its warning severity and exact text.
// §FS-rhei-run-tui.1.1 §FS-rhei-run.3
#[test]
fn the_consumes_advisory_is_retained_in_the_tui_journal() {
    let mut state = UiState::with_context(PathBuf::from("/ws"), 1, 0, None, None, None, false);
    let text = "warning: **Consumes:** declares export data-flow for prompt injection, not filesystem visibility. Workers can read undeclared sibling exports under runtime/exports/. For a blind round, schedule participants concurrently and brief them not to inspect sibling exports; neither measure enforces blindness once an export exists.";
    state.apply(&RunEvent::Message { level: MessageLevel::Warn, text: text.to_string() });

    let entry = state.journal.back().expect("warning journal entry");
    assert_eq!(entry.level, MessageLevel::Warn);
    assert_eq!(entry.text, text);
}
