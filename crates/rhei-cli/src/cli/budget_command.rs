// `rhei budget init`, `show`, and `adjust` — the first and last over the
// invocation dimension only, because there is no spend allowance to grant.
//
// Its own part because these three are a thin layer over the ledger and the
// resolution next door: they decide nothing a run does not already decide, and
// the one thing they own — that a project may opt into a lifetime total instead
// of a daily rate — is a single receipt.

// §AR-source-file-size.3 §FS-rhei-budgets.10

use rhei_core::budget::{BudgetLine, Inspection};

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
        BudgetCommand::Forget { input, reason } => budget_forget_command(input, &reason),
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
    let (_, mut journal) =
        budget_result_at(&project_root, Account::establish(&project_root, &audit))?;
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
    let Some(account) = budget_result_at(&project_root, Account::locate(&project_root))? else {
        return Err(budget_no_account(&project_root));
    };
    let audit = budget_audit(reason)?;
    let mut journal = budget_result_at(&project_root, account.open(true))?;
    let granted = budget_clamped_allowance(invocations, &bounds);
    budget_result_at(&project_root, journal.adjust(granted, &audit))?;
    println!("{}", budget_grant_line(invocations, granted, &bounds));
    Ok(())
}

/// Read-only: it takes no lock that mutates, appends no receipt, and debits
/// nothing.
///
/// It is also the command a refusal sends an operator to, so it may not fail
/// to open the account it was asked to describe: an account it cannot verify
/// is **reported** rather than refused for the missing file, and still exits
/// non-zero. §FS-rhei-budgets.6.3 §FS-rhei-budgets.10
fn budget_show_command(
    input: Option<PathBuf>,
    rhei: &[String],
    format: BudgetFormat,
) -> MietteResult<()> {
    let (project_root, bounds) = budget_command_context(input)?;
    let Some(account) = budget_result_at(&project_root, Account::locate(&project_root))? else {
        return Err(budget_no_account(&project_root));
    };
    let journal = match budget_result_at(&project_root, account.inspect())? {
        Inspection::Verified(journal) => *journal,
        Inspection::Damaged(diagnosis) => {
            return Err(budget_report_damaged(&diagnosis, &project_root, format))
        }
        Inspection::Absent => return Err(budget_no_account(&project_root)),
    };
    let snapshot = budget_result_at(&project_root, journal.snapshot())?;
    let lines = budget_result_at(
        &project_root,
        journal.lines(None, bounds.per_day.effective, 0, bounds.spend.effective, &run_is_gone),
    )?;
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
                    "spend_per_day": budget_bound_json(&bounds.spend),
                },
                "invocations": budget_line_json(&lines[0]),
                // In micro-units of the account's currency, so a reader never
                // has to parse a rendered `$`. §FS-rhei-budgets.10
                "spend": budget_spend_json(&lines[1], snapshot.currency.as_deref()),
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
                println!("{}", line.render());
                // The marks go beneath the line they qualify, in the words the
                // halt uses, and are absent where the whole day was measured.
                // §FS-rhei-budgets.10
                if let Some(marks) = line.marks_line() {
                    println!("{marks}");
                }
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

/// The spend dimension, with its currency and how much of the day was
/// estimated rather than measured. §FS-rhei-budgets.10 §FS-rhei-budgets.6.2
fn budget_spend_json(line: &BudgetLine, currency: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "bound": line.bound,
        "consumed": line.consumed,
        "outstanding": line.outstanding,
        "remaining": line.remaining,
        "mode": line.mode,
        "currency": currency,
        "unpriced": line.marks.unpriced,
        "unmeasurable": line.marks.unmeasurable,
        "unsettled": line.marks.unsettled,
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

/// An absent account is lawful and silent, so the help names every route that
/// ends it: establishment follows the first charge of either kind, and applying
/// an edge by hand is one of them. A refusal at a bound sends its reader here,
/// so a route this line leaves out is a route they will not find.
/// §FS-rhei-budgets.5.4
fn budget_no_account(project_root: &Path) -> miette::Report {
    miette!(
        help = "an account is established by the first charge of either kind: run the plan, \
                apply an edge with `rhei transition`, or establish one explicitly with: \
                rhei budget init <TARGET> --invocations <N> --reason <TEXT>",
        "{} has no budget account yet",
        project_root.display()
    )
}
