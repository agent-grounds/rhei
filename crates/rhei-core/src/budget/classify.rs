//! Which of the sub-cases of §FS-rhei-budgets.5.4 a root's journal and witness
//! are, decided in one place.
//!
//! Three callers ask it: opening the journal, opening with the account
//! directory gone, and `rhei budget forget` before it retires a root. Each used
//! to carry its own copy of the rule, so a witness that ends mid-receipt was
//! read as a lost tail by all three alike and offered the copy that carries the
//! fragment into the journal. Each now hands this the bytes it read under the
//! authority lock and renders the one reading that comes back.
//! §FS-rhei-budgets.5.4 §FS-rhei-budgets.5.4.3

use super::diagnosis::{foreign_tail, journal_path, Damage, Diagnosis, History, Torn};
use super::journal::{replay_chain, Replayed};
use super::types::BudgetError;
use std::path::Path;

/// One root's two files, as read under the authority lock.
pub(crate) struct Sides<'a> {
    /// The resolved project root, because a foreign tail is told by whether a
    /// source path lies under it, and sources are recorded resolved.
    /// §FS-rhei-budgets.5.4.2
    pub(crate) root: &'a Path,
    pub(crate) uuid: &'a str,
    pub(crate) witness_path: &'a Path,
    pub(crate) witness: &'a [u8],
    /// `None` where the journal file is not there at all.
    pub(crate) journal: Option<&'a [u8]>,
}

/// What the two files are, read together.
pub(crate) enum Reading {
    /// No receipt on either side: the lawful **absent** state, which the next
    /// charge establishes. §FS-rhei-budgets.5.4
    Absent,
    /// The journal replays and the witness is the same bytes, or there is no
    /// witness yet and the caller adopts the journal. §FS-rhei-budgets.5.4
    Verified(Replayed),
    /// A damaged account, with the refusal its sub-case raises, so the caller
    /// that refuses and the one that reports render one judgement.
    Damaged(Box<Diagnosis>, BudgetError),
}

/// The witness up to the end of its last whole receipt.
///
/// What follows is a fragment, whether or not it parses: a complete JSON object
/// without its newline is an append that did not finish. §FS-rhei-budgets.5.4.3
pub(crate) fn whole_receipts(witness: &[u8]) -> &[u8] {
    let end = witness.iter().rposition(|byte| *byte == b'\n').map_or(0, |at| at + 1);
    &witness[..end]
}

/// Decide the sub-case, in the precedence §FS-rhei-budgets.5.4.3 gives:
/// `held_elsewhere` is settled before the journal is opened, so it is not here;
/// then `foreign_tail`, whose set-aside already moves a fragment away; then
/// `witness_torn`; then the three that offer the copy, which a fragment would
/// carry into the journal. Among those three the order is the one
/// §FS-rhei-budgets.5.4 has always had.
pub(crate) fn classify(sides: &Sides) -> Reading {
    let whole = whole_receipts(sides.witness).len();
    let torn = (whole < sides.witness.len())
        .then(|| Torn { whole: whole as u64, fragment: (sides.witness.len() - whole) as u64 });
    let Some(journal) = sides.journal else {
        return match torn {
            Some(torn) => sides.torn(torn, false),
            None if sides.witness.is_empty() => Reading::Absent,
            None => {
                let diagnosis = sides.diagnosis(Damage::JournalAbsent);
                let error = absent_journal_refusal(sides.witness_path, &diagnosis.history);
                Reading::Damaged(Box::new(diagnosis), error)
            }
        };
    };
    let project_id = format!("panta:{}", sides.uuid);
    // Verified on its own terms first, so an adopted journal is one this
    // machine has checked rather than one it merely inherited.
    let replayed =
        if journal.is_empty() { Ok(None) } else { replay_chain(&project_id, journal).map(Some) };
    if replayed.is_ok() {
        if let Some(foreign) = foreign_tail(sides.root, journal, sides.witness) {
            let diagnosis = sides.diagnosis(Damage::ForeignTail).with_foreign(Some(foreign));
            let error = foreign_tail_refusal(&diagnosis);
            return Reading::Damaged(Box::new(diagnosis), error);
        }
    }
    if let Some(torn) = torn {
        return sides.torn(torn, true);
    }
    match replayed {
        Err(error) => sides.damaged(Damage::ChainBroken, error),
        Ok(None) if sides.witness.is_empty() => Reading::Absent,
        Ok(None) => sides.damaged(Damage::JournalTruncated, sides.ahead_refusal()),
        Ok(Some(replayed)) if sides.witness.is_empty() || sides.witness == journal => {
            Reading::Verified(replayed)
        }
        // A journal the witness merely continues lost its tail; one that
        // diverges was rolled back or edited, which is the chain's own failure
        // however it was spelled. §FS-rhei-budgets.5.4
        Ok(Some(_)) if sides.witness.starts_with(journal) => {
            sides.damaged(Damage::JournalTruncated, sides.mismatch_refusal())
        }
        Ok(Some(_)) => sides.damaged(Damage::ChainBroken, sides.mismatch_refusal()),
    }
}

impl Sides<'_> {
    /// What the recorded history holds is read from the witness's whole
    /// receipts alone. §FS-rhei-budgets.5.4.3
    fn diagnosis(&self, damage: Damage) -> Diagnosis {
        let history = History::read(&format!("panta:{}", self.uuid), self.witness);
        Diagnosis::new(damage, self.root, self.uuid, self.witness_path, history)
    }

    fn damaged(&self, damage: Damage, error: BudgetError) -> Reading {
        Reading::Damaged(Box::new(self.diagnosis(damage)), error)
    }

    /// `witness_torn`, with the journal there or not: only that decides
    /// whether `forget` is offered beside the trim. §FS-rhei-budgets.5.4.3
    fn torn(&self, torn: Torn, journal_present: bool) -> Reading {
        let diagnosis = self.diagnosis(Damage::WitnessTorn).with_tear(torn, journal_present);
        let error = torn_refusal(&diagnosis);
        Reading::Damaged(Box::new(diagnosis), error)
    }

    /// The words for a witness that knows of receipts an empty journal does not
    /// have, unchanged: copying the witness back restores this project's own
    /// history. §FS-rhei-budgets.5.4
    fn ahead_refusal(&self) -> BudgetError {
        BudgetError::corrupt(format!(
            "the committed history at {} records receipts the project journal at {} \
             does not have; restore the journal by copying the witness back over it",
            self.witness_path.display(),
            journal_path(self.root, self.uuid).display()
        ))
    }

    /// The words for a journal that is not the witness's bytes, unchanged.
    /// §FS-rhei-budgets.5.4
    fn mismatch_refusal(&self) -> BudgetError {
        BudgetError::corrupt(format!(
            "the committed history at {} does not match the project journal at {}; \
             restore the journal by copying the witness back over it",
            self.witness_path.display(),
            journal_path(self.root, self.uuid).display()
        ))
    }
}

/// The refusal a witness that ends mid-receipt raises: the witness, how many
/// bytes follow its last whole receipt, and the trim — never the copy, which
/// would write the fragment into the journal. Where the journal is wholly
/// absent, `forget` is offered beside it. §FS-rhei-budgets.5.4.3
fn torn_refusal(diagnosis: &Diagnosis) -> BudgetError {
    let forget = diagnosis
        .forget_command()
        .map(|forget| {
            format!("; if the path was reused, retire that record instead with: {forget}")
        })
        .unwrap_or_default();
    BudgetError::corrupt(format!(
        "the committed history at {} ends mid-receipt, {}, the fragment of an append that was \
         cut short; copying it anywhere would carry the fragment along, so keep it and cut it \
         back to its last whole receipt with: {}{forget}",
        diagnosis.witness.display(),
        diagnosis.fragment_phrase().unwrap_or_default(),
        diagnosis.trim_command().unwrap_or_default(),
    ))
}

/// The refusal a tail written from another root raises: it names that root and
/// the receipts, and offers setting the witness aside — never the copy.
/// §FS-rhei-budgets.5.4.2
fn foreign_tail_refusal(diagnosis: &Diagnosis) -> BudgetError {
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
/// Where a journal is present its tail is what was lost, so the sub-cases that
/// offer the copy keep the words they have always had — deliberately, because
/// restoring from the witness is still right there. The reason code stays
/// `untrustworthy_ledger` throughout, so a harness routing on the stable
/// identifier is not broken by any of this. §FS-rhei-budgets.5.4
fn absent_journal_refusal(witness: &Path, history: &History) -> BudgetError {
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
