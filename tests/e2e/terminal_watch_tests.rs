//! What the attended scenarios of §FS-rhei-recover.5 wait on: the prompt as the terminal
//! shows it, whatever bytes a ConPTY frame drew it with. §REQ-cross-platform.7.1

use super::terminal_watch_support::TerminalWatch;
use std::collections::VecDeque;
use std::io::Read;
use std::time::Duration;

/// Long enough for the reader thread to see every chunk; a missed prompt waits it out.
const WAIT: Duration = Duration::from_secs(3);

/// The needle the attended harness waits on.
const PROMPT: &str = ": type ";

/// The screen ConPTY drew in agent-grounds/rhei#448, up to where the prompt's space goes.
const HEAD: &str = "\x1b[?9001h\x1b[?1004h\x1b[?25l\x1b[2J\x1b[m\x1b[H\
                    Operator runnervmfi6oq\\runneradmin: type";

/// The window title ConPTY slipped into the line after the space, and the cursor it showed.
const TITLE: &str = "\x1b]0;D:\\a\\rhei\\rhei\\target\\debug\\rhei.exe\x07\x1b[?25h";

/// Hands out one chunk per read, the way a terminal reader sees frames, then ends.
struct Chunks(VecDeque<Vec<u8>>);

impl Read for Chunks {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let Some(chunk) = self.0.pop_front() else {
            return Ok(0);
        };
        assert!(chunk.len() <= buf.len(), "a test chunk must fit one read");
        buf[..chunk.len()].copy_from_slice(&chunk);
        Ok(chunk.len())
    }
}

fn prompts(chunks: &[&str]) -> bool {
    let reader = Chunks(chunks.iter().map(|chunk| chunk.as_bytes().to_vec()).collect());
    TerminalWatch::start(reader, PROMPT).prompted_within(WAIT)
}

#[test]
fn a_literal_space_prompts() {
    let stream = format!("{HEAD} {TITLE}force plan.1 implement -> completed\r");
    assert!(prompts(&[&stream]), "the literal-space stream did not prompt");
}

#[test]
fn the_quoted_implement_stream_prompts() {
    let stream = format!("{HEAD}\x1b[1C{TITLE}force plan.1 implement -> completed\r");
    assert!(prompts(&[&stream]), "prompt not signalled for {stream:?}");
}

#[test]
fn the_quoted_human_gate_stream_prompts() {
    let stream = format!("{HEAD}\x1b[1C{TITLE}force plan.1 human-gate -> implement\r");
    assert!(prompts(&[&stream]), "prompt not signalled for {stream:?}");
}

#[test]
fn a_wider_cursor_forward_reads_as_that_many_spaces() {
    let stream = format!("{HEAD}\x1b[3C{TITLE}force plan.1 implement -> completed\r");
    assert!(prompts(&[&stream]), "prompt not signalled for {stream:?}");
}

#[test]
fn a_title_split_across_reads_prompts() {
    let first = format!("{HEAD}\x1b[1C\x1b]0;D:\\a\\rhei");
    let second =
        "\\rhei\\target\\debug\\rhei.exe\x07\x1b[?25hforce plan.1 implement -> completed\r";
    assert!(prompts(&[&first, second]), "prompt not signalled for {first:?} + {second:?}");
}

#[test]
fn a_cursor_forward_split_across_reads_prompts() {
    let first = format!("{HEAD}\x1b[");
    let second = format!("1C{TITLE}force plan.1 human-gate -> implement\r");
    assert!(prompts(&[&first, &second]), "prompt not signalled for {first:?} + {second:?}");
}

#[test]
fn a_stream_that_never_shows_the_prompt_does_not_prompt() {
    let stream = format!("{HEAD}{TITLE}force plan.1 implement -> completed\r");
    assert!(!prompts(&[&stream]), "a prompt was signalled that the terminal never showed");
}

/// A terminal fed by the test, one chunk at a time, that ends when the sender is dropped.
struct Fed(std::sync::mpsc::Receiver<Vec<u8>>);

impl Read for Fed {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let Ok(chunk) = self.0.recv() else {
            return Ok(0);
        };
        assert!(chunk.len() <= buf.len(), "a test chunk must fit one read");
        buf[..chunk.len()].copy_from_slice(&chunk);
        Ok(chunk.len())
    }
}

/// How long a wait that must not fire is given to fire anyway.
const QUIET: Duration = Duration::from_millis(500);

/// The prompt of agent-grounds/rhei#761 arrived as several writes; the wait fires only on its
/// whole line, so an answer never lands between `: type ` and the hop. §REQ-cross-platform.7.1
#[test]
fn a_prompt_written_in_pieces_prompts_only_on_its_line_end() {
    let (feed, fed) = std::sync::mpsc::channel();
    let watch = TerminalWatch::start(Fed(fed), PROMPT);
    for piece in ["Operator ", "vjovanov", ": type "] {
        feed.send(piece.as_bytes().to_vec()).unwrap();
    }
    assert!(!watch.prompted_within(QUIET), "prompt signalled at `: type `, before its hop");
    feed.send(b"force plan.1 human-gate -> implement".to_vec()).unwrap();
    assert!(!watch.prompted_within(QUIET), "prompt signalled before its line end");
    feed.send(b"\r\n".to_vec()).unwrap();
    assert!(watch.prompted_within(WAIT), "the whole prompt line did not prompt");
}
