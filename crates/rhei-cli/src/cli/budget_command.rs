// `rhei budget init`, `show`, and `adjust`, over the invocation dimension only.
//
// Its own part because these three are a thin layer over the ledger and the
// resolution next door: they decide nothing a run does not already decide, and
// the one thing they own — that a project may opt into a lifetime total instead
// of a daily rate — is a single receipt.

// §AR-source-file-size.3 §FS-rhei-budgets.10

use rhei_core::budget::BudgetLine;

/// All three resolve the whole project and its single account even when the
/// target is one member rhei: one account, one balance, whatever was named.
/// §FS-rhei-budgets.10
fn budget_command(command: BudgetCommand) -> MietteResult<()> {
    match command {
        BudgetCommand::Init { input, invocations, reason } => {
            budget_init_command(input, invocations, &reason)
        }
        BudgetCommand::Show { input, rhei, format } => budget_show_command(input, &rhei, format),
        BudgetCommand::Adjust { input, invocations, reason } => {
            budget_adjust_command(input, invocations, &reason)
        }
    }
}

/// The project the target belongs to, and the bounds in force for it.
fn budget_command_context(input: Option<PathBuf>) -> MietteResult<(PathBuf, CountBounds)> {
    let target = resolve_plan_target(input)?;
    let workspace_root = execution_workspace_root(target.path());
    let settings = load_merged_settings(&workspace_root)?;
    let project_root = budget_project_root(&workspace_root);
    let bounds = resolve_count_bounds(&settings, None);
    Ok((project_root, bounds))
}

/// Move the project from the window contract to a lifetime allowance.
///
/// Optional by design: a project that never runs this is bounded by the window
/// contract and is never refused for not having run it.
/// §FS-rhei-budgets.3.2 §FS-rhei-budgets.10
fn budget_init_command(
    input: Option<PathBuf>,
    invocations: u64,
    reason: &str,
) -> MietteResult<()> {
    let (project_root, bounds) = budget_command_context(input)?;
    let audit = budget_audit(reason)?;
    let (_, mut journal) = budget_result(Account::establish(&project_root, &audit))?;
    if journal.contract().map(|contract| contract.is_lifetime()).unwrap_or(false) {
        return Err(miette!(
            help = "change an existing allowance with: rhei budget adjust",
            "this project already holds a lifetime allowance; `init` establishes one"
        ));
    }
    let granted = budget_clamped_allowance(invocations, &bounds);
    budget_result(journal.adjust(granted, &audit))?;
    println!("{}", budget_grant_line(invocations, granted, &bounds));
    Ok(())
}

/// Change the allowance, never the consumption.
///
/// A request **above** the ceiling is clamped and reported, never refused for
/// being high; a request **below** `consumed + outstanding` is refused, because
/// it would put a project in deficit for work already done.
/// §FS-rhei-budgets.10
fn budget_adjust_command(
    input: Option<PathBuf>,
    invocations: u64,
    reason: &str,
) -> MietteResult<()> {
    let (project_root, bounds) = budget_command_context(input)?;
    let Some(account) = budget_result(Account::locate(&project_root))? else {
        return Err(budget_no_account(&project_root));
    };
    let audit = budget_audit(reason)?;
    let mut journal = budget_result(account.open(true))?;
    let granted = budget_clamped_allowance(invocations, &bounds);
    budget_result(journal.adjust(granted, &audit))?;
    println!("{}", budget_grant_line(invocations, granted, &bounds));
    Ok(())
}

/// Read-only: it takes no lock that mutates, appends no receipt, and debits
/// nothing. §FS-rhei-budgets.6.3
fn budget_show_command(
    input: Option<PathBuf>,
    rhei: &[String],
    format: BudgetFormat,
) -> MietteResult<()> {
    let (project_root, bounds) = budget_command_context(input)?;
    let Some(account) = budget_result(Account::locate(&project_root))? else {
        return Err(budget_no_account(&project_root));
    };
    let journal = budget_result(account.open(false))?;
    let snapshot = budget_result(journal.snapshot())?;
    let lines = budget_result(journal.lines(None, bounds.per_day.effective, 0))?;
    // `--rhei` narrows display only. There is one account and one balance
    // whatever the selection, and a narrowing that showed a second would be
    // describing capacity that does not exist. §FS-rhei-budgets.10
    let narrowed = (!rhei.is_empty()).then(|| rhei.join(", "));
    match format {
        BudgetFormat::Json => {
            let report = serde_json::json!({
                "project_id": snapshot.project_id,
                "project_root": project_root,
                "account": account.directory(),
                "health": snapshot.health,
                "contract": snapshot.contract.name(),
                "window": matches!(snapshot.contract, Contract::Window).then(|| snapshot.day.clone()),
                "bounds": {
                    "transition_limit": budget_bound_json(&bounds.travel),
                    "invocations_per_day": budget_bound_json(&bounds.per_day),
                    "invocation_lifetime_max": budget_bound_json(&bounds.lifetime_max),
                },
                "invocations": budget_line_json(&lines[0]),
                "lifetime_invocations": {
                    "consumed": snapshot.lifetime_invocations.consumed,
                    "outstanding": snapshot.lifetime_invocations.reserved,
                },
                "narrowed_to": narrowed,
            });
            println!("{}", serde_json::to_string_pretty(&report).expect("report serializes"));
        }
        BudgetFormat::Text => {
            println!("Project: {} ({})", project_root.display(), snapshot.project_id);
            println!("Account: {} [{}]", account.directory().display(), snapshot.health);
            for line in &lines {
                println!(
                    "{}: {} consumed + {} outstanding / {}; {} remaining ({})",
                    line.dimension,
                    line.consumed,
                    line.outstanding,
                    line.bound,
                    line.remaining,
                    line.mode
                );
            }
            println!(
                "lifetime invocations: {} consumed + {} outstanding",
                snapshot.lifetime_invocations.consumed, snapshot.lifetime_invocations.reserved
            );
            for bound in bounds.report_lines() {
                println!("{bound}");
            }
            if let Some(narrowed) = narrowed {
                println!("(narrowed to {narrowed}; one account, one balance)");
            }
        }
    }
    Ok(())
}

fn budget_bound_json(bound: &Bound) -> serde_json::Value {
    serde_json::json!({
        "effective": bound.effective,
        "source": bound.source.as_str(),
        "requested": bound.requested,
        "limited_by": bound.requested.map(|_| "machine"),
    })
}

fn budget_line_json(line: &BudgetLine) -> serde_json::Value {
    serde_json::json!({
        "bound": line.bound,
        "consumed": line.consumed,
        "outstanding": line.outstanding,
        "remaining": line.remaining,
        "mode": line.mode,
    })
}

/// Clamped, never refused for being high: refusing it would make a command
/// written on one machine fail on the next. §FS-rhei-budgets.10
fn budget_clamped_allowance(requested: u64, bounds: &CountBounds) -> u64 {
    requested.min(bounds.lifetime_max.effective)
}

fn budget_grant_line(requested: u64, granted: u64, bounds: &CountBounds) -> String {
    if granted < requested {
        return format!(
            "lifetime invocation allowance: {granted} (requested {requested}, limited by \
             `defaults.invocation_lifetime_max` = {})",
            bounds.lifetime_max.effective
        );
    }
    format!("lifetime invocation allowance: {granted}")
}

fn budget_no_account(project_root: &Path) -> miette::Report {
    miette!(
        help = "an account is established by the first neural start; run the plan, or \
                establish one explicitly with: rhei budget init <TARGET> --invocations <N> \
                --reason <TEXT>",
        "{} has no budget account yet",
        project_root.display()
    )
}
