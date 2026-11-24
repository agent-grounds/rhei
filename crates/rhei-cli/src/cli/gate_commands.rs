// `rhei gate show`, `list` and `schema`: read-only, no lock, no runtime write.
// The brief itself is built in the gate view; these print it for a script or
// a person, and print the schemas a caller holds the documents to.

// §FS-rhei-gate-cmd

/// Print one ticket's brief. §FS-rhei-gate-cmd.2
fn gate_show_command(
    input: &Path,
    rhei_scope: &[String],
    state_machine_path: Option<&Path>,
    task_id_str: &str,
    as_json: bool,
) -> MietteResult<()> {
    let input_buf = normalize_workspace_input(input);
    let input = input_buf.as_path();
    let loaded = load_plan_leniently(input)?;
    for skipped in &loaded.unloadable {
        eprintln!("warning: {skipped}");
    }
    let scope = resolve_rhei_scope(&loaded, rhei_scope)?;
    let task_id = resolve_cli_task_id(&loaded, task_id_str, &scope)?;
    let resolved = resolve_state_machines_for_loaded_plan(input, &loaded, state_machine_path)?;
    let machines = resolved.validator_set();
    let task = find_task_by_id_str(&loaded.rhei.tasks, &task_id)
        .ok_or_else(|| miette!(help = task_id_help(), "task '{}' not found in the plan", task_id))?;
    let machine = machines.for_task(&task.id);
    let source = GateViewSource { loaded: &loaded, input, machine, task };
    let Some(view) = gate_view(&source)? else {
        return Err(miette!(
            help = format!("list the tickets that wait at a gate with: {}", help_command("gate list")),
            "Task {} is in state '{}', which is not a gate (`gating: true`)",
            task_id,
            task.state.as_str()
        ));
    };
    if as_json {
        print_gate_json(&GateShowDocument::of(&view), &task_id)?;
    } else {
        print!("{}", render_gate_brief(&view));
    }
    Ok(())
}

fn print_gate_json(document: &impl serde::Serialize, what: &str) -> MietteResult<()> {
    let rendered = serde_json::to_string_pretty(document)
        // §FS-rhei-errors.6
        .map_err(|err| miette!(help = INTERNAL_DIAGNOSTIC_HELP, "failed to render {what} as JSON: {err}"))?;
    println!("{rendered}");
    Ok(())
}

/// The brief for a person, ending with the command that answers it.
/// §FS-rhei-gate-cmd.2.2
fn render_gate_brief(view: &GateView) -> String {
    use std::fmt::Write as _;
    let brief = &view.brief;
    let mut out = String::new();
    let _ = writeln!(out, "{}: {}", view.ticket, brief.title);
    let since = view.since.as_deref().map(|at| format!(", since {at}")).unwrap_or_default();
    let _ = writeln!(out, "Gate: {} (visit {}{since})", view.state, view.visit);
    if let Some(summary) = &brief.summary {
        let _ = writeln!(out, "\n{}", summary.trim_end());
    }
    let file_line = |file: &GateFileView| match &file.problem {
        None => format!("{} ({})", file.path, file.absolute),
        Some(problem) => format!("{} ({}) - UNREADABLE: {problem}", file.path, file.absolute),
    };
    if let Some(display) = &view.display {
        let _ = writeln!(out, "\nRead: {}", file_line(display));
    }
    for attachment in &view.attachments {
        let _ = writeln!(out, "Attached: {}", file_line(attachment));
    }
    let _ = writeln!(out, "\nQuestions:");
    for question in &brief.questions {
        let required = if question.required { "required" } else { "optional" };
        let _ = writeln!(out, "  {} ({}, {required}): {}", question.id, question.kind.as_str(), question.prompt);
        if let Some(help) = &question.help {
            let _ = writeln!(out, "      {help}");
        }
        for option in question.options.iter().flatten() {
            let label = option.label.as_deref().unwrap_or(&option.value);
            let described = option.description.as_deref().map(|text| format!(" - {text}")).unwrap_or_default();
            if label == option.value {
                let _ = writeln!(out, "      - {}{described}", option.value);
            } else {
                let _ = writeln!(out, "      - {} ({label}){described}", option.value);
            }
        }
    }
    let _ = writeln!(out, "\nDestinations:");
    for destination in &view.destinations {
        let mut notes = Vec::new();
        if destination.terminal {
            notes.push("final".to_string());
        }
        if let Some(condition) = &destination.condition {
            notes.push(format!("when {condition}"));
        }
        let notes = if notes.is_empty() { String::new() } else { format!(" [{}]", notes.join("; ")) };
        let described = destination.description.as_deref().map(|text| format!(" - {text}")).unwrap_or_default();
        let _ = writeln!(out, "  {}{notes}{described}", destination.state);
    }
    if !brief.routes.is_empty() {
        let _ = writeln!(out, "\nRoutes (first match wins):");
        for route in &brief.routes {
            let when = if route.when.is_empty() {
                "always".to_string()
            } else {
                route.when.iter().map(|(id, value)| format!("{id} = {value}")).collect::<Vec<_>>().join(", ")
            };
            let _ = writeln!(out, "  {} when {when}", route.to);
        }
    }
    if !brief.problems.is_empty() {
        let _ = writeln!(out, "\nThe brief degraded to the destination and a reason:");
        for problem in &brief.problems {
            let _ = writeln!(out, "  - {problem}");
        }
    }
    let _ = writeln!(out, "\nrhei gate answer {} --from {} --answers <file|->", view.ticket, view.state);
    out
}

/// List every ticket under the root that waits at a gate. §FS-rhei-gate-cmd.4
fn gate_list_command(
    input: &Path,
    rhei_scope: &[String],
    state_machine_path: Option<&Path>,
    as_json: bool,
) -> MietteResult<()> {
    let input_buf = normalize_workspace_input(input);
    let input = input_buf.as_path();
    let loaded = load_plan_leniently(input)?;
    for skipped in &loaded.unloadable {
        eprintln!("warning: {skipped}");
    }
    let scope = resolve_rhei_scope(&loaded, rhei_scope)?;
    let resolved = resolve_state_machines_for_loaded_plan(input, &loaded, state_machine_path)?;
    let machines = resolved.validator_set();
    let mut entries = Vec::new();
    for task in flatten_tasks(&loaded.rhei) {
        if let Some(scope) = &scope {
            if !task.id.to_string().split_once('.').is_some_and(|(rhei, _)| scope.contains(rhei)) {
                continue;
            }
        }
        let machine = machines.for_task(&task.id);
        let source = GateViewSource { loaded: &loaded, input, machine, task };
        if let Some(view) = gate_view(&source)? {
            let holds = gate_holds(&loaded.rhei, &machines, task);
            entries.push(GateListEntry::of(view, holds));
        }
    }
    // Oldest first, unknown last; the sort is stable, so ties keep plan order.
    entries.sort_by(|a, b| match (&a.since, &b.since) {
        (Some(a), Some(b)) => a.cmp(b),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    if as_json {
        let document = GateListDocument {
            version: rhei_core::gate::GATE_DOCUMENT_VERSION,
            root: absolutize(input).display().to_string(),
            gates: entries,
        };
        return print_gate_json(&document, "the gate list");
    }
    if entries.is_empty() {
        println!("No ticket waits at a gate.");
        return Ok(());
    }
    for entry in &entries {
        println!(
            "{}  {}  {}  required {}  holds {}",
            entry.ticket, entry.state, entry.title, entry.required, entry.holds
        );
    }
    Ok(())
}

/// The gate documents' schemas, by the short name `rhei gate schema` takes.
/// §FS-rhei-gate-brief.9
const GATE_SCHEMAS: [(&str, &str); 5] = [
    ("request", "rhei.gate.request.v1"),
    ("answer", "rhei.gate.answer.v1"),
    ("show", "rhei.gate.show.v1"),
    ("result", "rhei.gate.result.v1"),
    ("list", "rhei.gate.list.v1"),
];

fn gate_schema_value(schema_id: &str) -> MietteResult<serde_json::Value> {
    let text = PUBLISHED_ACCOUNTING_SCHEMAS
        .iter()
        .find(|(id, _)| *id == schema_id)
        .map(|(_, text)| *text)
        .ok_or_else(|| miette!(help = INTERNAL_DIAGNOSTIC_HELP, "schema {schema_id} is not published"))?;
    serde_json::from_str(text)
        .map_err(|err| miette!(help = INTERNAL_DIAGNOSTIC_HELP, "schema {schema_id} is not JSON: {err}"))
}

/// `rhei gate schema` with no name, its keys in the order the spec names
/// them: a struct, since a JSON map here sorts its keys. §FS-rhei-gate-cmd.5
#[derive(serde::Serialize)]
struct GateSchemaDocument {
    version: u64,
    schemas: GateSchemaSet,
}

#[derive(serde::Serialize)]
struct GateSchemaSet {
    request: serde_json::Value,
    answer: serde_json::Value,
    show: serde_json::Value,
    result: serde_json::Value,
    list: serde_json::Value,
}

/// Print one gate document's schema as published, or all five. §FS-rhei-gate-cmd.5
fn gate_schema_command(name: Option<&str>) -> MietteResult<()> {
    let Some(name) = name else {
        let [request, answer, show, result, list] = GATE_SCHEMAS.map(|(_, schema_id)| gate_schema_value(schema_id));
        let schemas = GateSchemaSet { request: request?, answer: answer?, show: show?, result: result?, list: list? };
        let document = GateSchemaDocument { version: rhei_core::gate::GATE_DOCUMENT_VERSION, schemas };
        return print_gate_json(&document, "the gate schemas");
    };
    if GATE_SCHEMAS.iter().all(|(short, _)| *short != name) {
        let names: Vec<&str> = GATE_SCHEMAS.iter().map(|(short, _)| *short).collect();
        return Err(miette!(
            help = format!("name one of: {}", names.join(", ")),
            "unknown gate schema '{name}'"
        ));
    }
    // The published file's bytes, as `rhei schema <id>` prints them. §FS-rhei-cost-accounting.8.1
    accounting_schema_command(GATE_SCHEMAS.iter().find(|(short, _)| *short == name).map(|(_, id)| *id))
}
