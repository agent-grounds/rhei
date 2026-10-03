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

/// Which of the five damaged sub-cases of §FS-rhei-budgets.5.4 this is.
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
    /// The uuid this root presents is bound to another root that still holds
    /// it: this root is a copy. §FS-rhei-budgets.5.4.1
    HeldElsewhere,
    /// The witness runs past the journal and that tail was written from
    /// another root. §FS-rhei-budgets.5.4.2
    ForeignTail,
}

impl Damage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::JournalAbsent => "journal_absent",
            Self::JournalTruncated => "journal_truncated",
            Self::ChainBroken => "chain_broken",
            Self::HeldElsewhere => "held_elsewhere",
            Self::ForeignTail => "foreign_tail",
        }
    }

    /// Whether a journal file is left at this root, which is what decides the
    /// remedy: a present journal bound to this root lost its tail and the
    /// witness restores it, an absent one says nothing about whose it was. A
    /// copy's and a contaminated original's journals are present too, and
    /// [`Diagnosis::restore_command`] withholds the restore from both.
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
    /// Where the tail came from, under `foreign_tail`. §FS-rhei-budgets.5.4.2
    pub(crate) foreign: Option<Foreign>,
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
    /// The root that still holds this account, under `held_elsewhere`.
    /// §FS-rhei-budgets.5.4.1
    pub held_by: Option<PathBuf>,
    /// The root the witness's tail was written from, under `foreign_tail`.
    /// §FS-rhei-budgets.5.4.2
    pub foreign: Option<Foreign>,
}

/// Where a witness's tail came from, when it was not this root.
/// §FS-rhei-budgets.5.4.2
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Foreign {
    /// The project root the tail's `identity` receipt names a source under.
    pub root: PathBuf,
    /// How many receipts the witness holds past this root's journal.
    pub receipts: usize,
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
            held_by: None,
            foreign: None,
        }
    }

    /// The same diagnosis, carrying the root that still holds the account.
    /// §FS-rhei-budgets.5.4.1
    pub(crate) fn held_by(mut self, holder: &Path) -> Self {
        self.held_by = Some(holder.to_path_buf());
        self
    }

    /// The same diagnosis, carrying where the witness's tail came from.
    /// §FS-rhei-budgets.5.4.2
    pub(crate) fn with_foreign(mut self, foreign: Option<Foreign>) -> Self {
        self.foreign = foreign;
        self
    }

    /// The account directory this project's journal belongs in, whether or not
    /// anything is there.
    ///
    /// Spelled the way a verified report spells `account`, so a reader of one
    /// member across both states of `show` gets one kind of value.
    /// §FS-rhei-budgets.10
    pub fn directory(&self) -> PathBuf {
        self.journal.parent().expect("a journal path has a parent").to_path_buf()
    }

    /// The identity this account's receipts are written under, the same
    /// `panta:<uuid>` a verified report carries. §FS-rhei-budgets.10
    pub fn project_id(&self) -> String {
        format!("panta:{}", self.uuid)
    }

    /// The command that restores this project's own journal from the witness.
    ///
    /// Offered in full rather than run: it is a copy the tool cannot vouch for,
    /// because whether this history is this project's is the very thing that is
    /// unknown. Where the journal is wholly absent so is the directory that
    /// would hold it — `Account::inspect` reaches that sub-case precisely when
    /// it does not exist — so the copy carries the one directory creation it
    /// needs. A command a refusal names has to run in the state it is named
    /// for, which is the whole reason `show` is readable there at all.
    ///
    /// Both halves come from `platform`, so the line is in the operator's own
    /// shell and every path in it is one word: `cmd` has neither `cp` nor
    /// `mkdir -p`, and unquoted, a project path holding a space makes `mkdir -p`
    /// read two words and build a directory tree relative to wherever the
    /// operator was standing — a remedy that fails *and* writes somewhere it
    /// did not name. §FS-rhei-budgets.5.4 §FS-rhei-budgets.10
    ///
    /// `None` where the witness is not this root's to restore from: a copy's
    /// witness is the holder's, and a tail written from another root would
    /// import transitions and spend this project never made.
    /// §FS-rhei-budgets.5.4.1 §FS-rhei-budgets.5.4.2
    pub fn restore_command(&self) -> Option<String> {
        let copy = crate::platform::copy_command(&self.witness, &self.journal);
        match self.damage {
            Damage::JournalAbsent => Some(format!(
                "{} && {copy}",
                crate::platform::make_directory_command(&self.directory())
            )),
            Damage::JournalTruncated | Damage::ChainBroken => Some(copy),
            Damage::HeldElsewhere | Damage::ForeignTail => None,
        }
    }

    /// The command that moves a contaminated witness aside under a kept name,
    /// keeping every byte; the next charge then adopts this project's own
    /// journal. Only under `foreign_tail`. §FS-rhei-budgets.5.4.2
    pub fn set_aside_command(&self) -> Option<String> {
        (self.damage == Damage::ForeignTail)
            .then(|| crate::platform::move_command(&self.witness, &self.set_aside_path()))
    }

    /// `history-set-aside-<stamp>.jsonl` beside the witness, stamped with its
    /// last receipt, so the name says which history it was and never spells a
    /// witness a lookup reads. §FS-rhei-budgets.5.4.2
    fn set_aside_path(&self) -> PathBuf {
        let stamp: String = self
            .history
            .last_written_at
            .as_deref()
            .unwrap_or("unknown")
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect();
        self.witness.with_file_name(format!("history-set-aside-{stamp}.jsonl"))
    }

    /// The command that retires the record, for the reading where a different
    /// project held the path before this one.
    ///
    /// `None` wherever a journal is present, because there the journal **is**
    /// this project's and retiring the root would discard a real account. The
    /// root is quoted for the same reason the restore's paths are: unquoted, a
    /// path with a space reaches `clap` as two arguments and the command is
    /// rejected before it runs. §FS-rhei-budgets.5.4 §FS-rhei-budgets.10
    ///
    /// A copy is the one present journal it is offered for: the account is the
    /// holder's, so retiring this root's claim gives up nothing of this
    /// project's. §FS-rhei-budgets.5.4.1
    pub fn forget_command(&self) -> Option<String> {
        matches!(self.damage, Damage::JournalAbsent | Damage::HeldElsewhere)
            .then(|| super::roots::forget_line(&self.root))
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
            Damage::HeldElsewhere => format!(
                "budget account panta:{} belongs to another project root{}",
                self.uuid,
                self.held_by
                    .as_ref()
                    .map(|holder| format!(", {}, which still holds it", holder.display()))
                    .unwrap_or_default()
            ),
            Damage::ForeignTail => format!(
                "the committed history runs past this project's journal with receipts written \
                 from another project root{}",
                self.foreign
                    .as_ref()
                    .map(|foreign| format!(", {}", foreign.root.display()))
                    .unwrap_or_default()
            ),
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

/// Where the witness's tail beyond `journal` was written from another root.
///
/// The tail is foreign when it holds an `identity` receipt whose `source_path`
/// lies outside `root`: identity receipts are the only ones that carry a path,
/// so a tail with none cannot be told apart from one this project lost, and it
/// stays `journal_truncated`. The other root is the nearest ancestor of that
/// source that holds an account directory, or the source's own directory where
/// none does any longer. §FS-rhei-budgets.5.4.2
pub(crate) fn foreign_tail(root: &Path, journal: &[u8], witness: &[u8]) -> Option<Foreign> {
    if witness.len() <= journal.len() || !witness.starts_with(journal) {
        return None;
    }
    let tail = &witness[journal.len()..];
    let receipts = tail.split(|byte| *byte == b'\n').filter(|line| !line.is_empty());
    let source = receipts.clone().find_map(|line| {
        let receipt: serde_json::Value = serde_json::from_slice(line).ok()?;
        if receipt["kind"] != "identity" {
            return None;
        }
        let source =
            crate::platform::plain_path(PathBuf::from(receipt["payload"]["source_path"].as_str()?));
        (!source.starts_with(root)).then_some(source)
    })?;
    Some(Foreign { root: holding_root(&source), receipts: receipts.count() })
}

/// The project root a ticket source sits under: the nearest ancestor holding an
/// account directory, else the directory the source is in.
fn holding_root(source: &Path) -> PathBuf {
    let parent = source.parent().unwrap_or(source);
    parent
        .ancestors()
        .find(|ancestor| ancestor.join(super::account::ACCOUNT_DIR).is_dir())
        .unwrap_or(parent)
        .to_path_buf()
}

/// The refusal a tail written from another root raises: it names that root and
/// the receipts, and offers setting the witness aside — never the copy.
/// §FS-rhei-budgets.5.4.2
pub(crate) fn foreign_tail_refusal(diagnosis: &Diagnosis) -> BudgetError {
    let foreign = diagnosis.foreign.as_ref().expect("a foreign tail names its root");
    BudgetError::new(
        "untrustworthy_ledger",
        format!(
            "budget journal is untrustworthy: the committed history at {} holds {} past the \
             project journal at {}, written from another project root, {}; copying the witness \
             back would import them, so set the witness aside instead, keeping every byte: {}",
            diagnosis.witness.display(),
            match foreign.receipts {
                1 => "1 receipt".to_string(),
                n => format!("{n} receipts"),
            },
            diagnosis.journal.display(),
            foreign.root.display(),
            diagnosis.set_aside_command().unwrap_or_default(),
        ),
    )
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
