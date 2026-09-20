// The `rhei cost` command surface: what it was asked for, which records that
// selects, and how the answer is rendered as text and as JSON.
//
// Reading the accounting roots is `accounting_roots`, selecting within them is
// `accounting_selection`, and reading one stored record is `accounting_reading`.
// What is left here is the command: one entry point, one payload, and the
// printing.

// §AR-source-file-size.3 §FS-rhei-cost-accounting.8

fn cost_command(options: CostCommandOptions<'_>) -> MietteResult<()> {
    // §FS-rhei-cost-accounting.8: `rhei cost` inspects without changing plan.
    let input_buf = normalize_workspace_input(options.input);
    // §FS-rhei-cost-accounting.8.2: an unreadable `<TIME>` is refused before
    // any record is read, so nothing can report an empty window as an answer.
    let selection = CostSelection::resolve(options.run, options.since, options.until)?;
    let loaded = load_plan(&input_buf)?;
    // §FS-rhei-panta.6.5: an id the project does not hold is refused here, the
    // way every other `--rhei` refuses one, rather than selecting nothing.
    let scope = resolve_rhei_scope(&loaded, options.scope)?;
    if let Some(task_id) = options.task {
        refuse_task_outside_scope(&loaded, task_id, &scope)?;
    }
    let roots = accounting_roots(&loaded, &execution_workspace_root(&input_buf), &scope);
    let inspection = read_cost_inspection_over(&roots, &scope);
    let selected = selection.apply(inspection.scoped(), inspection.unreadable_root);
    let budget = budget_inspection(&input_buf);

    if options.json {
        let mut payload = cost_json_payload(&loaded.rhei, &inspection, &selection, &selected, options);
        payload["budget"] = budget;
        println!("{}", serde_json::to_string_pretty(&payload).expect("cost json serializes"));
        return Ok(());
    }

    print!("{}", budget_inspection_text(&budget));
    for error in &inspection.errors {
        eprintln!("warning: {error}");
    }
    if inspection.invocations.is_empty() {
        // §FS-rhei-cost-accounting.8: Empty accounting exits 0 with this text,
        // and names the roots it searched underneath it.
        println!("(no accounting records found)");
        print!("{}", roots_tail(&input_buf, &inspection.roots));
        return Ok(());
    }
    if selection.is_active() && selected.records.is_empty() {
        // §FS-rhei-cost-accounting.8.2: a selection that matched nothing is a
        // different answer from a workspace that holds nothing.
        println!("(no accounting records match the selection)");
        print!("{}", roots_tail(&input_buf, &inspection.roots));
        return Ok(());
    }

    if let Some(task_id) = options.task {
        if selection.is_active() {
            print_selection_lines(&selection, &selected);
        }
        print_task_cost(&loaded.rhei, &selected.records, task_id);
    } else {
        print_run_cost(&loaded.rhei, &selection, &selected, options.by);
    }
    Ok(())
}

/// Refuse a `--task` the reading's scope does not hold, before any record is
/// read.
///
/// The ticket's records sit in a root this reading never opens, so answering it
/// would print the same `Direct: none` as a ticket that genuinely cost nothing
/// — the empty-that-reads-as-zero the scope rule exists to remove. The error
/// names the scope and the rhei the ticket belongs to, because the reader's
/// next move is to widen to one or the other. An id **no rhei holds** is left
/// alone: it is unknown, and the reading says so on its own.
/// §FS-rhei-panta.6.5
fn refuse_task_outside_scope(
    loaded: &LoadedPlan,
    task_id: &str,
    scope: &RheiScope,
) -> MietteResult<()> {
    if task_in_rhei_scope(scope, task_id) {
        return Ok(());
    }
    let held = flatten_tasks(&loaded.rhei).into_iter().any(|task| task.id.to_string() == task_id);
    if !held {
        return Ok(());
    }
    let owner = task_id.split_once('.').map(|(head, _)| head).unwrap_or(task_id);
    Err(miette!(
        help = cost_task_scope_help(),
        "task '{}' is outside the accounting scope of this reading ({}); it belongs to rhei '{}'",
        task_id,
        scope_label(scope),
        owner
    ))
}

/// What `rhei cost` was asked for. §FS-rhei-cost-accounting.8
#[derive(Clone, Copy)]
struct CostCommandOptions<'a> {
    input: &'a Path,
    /// The rheis the reading is narrowed to: `--rhei`, or the rhei the
    /// positional pointed at. Empty is the whole project. §FS-rhei-panta.6.5
    scope: &'a [String],
    task: Option<&'a str>,
    json: bool,
    by: CostGroup,
    run: Option<&'a str>,
    since: Option<&'a str>,
    until: Option<&'a str>,
}

/// The `rhei.accounting.cost.v1` payload.
///
/// `selection` and `run_attribution` are on every payload, whatever flags were
/// given: adding keys is additive, and a caller must be able to see the
/// unattributed share without having thought to ask for a grouping.
// §FS-rhei-cost-accounting.8.4
fn cost_json_payload(
    rhei: &rhei_core::ast::Rhei,
    inspection: &CostInspection,
    selection: &CostSelection,
    selected: &CostSelectionResult<'_>,
    options: CostCommandOptions<'_>,
) -> serde_json::Value {
    serde_json::json!({
        "schema": "rhei.accounting.cost.v1",
        "selection": {
            "run": selection.run_label(),
            "since": selection.since_label(),
            "until": selection.until_label(),
            "invocation_count": selected.records.len() as u64,
            "undated_invocation_count": selected.undated,
        },
        // §FS-rhei-cost-accounting.8.4: one entry per root the reading covered,
        // on every reading, so a total over one root is told from one over nine
        // without parsing text.
        "roots": roots_json(&inspection.roots),
        "run_attribution": {
            "attributed_invocation_count": selected.attributed_count,
            "unattributed_invocation_count": selected.unattributed.len() as u64,
            "unattributed": selected.unattributed_summary(),
        },
        "summary": selected.summary(),
        "task": options.task.map(|task_id| task_cost_json(rhei, &selected.records, task_id)),
        "groups": grouped_cost_json(selected, options.by),
        "errors": inspection.errors,
    })
}

/// One task node's direct and subtree totals, over the selection.
///
/// The plan tree is a selection axis like the others and composes with them, so
/// a task's totals are drawn from what `--run` and the window left standing —
/// not from every record the workspace holds.
// §FS-rhei-cost-accounting.6.1 §FS-rhei-cost-accounting.8.2
fn task_cost_json(
    rhei: &rhei_core::ast::Rhei,
    records: &[ScopedRecord<'_>],
    task_id: &str,
) -> serde_json::Value {
    let title = flatten_tasks(rhei)
        .into_iter()
        .find(|task| task.id.to_string() == task_id)
        .map(|task| task.title.clone());
    serde_json::json!({
        // §FS-rhei-cost-accounting.8: JSON uses stable runtime schema names.
        "task_id": task_id,
        "title": title,
        "direct": summarize_records(direct_records(records, task_id)),
        "subtree": summarize_records(subtree_records(records, task_id)),
        // The records themselves as stored, and the elapsed time of any that
        // stored none. §FS-rhei-cost-accounting.3.4.1
        "invocations": subtree_records(records, task_id)
            .map(|held| published_invocation_json(held.record))
            .collect::<Vec<_>>(),
    })
}

/// The records charged to one node. §FS-rhei-cost-accounting.6
fn direct_records<'a, 'r>(
    records: &'a [ScopedRecord<'r>],
    task_id: &'a str,
) -> impl Iterator<Item = ScopedRecord<'r>> + 'a {
    records.iter().copied().filter(move |held| held.record.task_id == task_id)
}

/// The records charged to one node or to any descendant of it.
/// §FS-rhei-cost-accounting.6
fn subtree_records<'a, 'r>(
    records: &'a [ScopedRecord<'r>],
    task_id: &'a str,
) -> impl Iterator<Item = ScopedRecord<'r>> + 'a {
    records.iter().copied().filter(move |held| {
        held.record.task_id == task_id || is_descendant_id(&held.record.task_id, task_id)
    })
}

fn grouped_cost_json(
    selected: &CostSelectionResult<'_>,
    by: CostGroup,
) -> Vec<serde_json::Value> {
    grouped_records(selected, by)
        .into_iter()
        .map(|(key, records)| {
            serde_json::json!({
                "key": key.key,
                "unattributed": key.unattributed,
                "summary": selected.group_summary(by, key.unattributed, &records),
            })
        })
        .collect()
}

fn print_run_cost(
    rhei: &rhei_core::ast::Rhei,
    selection: &CostSelection,
    selected: &CostSelectionResult<'_>,
    by: CostGroup,
) {
    if let Some(summary) = selected.summary() {
        println!(
            "Cost {} | Total {} | In {} | Out {} | Coverage {:?} | Invocations {}",
            format_summary_cost(&summary),
            format_dimension_value(&summary.total),
            format_dimension_value(&summary.input_total),
            format_dimension_value(&summary.output_total),
            summary.coverage,
            summary.invocation_count
        );
    }
    // §FS-rhei-cost-accounting.8.4: the unselected reading prints exactly what
    // it printed before, so what the selection has to say is said only when one
    // was asked for. `--json` carries it either way.
    if selection.is_active() {
        print_selection_lines(selection, selected);
    }
    println!("\nBy {:?}:", by);
    for (key, records) in grouped_records(selected, by) {
        if let Some(summary) = selected.group_summary(by, key.unattributed, &records) {
            println!(
                "  {}: {} total={} in={} out={} coverage={:?}",
                key.key,
                format_summary_cost(&summary),
                format_dimension_value(&summary.total),
                format_dimension_value(&summary.input_total),
                format_dimension_value(&summary.output_total),
                summary.coverage
            );
        }
    }
    println!("\nHighest subtree nodes:");
    for (task_id, title, summary) in
        highest_subtree_nodes(rhei, &selected.records).into_iter().take(8)
    {
        println!("  {task_id} {title}: {}", format_summary_cost(&summary));
    }
}

/// What the selection was, and how much of it nothing could attribute — said
/// beside the total whatever the coverage turned out to be.
// §FS-rhei-cost-accounting.6.2 §FS-rhei-cost-accounting.8.2
fn print_selection_lines(selection: &CostSelection, selected: &CostSelectionResult<'_>) {
    let mut parts = Vec::new();
    if let Some(run) = selection.run_label() {
        parts.push(format!("run {run}"));
    }
    if let Some(since) = selection.since_label() {
        parts.push(format!("since {since}"));
    }
    if let Some(until) = selection.until_label() {
        parts.push(format!("until {until}"));
    }
    println!("Selection: {} | Invocations {}", parts.join(" | "), selected.records.len());
    println!(
        "Attribution: {} named a run, {} named none",
        selected.attributed_count,
        selected.unattributed.len()
    );
    if selected.undated > 0 {
        println!(
            "Undated: {} record(s) could not be placed in this window",
            selected.undated
        );
    }
}

fn print_task_cost(
    rhei: &rhei_core::ast::Rhei,
    records: &[ScopedRecord<'_>],
    task_id: &str,
) {
    let title = flatten_tasks(rhei)
        .into_iter()
        .find(|task| task.id.to_string() == task_id)
        .map(|task| task.title.clone())
        .unwrap_or_else(|| "(unknown task)".to_string());
    println!("Task {task_id}: {title}");
    let direct = summarize_records(direct_records(records, task_id));
    let subtree = summarize_records(subtree_records(records, task_id));
    println!("  Direct: {}", direct.as_ref().map(format_summary_cost).unwrap_or_else(|| "none".to_string()));
    println!("  Subtree: {}", subtree.as_ref().map(format_summary_cost).unwrap_or_else(|| "none".to_string()));
    println!("  Invocations:");
    for held in subtree_records(records, task_id) {
        let usage = usage_summary_from_record(held.record, held.books);
        println!(
            "    {} {} {} {}",
            held.record.invocation_id,
            held.record.agent,
            held.record.model.as_deref().unwrap_or("-"),
            format_usage_cost(&usage)
        );
        // Historical records have no array and retain their old one-line
        // rendering; current failures explain each distinct reason beneath
        // the contributing invocation. §FS-rhei-cost-accounting.8
        for diagnostic in &held.record.extraction_diagnostics {
            println!("      extractor-failed: {diagnostic}");
        }
    }
}

/// Partition the selection. Every selected record lands in exactly one group,
/// whatever its `run_id` says or does not say.
// §FS-rhei-cost-accounting.6.1
fn grouped_records<'a>(
    selected: &CostSelectionResult<'a>,
    by: CostGroup,
) -> Vec<(CostGroupKey, Vec<ScopedRecord<'a>>)> {
    let mut groups: BTreeMap<String, (bool, Vec<ScopedRecord<'a>>)> = BTreeMap::new();
    for held in selected.records.iter().copied() {
        let key = cost_group_key(held.record, by);
        let entry = groups.entry(key.key).or_insert_with(|| (key.unattributed, Vec::new()));
        entry.1.push(held);
    }
    groups
        .into_iter()
        .map(|(key, (unattributed, records))| (CostGroupKey { key, unattributed }, records))
        .collect()
}

fn highest_subtree_nodes(
    rhei: &rhei_core::ast::Rhei,
    records: &[ScopedRecord<'_>],
) -> Vec<(String, String, rhei_tui::AccountingRunSummary)> {
    let mut rows = Vec::new();
    for task in flatten_tasks(rhei) {
        let task_id = task.id.to_string();
        // §FS-rhei-cost-accounting.6: subtree(node)=direct+descendants.
        if let Some(summary) = summarize_records(subtree_records(records, &task_id)) {
            rows.push((task_id, task.title.clone(), summary));
        }
    }
    rows.sort_by(|a, b| summary_sort_cost(&b.2).cmp(&summary_sort_cost(&a.2)));
    rows
}
