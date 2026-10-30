// Account trust at a start, independent of neural admission.
// §FS-rhei-budgets.5.4 §FS-rhei-budgets.9

/// Verify existing history without establishing an account or reserving capacity.
/// Every spawn path asks this one question before it reports a spent visit, so
/// a damaged account is never hidden behind a `rhei retry` that cannot get past
/// it. It refuses what `rhei budget show` reports as damaged and passes what it
/// reports as absent. The journal and authority locks are released before it
/// returns; external work never holds them. §FS-rhei-budgets.5.4
fn verify_account_trust(project: &Path) -> Result<(), BudgetError> {
    if !project.is_dir() {
        return Ok(());
    }
    Account::locate(project).and_then(|account| {
        // Absent, even where the index still names the root, belongs to the
        // first charge, not this check. §FS-rhei-budgets.5.4
        account.as_ref().map_or(Ok(()), Account::verify)
    })
}

/// Account trust at a program start. Both program dispatch paths call this
/// before reporting a spent visit, and so before assigning a slot or recording
/// an attempt. §FS-rhei-budgets.5.4
fn verify_program_account(
    task_root: &Path,
    task: &str,
    state: &str,
) -> MietteResult<()> {
    let project = budget_project_root(task_root);
    verify_account_trust(&project).map_err(|error| {
        miette::Report::new(ProgramAccountRefusal {
            task: task.to_string(),
            state: state.to_string(),
            diagnosis: error.message,
            remedy: budget_inspect_help_for(&project),
        })
    })
}

/// Account trust behind an agent's spent visit, at the root admission uses.
///
/// An unspent visit learns of a damaged account from admission; a spent one
/// never reaches admission, so it asks here. A damaged account is refused
/// exactly as admission would refuse it, and an absent one passes, as
/// admission would establish it, so the spent visit is reported.
/// §FS-rhei-budgets.5.4
fn verify_spent_agent_account(workspace_root: &Path) -> MietteResult<()> {
    let project = budget_project_root(workspace_root);
    budget_result_at(&project, verify_account_trust(&project))
}

/// A pre-start refusal, kept distinct from a program's exit or spawn failure.
/// The diagnostic retains the existing budget message and help; its task and
/// state let the fallback report explain the halt without inventing execution.
/// §FS-rhei-budgets.9
#[derive(Clone, Debug)]
struct ProgramAccountRefusal {
    task: String,
    state: String,
    diagnosis: String,
    remedy: String,
}

// Preserve the existing account refusal on stderr. §FS-rhei-budgets.9
impl std::fmt::Display for ProgramAccountRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.diagnosis)
    }
}

impl std::error::Error for ProgramAccountRefusal {}

// Preserve the recovery command alongside the diagnosis. §FS-rhei-budgets.9
impl miette::Diagnostic for ProgramAccountRefusal {
    fn help(&self) -> Option<Box<dyn std::fmt::Display + '_>> {
        Some(Box::new(&self.remedy))
    }
}
