// Account trust at a program start, independent of neural admission.
// §FS-rhei-budgets.5.4 §FS-rhei-budgets.9

/// Verify existing history without establishing an account or reserving capacity.
/// Both program dispatch paths call this before assigning a slot or recording
/// an attempt. The journal and authority locks are released before it returns;
/// external work never holds them. §FS-rhei-budgets.5.4
fn verify_program_account(
    task_root: &Path,
    task: &str,
    state: &str,
) -> MietteResult<()> {
    let project = budget_project_root(task_root);
    if !project.is_dir() {
        return Ok(());
    }
    let verified = Account::locate(&project).and_then(|account| {
        // An absent account belongs to the first charge, not this check.
        // §FS-rhei-budgets.5.4
        account.map(|account| account.open(false)).transpose().map(drop)
    });
    verified.map_err(|error| {
        miette::Report::new(ProgramAccountRefusal {
            task: task.to_string(),
            state: state.to_string(),
            diagnosis: error.message,
            remedy: budget_inspect_help_for(&project),
        })
    })
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
