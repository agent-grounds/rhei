// The admission checkpoint before a spawn, and the claim it holds for the
// visit it paid for.
//
// Its own part because everything here is *plumbing* — finding the project,
// finding the ticket's identity, deciding what an audit record says — while the
// arithmetic it plumbs into lives in `rhei_core::budget`. The travel charge on
// the shared transition path is the part next door; adding a third call site is
// how this boundary stops being one.

// §AR-source-file-size.3 §FS-rhei-budgets.6 §AR-neural-admission.7

use rhei_core::budget::{Account, AdmissionRequest, Arm, Audit, BudgetError, Journal};

/// Where a ticket's budget identity is persisted.
///
/// `rhei reset` deletes `stateVisits`, `providerLimits` and the supervision
/// block by name; this key is deliberately not in that list, because removing
/// it would hand the ticket a fresh travel history.
/// §FS-rhei-reset.2 §FS-rhei-budgets.4.1
const BUDGET_TICKET_KEY: &str = "budgetTicketId";

/// The ancestry token a nested `rhei run` inherits from the invocation that
/// started it. §FS-rhei-budgets.7
const PARENT_RESERVATION_ENV: &str = "RHEI_BUDGET_PARENT_RESERVATION";

/// What an admission decided, in the three shapes the caller acts on.
enum BudgetAdmission {
    /// Every unit is reserved and the spawn may proceed.
    Admitted,
    /// A bound, or an account that cannot be trusted, refused the spawn. The
    /// text is the whole halt of §FS-rhei-budgets.8, ready to print.
    Refused { halt: String },
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
struct HeldClaim {
    travel: Option<String>,
    arms: Vec<String>,
    /// Whether a start has been recorded for this claim. Until one has, the
    /// engine's own knowledge that it never spawned *is* the proof of
    /// non-start; after one has, nothing refunds it. §FS-rhei-budgets.6.2
    started: bool,
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

fn budget_result<T>(result: Result<T, BudgetError>) -> MietteResult<T> {
    result.map_err(|err| miette!(help = budget_inspect_help(), "{}", err.message))
}

fn budget_inspect_help() -> &'static str {
    "inspect the project's account with: rhei budget show"
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

/// The ticket's persistent identity, minted and written into the plan's
/// metadata the first time it is needed.
///
/// It is written under the owning rhei's metadata lock, which is why the lock
/// order puts metadata *above* the account: assigning an identity that does not
/// exist yet takes that lock, and an ordinary admission takes neither.
/// §AR-neural-admission.3
fn ticket_budget_uuid(input: &Path, loaded: &LoadedPlan, task_id_str: &str) -> MietteResult<String> {
    let route = loaded.task_route(task_id_str, input);
    let metadata_id = parse_task_id(&route.metadata_id);
    let lock = LockedPlanFile::open(&route.metadata_file)?;
    let raw = lock.read_to_string("failed to read plan metadata file")?;
    let on_disk = parse_metadata_from_raw(&route.metadata_file, &raw)?;
    if let Some(existing) = task_metadata_map(on_disk.as_ref(), &metadata_id)
        .and_then(|task| task.get(yaml_key(BUDGET_TICKET_KEY)))
        .and_then(YamlValue::as_str)
    {
        return Ok(existing.to_string());
    }
    let minted = uuid::Uuid::new_v4().to_string();
    let mut root = on_disk.unwrap_or_default();
    let metadata_section = ensure_mapping(&mut root, yaml_key("metadata"));
    let tasks = ensure_mapping(metadata_section, yaml_key("tasks"));
    let task_entry = ensure_mapping(tasks, task_id_yaml_key(&metadata_id));
    task_entry.insert(yaml_key(BUDGET_TICKET_KEY), YamlValue::String(minted.clone()));
    let rewritten = rewrite_frontmatter(&raw, &root)?;
    write_file_atomic_locked(&route.metadata_file, &rewritten, Some(&lock))?;
    Ok(minted)
}

/// Whether the run that owns a reservation is gone, which is the proof of
/// non-start: its execution-root lock can be acquired, so nothing holds it.
/// §FS-rhei-budgets.6.2
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
/// the ticket and one invocation unit per arm, append one reservation carrying
/// every unit — or refuse and append nothing. Only then may the caller create
/// the subprocess. §FS-rhei-budgets.6.1 §FS-rhei-run.3.4
#[allow(clippy::too_many_arguments)]
fn budget_admit_spawn(
    input: &Path,
    loaded: &LoadedPlan,
    workspace_root: &Path,
    machine: &rhei_validator::StateMachine,
    settings: &RheiSettings,
    task: &rhei_core::ast::Task,
    task_id_str: &str,
) -> MietteResult<BudgetAdmission> {
    let project_root = budget_project_root(workspace_root);
    let bounds = resolve_count_bounds(settings, node_transition_limit(machine, Some(task)));
    // The identity is assigned under the metadata lock, before the account is
    // opened: metadata sits above the account in the lock order.
    // §AR-neural-admission.3
    let ticket_uuid = ticket_budget_uuid(input, loaded, task_id_str)?;
    let route = loaded.task_route(task_id_str, input);
    let Some(mut journal) = open_project_account(&project_root, "admit a neural start")? else {
        return Ok(BudgetAdmission::NotAccounted);
    };
    let account = budget_result(Account::locate(&project_root))?.ok_or_else(|| {
        miette!(help = budget_inspect_help(), "the account just established has no identity")
    })?;
    let ticket = account.ticket_identity(&ticket_uuid);
    let audit = budget_audit("admit a neural start")?;
    // A fanout reaches this checkpoint as several work items rather than as
    // one group, so the ticket's single travel unit goes to whichever arm of a
    // visit arrives first and every later arm of the same visit takes an
    // invocation unit alone. One applied edge, however many arms.
    // §FS-rhei-budgets.4.2
    let travel = with_claims(|claims| !claims.contains_key(task_id_str));
    let attempts = vec![format!("attempt:{}", uuid::Uuid::new_v4())];
    // A descendant envelope of one lets a nested `rhei run` this invocation
    // starts draw a single admission through the ancestry path, which is the
    // one door through which anything a program does is counted.
    // §FS-rhei-budgets.7
    let arms: Vec<Arm<'_>> = attempts
        .iter()
        .map(|attempt| Arm { attempt_identity: attempt, descendant_envelope: 1 })
        .collect();
    let execution_root = workspace_root.to_string_lossy().into_owned();
    let project_label = budget_project_label(&project_root);
    let parent = nested_parent_reservation();
    let admitted = (|| -> Result<_, BudgetError> {
        journal.bind_ticket(&ticket, task_id_str, &route.metadata_file, &audit)?;
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
            parent_reservation: parent.as_deref(),
            travel,
        };
        journal.reserve(&request, bounds.effective(), &audit)
    })();
    match admitted {
        Ok(group) => {
            with_claims(|claims| {
                let claim = claims.entry(task_id_str.to_string()).or_insert_with(|| HeldClaim {
                    travel: None,
                    arms: Vec::new(),
                    started: false,
                });
                if claim.travel.is_none() {
                    claim.travel = group.travel_reservation_id.clone();
                }
                claim.arms.extend(group.reservation_ids.clone());
            });
            Ok(BudgetAdmission::Admitted)
        }
        Err(refusal) => {
            Ok(BudgetAdmission::Refused { halt: budget_halt_text(&refusal, &bounds, &journal) })
        }
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

fn nested_parent_reservation() -> Option<String> {
    std::env::var(PARENT_RESERVATION_ENV).ok().filter(|value| !value.is_empty())
}

/// Record that the subprocess is being created.
///
/// Appended *immediately before* the spawn call with `confirmed: false`, and
/// refined after it returns. A crash between the two can never refund the
/// invocation; confirmation only refines what was already consumed.
/// §FS-rhei-budgets.6.2
fn budget_record_start(workspace_root: &Path, task_id_str: &str, confirmed: bool) {
    let arms = with_claims(|claims| {
        claims.get_mut(task_id_str).map(|claim| {
            claim.started = true;
            claim.arms.clone()
        })
    });
    let Some(arms) = arms else { return };
    let Ok(audit) = budget_audit("record a neural start") else { return };
    let project_root = budget_project_root(workspace_root);
    let Ok(Some(account)) = Account::locate(&project_root) else { return };
    let Ok(mut journal) = account.open(true) else { return };
    for arm in &arms {
        let _ = journal.record_start(arm, confirmed, &audit);
    }
}

/// End the visit this claim paid for.
///
/// A travel unit still held is one no edge was applied against — a poll wait, a
/// completion that selected nothing, a stall — and it goes back. An invocation
/// unit goes back only when no start was ever recorded, which is this engine
/// knowing it never created the subprocess; once a start exists the unit is
/// consumed, ambiguous or not. §FS-rhei-budgets.4.1 §FS-rhei-budgets.6.2
fn budget_settle_visit(workspace_root: &Path, task_id_str: &str) {
    let Some(claim) = with_claims(|claims| claims.remove(task_id_str)) else { return };
    if claim.travel.is_none() && claim.started {
        return;
    }
    // Best effort by design: a visit that ends while the account cannot be
    // opened leaves the unit outstanding, which is the conservative direction.
    // Failing the run over a unit nobody spent would be the other one.
    let Ok(audit) = budget_audit("end a visit") else { return };
    let project_root = budget_project_root(workspace_root);
    let Ok(Some(account)) = Account::locate(&project_root) else { return };
    let Ok(mut journal) = account.open(true) else { return };
    if claim.started {
        if let Some(travel) = claim.travel {
            let _ = journal.release_travel(&travel, &audit);
        }
        return;
    }
    for arm in &claim.arms {
        let _ = journal.release_unstarted(arm, &audit);
    }
}
