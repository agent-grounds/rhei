//! The terminal the shape pairs run on. The console task tree is the rich
//! summary, which `rhei run` writes only to a terminal (§FS-rhei-run-report.3.4),
//! so the sync gate drives each shape through the suite's `portable-pty`
//! harness — native on Unix, ConPTY on Windows — and reads back the rows the
//! run wrote rather than the bytes that carried them.
// §FS-rhei-run-report.3.2 §REQ-cross-platform.2

use std::path::Path;

use super::*;

/// Run a workspace to the end on a real terminal and return everything it
/// wrote there, so the rich end-of-run summary is reached at all. The shape
/// finds its own machine, as a copy of it does for a reader: a project names
/// one per member, and `--state-machine` would force one on all of them.
pub(super) fn run_on_a_terminal(workspace: &Path) -> String {
    use portable_pty::{native_pty_system, CommandBuilder, PtySize};
    use std::io::Read;

    let source = rhei_command(workspace.join(".home"));
    let mut command = CommandBuilder::new(source.get_program());
    for (key, value) in source.get_envs() {
        match value {
            Some(value) => command.env(key, value),
            None => command.env_remove(key),
        }
    }
    command.env("NO_COLOR", "1");
    command.arg("run");
    command.arg(workspace);
    command.arg("--no-dashboard");
    // The plain console frontend: on a pty `Auto` picks the TUI, which waits
    // for `q` after the run (§FS-rhei-run-tui.1.4) and never returns here.
    command.arg("--no-tui");

    let pair = native_pty_system()
        .openpty(PtySize { rows: TERMINAL_ROWS, cols: 240, pixel_width: 0, pixel_height: 0 })
        .expect("open a pty");
    let mut child = pair.slave.spawn_command(command).expect("spawn the run on a pty");
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().expect("read the pty");
    let transcript = std::thread::spawn(move || {
        let mut text = String::new();
        let mut buffer = [0u8; 4096];
        while let Ok(count) = reader.read(&mut buffer) {
            if count == 0 {
                break;
            }
            text.push_str(&String::from_utf8_lossy(&buffer[..count]));
        }
        text
    });
    let status = child.wait().expect("the run returns on its own");
    drop(pair.master);
    let transcript = transcript.join().expect("terminal transcript drained");
    assert_eq!(status.exit_code(), 0, "the example run finishes:\n{transcript}");
    untransported(&transcript)
}

/// The height of the terminal the run is driven on, which is also the row a
/// newline stops advancing at once the screen scrolls.
const TERMINAL_ROWS: u16 = 60;

/// The rows the run wrote, without what the terminal added carrying them. A
/// native pty passes the bytes through, but ConPTY re-renders its screen
/// (§REQ-cross-platform.2): it sets the window title, toggles modes and the
/// cursor, and while the output still fits the screen it spells a jump down the
/// viewport as a cursor position rather than the newlines it stands for. So
/// CSI and OSC sequences, a bare `\r` and trailing blanks are dropped, and a
/// forward cursor position becomes the newlines it replaced. Nothing a row
/// says is touched, so the console tree of §FS-rhei-run-report.3.2 is still
/// compared byte for byte.
fn untransported(transcript: &str) -> String {
    let mut text = String::new();
    let mut row = 1;
    let mut chars = transcript.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\x1b' if chars.peek() == Some(&'[') => {
                chars.next();
                let mut params = String::new();
                let mut command = None;
                for c in chars.by_ref() {
                    if ('\x40'..='\x7e').contains(&c) {
                        command = Some(c);
                        break;
                    }
                    params.push(c);
                }
                if command == Some('H') {
                    let target = params.split(';').next().and_then(|r| r.parse().ok()).unwrap_or(1);
                    if target > row {
                        text.push_str(&"\n".repeat(target - row));
                    }
                    row = target;
                }
            }
            '\x1b' if chars.peek() == Some(&']') => {
                while let Some(c) = chars.next() {
                    if c == '\x07' || (c == '\x1b' && chars.next_if_eq(&'\\').is_some()) {
                        break;
                    }
                }
            }
            '\x1b' => {
                chars.next();
            }
            '\r' => {}
            '\n' => {
                row = (row + 1).min(usize::from(TERMINAL_ROWS));
                text.push('\n');
            }
            c => text.push(c),
        }
    }
    text.lines().map(str::trim_end).collect::<Vec<_>>().join("\n")
}

/// The bytes ConPTY wrote for `parts-of-a-feature` on `windows-latest`, cut
/// to the end of the run: the blank lines around `Tasks` arrive as cursor
/// positions, and must come back as the blank lines the task tree is cut by.
// §FS-rhei-run-report.3.2 §REQ-cross-platform.2
#[test]
fn a_conpty_transcript_reads_as_the_rows_the_run_wrote() {
    let conpty = "\x1b[?9001h\x1b[?25l\x1b[2J\x1b[m\x1b[HRunning workspace\r\n\
                  \x1b]0;D:\\a\\rhei.exe\x07\x1b[?25hRun Report  3.2s\r\n  completed\x1b[4;1H\
                  \x20 Work      4 agents\x1b[6;1HTasks   4 tasks \u{b7} source order\r\n\
                  \x20 \u{2713} nested.1   completed \u{2014} 3 subtasks: 3 completed\x1b[9;1H\
                  Report     runtime/run-report.md\r\n\x1b[?25h\x1b[?9001l";
    assert_eq!(
        untransported(conpty),
        "Running workspace\nRun Report  3.2s\n  completed\n  Work      4 agents\n\nTasks   4 tasks \
         \u{b7} source order\n  \u{2713} nested.1   completed \u{2014} 3 subtasks: 3 completed\n\n\
         Report     runtime/run-report.md"
    );
}
