// `rhei gate answer`: check an answer against the brief the ticket is at,
// pick where it leads, and take that edge through the ordinary transition
// path, which records the answer file under the plan lock. Every refusal is
// one of the kinds a caller can branch on.

// §FS-rhei-gate-cmd.3 §FS-rhei-gate-brief.7

/// What an answer records, carried into the transition so the file is written
/// under the same plan lock as the state. §FS-rhei-gate-brief.7.1
#[derive(Debug, Clone)]
struct GateAnswerRecord {
    /// Run every check and stop before the first write. §FS-rhei-gate-cmd.3.1
    dry_run: bool,
    /// `runtime/gates/<task-id>/<state>.<visit>.answer.json`, under the root.
    path: String,
    /// The document as recorded; `to` is filled with the effective target.
    document: rhei_core::gate::GateAnswer,
}

/// The answer file while its transition is pending: unless kept, it is
/// removed again, or the file it replaced is put back, and so are the
/// directories it made. §FS-rhei-gate-brief.7.1
struct GateAnswerFile {
    path: Option<PathBuf>,
    previous: Option<Vec<u8>>,
    made: Vec<PathBuf>,
    armed: bool,
}

impl GateAnswerFile {
    /// Write the answer `record` carries, naming the target `to` the ticket
    /// enters. `None` is a dry run, which writes nothing; a transition that is
    /// no answer gets a guard that does nothing. §FS-rhei-gate-cmd.3.1
    fn write(record: Option<&GateAnswerRecord>, root: &Path, to: &str) -> MietteResult<Option<Self>> {
        let Some(record) = record else {
            return Ok(Some(Self { path: None, previous: None, made: Vec::new(), armed: false }));
        };
        if record.dry_run {
            return Ok(None);
        }
        let path = root.join(&record.path);
        let mut document = record.document.clone();
        document.to = Some(to.to_string());
        let text = serde_json::to_string_pretty(&document)
            // §FS-rhei-errors.6
            .map_err(|err| miette!(help = INTERNAL_DIAGNOSTIC_HELP, "failed to render the gate answer: {err}"))?;
        let parent = path.parent().unwrap_or(root);
        let made: Vec<PathBuf> =
            parent.ancestors().take_while(|dir| !dir.exists()).map(Path::to_path_buf).collect();
        fs::create_dir_all(parent)
            .map_err(|err| file_io_report(parent, "failed to create the gate answer directory", err))?;
        let previous = fs::read(&path).ok();
        // Armed before the write, so a write that fails takes its directories back.
        let file = Self { path: Some(path.clone()), previous, made, armed: true };
        write_file_atomic(&path, &format!("{text}\n"))?;
        Ok(Some(file))
    }

    /// The state write landed: the answer stays.
    fn keep(&mut self) {
        self.armed = false;
    }

    /// Take the answer back. Best effort: the refusal that called for it is
    /// what the caller reports. §FS-rhei-gate-brief.7.1
    fn undo(&mut self) {
        self.armed = false;
        let Some(path) = self.path.take() else {
            return;
        };
        let _ = match self.previous.take() {
            Some(previous) => fs::write(&path, previous),
            None => fs::remove_file(&path),
        };
        for dir in &self.made {
            let _ = fs::remove_dir(dir);
        }
    }
}

impl Drop for GateAnswerFile {
    fn drop(&mut self) {
        if self.armed {
            self.undo();
        }
    }
}

/// One `rhei gate answer`, as the dispatch resolved it. §FS-rhei-gate-cmd.3
struct GateAnswerRequest<'a> {
    input: &'a Path,
    rhei_scope: &'a [String],
    state_machine_path: Option<&'a Path>,
    task: &'a str,
    from: &'a str,
    answers: &'a str,
    dry_run: bool,
    json: bool,
}

/// What `rhei gate answer` prints, on success and on refusal. §FS-rhei-gate-cmd.3.2
#[derive(serde::Serialize)]
struct GateAnswerOutcome {
    version: u64,
    ok: bool,
    dry_run: bool,
    ticket: String,
    from: String,
    to: Option<String>,
    route: Option<rhei_core::gate::RouteChoice>,
    answer_path: Option<String>,
    result: Option<String>,
    errors: Vec<rhei_core::gate::GateError>,
}

impl GateAnswerOutcome {
    /// Nothing checked yet: not ok, and nowhere to go.
    fn new(ticket: &str, from: &str, dry_run: bool) -> Self {
        Self {
            version: rhei_core::gate::GATE_DOCUMENT_VERSION,
            ok: false,
            dry_run,
            ticket: ticket.to_string(),
            from: from.to_string(),
            to: None,
            route: None,
            answer_path: None,
            result: None,
            errors: Vec::new(),
        }
    }

    fn refuse(&mut self, kind: rhei_core::gate::GateErrorKind, message: impl Into<String>) {
        self.errors.push(rhei_core::gate::GateError::new(kind, None, message));
    }
}

/// Answer the ticket's brief and move it; print the outcome, and exit 1 on
/// any refusal with the outcome still on stdout. §FS-rhei-gate-cmd.3
fn gate_answer_command(request: &GateAnswerRequest<'_>) -> MietteResult<()> {
    let input_buf = normalize_workspace_input(request.input);
    let input = input_buf.as_path();
    let loaded = load_plan(input)?;
    let scope = resolve_rhei_scope(&loaded, request.rhei_scope)?;
    let task_id = resolve_cli_task_id(&loaded, request.task, &scope)?;
    let resolved = resolve_state_machines_for_loaded_plan(input, &loaded, request.state_machine_path)?;
    let machines = ExecutionMachines::build(&resolved, input, &loaded)?;
    let task = find_task_by_id_str(&loaded.rhei.tasks, &task_id)
        .ok_or_else(|| miette!(help = task_id_help(), "task '{}' not found in the plan", task_id))?;
    let mut outcome = GateAnswerOutcome::new(&task_id, request.from, request.dry_run);
    let ask = GateAsk {
        from: request.from,
        answers: GateAnswerText::Source(request.answers),
        dry_run: request.dry_run,
        no_callbacks: false,
        advise_live_run: true,
    };
    let scene = GateAnswerScene { ask, input, loaded: &loaded, machines: &machines, task, task_id: &task_id };
    answer_gate(&scene, &mut outcome)?;
    outcome.ok = outcome.errors.is_empty();
    if request.json {
        print_gate_json(&outcome, "the gate answer")?;
    } else if outcome.ok {
        let to = outcome.to.as_deref().unwrap_or_default();
        if request.dry_run {
            println!("Dry run: Task {task_id} would transition: '{}' → '{to}'; nothing was written", request.from);
        } else {
            println!("Task {task_id} transitioned: '{}' → '{to}'", request.from);
        }
        println!("{}", outcome.result.as_deref().unwrap_or_default());
    } else {
        for error in &outcome.errors {
            let kind = serde_json::to_value(error.kind).ok();
            eprintln!("{}: {}", kind.as_ref().and_then(|kind| kind.as_str()).unwrap_or("refused"), error.message);
        }
    }
    if !outcome.ok {
        std::process::exit(1);
    }
    Ok(())
}

/// What one answer asks, wherever it was posted from: the CLI names a file or
/// stdin, a live surface carries the document itself. §FS-rhei-viz.5.1.2
struct GateAsk<'a> {
    from: &'a str,
    answers: GateAnswerText<'a>,
    dry_run: bool,
    /// The run's `--no-callbacks`, which a surface inside that run honors as its
    /// release does; a dry run runs no callback either way.
    no_callbacks: bool,
    /// Tell a live run its gate moved. A surface inside the run is that run.
    advise_live_run: bool,
}

/// Where the answer document comes from.
enum GateAnswerText<'a> {
    /// `--answers`: a path, or `-` for stdin.
    Source(&'a str),
    /// The document itself, as a surface posts it.
    Inline(&'a str),
}

/// What one answer is checked against.
struct GateAnswerScene<'a> {
    ask: GateAsk<'a>,
    input: &'a Path,
    loaded: &'a LoadedPlan,
    machines: &'a ExecutionMachines,
    task: &'a rhei_core::ast::Task,
    task_id: &'a str,
}

/// Steps 1 to 6 of the answer: each refusal goes into `outcome`, and the
/// first step that refuses ends it. An `Err` is a failure to read, not a
/// refusal. §FS-rhei-gate-cmd.3
fn answer_gate(scene: &GateAnswerScene<'_>, outcome: &mut GateAnswerOutcome) -> MietteResult<()> {
    use rhei_core::gate::GateErrorKind;
    let (task_id, from) = (scene.task_id, scene.ask.from);
    let machine = scene.machines.for_task_str(task_id);
    if !machine.states.get(from).is_some_and(|def| def.gating) {
        outcome.refuse(GateErrorKind::NotAGate, format!("'{from}' is not a gate (`gating: true`) of {task_id}'s machine"));
        return Ok(());
    }
    let current = normalized_state_name(scene.task.state.as_str(), machine);
    if current != from {
        outcome.refuse(GateErrorKind::Conflict, format!("Task {task_id} is in state '{current}', not '{from}'"));
        return Ok(());
    }
    let source = GateViewSource { loaded: scene.loaded, input: scene.input, machine, task: scene.task };
    let Some(view) = gate_view(&source)? else {
        outcome.refuse(GateErrorKind::NotAGate, format!("Task {task_id} is not at a gate"));
        return Ok(());
    };
    let answer_path = view.answer_path();
    outcome.answer_path = Some(answer_path.clone());
    let text = match scene.ask.answers {
        GateAnswerText::Source(source) => read_answer_text(source),
        GateAnswerText::Inline(text) => Ok(text.to_string()),
    };
    let document = text.and_then(|text| rhei_core::gate::parse_answer(&text));
    let document = match document {
        Ok(document) => document,
        Err(errors) => {
            outcome.errors = errors;
            return Ok(());
        }
    };
    outcome.errors = rhei_core::gate::answer_problems(&view.brief, &document);
    if outcome.errors.iter().any(|error| error.kind == GateErrorKind::Stale) {
        // A visit read after another answer moved the ticket is a new one: that
        // answer won, and the ticket is no longer here. §FS-rhei-gate-cmd.3
        if let Some(now) = ticket_state_now(scene, machine).filter(|now| now != from) {
            outcome.errors.clear();
            outcome.refuse(GateErrorKind::Conflict, format!("Task {task_id} is in state '{now}', not '{from}'"));
        }
    }
    if !outcome.errors.is_empty() {
        return Ok(());
    }
    // Known once the answers hold, so an `unrouted` refusal carries it too. §FS-rhei-gate-cmd.3.2
    let rendered = rhei_core::gate::render_answers(&view.brief, &document.answers, &answer_path);
    outcome.result = Some(rendered.clone());
    let (to, route) = match rhei_core::gate::pick_destination(&view.brief, &document.answers) {
        Ok(picked) => picked,
        Err(error) => {
            outcome.errors.push(error);
            return Ok(());
        }
    };
    (outcome.to, outcome.route) = (Some(to.clone()), route.clone());
    let answered_by = match invoking_os_account() {
        Ok(account) => account,
        Err(message) => {
            outcome.refuse(GateErrorKind::Refused, message);
            return Ok(());
        }
    };
    let answered_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let record = GateAnswerRecord {
        dry_run: scene.ask.dry_run,
        path: answer_path,
        document: rhei_core::gate::GateAnswer {
            ticket: Some(task_id.to_string()),
            state: Some(view.state.clone()),
            visit: Some(view.visit),
            to: None,
            route: Some(route),
            answered_by: Some(answered_by),
            answered_at: Some(answered_at),
            ..document
        },
    };
    let route = scene.loaded.task_route(task_id, scene.input);
    let moved = execute_transition_with_origin(
        TransitionFiles {
            task_file: &route.task_file,
            role: scene.loaded.document_role(&route.task_file),
            metadata_file: &route.metadata_file,
            metadata_id: &route.metadata_id,
            artifact_root: &route.execution_root,
            artifact_id: task_id,
        },
        scene.machines.callbacks_for_str(task_id),
        machine,
        &route.local_id,
        from,
        &to,
        // A dry run runs no callback. §FS-rhei-gate-cmd.3.1
        scene.ask.dry_run || scene.ask.no_callbacks,
        // An answer takes the declared edge as a person's release of the gate,
        // never a forced move. §FS-rhei-gate-brief.7
        TransitionOrigin {
            manual_release: Some(false),
            result_message: Some(rendered),
            gate_answer: Some(record),
            ..TransitionOrigin::default()
        },
        None,
        None,
    );
    match moved {
        Ok(effective) => {
            if !scene.ask.dry_run && scene.ask.advise_live_run {
                // §FS-rhei-transition-cmd.3.4
                advise_live_gate_budget(scene.input, &route, machine, task_id, from, &effective);
            }
            outcome.to = Some(effective);
        }
        // The compare-and-swap again, under the lock: another caller moved it. §FS-rhei-gate-cmd.3
        Err(err) if err.to_string().starts_with("conflict:") => {
            outcome.refuse(GateErrorKind::Conflict, err.to_string().trim_start_matches("conflict: ").to_string());
        }
        Err(err) => outcome.refuse(GateErrorKind::Refused, err.to_string()),
    }
    Ok(())
}

/// The ticket's state as the plan reads now, for a refusal that may have
/// raced another answer.
fn ticket_state_now(scene: &GateAnswerScene<'_>, machine: &rhei_validator::StateMachine) -> Option<String> {
    let loaded = load_plan(scene.input).ok()?;
    let task = find_task_by_id_str(&loaded.rhei.tasks, scene.task_id)?;
    Some(normalized_state_name(task.state.as_str(), machine))
}

/// The answer document from `--answers`: a path, or `-` for stdin.
fn read_answer_text(source: &str) -> Result<String, Vec<rhei_core::gate::GateError>> {
    let failed = |err: std::io::Error| {
        let message = format!("cannot read --answers {source}: {err}");
        vec![rhei_core::gate::GateError::new(rhei_core::gate::GateErrorKind::Invalid, None, message)]
    };
    if source == "-" {
        let mut text = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut text).map_err(failed)?;
        return Ok(text);
    }
    fs::read_to_string(source).map_err(failed)
}

/// The OS account that answers, as the operating system's `whoami` names it,
/// never an environment variable. §FS-rhei-gate-brief.7.1
fn invoking_os_account() -> Result<String, String> {
    let output = std::process::Command::new("whoami")
        .output()
        .map_err(|err| format!("cannot identify the OS account that answers: {err}"))?;
    let account = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !output.status.success() || account.is_empty() {
        return Err("cannot identify the OS account that answers: `whoami` named none".to_string());
    }
    Ok(account)
}
