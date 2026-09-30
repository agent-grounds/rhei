// The ancestry descriptor as this process reads it and says it aloud.
//
// Its own part because everything here is about the *environment* — which two
// variables carry the descriptor, what a refusal should say about where a value
// came from, and how a run tells an operator that the descriptor it inherited
// was somebody else's. None of it is arithmetic: `rhei_core::budget` decides
// what the descriptor buys and deliberately learns no variable name.

// §FS-rhei-budgets.7.1 §FS-rhei-budgets.7.2 §AR-neural-admission.6

/// The reservation a nested `rhei run` inherits from the invocation that
/// started it. §FS-rhei-budgets.7.1
const PARENT_RESERVATION_ENV: &str = "RHEI_BUDGET_PARENT_RESERVATION";

/// The account that minted it, set **together** with the reservation on every
/// agent Rhei starts. Absent is a parent too old to say whose the reservation
/// is, and the reservation is then taken as this project's exactly as it was
/// before this variable existed. §FS-rhei-budgets.7.1
const PARENT_ACCOUNT_ENV: &str = "RHEI_BUDGET_PARENT_ACCOUNT";

/// What a refusal says about where the value came from.
///
/// `no such reservation` alone reads as damaged budget state and sends a reader
/// to the account directory rather than to the environment. The phrase travels
/// to `rhei_core::budget` as data, because the name of a variable this process
/// reads is not something admission may know. §FS-rhei-budgets.7.1
const PARENT_RESERVATION_ORIGIN: &str = "from RHEI_BUDGET_PARENT_RESERVATION";

/// The descriptor this process inherited, owned so it can outlive the read.
/// §FS-rhei-budgets.7.1
struct InheritedAncestry {
    reservation: String,
    account: Option<String>,
}

impl InheritedAncestry {
    /// The two variables as one descriptor. An empty value is no value: an
    /// exporter that sets a variable to nothing has said nothing.
    fn from_environment() -> Option<Self> {
        let read = |key: &str| std::env::var(key).ok().filter(|value| !value.is_empty());
        read(PARENT_RESERVATION_ENV)
            .map(|reservation| Self { reservation, account: read(PARENT_ACCOUNT_ENV) })
    }

    /// The borrowed form admission takes, carrying this process's provenance so
    /// a refusal can name it. §FS-rhei-budgets.7.1
    fn descriptor(&self) -> AncestryDescriptor<'_> {
        AncestryDescriptor {
            reservation: &self.reservation,
            account: self.account.as_deref(),
            origin: Some(PARENT_RESERVATION_ORIGIN),
        }
    }
}

/// Whether this run still owes the operator the cross-project note.
///
/// Once per `rhei run` and not once per admission: the descriptor is a property
/// of the run's environment and does not change while the run lasts, so a plan
/// of thirty tickets repeats it no more than a plan of one.
/// §FS-rhei-budgets.7.2
static CROSS_PROJECT_NOTE_OWED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(true);

/// Arm the note for a run that is beginning.
///
/// A latch cleared at the start of a run rather than a one-shot, because two
/// runs in one process — which is how the in-process tests reach this — are two
/// runs and each owes its own note. §FS-rhei-budgets.7.2
fn begin_budget_run() {
    CROSS_PROJECT_NOTE_OWED.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// Take the note this run owes, if it still owes one.
fn claim_cross_project_note() -> bool {
    CROSS_PROJECT_NOTE_OWED.swap(false, std::sync::atomic::Ordering::SeqCst)
}

/// How the note names the project a descriptor was minted for: the directory
/// this machine's witness index remembers for that account, and the account
/// uuid where it remembers none. §FS-rhei-budgets.7.2 §FS-rhei-budgets.8
fn minting_project_label(account_uuid: &str) -> String {
    match rhei_core::budget::witnessed_root(account_uuid) {
        Ok(Some(root)) => budget_project_label(&root),
        _ => account_uuid.to_string(),
    }
}

/// The note a run owes when the descriptor it inherited was minted elsewhere.
///
/// `None` once this run has already said it. Info rather than a warning:
/// nothing is wrong, and a warning invites someone to fix what is working. One
/// line, because a reader greps for it. §FS-rhei-budgets.7.2
fn cross_project_note(ancestry: &Ancestry, own_label: &str) -> Option<String> {
    let (reservation, account) = ancestry.minted_elsewhere()?;
    if !claim_cross_project_note() {
        return None;
    }
    Some(format!(
        "note: ancestor {reservation} was minted for another project ({}), so this run is \
         admitted against its own account ({own_label}) and is not bounded by that ancestor's \
         envelope or deadline",
        minting_project_label(account)
    ))
}
