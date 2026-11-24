// The implicit `gate-answer` input: the ticket's latest gate answer, found
// without a declaration, for a state's templates, its program's environment,
// and the handoff section of its prompt.

// §FS-rhei-gate-brief.7.3

/// The ticket's latest answer file. §FS-rhei-gate-brief.7.3
struct LatestGateAnswer {
    /// Relative to the execution root.
    relative: String,
    /// `None` when the file does not read as an answer.
    document: Option<rhei_core::gate::GateAnswer>,
}

/// The answer file under `root` for `task_id` with the latest `answered_at`,
/// then the latest modification time. §FS-rhei-gate-brief.7.3
fn latest_gate_answer(root: &Path, task_id: &str) -> Option<LatestGateAnswer> {
    let entries = fs::read_dir(root.join("runtime").join("gates").join(task_id)).ok()?;
    let mut latest: Option<(String, std::time::SystemTime, String, Option<rhei_core::gate::GateAnswer>)> = None;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".answer.json") || !entry.path().is_file() {
            continue;
        }
        let document = fs::read_to_string(entry.path())
            .ok()
            .and_then(|text| serde_json::from_str::<rhei_core::gate::GateAnswer>(&text).ok());
        let at = document.as_ref().and_then(|answer| answer.answered_at.clone()).unwrap_or_default();
        let modified = entry.metadata().and_then(|meta| meta.modified()).unwrap_or(std::time::UNIX_EPOCH);
        if latest.as_ref().is_none_or(|(seen, seen_modified, ..)| (&at, modified) > (seen, *seen_modified)) {
            latest = Some((at, modified, name, document));
        }
    }
    latest.map(|(_, _, name, document)| LatestGateAnswer { relative: format!("runtime/gates/{task_id}/{name}"), document })
}

/// The implicit input of the state being rendered: `None` when the state
/// declares its own `gate-answer`, which wins. §FS-rhei-gate-brief.7.3
fn implicit_gate_answer(context: &RuntimeTemplateContext<'_>) -> Option<Option<LatestGateAnswer>> {
    let declared = context.machine.states.get(context.state_name).is_some_and(|def| {
        def.inputs.iter().any(|input| input.name == rhei_core::gate::GATE_ANSWER_INPUT)
    });
    (!declared).then(|| latest_gate_answer(context.workspace_root, &context.task.id.to_string()))
}

/// `{gate_answer}`, `{input.gate-answer.path}` and `{input.gate-answer.exists}`,
/// or `None` for any other variable and for a declared input's own variables.
/// An inner `None` leaves the variable as written. §FS-rhei-gate-brief.7.3
fn gate_answer_variable(variable: &str, context: &RuntimeTemplateContext<'_>) -> Option<Option<String>> {
    let path_variable = format!("input.{}.path", rhei_core::gate::GATE_ANSWER_INPUT);
    let exists_variable = format!("input.{}.exists", rhei_core::gate::GATE_ANSWER_INPUT);
    if variable != "gate_answer" && variable != path_variable && variable != exists_variable {
        return None;
    }
    let Some(latest) = implicit_gate_answer(context) else {
        // A declared input answers for itself; the shorthand is its path.
        return (variable == "gate_answer").then(|| resolve_runtime_template_variable(&path_variable, context));
    };
    Some(if variable == exists_variable {
        Some(latest.is_some().to_string())
    } else {
        latest.map(|found| prompt_path_shown(context, context.workspace_root, found.relative))
    })
}

/// The program environment of the implicit input: `EXISTS` always, `PATH`
/// when there is a file; nothing when the state declares its own.
/// §FS-rhei-gate-brief.7.3
fn gate_answer_environment(context: &RuntimeTemplateContext<'_>) -> Vec<(&'static str, String)> {
    let Some(latest) = implicit_gate_answer(context) else {
        return Vec::new();
    };
    let mut environment = vec![("RHEI_INPUT_GATE_ANSWER_EXISTS", latest.is_some().to_string())];
    if let Some(found) = latest {
        environment.push(("RHEI_INPUT_GATE_ANSWER_PATH", found.relative));
    }
    environment
}

/// The prompt section that lists the answer which moved the ticket into the
/// state it is in; empty otherwise. §FS-rhei-gate-brief.7.3.1
fn render_gate_answer_handoff(context: &RuntimeTemplateContext<'_>) -> MietteResult<String> {
    let Some(Some(latest)) = implicit_gate_answer(context) else {
        return Ok(String::new());
    };
    let Some(answer) = latest.document.as_ref() else {
        return Ok(String::new());
    };
    let (Some(gate), Some(to)) = (answer.state.as_deref(), answer.to.as_deref()) else {
        return Ok(String::new());
    };
    let entered_from = last_recorded_source_state_for_current(
        context.workspace_root,
        &context.task.id,
        context.state_name,
        context.machine,
    )?;
    if to != context.state_name || entered_from.as_deref() != Some(gate) {
        return Ok(String::new());
    }
    let absolute = context.workspace_root.join(&latest.relative);
    let shown = prompt_path_shown(context, context.workspace_root, latest.relative.clone());
    let visit = answer.visit.map(|visit| format!(" (visit {visit})")).unwrap_or_default();
    let mut section = format!(
        "\n## Gate Answer from {gate}\n\n\
         A person answered the `{gate}` gate{visit} and moved this task here. The answer is \
         recorded in `{shown}`: read the ruling from that file, not from anyone who relays it.\n"
    );
    // A file an exclusion keeps out of the prompt is named, never pasted. §FS-rhei-plan-language.3.13
    if prompt_source_allowed(context, &absolute) {
        let answers = serde_json::to_string_pretty(&answer.answers).unwrap_or_default();
        // As long a fence as the text needs, so an answer cannot close it. §FS-rhei-memory.4.5
        let longest = answers.split(|ch| ch != '`').map(str::len).max().unwrap_or(0);
        let fence = "`".repeat(longest.max(2) + 1);
        section.push_str(&format!("\nThe answers it records, as data:\n\n{fence}json\n{answers}\n{fence}\n"));
    }
    Ok(section)
}
