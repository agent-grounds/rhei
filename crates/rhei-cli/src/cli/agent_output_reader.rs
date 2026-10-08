// An agent's stdout and stderr readers, and the drain that ends them.
//
// Each stream has one reader thread, which owns the pipe and does every line's
// work: capture for provider-limit recognition, the log write, the usage
// capture and the live events. The direct agent's exit is the boundary the
// drain holds them to. What the pipe held when the reader learns of that exit
// is read and processed whole, however far behind the reader is, and only
// then does the short grace for a descendant's later output start.
// agent-grounds/rhei#484: a grace alone detached a reader still working
// through output written before the exit, and recognition read a prefix of it.

// §FS-rhei-agent-output-drain

/// How long each captured stream may keep draining once everything it held at
/// the direct agent's exit is captured, before its reader is detached
/// (§FS-rhei-agent-output-drain.2). The test build waits less.
#[cfg(not(test))]
const AGENT_OUTPUT_DRAIN_GRACE: Duration = Duration::from_millis(100);
#[cfg(test)]
const AGENT_OUTPUT_DRAIN_GRACE: Duration = Duration::from_millis(20);

/// How long a reader waits on an idle pipe before it looks again for the exit.
/// A wake-up interval, not a cap: no timer cuts the pre-exit capture short
/// (§FS-rhei-agent-output-drain.1).
const AGENT_OUTPUT_WAKE: Duration = Duration::from_millis(10);

/// The most a reader takes from its pipe in one read.
const AGENT_OUTPUT_CHUNK: usize = 8 * 1024;

/// A source a stream's reader reads an agent's output from. A pipe can say,
/// without blocking, whether a read would block and how many bytes it holds
/// unread. A source that cannot say is read to its end.
///
/// Only the reader thread, which owns the source, queries it, so a query never
/// reaches a handle another thread has closed and the OS may have reused.
trait AgentOutputSource: Read + Send + 'static {
    /// Whether a read would return without blocking, with bytes or the end,
    /// waiting up to `wake` for it.
    fn readable_within(&self, _wake: Duration) -> std::io::Result<bool> {
        Ok(true)
    }

    /// The bytes the pipe holds unread right now, or `None` when the source
    /// cannot say.
    fn unread_bytes(&self) -> std::io::Result<Option<u64>> {
        Ok(None)
    }
}

impl AgentOutputSource for std::process::ChildStdout {
    #[cfg(any(unix, windows))]
    fn readable_within(&self, wake: Duration) -> std::io::Result<bool> {
        pipe_readable_within(self, wake)
    }

    #[cfg(any(unix, windows))]
    fn unread_bytes(&self) -> std::io::Result<Option<u64>> {
        pipe_unread_bytes(self).map(Some)
    }
}

impl AgentOutputSource for std::process::ChildStderr {
    #[cfg(any(unix, windows))]
    fn readable_within(&self, wake: Duration) -> std::io::Result<bool> {
        pipe_readable_within(self, wake)
    }

    #[cfg(any(unix, windows))]
    fn unread_bytes(&self) -> std::io::Result<Option<u64>> {
        pipe_unread_bytes(self).map(Some)
    }
}

/// A test's in-memory output: it never blocks, so its end is its drain.
#[cfg(test)]
impl<T: AsRef<[u8]> + Send + 'static> AgentOutputSource for std::io::Cursor<T> {}

#[cfg(unix)]
fn pipe_readable_within(pipe: &impl std::os::fd::AsFd, wake: Duration) -> std::io::Result<bool> {
    use rustix::event::{poll, PollFd, PollFlags, Timespec};
    let timeout = Timespec::try_from(wake).map_err(std::io::Error::other)?;
    let mut fds = [PollFd::new(pipe, PollFlags::IN)];
    match poll(&mut fds, Some(&timeout)) {
        // A hang-up or an error is readable too: the read reports it.
        Ok(ready) => Ok(ready > 0),
        // A signal woke the wait early; the caller looks again.
        Err(rustix::io::Errno::INTR) => Ok(false),
        Err(err) => Err(err.into()),
    }
}

#[cfg(unix)]
fn pipe_unread_bytes(pipe: &impl std::os::fd::AsFd) -> std::io::Result<u64> {
    Ok(rustix::io::ioctl_fionread(pipe)?)
}

/// Windows cannot wait on an anonymous pipe, so an idle one is looked at again
/// after `wake`.
#[cfg(windows)]
fn pipe_readable_within(
    pipe: &impl std::os::windows::io::AsRawHandle,
    wake: Duration,
) -> std::io::Result<bool> {
    match windows_pipe_peek(pipe)? {
        Some(0) => {
            std::thread::sleep(wake);
            Ok(false)
        }
        // Bytes to read, or the end: the writers are gone and the read says so.
        _ => Ok(true),
    }
}

#[cfg(windows)]
fn pipe_unread_bytes(pipe: &impl std::os::windows::io::AsRawHandle) -> std::io::Result<u64> {
    Ok(windows_pipe_peek(pipe)?.unwrap_or(0))
}

/// The bytes the pipe holds unread, or `None` once every writer has closed it
/// and nothing is left.
#[cfg(windows)]
fn windows_pipe_peek(
    pipe: &impl std::os::windows::io::AsRawHandle,
) -> std::io::Result<Option<u64>> {
    use windows_sys::Win32::Foundation::ERROR_BROKEN_PIPE;
    use windows_sys::Win32::System::Pipes::PeekNamedPipe;

    let mut available: u32 = 0;
    // SAFETY: the handle is borrowed from the pipe the calling reader thread
    // owns, so it is open for this call; no buffer is passed, and `available`
    // outlives the call.
    let peeked = unsafe {
        PeekNamedPipe(
            pipe.as_raw_handle(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            &mut available,
            std::ptr::null_mut(),
        )
    };
    if peeked != 0 {
        return Ok(Some(u64::from(available)));
    }
    let err = std::io::Error::last_os_error();
    if err.raw_os_error() == Some(ERROR_BROKEN_PIPE as i32) {
        Ok(None)
    } else {
        Err(err)
    }
}

/// What the spawn and one stream's reader tell each other at the exit: the
/// spawn that the direct agent has exited, the reader that everything its
/// pipe held then is captured (§FS-rhei-agent-output-drain.1).
#[derive(Default)]
struct AgentOutputDrain {
    exited: std::sync::atomic::AtomicBool,
    captured: Mutex<bool>,
    captured_changed: std::sync::Condvar,
}

impl AgentOutputDrain {
    fn signal_exit(&self) {
        self.exited.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    fn exited(&self) -> bool {
        self.exited.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn mark_captured(&self) {
        *self.captured.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = true;
        self.captured_changed.notify_all();
    }

    /// Waits, with no timeout, until the reader has captured everything its
    /// pipe held at the exit, or has ended (§FS-rhei-agent-output-drain.1).
    fn wait_captured(&self) {
        let captured = self.captured.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let _captured = self
            .captured_changed
            .wait_while(captured, |captured| !*captured)
            .unwrap_or_else(std::sync::PoisonError::into_inner);
    }
}

/// Marks the drain captured when the reader ends, however it ends, so the
/// spawn never waits on a reader that is gone.
struct CapturedWhenDropped(Arc<AgentOutputDrain>);

impl Drop for CapturedWhenDropped {
    fn drop(&mut self) {
        self.0.mark_captured();
    }
}

/// A stream's reader thread and the drain it shares with the spawn.
struct AgentOutputReader {
    handle: std::thread::JoinHandle<std::io::Result<()>>,
    drain: Arc<AgentOutputDrain>,
}

impl AgentOutputReader {
    /// Waits for the thread to end, as [`std::thread::JoinHandle::join`].
    fn join(self) -> std::thread::Result<std::io::Result<()>> {
        self.handle.join()
    }
}

/// Everything one stream's lines go to, and the work done on each of them.
struct AgentOutputLines {
    stream: rhei_tui::AgentStream,
    log_file: Arc<Mutex<fs::File>>,
    sink: Arc<dyn rhei_tui::EventSink>,
    slot: rhei_tui::Slot,
    task_id: String,
    usage_capture: Option<AgentUsageCapture>,
    captured_lines: Arc<Mutex<Vec<(rhei_tui::AgentStream, String)>>>,
}

impl AgentOutputLines {
    /// One logical line, `buf` with its newline if it had one. Capture the raw
    /// line with its stream so provider recognition can interpret stdout
    /// independently of usage parsing and display. §FS-rhei-agents.2.3
    fn take(&self, buf: &[u8]) -> std::io::Result<()> {
        let stream = self.stream;
        let usage_capture = self.usage_capture.as_ref();
        let raw_line = output_line(buf);
        if let Ok(mut captured) = self.captured_lines.lock() {
            captured.push((stream, raw_line.clone()));
        }
        let display_line = display_agent_output_line(usage_capture, stream, &raw_line);
        let is_claude_result = stream == rhei_tui::AgentStream::Stdout
            && usage_capture.is_some_and(|capture| capture.extractor == AgentUsageExtractor::Claude)
            && matches!(parse_claude_result_line(&raw_line), ClaudeResultLine::Result(_));
        with_agent_log(&self.log_file, |f| {
            if is_claude_result {
                if let Some(line) = display_line.as_deref() {
                    f.write_all(line.as_bytes())?;
                    if buf.ends_with(b"\n") && !line.ends_with('\n') {
                        f.write_all(b"\n")?;
                    }
                }
            } else {
                f.write_all(buf)?;
            }
            f.flush()
        })?;

        capture_agent_output_usage(usage_capture, stream, &raw_line, &self.sink);
        if let Some(line) = display_line {
            for line in agent_output_lines(line, is_claude_result) {
                self.sink.emit(rhei_tui::RunEvent::AgentOutput {
                    slot: self.slot,
                    task: self.task_id.clone(),
                    stream,
                    line,
                    wall_clock: std::time::SystemTime::now(),
                });
            }
        }
        Ok(())
    }

    /// Takes every complete line at the front of `pending` and leaves the rest.
    fn take_complete(&self, pending: &mut Vec<u8>) -> std::io::Result<()> {
        let mut start = 0;
        while let Some(offset) = pending[start..].iter().position(|&byte| byte == b'\n') {
            let end = start + offset + 1;
            self.take(&pending[start..end])?;
            start = end;
        }
        pending.drain(..start);
        Ok(())
    }

    /// Takes what is left of `pending` as a line of its own: an unterminated
    /// last line (§FS-rhei-agent-output-drain.1).
    fn take_rest(&self, pending: &mut Vec<u8>) -> std::io::Result<()> {
        if !pending.is_empty() {
            self.take(pending)?;
            pending.clear();
        }
        Ok(())
    }
}

/// Where a reader stands against the direct agent's exit.
enum AgentOutputCapture {
    /// The exit has not reached this reader.
    Live,
    /// The exit has reached it, and this many of the bytes the pipe held then
    /// are still to be read.
    Owed(u64),
    /// Everything held at the exit is captured, so what comes now is read
    /// best-effort; or the source cannot say what it held, and its end is
    /// its drain.
    Trailing,
}

/// Read `source` line by line until its end. Once the exit is signalled, the
/// reader itself counts what its pipe holds, reads exactly that, takes every
/// line of it with any unterminated tail, and only then marks the drain
/// captured (§FS-rhei-agent-output-drain.1). Counting on this thread, between
/// its own reads, is what keeps a read in flight out of the count.
fn read_agent_output<R: AgentOutputSource>(
    mut source: R,
    lines: &AgentOutputLines,
    drain: &AgentOutputDrain,
) -> std::io::Result<()> {
    let mut chunk = vec![0; AGENT_OUTPUT_CHUNK];
    let mut pending = Vec::new();
    let mut capture = AgentOutputCapture::Live;
    loop {
        let limit = match capture {
            AgentOutputCapture::Live if drain.exited() => {
                capture = match source.unread_bytes()? {
                    Some(unread) => AgentOutputCapture::Owed(unread),
                    None => AgentOutputCapture::Trailing,
                };
                continue;
            }
            AgentOutputCapture::Live => {
                // Never block in a read the exit could not interrupt.
                if !source.readable_within(AGENT_OUTPUT_WAKE)? {
                    continue;
                }
                chunk.len()
            }
            AgentOutputCapture::Owed(0) => {
                lines.take_rest(&mut pending)?;
                drain.mark_captured();
                capture = AgentOutputCapture::Trailing;
                continue;
            }
            // The bytes are in the pipe, so this read does not block.
            AgentOutputCapture::Owed(owed) => {
                chunk.len().min(usize::try_from(owed).unwrap_or(usize::MAX))
            }
            AgentOutputCapture::Trailing => chunk.len(),
        };
        let read = match source.read(&mut chunk[..limit]) {
            Ok(0) => break,
            Ok(read) => read,
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err),
        };
        if let AgentOutputCapture::Owed(owed) = &mut capture {
            *owed -= read as u64;
        }
        pending.extend_from_slice(&chunk[..read]);
        lines.take_complete(&mut pending)?;
    }
    lines.take_rest(&mut pending)
}

/// Keep the thread's independently owned inputs explicit, as in `spawn_and_wait_agent`.
#[allow(clippy::too_many_arguments)]
fn spawn_agent_output_reader<R>(
    source: R,
    stream: rhei_tui::AgentStream,
    log_file: Arc<Mutex<fs::File>>,
    sink: Arc<dyn rhei_tui::EventSink>,
    slot: rhei_tui::Slot,
    task_id: String,
    usage_capture: Option<AgentUsageCapture>,
    captured_lines: Arc<Mutex<Vec<(rhei_tui::AgentStream, String)>>>,
) -> AgentOutputReader
where
    R: AgentOutputSource,
{
    let lines =
        AgentOutputLines { stream, log_file, sink, slot, task_id, usage_capture, captured_lines };
    let drain = Arc::new(AgentOutputDrain::default());
    let captured = CapturedWhenDropped(Arc::clone(&drain));
    let handle = std::thread::spawn(move || {
        // Held whole, so it drops after the last line is taken.
        let captured = captured;
        read_agent_output(source, &lines, &captured.0)
    });
    AgentOutputReader { handle, drain }
}

/// Join a stream's reader after the direct agent has exited. It first captures
/// everything the stream held at the exit, however long that takes
/// (§FS-rhei-agent-output-drain.1). Then it gets at most
/// `AGENT_OUTPUT_DRAIN_GRACE` to reach the end, and is detached otherwise: a
/// descendant that still holds the inherited pipe does not delay completion
/// (§FS-rhei-agent-output-drain.2).
fn drain_agent_output_reader(
    reader: AgentOutputReader,
    stream: rhei_tui::AgentStream,
) -> MietteResult<()> {
    reader.drain.signal_exit();
    reader.drain.wait_captured();
    let deadline = Instant::now() + AGENT_OUTPUT_DRAIN_GRACE;
    while !reader.handle.is_finished() {
        if Instant::now() >= deadline {
            // A descendant may still hold the inherited pipe open after the
            // direct agent process exits. Detach the reader instead of
            // blocking run completion forever; future bytes may still be
            // captured best-effort until process exit.
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(1));
    }

    match reader.join() {
        Ok(Ok(())) => Ok(()),
        Ok(Err(err)) => {
            Err(miette!(
                help = agent_command_help(),
                "failed to capture agent {}: {err}", agent_stream_label(stream)
            ))
        }
        Err(_) => Err(miette!(
            help = internal_error_help(),
            "agent {} capture thread panicked", agent_stream_label(stream)
        )),
    }
}

fn output_line(buf: &[u8]) -> String {
    let line = buf.strip_suffix(b"\n").unwrap_or(buf);
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    String::from_utf8_lossy(line).into_owned()
}
