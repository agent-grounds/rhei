// The files a gate's brief is made of, resolved for one ticket: the templates
// rendered, the display found, the request read and checked. Entering a gate
// refuses on them; reading its brief reports on them; `rhei transition` asks
// whether the brief has authored questions it must not skip.

// §FS-rhei-gate-brief.1.1 §FS-rhei-gate-brief.2

/// What a gate's templates resolve against for one ticket. §FS-rhei-gate-brief.1.1
struct GateTicket<'a> {
    /// Project-qualified id, the spelling artifact paths take.
    task_id: &'a str,
    task: Option<&'a rhei_core::ast::Task>,
    /// Canonical gate state name.
    state: &'a str,
    /// What only `title` and `summary` take; `None` where only paths render.
    prose: Option<GateProse<'a>>,
}

/// The prose-only values of a gate's templates. §FS-rhei-gate-brief.1.1
struct GateProse<'a> {
    plan_title: &'a str,
    plan_path: &'a Path,
    metadata: Option<&'a Metadata>,
}

impl GateTicket<'_> {
    fn resolve(&self, name: &str) -> Option<String> {
        match name {
            "task_id" => Some(self.task_id.to_string()),
            "task_id_local" => Some(
                self.task.map(rhei_local_id_of).unwrap_or_else(|| rhei_local_id_str(self.task_id).to_string()),
            ),
            "rhei_id" => self.task.and_then(rhei_id_of),
            "state" => Some(self.state.to_string()),
            _ => {
                let prose = self.prose.as_ref()?;
                match name {
                    "task_title" => self.task.map(|task| task.title.clone()),
                    "plan_title" => Some(prose.plan_title.to_string()),
                    "plan_path" => Some(prose.plan_path.display().to_string()),
                    // A key the ticket does not carry stays as written. §FS-rhei-gate-brief.1.1
                    _ => {
                        let key = name.strip_prefix("meta.")?;
                        task_metadata_map(prose.metadata, &self.task?.id)
                            .and_then(|task_map| task_map.get(yaml_key(key)))
                            .and_then(yaml_value_to_template_string)
                    }
                }
            }
        }
    }

    fn render(&self, template: &str) -> String {
        rhei_core::gate::render_template(template, |name| self.resolve(name))
    }
}

/// A gate block's `display` and `request` paths for one ticket, rendered.
struct GateFiles {
    display: Option<String>,
    request: Option<String>,
}

fn gate_files(gate: &rhei_core::gate::GateDef, ticket: &GateTicket<'_>) -> GateFiles {
    GateFiles {
        display: gate.display.as_deref().map(|path| ticket.render(path)),
        request: gate.request.as_deref().map(|path| ticket.render(path)),
    }
}

/// Read and check the request `relative` names under `root`, every problem
/// at once, each naming the file. §FS-rhei-gate-brief.2 §FS-rhei-gate-brief.4.2
fn read_gate_request(
    root: &Path,
    gate: &rhei_core::gate::GateDef,
    relative: &str,
) -> Result<rhei_core::gate::GateRequest, Vec<String>> {
    if let Some(problem) = rhei_core::gate::relative_path_problem(relative) {
        return Err(vec![format!("the request path '{relative}' {problem}")]);
    }
    let text = match fs::read_to_string(root.join(relative)) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Err(vec![format!("the request file '{relative}' does not exist")]);
        }
        Err(err) => return Err(vec![format!("the request file '{relative}' cannot be read: {err}")]),
    };
    rhei_core::gate::parse_request(&text, gate.display.is_some(), &gate.routes)
        .map_err(|problems| problems.into_iter().map(|problem| format!("{relative}: {problem}")).collect())
}

/// Every problem that keeps a ticket from entering `state_name`'s gate: a
/// missing display or request file, or a request that does not hold.
/// §FS-rhei-gate-brief.2
fn gate_entry_problems(
    root: &Path,
    task: Option<&rhei_core::ast::Task>,
    task_id: &str,
    state_name: &str,
    gate: &rhei_core::gate::GateDef,
) -> Vec<String> {
    let ticket = GateTicket { task_id, task, state: state_name, prose: None };
    let files = gate_files(gate, &ticket);
    let mut problems = Vec::new();
    if let Some(display) = &files.display {
        if let Some(problem) = rhei_core::gate::relative_path_problem(display) {
            problems.push(format!("the display path '{display}' {problem}"));
        } else if !root.join(display).exists() {
            problems.push(format!("the display file '{display}' does not exist"));
        }
    }
    if let Some(request) = &files.request {
        match read_gate_request(root, gate, request) {
            Ok(read) => {
                // The block's own display was checked above; one the request names
                // must exist too. §FS-rhei-gate-brief.2
                if let Some(display) = read.display.as_deref().filter(|_| gate.display.is_none()) {
                    if !root.join(display).exists() {
                        problems.push(format!("{request}: the display file '{display}' does not exist"));
                    }
                }
            }
            Err(found) => problems.extend(found),
        }
    }
    problems
}

/// Refuse the edge into a gate whose brief cannot be shown, as a missing
/// declared input refuses it, listing every problem. §FS-rhei-gate-brief.2
fn ensure_gate_inputs_on_entry(
    root: &Path,
    task: Option<&rhei_core::ast::Task>,
    task_id: &str,
    state_name: &str,
    state_def: &rhei_validator::StateDef,
    context: &str,
) -> MietteResult<()> {
    let Some(gate) = state_def.gate.as_ref().filter(|_| state_def.gating) else {
        return Ok(());
    };
    let problems = gate_entry_problems(root, task, task_id, state_name, gate);
    if problems.is_empty() {
        return Ok(());
    }
    Err(miette!(
        help = format!(
            "the state before the gate writes these files, under {}; \
             `rhei gate schema request` prints the request format",
            absolutize(root).display()
        ),
        "{context}\nGate '{state_name}' cannot show its brief:\n  - {}",
        problems.join("\n  - ")
    ))
}

/// How many required questions an author asked at `state_name`'s gate for
/// this ticket: none without a block, and none for a degraded brief.
/// §FS-rhei-gate-brief.8
fn gate_authored_required(
    root: &Path,
    task: Option<&rhei_core::ast::Task>,
    task_id: &str,
    state_name: &str,
    state_def: &rhei_validator::StateDef,
) -> usize {
    let Some(gate) = state_def.gate.as_ref().filter(|_| state_def.gating) else {
        return 0;
    };
    let required = |questions: &[rhei_core::gate::Question]| {
        questions.iter().filter(|question| question.required).count()
    };
    match (&gate.request, &gate.questions) {
        (Some(_), _) => {
            let ticket = GateTicket { task_id, task, state: state_name, prose: None };
            let relative = gate_files(gate, &ticket).request.unwrap_or_default();
            read_gate_request(root, gate, &relative).map_or(0, |request| required(&request.questions))
        }
        (None, Some(questions)) => required(questions),
        (None, None) => 0,
    }
}

/// Refuse leaving a gate with authored required questions by `rhei transition`:
/// it is answered, not transitioned. §FS-rhei-transition-cmd.3.5
fn ensure_gate_left_by_answer(
    root: &Path,
    task: &rhei_core::ast::Task,
    task_id: &str,
    from: &str,
    machine: &rhei_validator::StateMachine,
) -> MietteResult<()> {
    let Some(state_def) = machine.states.get(from) else {
        return Ok(());
    };
    let asked = gate_authored_required(root, Some(task), task_id, from, state_def);
    if asked == 0 {
        return Ok(());
    }
    let questions = if asked == 1 { "question" } else { "questions" };
    Err(miette!(
        help = format!(
            "read the brief with: {}\nanswer it with: {}",
            help_command_with("gate show", &format!("--task {task_id}")),
            help_command_with("gate answer", &format!("--task {task_id} --from {from} --answers <file|->"))
        ),
        "Task {task_id} waits at gate '{from}', which asks {asked} required {questions}."
    ))
}
