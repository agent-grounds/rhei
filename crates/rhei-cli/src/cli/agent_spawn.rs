
/// Outcome of a single agent spawn cycle.
///
/// `timed_out` is set when the engine's watchdog fired before the agent
/// exited cleanly. The caller uses this to decide whether to route a
/// non-zero exit through the timeout transition path (with `triggeredBy:
/// 'system'` and `transitionData.timeout`) or through the generic non-zero
/// exit path.
///
/// `interrupted` is set when the run was shutting down: the engine ended the
/// agent's process group, so the exit status says nothing about the ticket.
// §FS-rhei-run.3.2: an interrupted invocation fires no transition.
#[derive(Debug, Clone)]
struct AgentSpawnOutcome {
    status: std::process::ExitStatus,
    timed_out: bool,
    interrupted: bool,
    timeout_secs: Option<u64>,
    usage_capture_path: Option<PathBuf>,
    cli_session: Option<AccountingCliSession>,
    /// A strict provider refusal recognized after output drain and before the
    /// generic non-zero completion paths run. §FS-rhei-agents.2
    provider_limit: Option<ProviderLimit>,
}

#[cfg(not(test))]
const AGENT_OUTPUT_DRAIN_GRACE: Duration = Duration::from_millis(100);
#[cfg(test)]
const AGENT_OUTPUT_DRAIN_GRACE: Duration = Duration::from_millis(20);

impl InvocationOutcome for AgentSpawnOutcome {
    fn was_interrupted(&self) -> bool {
        self.interrupted
    }

    fn timed_out(&self) -> bool {
        self.timed_out
    }

    fn status(&self) -> std::process::ExitStatus {
        self.status
    }
}

fn with_agent_log<T>(
    log_file: &Arc<Mutex<fs::File>>,
    write: impl FnOnce(&mut fs::File) -> std::io::Result<T>,
) -> std::io::Result<T> {
    let mut guard = log_file
        .lock()
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "agent log lock poisoned"))?;
    write(&mut guard)
}

fn output_line(buf: &[u8]) -> String {
    let line = buf.strip_suffix(b"\n").unwrap_or(buf);
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    String::from_utf8_lossy(line).into_owned()
}

fn agent_stream_label(stream: rhei_tui::AgentStream) -> &'static str {
    match stream {
        rhei_tui::AgentStream::Stdout => "stdout",
        rhei_tui::AgentStream::Stderr => "stderr",
    }
}

/// Keep the thread's independently owned inputs explicit, as in `spawn_and_wait_agent`.
/// Raw capture is separate from usage parsing and display for provider recognition. §FS-rhei-agents.2
#[allow(clippy::too_many_arguments)]
fn spawn_agent_output_reader<R>(
    reader: R,
    stream: rhei_tui::AgentStream,
    log_file: Arc<Mutex<fs::File>>,
    sink: Arc<dyn rhei_tui::EventSink>,
    slot: rhei_tui::Slot,
    task_id: String,
    usage_capture: Option<AgentUsageCapture>,
    captured_lines: Arc<Mutex<Vec<String>>>,
) -> std::thread::JoinHandle<std::io::Result<()>>
where
    R: Read + Send + 'static,
{
    std::thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            let read = reader.read_until(b'\n', &mut buf)?;
            if read == 0 {
                break;
            }

            let raw_line = output_line(&buf);
            if let Ok(mut captured) = captured_lines.lock() {
                captured.push(raw_line.clone());
            }
            let display_line = display_agent_output_line(usage_capture.as_ref(), stream, &raw_line);
            let is_claude_result = stream == rhei_tui::AgentStream::Stdout
                && usage_capture
                    .as_ref()
                    .is_some_and(|capture| capture.extractor == AgentUsageExtractor::Claude)
                && matches!(parse_claude_result_line(&raw_line), ClaudeResultLine::Result(_));
            with_agent_log(&log_file, |f| {
                if is_claude_result {
                    if let Some(line) = display_line.as_deref() {
                        f.write_all(line.as_bytes())?;
                        if buf.ends_with(b"\n") && !line.ends_with('\n') {
                            f.write_all(b"\n")?;
                        }
                    }
                } else {
                    f.write_all(&buf)?;
                }
                f.flush()
            })?;

            capture_agent_output_usage(usage_capture.as_ref(), stream, &raw_line, &sink);
            if let Some(line) = display_line {
                for line in agent_output_lines(line, is_claude_result) {
                    sink.emit(rhei_tui::RunEvent::AgentOutput {
                        slot,
                        task: task_id.clone(),
                        stream,
                        line,
                        wall_clock: std::time::SystemTime::now(),
                    });
                }
            }
        }
        Ok(())
    })
}

fn drain_agent_output_reader(
    handle: std::thread::JoinHandle<std::io::Result<()>>,
    stream: rhei_tui::AgentStream,
) -> MietteResult<()> {
    let deadline = Instant::now() + AGENT_OUTPUT_DRAIN_GRACE;
    while !handle.is_finished() {
        if Instant::now() >= deadline {
            // A descendant may still hold the inherited pipe open after the
            // direct agent process exits. Detach the reader instead of
            // blocking run completion forever; future bytes may still be
            // captured best-effort until process exit.
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(1));
    }

    match handle.join() {
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

/// Spawn an agent, capture output to a log file, and wait with timeout.
///
/// Returns [`AgentSpawnOutcome`] describing the exit status and whether the
/// process was killed by the engine's timeout watchdog (so the caller can
/// route a SIGTERM-induced non-zero exit through the timeout transition
/// path rather than the generic non-zero exit path). `runtime_dir` is used
/// for the generated `mcp_config_flag` JSON file (see [`build_agent_command`]).
#[allow(clippy::too_many_arguments)]
fn spawn_and_wait_agent(
    resolved: &ResolvedAgent,
    price_book: &PriceBook,
    prompt: &str,
    rhei_root: &Path,
    checkout_root: &Path,
    worktree_root: Option<&Path>,
    plan_path: &Path,
    state_machine_path: Option<&Path>,
    task_id: &str,
    state_name: &str,
    visit_count: u64,
    tooling: &ResolvedTooling,
    log_path: &Path,
    runtime_dir: &Path,
    snapshot_preload: Option<&SnapshotPreload>,
    slot: rhei_tui::Slot,
    sink: Arc<dyn rhei_tui::EventSink>,
    intervene: Option<&Arc<RunInterveneSink>>,
    // Which attempt of which visit this is. Recorded here, beside the footer,
    // because this is the one place that knows the subprocess actually ran.
    // §FS-rhei-agents.8.4
    plan: &SpawnPlan,
    // Kept at this orchestration boundary because the caller and completion
    // path key the same invocation; the prompt already carries its result path.
    // §FS-rhei-states.3.3
    _result_identity: Option<&str>,
    budget_lease: Option<&BudgetLease>,
) -> MietteResult<AgentSpawnOutcome> {
    // Ensure log directory exists.
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| miette!(
                help = agent_log_help(),
                "failed to create log directory '{}': {e}", parent.display()
            ))?;
    }

    let log_file = Arc::new(Mutex::new(
        fs::File::create(log_path)
            .map_err(|e| miette!(
                help = agent_log_help(),
                "failed to create log file '{}': {e}", log_path.display()
            ))?,
    ));

    // §FS-rhei-agents.8: Agent log header format.

    // Write log header. The `v1` suffix is the structural-format version:
    // any future change to the header/footer layout must bump it.
    let started_wall = std::time::SystemTime::now();
    with_agent_log(&log_file, |f| {
        writeln!(f, "=== rhei agent log v1 ===")?;
        writeln!(f, "agent: {}", resolved.agent.id())?;
        if let Some(mode) = &resolved.mode {
            writeln!(f, "mode: {mode}")?;
        }
        if let Some(target) = &resolved.target {
            writeln!(f, "target: {}", target.selector())?;
        }
        if let Some(m) = &resolved.model {
            writeln!(f, "model: {m}")?;
        }
        if let Some(provider) = &resolved.model_provider {
            writeln!(f, "provider: {provider}")?;
        }
        if let Some(model_name) = &resolved.model_name {
            writeln!(f, "model_name: {model_name}")?;
        }
        writeln!(f, "task: {task_id}")?;
        writeln!(f, "state: {state_name}")?;
        writeln!(f, "started: {}", format_iso8601_utc(started_wall))?;
        if let Some(t) = resolved.timeout_secs {
            writeln!(f, "timeout: {}", format_duration_human(t))?;
        }
        writeln!(f, "plan: {}", plan_path.display())?;
        writeln!(f, "rhei_root: {}", rhei_root.display())?;
        writeln!(f, "checkout_root: {}", checkout_root.display())?;
        if let Some(path) = worktree_root {
            writeln!(f, "worktree_root: {}", path.display())?;
        }
        let mcp_line = format_tooling_log_line(&tooling.mcp_servers, |e| {
            (e.id.as_str(), e.optional, e.definition.is_some())
        });
        if let Some(line) = mcp_line {
            writeln!(f, "mcp_servers: {line}")?;
        }
        let skill_line = format_tooling_log_line(&tooling.skills, |e| {
            (e.id.as_str(), e.optional, e.definition.is_some())
        });
        if let Some(line) = skill_line {
            writeln!(f, "skills: {line}")?;
        }
        writeln!(f, "===\n")?;
        f.flush()
    })
    .map_err(|e| miette!(
        help = agent_log_help(),
        "failed to write log header '{}': {e}", log_path.display()
    ))?;

    // Emit spawn-time warnings for tooling the agent profile cannot wire.
    // §FS-rhei-agents.1.1.5 §FS-rhei-agents.6: Spawn-time tooling warnings.
    for warning in collect_unsupported_tooling_warnings(resolved, tooling) {
        let _ = with_agent_log(&log_file, |f| writeln!(f, "{warning}"));
        diag_warn!("{warning}");
    }

    // §FS-rhei-cost-accounting.4: Usage capture is declared before the agent starts.
    let usage_capture_path =
        accounting_capture_path_for_spawn(runtime_dir, task_id, state_name, resolved);
    if let Some(parent) = usage_capture_path.as_ref().and_then(|path| path.parent()) {
        let _ = fs::create_dir_all(parent);
    }
    let usage_capture = usage_capture_for_attempt(
        resolved,
        usage_capture_path.as_deref(),
        plan.accounting
            .as_ref()
            .expect("agent spawn plans carry accounting identity"),
        task_id,
        state_name,
        slot,
        price_book,
    );
    let cli_session_capture =
        usage_capture.as_ref().map(|capture| Arc::clone(&capture.cli_session));

    // The strategy flags go through `build_agent_command`, which owns the one
    // slot they belong in; appending them here would put them past the `--`.
    // §FS-rhei-snapshots.10.1
    let snapshot_args =
        snapshot_preload.map(|preload| preload.extra_args.as_slice()).unwrap_or_default();
    let mut cmd = build_agent_command(
        resolved,
        prompt,
        checkout_root,
        worktree_root,
        state_machine_path,
        task_id,
        state_name,
        visit_count,
        plan.attempt,
        tooling,
        runtime_dir,
        snapshot_args,
    );
    configure_accounting_capture(&mut cmd, usage_capture_path.as_deref());
    if let Some(snapshot_preload) = snapshot_preload {
        if let Some(session_dir) = snapshot_preload.session_dir.as_ref() {
            cmd.env("RHEI_SNAPSHOT_SESSION_DIR", session_dir);
        }
        if let Some(parent_ref) = snapshot_preload.parent_ref.as_ref() {
            cmd.env("RHEI_SNAPSHOT_PARENT_REF", parent_ref.to_string());
        }
    }
    cmd.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());

    // The agent leads its own process group, so its MCP servers and shell tools
    // are terminated with it, and it never inherits the operator's terminal.
    // §FS-rhei-run.3.2
    let capability = budget_lease.map(BudgetLease::prepare_process).transpose()?;
    if let Some(capability) = &capability { capability.configure(&mut cmd)?; }
    let spawned = Supervised::spawn_with_budget(&mut cmd, &format!("{task_id}@{state_name}"), capability.as_ref());
    let mut supervised = match spawned {
        Ok(supervised) => supervised,
        // Interrupted between the scheduler's check and the spawn. Nothing
        // started, so there is no verdict on the ticket and no transition
        // fires. §FS-rhei-run.3.2 §FS-rhei-agents.8
        Err(err) if spawn_was_interrupted(&err) => {
            let _ = with_agent_log(&log_file, |f| {
                writeln!(f, "\nagent not started: the run was interrupted first")?;
                writeln!(f, "\n=== exit ===")?;
                writeln!(f, "code: -")?;
                writeln!(f, "ended: {}", format_iso8601_utc(std::time::SystemTime::now()))?;
                writeln!(f, "interrupted: true")?;
                writeln!(f, "===")?;
                f.flush()
            });
            return Ok(AgentSpawnOutcome {
                status: never_started_status(),
                timed_out: false,
                interrupted: true,
                timeout_secs: resolved.timeout_secs,
                usage_capture_path: None,
                cli_session: None,
                provider_limit: None,
            });
        }
        // A command line too large to hand over fails here exactly as a missing
        // binary does, so the size is asked about first. §FS-rhei-errors.7
        Err(e) => return Err(spawn_failure_report(&e, resolved, prompt, &cmd)),
    };
    let child = &mut supervised.child;

    let captured_lines = Arc::new(Mutex::new(Vec::new()));
    let stdout_handle = child.stdout.take().map(|stdout| {
        spawn_agent_output_reader(
            stdout,
            rhei_tui::AgentStream::Stdout,
            log_file.clone(),
            sink.clone(),
            slot,
            task_id.to_string(),
            usage_capture.clone(),
            captured_lines.clone(),
        )
    });
    let stderr_handle = child.stderr.take().map(|stderr| {
        spawn_agent_output_reader(
            stderr,
            rhei_tui::AgentStream::Stderr,
            log_file.clone(),
            sink.clone(),
            slot,
            task_id.to_string(),
            None,
            captured_lines.clone(),
        )
    });

    // Write the prompt to stdin. EOF-driven agents must see EOF before starting,
    // so stdin is closed after the prompt. §FS-rhei-agents.1.1.2

    // `codex exec` is one of those; a profile opting into streaming
    // interventions is the exception. A failed write is said out loud: an agent
    // handed no prompt does whatever it does with nothing, and the run has no
    // other way of telling anyone why. Not fatal — the agent is already running.
    let deliver_prompt = |stdin: &mut std::process::ChildStdin, bytes: &[u8]| {
        use std::io::Write as _;
        if let Err(err) = stdin.write_all(bytes).and_then(|()| stdin.flush()) {
            diag_warn!("could not deliver the prompt on stdin for task {task_id}: {err}");
        }
    };
    let mut registered_intervene = false;
    let stdin_format = agent_stdin_format(resolved);
    if stdin_format == AgentStdinFormat::ClaudeCodeStreamJson {
        if let Some(mut stdin) = child.stdin.take() {
            deliver_prompt(&mut stdin, &stdin_message_bytes(stdin_format, prompt));
            match (resolved.profile.intervene_stdin, intervene) {
                (true, Some(registry)) => {
                    registry.register(
                        task_id,
                        slot,
                        state_name,
                        log_file.clone(),
                        stdin,
                        stdin_format,
                    );
                    registered_intervene = true;
                }
                _ => drop(stdin),
            }
        }
    } else if resolved.profile.stdin_prompt {
        if let Some(mut stdin) = child.stdin.take() {
            deliver_prompt(&mut stdin, prompt.as_bytes());
            match (resolved.profile.intervene_stdin, intervene) {
                (true, Some(registry)) => {
                    registry.register(
                        task_id,
                        slot,
                        state_name,
                        log_file.clone(),
                        stdin,
                        stdin_format,
                    );
                    registered_intervene = true;
                }
                _ => drop(stdin),
            }
        }
    } else if resolved.profile.intervene_stdin {
        if let Some(stdin) = child.stdin.take() {
            // Close unused intervention stdin when the live surface is absent. §FS-rhei-agents.1.1.2
            if let Some(registry) = intervene {
                registry.register(task_id, slot, state_name, log_file.clone(), stdin, stdin_format);
                registered_intervene = true;
            } else {
                drop(stdin);
            }
        }
    }

    let start = Instant::now();

    // One wait for all three endings: exit, deadline, run interruption. The
    // last two terminate the agent's whole group. §FS-rhei-run.3.2
    let ended = supervised
        .wait(
            resolved.timeout_secs.map(Duration::from_secs),
            &INTERRUPT,
            &notify_through_sink(&sink),
        )
        .map_err(|e| miette!(
            help = agent_command_help(),
            "error waiting for agent: {e}"
        ))?;
    let status = ended.status;
    let timed_out = ended.cause == EndCause::TimedOut;
    let interrupted = ended.cause == EndCause::Interrupted;

    // The agent has exited: drop its intervene registration, which ends the
    // stdin writer thread and closes the pipe.
    if registered_intervene {
        if let Some(registry) = intervene {
            registry.unregister(task_id, slot);
        }
    }

    if let Some(handle) = stdout_handle {
        drain_agent_output_reader(handle, rhei_tui::AgentStream::Stdout)?;
    }
    if let Some(handle) = stderr_handle {
        drain_agent_output_reader(handle, rhei_tui::AgentStream::Stderr)?;
    }

    // Write log footer. The `ended:` ISO timestamp and human-readable
    // `duration:` mirror the header. When the engine
    // killed the agent for exceeding its timeout, we emit the spec-required
    // `agent timed out after {duration}` line so operators see the cause
    // without inferring it from the exit code alone.

    // §FS-rhei-agents.8: Agent log footer, timeout and interruption causes.
    let elapsed = start.elapsed();
    let duration = format_duration_human(elapsed.as_secs());
    let ended_wall = std::time::SystemTime::now();
    let provider_limit = captured_lines.lock().ok().and_then(|lines| {
        classify_provider_limit(
            resolved,
            status,
            timed_out,
            interrupted,
            &lines,
            ended_wall,
        )
    });
    let timeout_message =
        if timed_out { resolved.timeout_secs.map(format_duration_human) } else { None };
    // The spawn ran. Say so where the next pass, the next run, and the engine's
    // own account of this state will all read it. §FS-rhei-agents.8.4
    plan.record_spawn(SpawnEnding {
        task_id,
        state_name,
        kind: "agent",
        worker: resolved.agent.id(),
        started: &format_iso8601_utc(started_wall),
        ended: &format_iso8601_utc(ended_wall),
        duration: &duration,
        code: status.code(),
        ending: if provider_limit.is_some() {
            "provider_limited"
        } else if timed_out {
            "timed out"
        } else if interrupted {
            "interrupted"
        } else {
            "exited"
        },
    });
    with_agent_log(&log_file, |f| {
        if let Some(limit) = &provider_limit {
            writeln!(f, "\nagent provider-limited until {}", limit.next_attempt_at)?;
        } else if let Some(duration) = &timeout_message {
            writeln!(f, "\nagent timed out after {duration}")?;
        } else if interrupted {
            // The run was shutting down, not the agent failing. §FS-rhei-run.3.2
            writeln!(f, "\nagent interrupted by run shutdown after {duration}")?;
        }
        writeln!(f, "\n=== exit ===")?;
        writeln!(f, "code: {}", status.code().unwrap_or(-1))?;
        writeln!(f, "duration: {duration}")?;
        writeln!(f, "ended: {}", format_iso8601_utc(ended_wall))?;
        if timed_out {
            writeln!(f, "timed_out: true")?;
        }
        if interrupted {
            writeln!(f, "interrupted: true")?;
        }
        if provider_limit.is_some() {
            writeln!(f, "provider_limited: true")?;
        }
        writeln!(f, "===")?;
        f.flush()
    })
    .map_err(|e| miette!(
        help = agent_log_help(),
        "failed to append to log file '{}': {e}", log_path.display()
    ))?;

    let cli_session = cli_session_capture
        .and_then(|capture| capture.lock().ok().and_then(|observed| observed.clone()));
    Ok(AgentSpawnOutcome {
        status,
        timed_out,
        interrupted,
        timeout_secs: resolved.timeout_secs,
        usage_capture_path,
        cli_session,
        provider_limit,
    })
}
