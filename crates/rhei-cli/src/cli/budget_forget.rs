// `rhei budget forget` — the operator's answer to the reading the tool cannot
// choose between.
//
// Its own part because it is the only budget command that gives a *root* up,
// and because the whole of it is the guard: most of its cases refuse, and the
// two that act are audited moves rather than deletions.

// §AR-source-file-size.3 §FS-rhei-budgets.5.3 §FS-rhei-budgets.10

use rhei_core::budget::Retirement;

/// Retire a stale root, and refuse everywhere else.
///
/// Journal present and verifying: refused, nothing written — that refusal is
/// the guarantee of §FS-rhei-budgets.5.3 and not a convenience, because
/// without it this command would be a way to reset a working balance. Journal
/// present and damaged: refused too, naming the restore, because that journal
/// is this project's own. No witness claims the root: refused, naming the path,
/// since the lawful **absent** state has nothing to retire. Journal wholly
/// absent: retired. A copy of a root that still holds the account: its claim
/// retired, the holder's history untouched. A tail written from another root:
/// refused, naming the set-aside. The account makes that choice under its own
/// lock; this command only says it in words. §FS-rhei-budgets.5.4
/// §FS-rhei-budgets.5.4.1 §FS-rhei-budgets.5.4.2 §FS-rhei-budgets.10
fn budget_forget_command(input: Option<PathBuf>, reason: &str) -> MietteResult<()> {
    let (project_root, _) = budget_command_context(input)?;
    let audit = budget_audit(reason)?;
    let Some(account) = budget_result_at(&project_root, Account::locate(&project_root))? else {
        return Err(budget_nothing_to_retire(&project_root));
    };
    let retired = match budget_result_at(&project_root, account.retire(&audit))? {
        Retirement::Sound => return Err(budget_sound_account(&project_root)),
        Retirement::NothingToRetire => return Err(budget_nothing_to_retire(&project_root)),
        Retirement::RestoreInstead(diagnosis) => return Err(budget_restore_instead(&diagnosis)),
        Retirement::Retired(retired) => retired,
    };
    println!("retired {} for {}", retired.uuid, retired.root.display());
    println!(
        "  {} ({} invocations) kept at {}",
        retired.history.receipts_phrase(),
        retired.history.invocations,
        retired.kept_at.display()
    );
    println!("  this path now has no account; the next run establishes one at zero consumed");
    Ok(())
}

/// The refusal that keeps `forget` a recovery command. `adjust` is named
/// because it is what does change an allowance, so the two never compete for a
/// case. §FS-rhei-budgets.5.3 §FS-rhei-errors.1.2
fn budget_sound_account(project_root: &Path) -> miette::Report {
    miette!(
        help = format!(
            "change an allowance with: rhei budget adjust {} --invocations <N> --reason <TEXT>",
            budget_target_argument(project_root)
        ),
        "the budget account at {} verifies; retiring a sound account would recreate capacity",
        budget_target(project_root)
    )
}

/// The refusal for a journal that is **there**, however damaged. Retiring the
/// root would give up an account that is genuinely this project's and leave the
/// journal that still fails to replay, so the operator would be worse off than
/// before they ran the command. The damaged report never offers `forget` here
/// for the same reason; this closes the case of being asked directly.
/// §FS-rhei-budgets.5.4 §FS-rhei-errors.1.2
///
/// A tail written from another root is refused the same way, but what it names
/// is setting the witness aside: the restore would import that root's receipts.
/// §FS-rhei-budgets.5.4.2
fn budget_restore_instead(diagnosis: &Diagnosis) -> miette::Report {
    if let Some(set_aside) = diagnosis.set_aside_command() {
        return miette!(
            help = format!("set the witness aside, keeping every byte, with: {set_aside}"),
            "the budget journal at {} is this project's own, but the committed history past it \
             was written from {}; retiring the root would discard a real account",
            diagnosis.journal.display(),
            diagnosis
                .foreign
                .as_ref()
                .map(|foreign| foreign.root.display().to_string())
                .unwrap_or_else(|| "another project root".into())
        );
    }
    miette!(
        help = format!(
            "restore this project's journal with: {}",
            diagnosis.restore_command().unwrap_or_default()
        ),
        "the budget journal at {} is this project's own and its tail is what was lost; \
         retiring the root would discard a real account and leave the journal in place",
        diagnosis.journal.display()
    )
}

/// A path no witness claims is far more often a typo than a repeat, so it is
/// named rather than treated as already done. §FS-rhei-budgets.10
fn budget_nothing_to_retire(project_root: &Path) -> miette::Report {
    miette!(
        help = format!(
            "see what this path resolves to with: rhei budget show {}",
            budget_target_argument(project_root)
        ),
        "no committed history claims {}, so there is no root to retire",
        budget_target(project_root)
    )
}
