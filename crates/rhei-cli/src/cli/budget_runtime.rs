// The admission checkpoint before a spawn, and the claim it holds for the
// visit it paid for.
//
// Its own part because everything here is *plumbing* — finding the project,
// finding the ticket's identity, deciding what an audit record says — while the
// arithmetic it plumbs into lives in `rhei_core::budget`. The travel charge on
// the shared transition path is the part next door; adding a third call site is
// how this boundary stops being one.

// §AR-source-file-size.3 §FS-rhei-budgets.6 §AR-neural-admission.7

use rhei_core::budget::{
    Account, AdmissionRequest, Ancestry, AncestryDescriptor, Arm, Audit, BudgetError, Journal,
    SpendBasis,
};

/// Where a ticket's budget identity is persisted.
///
/// Which of rhei's own keys `rhei reset` deletes is the register's own column
/// (§FS-rhei-transitions.2.5); this key is deliberately not among them, because
/// the document must not be made to disagree with the ledger. Removing it buys
/// no travel: `TicketIdentity::settle` below resolves a document that names no
/// identity from the binding the ledger already holds for it rather than minting
/// a new one. §FS-rhei-reset.2 §FS-rhei-budgets.5.2
const BUDGET_TICKET_KEY: &str = rhei_core::metadata::BUDGET_TICKET_ID_KEY;

/// What an admission decided, in the three shapes the caller acts on.
enum BudgetAdmission {
    /// Every unit is reserved and the spawn may proceed. The report is where
    /// both counts now stand, for the surfaces that show it while it is spent
    /// rather than only at exhaustion. §FS-rhei-budgets.9
    ///
    /// `note` is the one thing an admitted spawn may still owe an operator: the
    /// descriptor this run inherited was minted elsewhere, so the run charges
    /// its own account and says so once. §FS-rhei-budgets.7.2
    ///
    /// `arm` is the reservation this spawn was admitted on: the name every later
    /// receipt of *this* arm is written against, so a fanout's sibling arms never
    /// stand in for each other. §FS-rhei-budgets.6.2.1
    Admitted { bounds: Vec<rhei_tui::BoundReport>, note: Option<String>, arm: Option<String> },
    /// A bound, or an account that cannot be trusted, refused the spawn. The
    /// text is the whole halt of §FS-rhei-budgets.8, ready to print, and the
    /// record carries the same facts in a form a reader can route on.
    ///
    /// Boxed because a refusal is the rare arm and a `RunEvent` is wide: the
    /// ordinary admitted path should not pay for it.
    Refused {
        halt: String,
        event: Option<Box<rhei_tui::RunEvent>>,
        /// Current facts even when no spawn was admitted. §FS-rhei-budgets.9
        bounds: Vec<rhei_tui::BoundReport>,
    },
    /// This target has no project directory to account against. Nothing is
    /// bounded and nothing is charged, which is the same answer the engine gave
    /// before this checkpoint existed.
    NotAccounted,
}

/// One admitted spawn's claim on the account, held for the visit it paid for.
///
/// Keyed by ticket rather than owned by the spawning frame because the two
/// halves of a visit sit in different places: a parallel arm is admitted on the
/// scheduler's thread and completes on a worker's, and a travel unit has to
/// survive that hand-off to be applied or released by whoever ends the visit.
///
/// A fanout's arms share the one claim — and its one travel unit — but each
/// keeps its own entry, settled at its own exit; the claim ends with its last
/// live arm. §FS-rhei-budgets.6.2.1 §FS-rhei-budgets.4.2
struct HeldClaim {
    travel: Option<String>,
    /// The arms still live, each under the reservation it was admitted on.
    arms: Vec<HeldArm>,
    /// The account this claim was admitted against, so that the descriptor
    /// handed to a descendant can say whose its reservation is. Kept beside the
    /// arms rather than re-resolved at the spawn: the two halves must name the
    /// same admission or the child would compare against an account nothing
    /// minted. §FS-rhei-budgets.7.1
    account: String,
    /// The ticket's account identity and the bounds it was admitted under, so
    /// the account read after each later receipt reports the same lines the
    /// admission did. §FS-rhei-run-json.2.1
    ticket: String,
    bounds: CountBounds,
}

/// One admitted arm of a claim, from its admission to its own settle.
/// §FS-rhei-budgets.6.2.1
struct HeldArm {
    reservation: String,
    /// Whether a start has been recorded for this arm. Until one has, the
    /// engine's own knowledge that it never spawned *is* the proof of
    /// non-start; after one has, nothing refunds it. §FS-rhei-budgets.6.2
    started: bool,
    /// What this arm's invocation turned out to cost, from its own record.
    ///
    /// An arm with none is charged the reserve as `unmeasurable`, which is
    /// what makes an agent with no accounting extractor visible rather than
    /// free — that class writes no record at all, so a settle-side fallback
    /// would never run for it. §FS-rhei-budgets.6.2
    measured: Option<Measurement>,
}

impl HeldClaim {
    fn arm_mut(&mut self, reservation: &str) -> Option<&mut HeldArm> {
        self.arms.iter_mut().find(|arm| arm.reservation == reservation)
    }
}

fn held_claims() -> &'static std::sync::Mutex<BTreeMap<String, HeldClaim>> {
    static CLAIMS: std::sync::OnceLock<std::sync::Mutex<BTreeMap<String, HeldClaim>>> =
        std::sync::OnceLock::new();
    CLAIMS.get_or_init(|| std::sync::Mutex::new(BTreeMap::new()))
}

fn with_claims<T>(f: impl FnOnce(&mut BTreeMap<String, HeldClaim>) -> T) -> T {
    let mut claims = held_claims().lock().unwrap_or_else(|poison| poison.into_inner());
    f(&mut claims)
}

/// Who is spending, when, why, and the exact argv — the record every receipt
/// carries so that a number that changed has someone's name on it.
/// §FS-rhei-budgets.10
fn budget_audit(reason: &str) -> MietteResult<Audit> {
    let now = budget_result(rhei_core::budget::now())?;
    Ok(Audit {
        actor: std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "rhei".into()),
        written_at: rhei_core::budget::instant(now),
        reason: reason.to_string(),
        argv: std::env::args().collect(),
    })
}

/// A core refusal for a reader, with the help that names the command's own target:
/// a help line is a command they can paste, so it carries the argument that mattered.
/// `show` itself never takes this help: sending an operator back to the command they
/// just ran is not a next action. §FS-rhei-errors.1.2 §FS-rhei-budgets.10.1
fn budget_result<T>(result: Result<T, BudgetError>) -> MietteResult<T> {
    result.map_err(|err| miette!(help = budget_inspect_help(), "{}", budget_refusal_text(&err)))
}

/// `show` of the target the command was given, which a run passes no machine to,
/// because `rhei budget` takes none. §FS-rhei-errors.1.2.1
fn budget_inspect_help() -> String {
    format!("inspect the project's account with: {}", help_subcommand_or_bare("budget show", "", ""))
}

/// `forget` of the target the command was given, never the account root, which for
/// a lone plan is a directory no plan verb takes. It reaches the same root: the one
/// the witness index is keyed by, and what a retirement retracts.
/// §FS-rhei-budgets.10.1 §FS-rhei-budgets.5.4.1
fn budget_forget_line() -> String {
    help_subcommand_or_bare("budget forget", "", "--reason <TEXT>")
}

/// What core refused, whole: the one place its `forget` offer meets the target.
/// Every surface a refusal reaches a reader through reads this, not `message`.
/// §FS-rhei-budgets.10.1
fn budget_refusal_text(refusal: &BudgetError) -> String {
    refusal.told_with_forget(&budget_forget_line())
}

/// The project in the spelling every other budget surface prints it, so a help
/// line and the report it sends the reader to name one path. The witness index
/// is keyed by the resolved root, and a root that does not resolve is spelled
/// as it was given. §REQ-cross-platform.5
fn budget_target(project_root: &Path) -> String {
    rhei_core::platform::canonical_path(project_root)
        .unwrap_or_else(|_| project_root.to_path_buf())
        .display()
        .to_string()
}

/// The project's account, established when it is absent.
///
/// `None` means there is no project directory to account against, which is not
/// a refusal.
fn open_project_account(project_root: &Path, reason: &str) -> MietteResult<Option<Journal>> {
    if !project_root.is_dir() {
        return Ok(None);
    }
    let audit = budget_audit(reason)?;
    let (_, journal) = budget_result(Account::establish(project_root, &audit))?;
    Ok(Some(journal))
}

/// A ticket's budget identity, resolved under the owning rhei's metadata lock.
///
/// The identity follows *spending*, never attempting: a ticket that has never
/// been bound has spent nothing, so an admission weighed against a not-yet-
/// written identity is exact, and a run refused at that admission leaves the
/// authored plan byte-identical. §FS-rhei-budgets.6.1 §FS-rhei-budgets.8
struct TicketIdentity {
    /// The uuid the plan itself carried, or `None` until the admission settles
    /// one against the replayed ledger. Reading is not settling: the ledger
    /// cannot be consulted before the journal is open, and the journal sits
    /// below metadata in the lock order. §AR-neural-admission.3
    uuid: Option<String>,
    /// The write a freshly minted identity owes the plan, withheld until the
    /// same transaction has spent something on the ticket. `None` when the plan
    /// already carried an identity, which is nothing left to write.
    pending: Option<PendingIdentity>,
}

/// A minted identity, the metadata it will be folded into, and the lock that
/// has been held since it was read.
struct PendingIdentity {
    lock: LockedPlanFile,
    metadata_file: PathBuf,
    raw: String,
    on_disk: Option<Metadata>,
    metadata_id: TaskId,
}

/// Read the ticket's identity, reporting its absence rather than inventing one.
///
/// The metadata lock is taken here and held by the returned value until the
/// admission it is for has decided, which is why the lock order puts metadata
/// *above* the account: it is acquired first and may be held while the journal
/// lock is taken, never the other way around. §AR-neural-admission.3
fn ticket_budget_identity(
    input: &Path,
    loaded: &LoadedPlan,
    task_id_str: &str,
) -> MietteResult<TicketIdentity> {
    let route = loaded.task_route(task_id_str, input);
    let metadata_id = parse_task_id(&route.metadata_id);
    let lock = LockedPlanFile::open(&route.metadata_file)?;
    let raw = lock.read_to_string("failed to read plan metadata file")?;
    let on_disk = parse_metadata_from_raw(&route.metadata_file, &raw)?;
    if let Some(existing) = task_metadata_map(on_disk.as_ref(), &metadata_id)
        .and_then(|task| task.get(yaml_key(BUDGET_TICKET_KEY)))
        .and_then(YamlValue::as_str)
    {
        return Ok(TicketIdentity { uuid: Some(existing.to_string()), pending: None });
    }
    let pending =
        PendingIdentity { lock, metadata_file: route.metadata_file, raw, on_disk, metadata_id };
    Ok(TicketIdentity { uuid: None, pending: Some(pending) })
}

impl TicketIdentity {
    /// Settle which uuid this admission weighs: the plan's own, else the one the
    /// replayed ledger already binds to this source path and display id, else a
    /// fresh mint.
    ///
    /// Minting is the last resort rather than the first move, because a ticket
    /// whose plan write was lost after its receipts were appended has already
    /// spent against a uuid the plan no longer names, and minting a second one
    /// would hand it a second travel bound. §FS-rhei-budgets.6.1
    fn settle(&mut self, journal: &Journal, display: &str) -> MietteResult<String> {
        if let Some(uuid) = &self.uuid {
            return Ok(uuid.clone());
        }
        let adopted = match self.pending.as_ref() {
            Some(pending) => budget_result(journal.bound_ticket(display, &pending.metadata_file))?,
            None => None,
        };
        let settled = adopted.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        self.uuid = Some(settled.clone());
        Ok(settled)
    }

    /// Write a settled identity into the plan, under the lock still held from
    /// the read.
    ///
    /// Called only once the transaction that resolved it has spent something on
    /// this ticket. A ticket that already had one writes nothing; one whose
    /// admission refused is dropped instead, releasing the lock with the plan
    /// untouched. §FS-rhei-budgets.6.1
    fn commit(self) -> MietteResult<()> {
        let Some(pending) = self.pending else { return Ok(()) };
        // Nothing was settled, so nothing was weighed and nothing spent.
        let Some(uuid) = self.uuid else { return Ok(()) };
        let mut root = pending.on_disk.unwrap_or_default();
        let metadata_section = ensure_mapping(&mut root, yaml_key("metadata"));
        let tasks = ensure_mapping(metadata_section, yaml_key("tasks"));
        let task_entry = ensure_mapping(tasks, task_id_yaml_key(&pending.metadata_id));
        task_entry.insert(yaml_key(BUDGET_TICKET_KEY), YamlValue::String(uuid));
        let rewritten = rewrite_frontmatter(&pending.raw, &root)?;
        write_file_atomic_locked(&pending.metadata_file, &rewritten, Some(&pending.lock))
    }
}

/// Whether the run that owns a reservation is gone, which is the proof of
/// non-start: its execution-root lock can be acquired, so nothing holds it.
///
/// Called from inside the held account, which reaches back up the lock order of
/// §AR-neural-admission.3 and is permitted there for one reason: this is a
/// `try_lock_exclusive` and never waits, so it can close no cycle. A blocking
/// acquisition here would be the violation.
/// §FS-rhei-budgets.6.2 §AR-neural-admission.3
fn run_is_gone(execution_root: &str) -> bool {
    let lock_path = Path::new(execution_root).join(".rhei/run.lock");
    let Ok(file) = fs::OpenOptions::new().read(true).write(true).open(&lock_path) else {
        // No lock file at all: no run of that root is live.
        return true;
    };
    match file.try_lock_exclusive() {
        Ok(()) => {
            let _ = fs2::FileExt::unlock(&file);
            true
        }
        Err(_) => false,
    }
}

/// The admission checkpoint: one ordered, serialized transaction immediately
/// before a spawn.
///
/// Resolve the bounds, lock and replay the account, check one travel unit for
/// the ticket, one invocation unit per arm and the worst case each arm may
/// spend, append one reservation carrying every unit and every reserved
/// amount — or refuse and append nothing. Only then may the caller create the
/// subprocess.
///
/// `currency` is the composed price book's, which fixes an account's own on
/// its first amount and is refused where the account already holds another.
/// §FS-rhei-budgets.6.1 §FS-rhei-budgets.5.5 §FS-rhei-run.3.4
/// All admissions use their run's note entitlement. §FS-rhei-budgets.7.2
/// `state` is the normalized state the spawn is for, which a refusal records; it
/// is the caller's because `task` is loaded fresh, and so may already show another
/// arm's move. §FS-rhei-run-json.2.1
#[allow(clippy::too_many_arguments)]
fn budget_admit_spawn(
    input: &Path,
    loaded: &LoadedPlan,
    workspace_root: &Path,
    machine: &rhei_validator::StateMachine,
    settings: &RheiSettings,
    task: &rhei_core::ast::Task,
    task_id_str: &str,
    state: &str,
    currency: &str,
    run: &BudgetRun,
) -> MietteResult<BudgetAdmission> {
    let project_root = budget_project_root(workspace_root);
    let bounds = resolve_count_bounds(settings, node_transition_limit(machine, Some(task)));
    // Read under the metadata lock, which is held across the reserve below: a
    // settled identity is written only where the reserve spent something, so a
    // refusal writes nothing. §AR-neural-admission.3 §FS-rhei-budgets.6.1
    let mut identity = ticket_budget_identity(input, loaded, task_id_str)?;
    let route = loaded.task_route(task_id_str, input);
    let Some(mut journal) = open_project_account(&project_root, "admit a neural start")? else {
        return Ok(BudgetAdmission::NotAccounted);
    };
    let account = budget_result(Account::locate(&project_root))?.ok_or_else(|| {
        miette!(help = budget_inspect_help(), "the account just established has no identity")
    })?;
    // Settled against the replayed ledger rather than at the read, so a plan
    // that lost its key adopts the history it already spent. §FS-rhei-budgets.6.1
    let ticket = account.ticket_identity(&identity.settle(&journal, task_id_str)?);
    let audit = budget_audit("admit a neural start")?;
    // A fanout arrives as several work items, so the ticket's one travel unit
    // goes to the first arm of a visit and later arms take an invocation unit
    // alone. One applied edge, however many arms. §FS-rhei-budgets.4.2
    let travel = with_claims(|claims| !claims.contains_key(task_id_str));
    let attempts = [format!("attempt:{}", uuid::Uuid::new_v4())];
    // An envelope of one lets a nested `rhei run` draw a single admission
    // through the ancestry path, which is the one door anything a program
    // does is counted through. §FS-rhei-budgets.7
    let arms: Vec<Arm<'_>> = attempts
        .iter()
        .map(|attempt| Arm { attempt_identity: attempt, descendant_envelope: 1 })
        .collect();
    let execution_root = workspace_root.to_string_lossy().into_owned();
    let project_label = budget_project_label(&project_root);
    let parent = InheritedAncestry::from_environment();
    // §FS-rhei-budgets.5.2.1: said out loud here, where there is a channel for
    // it, and on what the journal did rather than on what the reservation did —
    // the binding has moved by the time a refusal could swallow the line.
    let bound = journal.bind_ticket(&ticket, task_id_str, &route.metadata_file, &audit);
    if let Ok(Some(moved)) = &bound {
        eprintln!("{}", moved.warning());
    }
    // Reported already, so what the move was is of no further interest here.
    let admitted = bound.and_then(|_| {
        // A reservation a crashed run left outstanding is given back before
        // this one is weighed, so a dead run's claim never refuses a live one.
        // §FS-rhei-budgets.6.2
        journal.release_abandoned(&run_is_gone, &audit)?;
        let request = AdmissionRequest {
            ticket_identity: &ticket,
            display_id: task_id_str,
            project_label: &project_label,
            execution_root: &execution_root,
            arms: &arms,
            parent_reservation: parent.as_ref().map(InheritedAncestry::descriptor),
            travel,
            spend_reserve_micro: built_in::SPEND_RESERVE,
            spend_currency: currency,
        };
        journal.reserve(&request, bounds.effective(), &audit)
    });
    match admitted {
        Ok(group) => {
            // The reserve spent, so the ticket has earned its durable identity.
            identity.commit()?;
            let reports = budget_reports(&journal, &ticket, &bounds).unwrap_or_default();
            // Said once per run, and only where the ledger actually declined
            // the ancestry rather than where this process merely offered one.
            // §FS-rhei-budgets.7.2
            let note = cross_project_note(run, &group.ancestry, &project_label);
            with_claims(|claims| {
                let claim = claims.entry(task_id_str.to_string()).or_insert_with(|| HeldClaim {
                    travel: None,
                    arms: Vec::new(),
                    account: account.uuid().to_string(),
                    ticket: ticket.clone(),
                    bounds: bounds.clone(),
                });
                if claim.travel.is_none() {
                    claim.travel = group.travel_reservation_id.clone();
                }
                claim.arms.extend(group.reservation_ids.iter().map(|reservation| HeldArm {
                    reservation: reservation.clone(),
                    started: false,
                    measured: None,
                }));
            });
            // One arm per admission, so its one reservation names it. §FS-rhei-budgets.6.2.1
            let arm = group.reservation_ids.first().cloned();
            Ok(BudgetAdmission::Admitted { bounds: reports, note, arm })
        }
        Err(refusal) => Ok(BudgetAdmission::Refused {
            halt: budget_halt_text(&refusal, &bounds, &journal),
            event: budget_halt_event(&refusal, &bounds, &journal, task_id_str, state).map(Box::new),
            // A trusted exhaustion can report without reserving or spending.
            // §FS-rhei-budgets.9 §FS-rhei-run-report.3.1
            bounds: if refusal.exhaustion.is_some() {
                budget_reports(&journal, &ticket, &bounds).unwrap_or_default()
            } else {
                Vec::new()
            },
        }),
    }
}

/// How a halt names the project: the directory a reader would recognize, not
/// the uuid that names the account on disk. §FS-rhei-budgets.8
fn budget_project_label(project_root: &Path) -> String {
    project_root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| project_root.display().to_string())
}

/// The descriptor a spawn's own descendants are handed: the arm this spawn was
/// admitted on — its own, not a sibling's — and the account that minted it.
///
/// A descriptor rather than a capability — the ledger decides whether the name
/// buys anything, and a child that cannot be placed under a live ancestor of
/// this very project refuses rather than opening a balance of its own. Both
/// halves or neither, because a reservation without its account is read as this
/// project's and that is only true of the account that minted it.
/// §FS-rhei-budgets.7 §FS-rhei-budgets.7.1 §AR-neural-admission.6 §FS-rhei-budgets.6.2.1
fn budget_ancestry_token(task_id_str: &str, arm: Option<&str>) -> Option<(String, String)> {
    let arm = arm?;
    with_claims(|claims| {
        let claim = claims.get_mut(task_id_str)?;
        let account = claim.account.clone();
        Some((claim.arm_mut(arm)?.reservation.clone(), account))
    })
}

/// Record that the subprocess is being created.
///
/// Appended *immediately before* the spawn call with `confirmed: false`, and
/// refined after it returns. A crash between the two can never refund the
/// invocation; confirmation only refines what was already consumed.
///
/// Only for `arm`, the one being spawned or returned: a fanout sibling's
/// return confirms nothing of an arm still running. §FS-rhei-budgets.6.2.1
fn budget_record_start(
    workspace_root: &Path,
    task_id_str: &str,
    arm: Option<&str>,
    confirmed: bool,
    sink: &Arc<dyn rhei_tui::EventSink>,
) {
    let Some(arm) = arm else { return };
    let held = with_claims(|claims| {
        let claim = claims.get_mut(task_id_str)?;
        claim.arm_mut(arm)?.started = true;
        Some((claim.ticket.clone(), claim.bounds.clone()))
    });
    let Some((ticket, bounds)) = held else { return };
    let Ok(audit) = budget_audit("record a neural start") else { return };
    let project_root = budget_project_root(workspace_root);
    let Ok(Some(account)) = Account::locate(&project_root) else { return };
    let Ok(mut journal) = account.open(true) else { return };
    let _ = journal.record_start(arm, confirmed, &audit);
    emit_budget_account(sink, &journal, &ticket, &bounds);
}

/// Where the account stands now that a receipt is durable, on the same event
/// the admission emits, so every surface that reads it ends at the run's last
/// receipt rather than at its last admission. Best effort like the receipt it
/// follows: an account that cannot be read emits nothing.
/// §FS-rhei-run-json.2.1 §FS-rhei-budgets.9
fn emit_budget_account(
    sink: &Arc<dyn rhei_tui::EventSink>,
    journal: &Journal,
    ticket: &str,
    bounds: &CountBounds,
) {
    if let Ok(bounds) = budget_reports(journal, ticket, bounds) {
        sink.emit(rhei_tui::RunEvent::BudgetSnapshot { bounds });
    }
}
