/// `rhei summary`: a read-only Markdown account of a run, compact enough to
/// paste into a pull request body — a lead line naming the workflow, one
/// numbered entry per recorded agent invocation, and the aggregate token
/// accounting. §FS-rhei-summary
///
/// It loads the plan and reads the accounting roots its scope selects
/// (§FS-rhei-panta.6.5), writes no file, spawns nothing, and estimates nothing:
/// a fact that was not recorded is omitted rather than guessed.
/// §FS-rhei-summary.1
#[derive(Args, Debug)]
struct SummaryOptions {
    /// Path to the markdown plan file (.rhei.md) or workspace directory
    #[arg(value_name = "RHEI_PLAN_OR_WORKSPACE", add = ArgValueCompleter::new(complete_rhei_plan_path))]
    input: Option<PathBuf>,
    /// Narrow to the named rhei (repeatable; one id per flag). A rhei id is its
    /// file stem or directory name; default is the whole project
    #[arg(long = "rhei", value_name = "RHEI_ID", add = ArgValueCompleter::new(complete_rhei_id))]
    rhei: Vec<String>,
    /// Wrap the summary in a collapsed <details> block
    #[arg(long)]
    details: bool,
    // The pair is one read-only alternate-book operation.
    // §FS-rhei-summary.1 §FS-rhei-cost-accounting.5.1
    /// Select one exact completed run for an alternate-price reading; requires --prices
    #[arg(long, value_name = "ID", requires = "prices")]
    run: Option<String>,
    /// Reprice that run with this local v1 book; requires --run and writes nothing
    #[arg(long, value_name = "BOOK", requires = "run")]
    prices: Option<PathBuf>,
}

fn summary_command(
    input: &Path,
    scope: &[String],
    state_machine: Option<&Path>,
    options: &SummaryOptions,
) -> MietteResult<()> {
    let alternate = match (options.run.as_deref(), options.prices.as_deref()) {
        (Some(run_id), Some(path)) => Some((run_id, load_price_book(path)?)),
        (None, None) => None,
        _ => {
            return Err(miette!(
                help = "pass both `--run <ID>` and `--prices <BOOK>`, or neither",
                "--run and --prices are one paired summary operation"
            ));
        }
    };
    let input_buf = normalize_workspace_input(input);
    let loaded = load_plan(&input_buf)?;
    // §FS-rhei-panta.6.5: scope is one thing for both reading commands, so
    // `--rhei` is refused and resolved here exactly as `rhei cost` does it.
    let scope = resolve_rhei_scope(&loaded, scope)?;
    let resolved = resolve_state_machine_for_loaded_plan(&input_buf, &loaded, state_machine)?;
    let run_root = execution_workspace_root(&input_buf);
    let roots = accounting_roots(&loaded, &run_root, &scope);
    let inspection = read_cost_inspection_over(&roots, &scope);
    let inspection = match alternate.as_ref() {
        Some((run_id, book)) => select_completed_run_for_summary(
            &loaded,
            &run_root,
            &scope,
            &roots,
            inspection,
            run_id,
            book,
        )?,
        None => inspection,
    };
    // A record that would not parse names a local file, so the warning goes to
    // stderr and stdout stays publishable verbatim. §FS-rhei-summary.4
    for error in &inspection.errors {
        eprintln!("warning: {error}");
    }
    print!(
        "{}",
        render_summary_reading(
            &loaded.rhei,
            &resolved.machine,
            &inspection,
            &scope,
            options.details,
            alternate.as_ref().map(|(run_id, _)| *run_id),
            alternate.as_ref().map(|(_, book)| book),
        )
    );
    Ok(())
}

/// The whole document, in the three parts of §FS-rhei-summary.2, optionally
/// wrapped in the collapsed block of §FS-rhei-summary.3.
#[cfg(test)]
fn render_summary(
    rhei: &rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
    inspection: &CostInspection,
    scope: &RheiScope,
    details: bool,
) -> String {
    render_summary_reading(rhei, machine, inspection, scope, details, None, None)
}

/// Render either the ordinary workspace reading or one exact alternate-book
/// reading without allowing their output rules to diverge.
/// §FS-rhei-summary.2 §FS-rhei-summary.3
fn render_summary_reading(
    rhei: &rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
    inspection: &CostInspection,
    scope: &RheiScope,
    details: bool,
    selected_run: Option<&str>,
    alternate_book: Option<&PriceBook>,
) -> String {
    let tail = summary_lead_tail(rhei, machine, inspection, scope, selected_run);
    let name = &machine.name;
    let mut out = String::new();
    if details {
        // The blank line after `</summary>` is what makes GitHub render the
        // Markdown inside the block. §FS-rhei-summary.3
        out.push_str("<details>\n");
        out.push_str(&format!("<summary>AI workflow: `{name}`, {tail}</summary>\n\n"));
    } else {
        out.push_str(&format!("`{name}` workflow: {tail}\n\n"));
    }
    let steps = summary_steps(inspection);
    if !steps.is_empty() {
        out.push_str(&steps);
        out.push('\n');
    }
    out.push_str(&summary_accounting_reading(inspection, alternate_book));
    if details {
        out.push('\n');
        out.push_str("</details>\n");
    }
    out
}

/// The lead line after the workflow name: invocation count, distinct models,
/// and the task tally. §FS-rhei-summary.2.1
fn summary_lead_tail(
    rhei: &rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
    inspection: &CostInspection,
    scope: &RheiScope,
    selected_run: Option<&str>,
) -> String {
    let invocations = inspection.invocations.len();
    let models: BTreeSet<&str> = inspection
        .invocations
        .iter()
        .filter_map(|held| held.record.model.as_deref())
        .collect();
    let run = selected_run.map(|id| format!("run `{id}`; ")).unwrap_or_default();
    let tally = if selected_run.is_some() {
        selected_run_task_tally(machine, inspection)
    } else {
        summary_task_tally(rhei, machine, scope)
    };
    format!(
        "{run}{invocations} agent invocation{} across {} model{}; {tally}.",
        plural_s(invocations),
        models.len(),
        plural_s(models.len()),
    )
}

/// A completed-run tally comes from the last recorded state of each represented
/// task, never from the plan's current state.
/// §FS-rhei-summary.2.1
fn selected_run_task_tally(
    machine: &rhei_validator::StateMachine,
    inspection: &CostInspection,
) -> String {
    let mut tasks: BTreeMap<&str, &str> = BTreeMap::new();
    for held in &inspection.invocations {
        tasks.insert(held.record.task_id.as_str(), held.record.state.as_str());
    }
    tally_states(machine, tasks.values().copied())
}

/// Tasks per terminal state in machine declaration order, with the
/// in-progress remainder appended so a mid-run summary says it is one.
///
/// The tally counts the tasks of the invocation's own scope: one sentence must
/// not describe two, and a member's invocations beside the whole project's task
/// counts reads as a summary of neither. §FS-rhei-summary.2.1
fn summary_task_tally(
    rhei: &rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
    scope: &RheiScope,
) -> String {
    let tasks: Vec<&rhei_core::ast::Task> = narrow_to_rhei_scope(flatten_tasks(rhei), scope);
    tally_states(machine, tasks.iter().map(|task| task.state.as_str()))
}

/// Count recorded or current task states in machine declaration order.
/// §FS-rhei-summary.2.1
fn tally_states<'a>(
    machine: &rhei_validator::StateMachine,
    states: impl IntoIterator<Item = &'a str>,
) -> String {
    let states: Vec<&str> = states.into_iter().collect();
    let mut parts: Vec<(usize, &str)> = Vec::new();
    for (state, def) in &machine.states {
        if !def.terminal {
            continue;
        }
        let count = states.iter().filter(|task_state| **task_state == state.as_str()).count();
        if count > 0 {
            parts.push((count, state.as_str()));
        }
    }
    let terminal: HashSet<&str> = machine
        .states
        .iter()
        .filter(|(_, def)| def.terminal)
        .map(|(state, _)| state.as_str())
        .collect();
    let in_progress = states.iter().filter(|state| !terminal.contains(**state)).count();
    if in_progress > 0 {
        parts.push((in_progress, "in progress"));
    }
    if parts.is_empty() {
        return "no tasks".to_string();
    }
    let mut tally = String::new();
    for (index, (count, label)) in parts.iter().enumerate() {
        if index == 0 {
            tally.push_str(&format!("{count} task{} {label}", plural_s(*count)));
        } else {
            tally.push_str(&format!(", {count} {label}"));
        }
    }
    tally
}

/// One numbered entry per invocation record. The reading sorts the whole union
/// of its roots, so the numbering is the `started_at` order the spec asks for
/// however many roots the scope selected. §FS-rhei-summary.2.2
fn summary_steps(inspection: &CostInspection) -> String {
    let mut per_task: BTreeMap<&str, usize> = BTreeMap::new();
    for held in &inspection.invocations {
        *per_task.entry(held.record.task_id.as_str()).or_default() += 1;
    }
    let mut out = String::new();
    for (index, held) in inspection.invocations.iter().enumerate() {
        let record = &held.record;
        // A repeated visit and a task with several records both need the visit
        // spelled out; a one-shot step stays clean. §FS-rhei-summary.2.2
        let sibling_records = per_task.get(record.task_id.as_str()).copied().unwrap_or(0);
        let repeated = record.visit > 1 || sibling_records > 1;
        let visit = if repeated { format!(" (visit {})", record.visit) } else { String::new() };
        out.push_str(&format!(
            "{}. `{}` {}{visit} — {}",
            index + 1,
            record.task_id,
            record.state,
            summary_step_actor(record)
        ));
        if let Some(duration) = summary_step_duration(record) {
            out.push_str(&format!(" — {duration}"));
        }
        if let Some(tokens) = summary_step_tokens(record, inspection.books_of(held)) {
            out.push_str(&format!(" — {tokens}"));
        }
        out.push('\n');
    }
    out
}

/// `<agent>, <provider>/<model>`, dropping whatever the record did not carry.
fn summary_step_actor(record: &AccountingInvocationRecord) -> String {
    match (record.provider.as_deref(), record.model.as_deref()) {
        (Some(provider), Some(model)) => format!("{}, {provider}/{model}", record.agent),
        (None, Some(model)) => format!("{}, {model}", record.agent),
        _ => record.agent.clone(),
    }
}

/// `ended_at - started_at`, humanized; `None` when either timestamp is
/// missing or unparseable, because a duration is not worth guessing. The
/// arithmetic is the one every reading of this archive shares.
/// §FS-rhei-summary.2.2 §FS-rhei-cost-accounting.3.4.1
fn summary_step_duration(record: &AccountingInvocationRecord) -> Option<String> {
    invocation_elapsed_ms(record).map(format_duration_short)
}

/// Humanized `in`/`out` counts, and only the sides the record measured.
/// §FS-rhei-summary.2.2
fn summary_step_tokens(
    record: &AccountingInvocationRecord,
    books: &ReachablePriceBooks,
) -> Option<String> {
    // A record written before the convention existed is read under the one its
    // own `agent` implies here too. §FS-rhei-cost-accounting.5.2
    let usage = usage_summary_from_record(record, books);
    let mut parts = Vec::new();
    if usage.input_total.value.is_some() {
        parts.push(format!("{} in", format_dimension_value(&usage.input_total)));
    }
    if usage.output_total.value.is_some() {
        parts.push(format!("{} out", format_dimension_value(&usage.output_total)));
    }
    (!parts.is_empty()).then(|| parts.join(" / "))
}

/// The aggregate strip, in the shape the per-run report uses; one line
/// instead when no record carried a measured total, because an empty table
/// reads like a zero. §FS-rhei-summary.2.3
#[cfg(test)]
fn summary_accounting(inspection: &CostInspection) -> String {
    summary_accounting_reading(inspection, None)
}

fn summary_accounting_reading(
    inspection: &CostInspection,
    alternate_book: Option<&PriceBook>,
) -> String {
    let Some(summary) = inspection.summary.as_ref().filter(|it| it.total.value.is_some()) else {
        let Some(book) = alternate_book else {
            return "Token accounting was not measured for this run.\n".to_string();
        };
        return format!(
            "| Accounting | Value |\n| --- | ---: |\n\
             | price book | {} |\n| currency | {} |\n| pricing | not-applicable |\n\n\
             Token accounting was not measured for this run.\n",
            md_cell(&book.price_book_id),
            md_cell(&book.currency),
        );
    };
    let mut out = String::new();
    out.push_str("| Accounting | Value |\n| --- | ---: |\n");
    if let Some(book) = alternate_book {
        // Only validated provenance, never its caller-owned path, enters the
        // publishable Markdown. §FS-rhei-summary.2.3 §FS-rhei-summary.4
        out.push_str(&format!("| price book | {} |\n", md_cell(&book.price_book_id)));
        out.push_str(&format!("| currency | {} |\n", md_cell(&book.currency)));
        out.push_str(&format!(
            "| pricing | {} |\n",
            alternate_pricing_label(summary.pricing_status)
        ));
        match summary.pricing_status {
            rhei_tui::PricingStatus::Priced => out.push_str(&format!(
                "| cost | {} |\n",
                md_cell(&format_summary_cost(summary))
            )),
            rhei_tui::PricingStatus::PartialPrice => out.push_str(&format!(
                "| priced cost (lower bound) | {} |\n",
                md_cell(&format_summary_cost(summary))
            )),
            rhei_tui::PricingStatus::Unpriced | rhei_tui::PricingStatus::NotApplicable => {}
        }
    } else if summary.cost_micro.or(summary.priced_cost_micro).is_some() {
        // Ordinary summary preserves its stored-pricing behavior.
        // §FS-rhei-summary.4
        out.push_str(&format!("| cost | {} |\n", md_cell(&format_summary_cost(summary))));
    }
    for (label, value) in AccountingTokenPresentation::new(summary).rows() {
        out.push_str(&format!("| {label} | {value} |\n"));
    }
    out.push_str(&format!("| coverage | {:?} |\n", summary.coverage));
    out
}

/// Public spelling of alternate-book rate coverage.
/// §FS-rhei-summary.2.3 §FS-rhei-cost-accounting.6.2
fn alternate_pricing_label(status: rhei_tui::PricingStatus) -> &'static str {
    match status {
        rhei_tui::PricingStatus::Priced => "priced",
        rhei_tui::PricingStatus::PartialPrice => "partial-price",
        rhei_tui::PricingStatus::Unpriced => "unpriced",
        rhei_tui::PricingStatus::NotApplicable => "not-applicable",
    }
}

fn plural_s(count: usize) -> &'static str {
    if count == 1 {
        ""
    } else {
        "s"
    }
}
