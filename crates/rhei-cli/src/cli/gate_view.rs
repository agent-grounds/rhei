// One ticket's place at a gate, read without a lock or a write: which visit it
// is on and since when, what its brief asks, which files it shows and whether
// they can be read, and how many tickets wait behind it. `rhei gate show`
// prints one; each `rhei gate list` entry summarizes one.

// §FS-rhei-gate-cmd.2.1 §FS-rhei-gate-brief.4

/// One file a brief shows, and whether a person can open it. §FS-rhei-gate-cmd.2.1
#[derive(Debug, Clone, serde::Serialize)]
struct GateFileView {
    path: String,
    absolute: String,
    readable: bool,
    problem: Option<String>,
}

fn gate_file_view(root: &Path, path: &str) -> GateFileView {
    let absolute = root.join(path);
    let problem = match rhei_core::gate::relative_path_problem(path) {
        Some(problem) => Some(problem.to_string()),
        None => match fs::metadata(&absolute) {
            Ok(meta) if meta.is_file() => fs::File::open(&absolute).err().map(|err| format!("cannot be read: {err}")),
            Ok(_) => Some("is not a file".to_string()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Some("does not exist".to_string()),
            Err(err) => Some(format!("cannot be read: {err}")),
        },
    };
    GateFileView {
        path: path.to_string(),
        absolute: absolute.display().to_string(),
        readable: problem.is_none(),
        problem,
    }
}

/// One ticket waiting at a gate, with its brief. §FS-rhei-gate-cmd.2.1
struct GateView {
    ticket: String,
    ticket_title: String,
    state: String,
    visit: u64,
    since: Option<String>,
    plan: PathBuf,
    root: PathBuf,
    brief: rhei_core::gate::Brief,
    display: Option<GateFileView>,
    attachments: Vec<GateFileView>,
    destinations: Vec<rhei_core::gate::Destination>,
}

impl GateView {
    /// Where an answer at this visit is recorded. §FS-rhei-gate-brief.7.1
    fn answer_path(&self) -> String {
        format!("runtime/gates/{}/{}.answer.json", self.ticket, self.visit)
    }
}

/// What a gate view is read from: the loaded plan, the input it was loaded
/// from, and the ticket with its machine.
struct GateViewSource<'a> {
    loaded: &'a LoadedPlan,
    input: &'a Path,
    machine: &'a rhei_validator::StateMachine,
    task: &'a rhei_core::ast::Task,
}

/// The run journal and ledger directory a run over `input` writes: the
/// project's for a project, the ticket's own root otherwise.
fn gate_runtime_dir(source: &GateViewSource<'_>, execution_root: &Path) -> PathBuf {
    if source.loaded.is_panta_project() {
        workspace::panta_project_dir(source.input).unwrap_or_else(|| source.input.to_path_buf()).join("runtime")
    } else {
        execution_root.join("runtime")
    }
}

/// Read the ticket's view at its gate; `None` when its state is not gating.
/// §FS-rhei-gate-cmd.2
fn gate_view(source: &GateViewSource<'_>) -> MietteResult<Option<GateView>> {
    let machine = source.machine;
    let ticket = source.task.id.to_string();
    let state = normalized_state_name(source.task.state.as_str(), machine);
    let Some(state_def) = machine.states.get(&state).filter(|def| def.gating) else {
        return Ok(None);
    };
    let route = source.loaded.task_route(&ticket, source.input);
    let root = absolutize(&route.execution_root);
    let runtime_dir = gate_runtime_dir(source, &route.execution_root);
    // The entry number agent logs are named by, so a gate's visit and its
    // worker's log agree. §FS-rhei-gate-brief.4.4 §FS-rhei-agents.8.1
    let visit = ticket_entry_number_report(&route.execution_root, &runtime_dir, &ticket, &state, Stay::Current)?;
    let since = gate_since(&route.execution_root, &runtime_dir, &ticket, &state);
    let plan = absolutize(&route.task_file);
    let plan_title = owning_rhei_title(source.loaded, &ticket);
    let gate_ticket = GateTicket {
        task_id: &ticket,
        task: Some(source.task),
        state: &state,
        prose: Some(GateProse {
            plan_title: &plan_title,
            plan_path: &plan,
            metadata: source.loaded.rhei.metadata.as_ref(),
        }),
    };
    let destinations = rhei_core::gate::destinations(machine, &state);
    let block = state_def.gate.as_ref();
    let files = block.map(|gate| gate_files(gate, &gate_ticket));
    let request = block.zip(files.as_ref().and_then(|files| files.request.as_deref()))
        .map(|(gate, relative)| read_gate_request(&root, gate, relative));
    let brief = rhei_core::gate::Brief::build(rhei_core::gate::BriefParts {
        ticket: &ticket,
        ticket_title: &source.task.title,
        state: &state,
        visit,
        block,
        block_title: block.and_then(|gate| gate.title.as_deref()).map(|title| gate_ticket.render(title)),
        block_summary: block.and_then(|gate| gate.summary.as_deref()).map(|text| gate_ticket.render(text)),
        block_display: files.and_then(|files| files.display),
        request,
        destinations: &destinations,
    });
    let display = brief.display.as_deref().map(|path| gate_file_view(&root, path));
    let attachments = brief.attachments.iter().map(|path| gate_file_view(&root, path)).collect();
    Ok(Some(GateView {
        ticket: ticket.clone(),
        ticket_title: source.task.title.clone(),
        state,
        visit,
        since,
        plan,
        root,
        brief,
        display,
        attachments,
        destinations,
    }))
}

/// The title of the rhei that owns `ticket`, the plan title its templates see.
fn owning_rhei_title(loaded: &LoadedPlan, ticket: &str) -> String {
    ticket
        .split_once('.')
        .and_then(|(rhei, _)| loaded.rhei_titles.get(rhei))
        .cloned()
        .unwrap_or_else(|| loaded.rhei.title.clone())
}

/// When the ticket last entered `state`, from the run journal: its latest
/// journal line, if the ledger's latest movement of the ticket is that entry
/// and the line names it. `None` when no run journalled it. §FS-rhei-gate-brief.4.4
fn gate_since(root: &Path, runtime_dir: &Path, ticket: &str, state: &str) -> Option<String> {
    let owning = root.join("runtime");
    let dirs: Vec<&Path> = if owning == runtime_dir { vec![&owning] } else { vec![&owning, runtime_dir] };
    let mut last = None;
    for dir in &dirs {
        let Ok(raw) = fs::read_to_string(dir.join("state-transitions.log")) else {
            continue;
        };
        let movements = rhei_core::transition_history::parse(&raw).ok()?;
        if let Some(movement) = movements.into_iter().rfind(|movement| movement.task_id == ticket) {
            last = Some(movement);
        }
    }
    let movement = last.filter(|movement| movement.to == state)?;
    let mut latest: Option<(String, String)> = None;
    for dir in &dirs {
        let Ok(raw) = fs::read_to_string(dir.join("transitions.log")) else {
            continue;
        };
        for line in raw.lines() {
            let mut fields = line.split("  ");
            let (Some(at), Some(task), Some(step)) = (fields.next(), fields.next(), fields.next()) else {
                continue;
            };
            let names_a_move = step.starts_with("start@") || step.starts_with("end@") || step.contains('\u{2192}');
            if task != ticket || !names_a_move {
                continue;
            }
            if latest.as_ref().is_none_or(|(seen, _)| at >= seen.as_str()) {
                latest = Some((at.to_string(), step.to_string()));
            }
        }
    }
    let (at, step) = latest?;
    let entered = format!("{}\u{2192}{state}", movement.from);
    let ended = format!("end@{}", movement.from);
    (step == entered || step == ended).then_some(at)
}

/// How many open tickets cannot become ready until `gate` leaves its gate: its
/// open descendants, and every open ticket whose prior chain reaches it or one
/// of them. §FS-rhei-gate-cmd.4
fn gate_holds(rhei: &rhei_core::ast::Rhei, machines: &rhei_validator::MachineSet, gate: &rhei_core::ast::Task) -> usize {
    fn walk<'a>(task: &'a rhei_core::ast::Task, out: &mut Vec<&'a rhei_core::ast::Task>) {
        out.push(task);
        for child in &task.children {
            walk(child, out);
        }
    }
    let mut all = Vec::new();
    for task in &rhei.tasks {
        walk(task, &mut all);
    }
    let mut beneath = Vec::new();
    for child in &gate.children {
        walk(child, &mut beneath);
    }
    let gate_id = gate.id.to_string();
    let mut reached: std::collections::BTreeSet<String> = beneath.iter().map(|task| task.id.to_string()).collect();
    reached.insert(gate_id.clone());
    // Priors form a DAG, so this settles in at most one pass per ticket.
    loop {
        let before = reached.len();
        for task in &all {
            if task.prior.iter().any(|prior| reached.contains(&prior.to_string())) {
                reached.insert(task.id.to_string());
            }
        }
        if reached.len() == before {
            break;
        }
    }
    all.iter()
        .filter(|task| {
            let id = task.id.to_string();
            id != gate_id
                && reached.contains(&id)
                && !is_terminal_state(task.state.as_str(), machines.for_task(&task.id))
        })
        .count()
}

/// `rhei gate show --json`: the brief, field for field. §FS-rhei-gate-cmd.2.1
#[derive(serde::Serialize)]
struct GateShowDocument<'a> {
    version: u64,
    ticket: &'a str,
    ticket_title: &'a str,
    title: &'a str,
    state: &'a str,
    visit: u64,
    since: Option<&'a str>,
    plan: String,
    root: String,
    source: rhei_core::gate::BriefSource,
    summary: Option<&'a str>,
    display: Option<&'a GateFileView>,
    attachments: &'a [GateFileView],
    questions: &'a [rhei_core::gate::Question],
    destinations: &'a [rhei_core::gate::Destination],
    routes: &'a [rhei_core::gate::Route],
    digest: &'a str,
    answer_path: String,
    problems: &'a [String],
}

impl<'a> GateShowDocument<'a> {
    fn of(view: &'a GateView) -> Self {
        Self {
            version: rhei_core::gate::GATE_DOCUMENT_VERSION,
            ticket: &view.ticket,
            ticket_title: &view.ticket_title,
            title: &view.brief.title,
            state: &view.state,
            visit: view.visit,
            since: view.since.as_deref(),
            plan: view.plan.display().to_string(),
            root: view.root.display().to_string(),
            source: view.brief.source,
            summary: view.brief.summary.as_deref(),
            display: view.display.as_ref(),
            attachments: &view.attachments,
            questions: &view.brief.questions,
            destinations: &view.destinations,
            routes: &view.brief.routes,
            digest: &view.brief.digest,
            answer_path: view.answer_path(),
            problems: &view.brief.problems,
        }
    }
}

/// One `rhei gate list --json` entry. §FS-rhei-gate-cmd.4
#[derive(serde::Serialize)]
struct GateListEntry {
    ticket: String,
    ticket_title: String,
    title: String,
    state: String,
    plan: String,
    root: String,
    visit: u64,
    since: Option<String>,
    source: rhei_core::gate::BriefSource,
    required: usize,
    holds: usize,
    problems: Vec<String>,
}

impl GateListEntry {
    fn of(view: GateView, holds: usize) -> Self {
        Self {
            required: view.brief.required(),
            holds,
            ticket: view.ticket,
            ticket_title: view.ticket_title,
            title: view.brief.title,
            state: view.state,
            plan: view.plan.display().to_string(),
            root: view.root.display().to_string(),
            visit: view.visit,
            since: view.since,
            source: view.brief.source,
            problems: view.brief.problems,
        }
    }
}

/// `rhei gate list --json`. §FS-rhei-gate-cmd.4
#[derive(serde::Serialize)]
struct GateListDocument {
    version: u64,
    root: String,
    gates: Vec<GateListEntry>,
}
