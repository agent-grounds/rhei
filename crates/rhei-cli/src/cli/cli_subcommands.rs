// The subcommand groups the root command flattens: one `Subcommand` enum per
// group, so a group's declarations travel with the command they belong to
// rather than inside the root enum.

// §FS-rhei-budgets.10 §FS-rhei-migrate.1

/// Explicit authored-plan compatibility operations. §FS-rhei-migrate.1
/// The invocation dimension only. A ticket's travel bound is a *settings*
/// value rather than an account balance, so there is nothing here to set it
/// with: it is raised in a settings file or on a profile.
/// §FS-rhei-budgets.10
#[derive(Subcommand, Debug)]
enum BudgetCommand {
    /// Move the project from the daily window to a lifetime invocation
    /// allowance
    Init {
        /// Path to a plan, workspace, or Panta project; omitted, discover it
        #[arg(
            value_name = "RHEI_PLAN_OR_WORKSPACE",
            add = ArgValueCompleter::new(complete_rhei_plan_path)
        )]
        input: Option<PathBuf>,
        /// Invocations the project may ever be admitted, clamped by
        /// `defaults.invocation_lifetime_max`
        #[arg(long, value_name = "N")]
        invocations: u64,
        /// Why, recorded in the audited receipt beside the actor and the argv
        #[arg(long, value_name = "TEXT")]
        reason: String,
    },
    /// Report both counts, their bounds, and where each bound came from
    Show {
        /// Path to a plan, workspace, or Panta project; omitted, discover it
        #[arg(
            value_name = "RHEI_PLAN_OR_WORKSPACE",
            add = ArgValueCompleter::new(complete_rhei_plan_path)
        )]
        input: Option<PathBuf>,
        /// Narrow the display to the named rhei. One account, one balance,
        /// whatever the selection
        #[arg(long = "rhei", value_name = "RHEI_ID", add = ArgValueCompleter::new(complete_rhei_id))]
        rhei: Vec<String>,
        /// Output format
        #[arg(long, value_enum, default_value_t = BudgetFormat::Text)]
        format: BudgetFormat,
    },
    /// Change an existing lifetime allowance, never its consumption
    Adjust {
        /// Path to a plan, workspace, or Panta project; omitted, discover it
        #[arg(
            value_name = "RHEI_PLAN_OR_WORKSPACE",
            add = ArgValueCompleter::new(complete_rhei_plan_path)
        )]
        input: Option<PathBuf>,
        /// The new allowance. Above the ceiling it is clamped and reported;
        /// below what is already consumed or outstanding it is refused
        #[arg(long, value_name = "N")]
        invocations: u64,
        /// Why, recorded in the audited receipt beside the actor and the argv
        #[arg(long, value_name = "TEXT")]
        reason: String,
    },
    /// Retire a damaged root, keeping its receipts; refused where the journal
    /// verifies
    Forget {
        /// Path to a plan, workspace, or Panta project; omitted, discover it
        #[arg(
            value_name = "RHEI_PLAN_OR_WORKSPACE",
            add = ArgValueCompleter::new(complete_rhei_plan_path)
        )]
        input: Option<PathBuf>,
        /// Why, recorded in the retirement receipt beside the actor and the
        /// argv
        #[arg(long, value_name = "TEXT")]
        reason: String,
    },
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum BudgetFormat {
    Text,
    Json,
}

#[derive(Subcommand, Debug)]
enum MigrateCommand {
    /// Add direct Prior edges required by declared Consumes relationships
    ExportPriors {
        /// Path to a plan, workspace, or Panta project; omitted, discover it
        #[arg(
            value_name = "RHEI_PLAN_OR_WORKSPACE",
            add = ArgValueCompleter::new(complete_rhei_plan_path)
        )]
        input: Option<PathBuf>,
        /// Show the exact additions without replacing authored files
        #[arg(long)]
        dry_run: bool,
    },
}
