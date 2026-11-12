/// `rhei summary`: a read-only Markdown account of a run, compact enough to
/// paste into a pull request body — a lead line naming the workflow, one
/// numbered entry per step of the run, and the aggregate token accounting.
/// §FS-rhei-summary
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
    // Each refuses the other and the `--run`/`--prices` pairing, as clap's usage
    // error naming both flags. §FS-rhei-summary.1
    /// Print the checkpoint document of the records, spawns and task states this
    /// summary covers, instead of Markdown
    #[arg(long, conflicts_with_all = ["checkpoint", "details", "run", "prices"])]
    print_checkpoint: bool,
    /// Print the summary of exactly the set a checkpoint document names
    /// (`-` reads it from stdin), whatever has been recorded since
    #[arg(long, value_name = "FILE|-", conflicts_with_all = ["run", "prices"])]
    checkpoint: Option<String>,
}

/// Resolve the machine that gives a summary's task tally its meaning.
///
/// A Panta may contain self-declaring rheis, so its merged task graph is not
/// necessarily governed by the manifest's default machine. A summary can
/// name one workflow only; when its task scope resolves to several distinct
/// machines, refuse to combine their terminal definitions and ask the caller
/// to narrow the scope instead. §FS-rhei-summary.2.1 §DA-per-rhei-state-machines
fn resolve_summary_machine(
    input: &Path,
    loaded: &LoadedPlan,
    state_machine_path: Option<&Path>,
    scope: &RheiScope,
) -> MietteResult<ResolvedStateMachine> {
    // An explicit override retains the existing whole-scope behavior. A
    // non-Panta plan has one effective machine by construction.
    if state_machine_path.is_some() || !loaded.is_panta_project() {
        return resolve_state_machine_for_loaded_plan(input, state_machine_path);
    }

    let machines = resolve_state_machines_for_loaded_plan(input, loaded, None)?;
    let tasks = narrow_to_rhei_scope(flatten_tasks(&loaded.rhei), scope);
    let mut candidates: BTreeMap<String, ResolvedStateMachine> = BTreeMap::new();

    for task in tasks {
        let task_id = task.id.to_string();
        let resolved = machines.for_task_str(&task_id);
        candidates.entry(resolved.machine.fingerprint()).or_insert_with(|| resolved.clone());
    }

    // An empty member has no task from which to infer a machine, but a
    // `--rhei` selection still has an authored declaration to honor. The
    // project-wide empty summary falls back to the project default.
    if candidates.is_empty() {
        let selected = scope
            .as_ref()
            .map(|ids| ids.iter().cloned().collect::<Vec<_>>())
            .unwrap_or_else(|| loaded.rhei_ids.clone());
        for rhei_id in selected {
            let resolved = machines.per_rhei.get(&rhei_id).unwrap_or(&machines.default);
            candidates.entry(resolved.machine.fingerprint()).or_insert_with(|| resolved.clone());
        }
    }

    if candidates.is_empty() {
        candidates.insert(machines.default.machine.fingerprint(), machines.default);
    }

    match candidates.len() {
        1 => Ok(candidates.into_values().next().expect("one summary machine")),
        _ => {
            let names = candidates
                .values()
                .map(|resolved| resolved.machine.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            Err(miette!(
                help = "narrow the summary with `--rhei <ID>` so it covers one state machine",
                "summary scope spans multiple state machines: {names}"
            ))
        }
    }
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
    let resolved = resolve_summary_machine(&input_buf, &loaded, state_machine, &scope)?;
    let run_root = execution_workspace_root(&input_buf);
    let roots = accounting_roots(&loaded, &run_root, &scope);
    let inspection = read_cost_inspection_beside(&roots, &scope, Some(&run_root));
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
    if options.print_checkpoint {
        print!("{}", SummaryCheckpoint::of(&inspection, &loaded.rhei, &scope).canonical());
        return Ok(());
    }
    // A replay reads its spawns again wherever the live reading found them.
    // §FS-rhei-summary.6.1
    let (inspection, checkpoint) = match options.checkpoint.as_deref() {
        Some(given) => {
            let checkpoint = read_summary_checkpoint(given)?;
            let mut spawn_roots: Vec<PathBuf> = roots.iter().map(|root| root.path.clone()).collect();
            spawn_roots.push(run_root.join(ACCOUNTING_DIR));
            (replay_summary_checkpoint(inspection, &checkpoint, given, &spawn_roots)?, checkpoint)
        }
        None => {
            let checkpoint = SummaryCheckpoint::of(&inspection, &loaded.rhei, &scope);
            (inspection, checkpoint)
        }
    };
    let template = summary_template_provenance(resolved.path.as_deref());
    let reading = SummaryReading {
        selected_run: alternate.as_ref().map(|(run_id, _)| *run_id),
        alternate_book: alternate.as_ref().map(|(_, book)| book),
        recorded_tasks: options.checkpoint.is_some().then_some(checkpoint.tasks.as_slice()),
        provenance: Some(summary_provenance_line(&inspection, &checkpoint.id(), &template)),
    };
    print!(
        "{}",
        render_summary_reading(
            &loaded.rhei,
            &resolved.machine,
            &inspection,
            &scope,
            options.details,
            &reading
        )
    );
    Ok(())
}

/// What one reading adds to the workspace's: the exact run and its book, the
/// task states a checkpoint recorded, and the provenance line.
/// §FS-rhei-summary.1 §FS-rhei-summary.2.4
#[derive(Default)]
struct SummaryReading<'a> {
    selected_run: Option<&'a str>,
    alternate_book: Option<&'a PriceBook>,
    recorded_tasks: Option<&'a [CheckpointTask]>,
    provenance: Option<String>,
}

/// The whole document, in the four parts of §FS-rhei-summary.2, optionally
/// wrapped in the collapsed block of §FS-rhei-summary.3.
#[cfg(test)]
fn render_summary(
    rhei: &rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
    inspection: &CostInspection,
    scope: &RheiScope,
    details: bool,
) -> String {
    render_summary_reading(rhei, machine, inspection, scope, details, &SummaryReading::default())
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
    reading: &SummaryReading,
) -> String {
    let tail = summary_lead_tail(rhei, machine, inspection, scope, reading);
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
    out.push_str(&summary_accounting_reading(inspection, reading.alternate_book));
    // Last, so the accounting keeps its number; inside the block before it
    // closes. §FS-rhei-summary.2.4 §FS-rhei-summary.3
    if let Some(provenance) = &reading.provenance {
        out.push('\n');
        out.push_str(provenance);
        out.push('\n');
    } else if details {
        out.push('\n');
    }
    if details {
        out.push_str("</details>\n");
    }
    out
}

/// The lead line after the workflow name: invocation count, distinct models,
/// and the task tally. The count is the known agent invocations, records and
/// unrecorded spawns alike, and the gap follows the model count, which still
/// counts the records' models. §FS-rhei-summary.2.1 §FS-rhei-cost-accounting.6.2.1
fn summary_lead_tail(
    rhei: &rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
    inspection: &CostInspection,
    scope: &RheiScope,
    reading: &SummaryReading,
) -> String {
    let selected_run = reading.selected_run;
    let unrecorded = inspection.unrecorded.len();
    let invocations = inspection.invocations.len() + unrecorded;
    let gap = if unrecorded > 0 {
        format!(", {unrecorded} of them with no accounting record")
    } else {
        String::new()
    };
    let models: BTreeSet<&str> = inspection
        .invocations
        .iter()
        .filter_map(|held| held.record.model.as_deref())
        .collect();
    let run = selected_run.map(|id| format!("run `{id}`; ")).unwrap_or_default();
    // A replay counts the states its checkpoint recorded. §FS-rhei-summary.1
    let tally = if selected_run.is_some() {
        selected_run_task_tally(machine, inspection)
    } else if let Some(tasks) = reading.recorded_tasks {
        tally_states(machine, tasks.iter().map(|task| task.state.as_str()))
    } else {
        summary_task_tally(rhei, machine, scope)
    };
    format!(
        "{run}{invocations} agent invocation{} across {} model{}{gap}; {tally}.",
        plural_s(invocations),
        models.len(),
        plural_s(models.len()),
    )
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
        return "Token accounting was not measured for this run.\n".to_string();
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
    // The gap is named after the word it demoted. §FS-rhei-summary.2.3
    out.push_str(&format!("| coverage | {} |\n", coverage_label(summary)));
    out
}

/// Public spelling of alternate-book rate coverage.
/// §FS-rhei-summary.2.3 §FS-rhei-cost-accounting.6.2
fn alternate_pricing_label(status: rhei_tui::PricingStatus) -> &'static str {
    match status {
        rhei_tui::PricingStatus::Priced => "priced",
        rhei_tui::PricingStatus::PartialPrice => "partial-price",
        rhei_tui::PricingStatus::Unpriced => "unpriced",
        rhei_tui::PricingStatus::NotApplicable => "unpriced",
    }
}

fn plural_s(count: usize) -> &'static str {
    if count == 1 {
        ""
    } else {
        "s"
    }
}
