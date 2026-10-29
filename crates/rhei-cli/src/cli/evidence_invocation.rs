// The start and end records of one agent or program process.
// §FS-rhei-evidence.3 §FS-rhei-evidence.4

const EVIDENCE_INVOCATION_SCHEMA: &str = "rhei.evidence.invocation.v1";

/// One process's evidence, from its published start record to its end.
struct EvidenceInvocation {
    run: Arc<EvidenceRun>,
    kind: &'static str,
    id_field: &'static str,
    id: String,
    task_id: String,
    start_path: PathBuf,
}

/// A program's invocation id: the agent's spelling with `program` as its
/// target segment. §FS-rhei-evidence.2.1
fn program_invocation_id(
    task_id: &str,
    state: &str,
    visit: u64,
    run_id: &str,
    moves: u64,
    attempt: u64,
) -> String {
    format!(
        "{}::{}::program::visit-{visit}::run-{}::move-{moves}::attempt-{attempt}",
        safe_accounting_file_segment(task_id),
        safe_accounting_file_segment(state),
        safe_accounting_file_segment(run_id),
    )
}

/// The argument list handed to the OS, with the prompt shown as `<prompt>`.
/// §FS-rhei-evidence.3.2
fn evidence_argv(cmd: &std::process::Command, prompt: Option<&str>) -> Vec<String> {
    std::iter::once(cmd.get_program())
        .chain(cmd.get_args())
        .map(|arg| {
            let arg = arg.to_string_lossy().into_owned();
            if prompt.is_some_and(|prompt| prompt == arg) { "<prompt>".to_string() } else { arg }
        })
        .collect()
}

/// The absolute directory the process starts in. §FS-rhei-evidence.3.2
fn evidence_cwd(cmd: &std::process::Command) -> String {
    let here = std::env::current_dir().unwrap_or_default();
    let dir = cmd.get_current_dir().map_or_else(|| here.clone(), |dir| here.join(dir));
    rhei_core::platform::canonical_path(&dir).unwrap_or(dir).display().to_string()
}

/// The seal generation the strict run checked before this start, as
/// `{rhei, generation, path}`; `null` outside a strict run. §FS-rhei-evidence.3.1
fn evidence_seal(run: &EvidenceRun, task_id: &str) -> serde_json::Value {
    match strict_checked_seal(task_id) {
        Some((rhei, generation, file)) => serde_json::json!({
            "rhei": rhei,
            "generation": generation,
            "path": run.relative(&file),
        }),
        None => serde_json::Value::Null,
    }
}

/// How the prompt travelled. §FS-rhei-evidence.5.1
fn prompt_delivery(resolved: &ResolvedAgent, argv: &[String]) -> &'static str {
    if agent_stdin_format(resolved) == AgentStdinFormat::ClaudeCodeStreamJson {
        "stdin-json"
    } else if resolved.profile.stdin_prompt {
        "stdin"
    } else if argv.iter().any(|arg| arg == "<prompt>") {
        "argument"
    } else {
        "none"
    }
}

impl SpawnPlan {
    /// The grants the spawn record carries for this visit, as the start
    /// record copies them. §FS-rhei-evidence.3.1 §FS-rhei-retry.2
    fn evidence_grants(&self) -> serde_json::Value {
        let grants = read_spawn_record(&self.record)
            .filter(|stored| stored.moves == self.moves)
            .map(|stored| stored.grants)
            .or_else(|| self.previous.as_ref().map(|previous| previous.grants.clone()))
            .unwrap_or_default();
        serde_json::to_value(grants).unwrap_or_else(|_| serde_json::json!([]))
    }
}

/// The state's part of an agent's consumed configuration: its effort, its
/// attempt budget and its snapshot rules. §FS-rhei-evidence.6
fn agent_evidence_state(state_def: Option<&rhei_validator::StateDef>, budget: AttemptBudget) -> serde_json::Value {
    let (per, max) = match budget {
        AttemptBudget::Visit(max) => ("visit", max),
        AttemptBudget::Poll { max_attempts } => ("poll", max_attempts),
    };
    serde_json::json!({
        "effort": state_def.and_then(|state| state.effort).map(|effort| effort.as_str()),
        "attempt_budget": { "per": per, "max": max },
        "snapshot": state_def.and_then(|state| state.snapshot.as_ref()),
        "session": state_def.and_then(|state| state.session),
    })
}

/// The resolved agent configuration this spawn was built from, with the
/// state's part of it. §FS-rhei-evidence.6
fn agent_evidence_config(
    resolved: &ResolvedAgent,
    tooling: &ResolvedTooling,
    state: &serde_json::Value,
) -> serde_json::Value {
    let mcp: Vec<serde_json::Value> = tooling
        .mcp_servers
        .iter()
        .filter(|entry| entry.definition.is_some())
        .map(|entry| serde_json::json!({ "id": entry.id, "definition": entry.definition }))
        .collect();
    let skills: Vec<serde_json::Value> = tooling
        .skills
        .iter()
        .filter(|entry| entry.definition.is_some())
        .map(|entry| serde_json::json!({ "id": entry.id, "definition": entry.definition }))
        .collect();
    serde_json::json!({
        "agent": resolved.agent.id(),
        "family": resolved.family(),
        "profile": resolved.profile,
        "mode": resolved.mode,
        "target": resolved.target.as_ref().map(|target| target.selector()),
        "model": resolved.model,
        "model_provider": resolved.model_provider,
        "model_name": resolved.model_name,
        "autonomous_args": resolved.autonomous_args,
        "timeout_secs": resolved.timeout_secs,
        "mcp_servers": mcp,
        "skills": skills,
        "effort": state["effort"],
        "attempt_budget": state["attempt_budget"],
        "snapshot": state["snapshot"],
        "session": state["session"],
    })
}

/// What an agent spawn's start record is built from.
struct AgentEvidenceStart<'a> {
    resolved: &'a ResolvedAgent,
    tooling: &'a ResolvedTooling,
    prompt: &'a str,
    cmd: &'a std::process::Command,
    task_id: &'a str,
    state_name: &'a str,
    plan: &'a SpawnPlan,
    snapshot: Option<&'a SnapshotPreload>,
    log_path: &'a Path,
}

/// Publish an agent's start record before it spawns; `None` outside a run.
/// A record that cannot be written refuses the spawn. §FS-rhei-evidence.3.3
fn begin_agent_evidence(start: AgentEvidenceStart<'_>) -> MietteResult<Option<EvidenceInvocation>> {
    let Some(run) = evidence_run_holding(start.log_path) else { return Ok(None) };
    let Some(id) = start.plan.accounting.as_ref().map(|a| a.invocation_id.clone()) else {
        return Ok(None);
    };
    let argv = evidence_argv(start.cmd, Some(start.prompt));
    let mut prompt = run.retain(start.prompt.as_bytes(), "prompt");
    prompt["delivery"] = serde_json::json!(prompt_delivery(start.resolved, &argv));
    let cwd = evidence_cwd(start.cmd);
    let (version, note) = run.agent_cli_version(start.resolved, Path::new(&cwd));
    let snapshot = start.snapshot.and_then(|preload| {
        Some(serde_json::json!({
            "ref": preload.parent_ref.clone()?,
            "transcript_sha256": preload.transcript_sha256.clone()?,
        }))
    });
    let record = serde_json::json!({
        "kind": "agent",
        "invocation_id": id,
        "task_id": start.task_id,
        "state": start.state_name,
        "argv": argv,
        "cwd": cwd,
        "config": evidence_config(&run, agent_evidence_config(start.resolved, start.tooling, &start.plan.evidence_state)),
        "prompt": prompt,
        "agent_cli_version": version,
        "agent_cli_version_note": note,
        "snapshot": snapshot,
        "grants": start.plan.evidence_grants(),
        "seal": evidence_seal(&run, start.task_id),
        "log_path": run.relative(start.log_path),
    });
    EvidenceInvocation::publish(run, "agent", "invocation_id", id, record).map(Some)
}

/// Publish a program's start record before it spawns; `None` outside a run.
/// §FS-rhei-evidence.3.3 §FS-rhei-evidence.6
fn begin_program_evidence(
    resolved: &ResolvedProgram,
    cmd: &std::process::Command,
    context: &RuntimeTemplateContext<'_>,
    plan: &SpawnPlan,
    log_path: &Path,
) -> MietteResult<Option<EvidenceInvocation>> {
    let Some(run) = evidence_run_holding(log_path) else { return Ok(None) };
    let task_id = &context.task.id.to_string();
    let state_name = context.state_name;
    let visit = render_visit_count(
        context.metadata,
        &context.task.id,
        state_name,
        context.current_state_raw,
        context.machine,
    );
    let id = program_invocation_id(task_id, state_name, visit, &run.run_id, plan.moves, plan.attempt);
    let argv = evidence_argv(cmd, None);
    let config = serde_json::json!({
        "program": {
            "command": argv,
            "shell": resolved.program.shell,
            "working_directory": resolved.program.working_directory,
            "env": resolved.program.env,
        },
        "timeout_secs": resolved.timeout_secs,
    });
    let record = serde_json::json!({
        "kind": "program",
        "invocation_id": id,
        "task_id": task_id,
        "state": state_name,
        "argv": argv,
        "cwd": evidence_cwd(cmd),
        "config": evidence_config(&run, config),
        "grants": plan.evidence_grants(),
        "seal": evidence_seal(&run, task_id),
        "log_path": run.relative(log_path),
    });
    EvidenceInvocation::publish(run, "program", "invocation_id", id, record).map(Some)
}

impl EvidenceInvocation {
    /// Write the start record under the id's key, exclusively.
    fn publish(
        run: Arc<EvidenceRun>,
        kind: &'static str,
        id_field: &'static str,
        id: String,
        mut record: serde_json::Value,
    ) -> MietteResult<Self> {
        let key = if kind == "callback" { id.clone() } else { evidence_key(&id) };
        let start_path = run.run_dir().join(format!("{key}.start.json"));
        record["schema"] = serde_json::json!(EVIDENCE_INVOCATION_SCHEMA);
        record["phase"] = serde_json::json!("start");
        record["run_id"] = serde_json::json!(run.run_id);
        record["started_at"] = serde_json::json!(format_iso8601_utc(std::time::SystemTime::now()));
        write_evidence_exclusively(&start_path, &record).map_err(|err| {
            miette!(
                help = "evidence records live under runtime/evidence/; check that it is writable",
                "refusing to start {kind} {id}: its evidence start record {} could not be written: {err}",
                start_path.display()
            )
        })?;
        let task_id = record["task_id"].as_str().unwrap_or_default().to_string();
        if kind != "callback" {
            // A new process of the task supersedes any exit still unrouted.
            run.routed_exits().remove(&task_id);
        }
        Ok(EvidenceInvocation { run, kind, id_field, id, task_id, start_path })
    }

    /// The spawn never started: remove the start record, as its log is
    /// removed, so the next attempt can take the key. §FS-rhei-evidence.3.3
    fn abandon(self) {
        let _ = fs::remove_file(&self.start_path);
    }

    /// Write the end record once the log's footer is its last byte.
    /// §FS-rhei-evidence.4
    fn finish(
        self,
        exit_code: Option<i32>,
        ending: &str,
        log: Option<&Path>,
        interventions: Option<Vec<DeliveredIntervention>>,
    ) {
        let mut record = serde_json::json!({
            "schema": EVIDENCE_INVOCATION_SCHEMA,
            "phase": "end",
            "kind": self.kind,
            "run_id": self.run.run_id,
            "ended_at": format_iso8601_utc(std::time::SystemTime::now()),
            "exit_code": exit_code,
            "ending": ending,
        });
        record[self.id_field] = serde_json::json!(self.id);
        if let Some(log) = log {
            let bytes = fs::read(log).unwrap_or_default();
            record["log"] = serde_json::json!({
                "path": self.run.relative(log),
                "sha256": sha256_hex(&bytes),
                "bytes": bytes.len(),
            });
        }
        if let Some(interventions) = interventions {
            let retained: Vec<serde_json::Value> = interventions
                .iter()
                .map(|delivered| {
                    let mut value = self.run.retain(delivered.message.as_bytes(), "intervention");
                    value["at"] = serde_json::json!(delivered.at);
                    value["outcome"] = serde_json::json!(delivered.outcome);
                    value
                })
                .collect();
            record["interventions"] = serde_json::json!(retained);
        }
        let end_path = self.start_path.with_file_name(
            self.start_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .replace(".start.json", ".end.json"),
        );
        if let Err(err) = write_evidence_exclusively(&end_path, &record) {
            diag_warn!("evidence: could not write {}: {err}", end_path.display());
        }
        // The orchestrator routes this exit next; its callbacks name this id.
        if self.kind != "callback" {
            self.run.routed_exits().insert(self.task_id, self.id);
        }
    }
}
