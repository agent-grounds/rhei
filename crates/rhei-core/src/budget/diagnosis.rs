//! What a damaged account is, told apart by sub-case, and what the recorded
//! history holds.
//!
//! The classification is the same one `Journal::load` has always made; what is
//! new is that it travels as a value. The caller that **refuses** and the
//! caller that **reports** then render one judgement rather than two, which is
//! what lets `rhei budget show` describe the very state a run was refused for.
//! §FS-rhei-budgets.5.4 §FS-rhei-budgets.10

use super::journal::{replay_chain, Journal, Receipt};
use super::types::BudgetError;
use std::path::{Path, PathBuf};

/// Which of the three damaged sub-cases of §FS-rhei-budgets.5.4 this is.
///
/// A closed vocabulary, because it is what a machine reader gets from
/// `rhei budget show --format json`, and no value may appear here that
/// §FS-rhei-budgets.10 has not named. §FS-rhei-budgets.5.4
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Damage {
    /// The journal file is not there at all — the one sub-case where a lost
    /// journal and a reused path leave identical state.
    JournalAbsent,
    /// A journal exists but holds fewer receipts than the witness.
    JournalTruncated,
    /// A journal exists and its identity, sequence or hash chain does not
    /// verify.
    ChainBroken,
}

impl Damage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::JournalAbsent => "journal_absent",
            Self::JournalTruncated => "journal_truncated",
            Self::ChainBroken => "chain_broken",
        }
    }

    /// Whether the journal that is left is this project's own, which is what
    /// decides the remedy: a present journal lost its tail and the witness
    /// restores it, an absent one says nothing about whose it was.
    /// §FS-rhei-budgets.5.4
    pub fn journal_exists(self) -> bool {
        !matches!(self, Self::JournalAbsent)
    }
}

impl std::fmt::Display for Damage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A classified damaged account, carrying the refusal its sub-case raises.
///
/// The refusal travels with the classification so that a caller which only
/// wanted to open the account gets exactly the words it got before this split
/// existed. §FS-rhei-budgets.5.4
pub(crate) struct Damaged {
    pub(crate) damage: Damage,
    pub(crate) error: BudgetError,
}

/// What the committed history holds, for a report that has no journal to read.
///
/// Replayed through the journal's own verifier rather than a second parser:
/// the witness is byte-identical to a journal, so whatever reads one reads the
/// other. A witness that does not itself replay still reports its line count,
/// because a report that refused to describe a broken witness would leave the
/// operator with nothing at all. §FS-rhei-budgets.5.3
#[derive(Clone, Debug, Default)]
pub struct History {
    pub receipts: usize,
    pub first_written_at: Option<String>,
    pub last_written_at: Option<String>,
    /// Invocations the recorded history charges to this path.
    pub invocations: u64,
}

impl History {
    pub(crate) fn read(project_id: &str, bytes: &[u8]) -> Self {
        let lines = bytes.split(|byte| *byte == b'\n').filter(|line| !line.is_empty()).count();
        let Ok(replayed) = replay_chain(project_id, bytes) else {
            return Self { receipts: lines, ..Self::default() };
        };
        Self {
            receipts: replayed.receipts.len(),
            first_written_at: written_at(replayed.receipts.first()),
            last_written_at: written_at(replayed.receipts.last()),
            invocations: replayed.state.lifetime_counter().map(|c| c.consumed).unwrap_or(0),
        }
    }

    /// `4 receipts`, singular where there is one, because a report that says
    /// `1 receipts` reads as a template nobody finished. §FS-rhei-budgets.10
    pub fn receipts_phrase(&self) -> String {
        match self.receipts {
            1 => "1 receipt".to_string(),
            n => format!("{n} receipts"),
        }
    }

    /// `4 receipts, first …, last …, charging 3 invocations to this path`.
    pub fn summary(&self) -> String {
        let mut written = self.receipts_phrase();
        if let Some(first) = &self.first_written_at {
            written.push_str(&format!(", first {first}"));
        }
        if let Some(last) = &self.last_written_at {
            written.push_str(&format!(", last {last}"));
        }
        written.push_str(&format!(", charging {} invocations to this path", self.invocations));
        written
    }
}

fn written_at(receipt: Option<&Receipt>) -> Option<String> {
    receipt.map(|receipt| receipt.written_at.clone())
}

/// Everything a damaged account can be described by without opening it.
///
/// This is what `rhei budget show` renders and what `rhei budget forget` acts
/// on, so the two can never disagree about which sub-case they are looking at.
/// §FS-rhei-budgets.10
#[derive(Clone, Debug)]
pub struct Diagnosis {
    pub damage: Damage,
    /// The project root, in the spelling the witness index is keyed by.
    pub root: PathBuf,
    pub uuid: String,
    pub journal: PathBuf,
    pub witness: PathBuf,
    pub history: History,
}

impl Diagnosis {
    pub(crate) fn new(
        damage: Damage,
        root: &Path,
        uuid: &str,
        witness: &Path,
        history: History,
    ) -> Self {
        Self {
            damage,
            root: root.to_path_buf(),
            uuid: uuid.to_string(),
            journal: journal_path(root, uuid),
            witness: witness.to_path_buf(),
            history,
        }
    }

    /// The command that restores this project's own journal from the witness.
    ///
    /// Offered in full rather than run: it is a copy the tool cannot vouch for,
    /// because whether this history is this project's is the very thing that is
    /// unknown. §FS-rhei-budgets.5.4
    pub fn restore_command(&self) -> String {
        format!("cp {} {}", self.witness.display(), self.journal.display())
    }

    /// The command that retires the record, for the reading where a different
    /// project held the path before this one.
    ///
    /// `None` wherever a journal is present, because there the journal **is**
    /// this project's and retiring the root would discard a real account.
    /// §FS-rhei-budgets.5.4
    pub fn forget_command(&self) -> Option<String> {
        (!self.damage.journal_exists())
            .then(|| format!("rhei budget forget {} --reason <TEXT>", self.root.display()))
    }

    /// One line saying what is wrong, in the words the refusal uses.
    pub fn headline(&self) -> String {
        match self.damage {
            Damage::JournalAbsent => "this project has no budget journal".to_string(),
            Damage::JournalTruncated => {
                "this project's journal holds fewer receipts than the committed history".to_string()
            }
            Damage::ChainBroken => {
                "this project's journal does not verify against the committed history".to_string()
            }
        }
    }
}

/// What an inspection of a project's account found.
///
/// Three arms rather than a `Result`, because a damaged account is an answer
/// `show` was asked for rather than a failure to answer. §FS-rhei-budgets.10
#[derive(Debug)]
pub enum Inspection {
    /// A verified account, with its read-only transaction still held.
    Verified(Box<Journal>),
    Damaged(Box<Diagnosis>),
    /// Neither a journal nor a witness: the lawful **absent** state, which the
    /// next admission establishes silently. §FS-rhei-budgets.5.4
    Absent,
}

/// What a retirement recorded, for the line the command prints.
/// §FS-rhei-budgets.10
#[derive(Clone, Debug)]
pub struct Retired {
    pub uuid: String,
    pub root: PathBuf,
    pub kept_at: PathBuf,
    pub damage: Damage,
    pub history: History,
}

/// The journal path an account of this uuid has under this root, whether or
/// not anything is there.
pub(crate) fn journal_path(root: &Path, uuid: &str) -> PathBuf {
    root.join(super::account::ACCOUNT_DIR).join(uuid).join("journal.jsonl")
}

/// The refusal a wholly absent journal raises.
///
/// Only this sub-case is new. Where a journal is present its tail is what was
/// lost, so the other two keep the words they have always had — deliberately,
/// because restoring from the witness is still right there. The reason code
/// stays `untrustworthy_ledger` throughout, so a harness routing on the stable
/// identifier is not broken by any of this. §FS-rhei-budgets.5.4
pub(crate) fn absent_journal_refusal(witness: &Path, history: &History) -> BudgetError {
    let last = history
        .last_written_at
        .as_deref()
        .map(|at| format!(", the last on {at}"))
        .unwrap_or_default();
    BudgetError::new(
        "untrustworthy_ledger",
        format!(
            "this project has no budget journal, but a committed history at {} records {} \
             for this path{last}",
            witness.display(),
            history.receipts_phrase(),
        ),
    )
}
