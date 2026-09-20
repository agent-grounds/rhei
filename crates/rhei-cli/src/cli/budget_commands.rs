// Operator commands share the embedded ledger and never invent a new balance
// when history or identity is ambiguous. §FS-rhei-budgets.8

#[derive(Subcommand, Debug)]
enum BudgetCommand {
    /// Initialize a persistent Panta allowance after importing its history
    Init {
        target: PathBuf,
        #[arg(long)]
        invocations: u64,
        #[arg(long)]
        spend_micro: u64,
        #[arg(long)]
        currency: String,
        #[arg(long)]
        reason: String,
        #[arg(long)]
        history: Option<PathBuf>,
    },
    /// Inspect the lifetime allowance without debiting it
    Show {
        target: PathBuf,
        #[arg(long)]
        rhei: Vec<String>,
        #[arg(long, value_enum, default_value = "text")]
        format: SnapshotListFormat,
    },
    /// Adjust ceilings without resetting consumption
    Adjust {
        target: PathBuf,
        #[arg(long)]
        invocations: Option<u64>,
        #[arg(long)]
        spend_micro: Option<u64>,
        #[arg(long)]
        reason: String,
    },
    /// Reconcile an invocation against authoritative provider evidence
    Reconcile {
        target: PathBuf,
        #[arg(long)]
        invocation: String,
        #[arg(long)]
        evidence: PathBuf,
        #[arg(long)]
        reason: String,
    },
    /// Recover a damaged journal from a complete trusted copy
    Recover {
        target: PathBuf,
        #[arg(long)]
        journal: PathBuf,
        #[arg(long)]
        reason: String,
    },
}

/// Keep reason codes in the core; the CLI supplies a recovery action.
/// §FS-rhei-budgets.9
fn budget_error(error: rhei_core::budget::BudgetError) -> Report {
    Report::new(BudgetDiagnostic(error))
}

#[derive(Debug)]
struct BudgetDiagnostic(rhei_core::budget::BudgetError);

impl std::fmt::Display for BudgetDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.0.reason_code, self.0.message)
    }
}

impl std::error::Error for BudgetDiagnostic {}

impl miette::Diagnostic for BudgetDiagnostic {
    fn help<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        Some(Box::new("inspect `rhei budget show <TARGET>`; missing bounds need `budget init`, exhausted allowances need an audited `budget adjust`, and missing qualification needs verified transport evidence"))
    }
}

fn budget_command(command: BudgetCommand) -> MietteResult<()> {
    use rhei_core::budget::{Allowance, Journal, Money};
    match command {
        BudgetCommand::Init { target, invocations, spend_micro, currency, reason, history } => {
            budget_initialize(
                &target,
                Allowance { invocations, spend: Money { currency, amount_micro: spend_micro } },
                &reason,
                history.as_deref(),
            )
        }
        BudgetCommand::Show { target, rhei, format } => {
            let project = BudgetProject::resolve(&target)?;
            // Scope narrows task display only; the allowance always belongs
            // to the complete Panta. §FS-rhei-budgets.8
            let loaded = load_plan(&project.input)?;
            let scope = resolve_rhei_scope(&loaded, &rhei)?;
            let uuid = project.required_identity()?;
            let ledger = Journal::open(&project.root, &uuid, false).map_err(budget_error)?;
            let mut snapshot = ledger.snapshot().map_err(budget_error)?;
            let resolved = resolve_state_machines_for_loaded_plan(&project.input, &loaded, None)?;
            let machines = ExecutionMachines::build(&resolved, &project.input, &loaded)?;
            budget_add_travel_limits(&mut snapshot, &loaded, &machines.set, &uuid)?;
            if !rhei.is_empty() {
                let selected = budget_task_list(&loaded.rhei.tasks)
                    .into_iter()
                    .filter(|task| task_in_rhei_scope(&scope, &task.id.to_string()))
                    .filter_map(|task| budget_ticket_identity(&loaded, &task.id, &uuid).ok())
                    .collect::<BTreeSet<_>>();
                snapshot.travel.retain(|ticket, _| selected.contains(ticket));
                snapshot.travel_limits.retain(|ticket, _| selected.contains(ticket));
                snapshot.travel_remaining.retain(|ticket, _| selected.contains(ticket));
                snapshot.reservations.retain(|_, row| {
                    row["ticket_identity"].as_str().is_some_and(|ticket| selected.contains(ticket))
                });
            }
            match format {
                SnapshotListFormat::Json => println!(
                    "{}",
                    serde_json::to_string_pretty(&snapshot)
                        .map_err(|e| miette!(help = "re-run without --format json to read the allowance as text", "cannot render the budget snapshot as JSON: {e}"))?
                ),
                SnapshotListFormat::Text => {
                    println!("Project: {}", snapshot.project_id);
                    println!(
                        "Invocations: {} consumed + {} reserved / {}; {} remaining",
                        snapshot.consumed.invocations,
                        snapshot.reserved.invocations,
                        snapshot.allowance.invocations,
                        snapshot.remaining.invocations
                    );
                    println!(
                        "Spend: {} consumed + {} reserved / {} micro-{}; {} remaining",
                        snapshot.consumed.spend.amount_micro,
                        snapshot.reserved.spend.amount_micro,
                        snapshot.allowance.spend.amount_micro,
                        snapshot.allowance.spend.currency,
                        snapshot.remaining.spend.amount_micro
                    );
                    println!("Journal: {}", ledger.path().display());
                    println!("History authority: {}", ledger.authority_path().display());
                    for (ticket, travel) in &snapshot.travel {
                        println!(
                            "Travel {ticket}: {} applied + {} reserved / {}; {} remaining",
                            travel.consumed,
                            travel.reserved,
                            snapshot
                                .travel_limits
                                .get(ticket)
                                .map(u64::to_string)
                                .unwrap_or_else(|| "unknown".into()),
                            snapshot
                                .travel_remaining
                                .get(ticket)
                                .map(u64::to_string)
                                .unwrap_or_else(|| "unknown".into())
                        );
                    }
                    if snapshot.breached {
                        println!("Budget halt: breach retained; requalification required");
                    }
                    for line in snapshot.reservation_lines() {
                        println!("{line}");
                    }
                    for entry in &snapshot.audit {
                        if let Some(reason) = entry["audit"]["reason"].as_str() {
                            println!("Audit: {reason}");
                        }
                    }
                }
            }
            Ok(())
        }
        BudgetCommand::Adjust { target, invocations, spend_micro, reason } => {
            if invocations.is_none() && spend_micro.is_none() {
                return Err(miette!(help = "name the unit you are changing: --invocations <n>, --spend-micro <n>, or both", "budget adjust requires --invocations or --spend-micro"));
            }
            let project = BudgetProject::resolve(&target)?;
            let mut transaction = budget_ensure_ticket_identities(&project)?;
            let ledger = &mut transaction.journal;
            let mut next = ledger.snapshot().map_err(budget_error)?.allowance;
            if let Some(n) = invocations {
                next.invocations = n;
            }
            if let Some(n) = spend_micro {
                next.spend.amount_micro = n;
            }
            ledger.adjust(next, &budget_audit(&reason)?).map_err(budget_error)?;
            println!("Allowance adjusted; consumption and outstanding exposure preserved.");
            Ok(())
        }
        BudgetCommand::Reconcile { target, invocation, evidence, reason } => {
            let project = BudgetProject::resolve(&target)?;
            let project_uuid = project.required_identity()?;
            let mut ledger = Journal::open(&project.root, &project_uuid, true)
                .map_err(budget_error)?;
            let snapshot = ledger.snapshot().map_err(budget_error)?;
            let reservation = snapshot
                .reservations
                .get(&invocation)
                .ok_or_else(|| miette!(help = "`rhei budget show --format json` lists the reservation ids this evidence can settle", "invocation has no existing budget reservation: {invocation}"))?;
            let tuple: rhei_core::budget::LaunchTuple = serde_json::from_value(
                reservation["qualification"].clone(),
            )
            .map_err(|error| miette!(help = "the ledger is untrustworthy; recover it with `rhei budget recover` before reconciling", "reservation qualification is invalid: {error}"))?;
            let bytes = fs::read(&evidence)
                .map_err(|e| file_io_report(&evidence, "cannot read reconciliation evidence", e))?;
            let signature = budget_evidence_signature(&evidence)?;
            let authority = rhei_core::budget::Registry::evidence_authority(
                &tuple,
                &format!("panta:{project_uuid}"),
                &snapshot.allowance.spend.currency,
            )
            .map_err(budget_error)?;
            let verified = authority
                .verify_reconciliation(
                    &bytes,
                    &signature,
                    &invocation,
                    reservation["attempt_identity"].as_str().unwrap_or(""),
                )
                .map_err(budget_error)?;
            ledger
                .reconcile_verified(verified, &budget_audit(&reason)?)
                .map_err(budget_error)?;
            println!("Reservation reconciled from final provider/account evidence.");
            Ok(())
        }
        BudgetCommand::Recover { target, journal, reason } => {
            let project = BudgetProject::resolve(&target)?;
            Journal::recover(
                &project.root,
                &project.required_identity()?,
                &journal,
                &budget_audit(&reason)?,
            )
            .map_err(budget_error)?;
            println!("Recovered complete witnessed history; consumption and outstanding exposure preserved.");
            Ok(())
        }
    }
}

fn budget_evidence_signature(path: &Path) -> MietteResult<[u8; 64]> {
    let signature_path = PathBuf::from(format!("{}.sig", path.display()));
    let raw = fs::read_to_string(&signature_path)
        .map_err(|error| file_io_report(&signature_path, "cannot read evidence signature", error))?;
    let raw = raw.trim();
    if raw.len() != 128
        || !raw
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(miette!(help = "pass the detached Ed25519 signature as 128 lowercase hex characters", "evidence signature must be 64 lowercase-hex bytes"));
    }
    let mut signature = [0u8; 64];
    for (index, pair) in raw.as_bytes().chunks_exact(2).enumerate() {
        let text = std::str::from_utf8(pair).expect("hex is ASCII");
        signature[index] = u8::from_str_radix(text, 16)
            .map_err(|_| miette!(help = "pass the detached Ed25519 signature as 128 lowercase hex characters", "evidence signature is not lowercase hex"))?;
    }
    Ok(signature)
}
