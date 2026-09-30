//! Whose ancestor a nested run claims, and what admission made of the claim.
//!
//! A reservation name says *which* reservation and never *whose*, and an
//! ancestor is meaningful only in the journal that holds it: the same name in
//! another project's ledger is not a dead ancestor, it is somebody else's live
//! one. These two types are the descriptor a caller offers and the answer it
//! gets back, kept beside each other and apart from the arithmetic they feed.
//! §FS-rhei-budgets.7.1 §AR-neural-admission.6

/// Whose ancestor, and not only which one.
///
/// The minting account is what lets a child tell a dead ancestor of its own
/// project from a live one of somebody else's, and `origin` is how a caller
/// lends its own provenance to a refusal without this layer learning where any
/// caller reads its values from.
/// §FS-rhei-budgets.7.1 §AR-neural-admission.6
#[derive(Clone, Copy, Debug)]
pub struct AncestryDescriptor<'a> {
    /// The reservation the caller claims as its ancestor.
    pub reservation: &'a str,
    /// The account that minted `reservation`, where the caller knows it. Absent
    /// is "take it as this project's", which is what a caller too old to say
    /// whose it is gets, and it must lose nothing by it. §FS-rhei-budgets.7.1
    pub account: Option<&'a str>,
    /// A phrase naming where the value came from, interpolated into a refusal
    /// so that `no such reservation` does not read as damaged budget state.
    /// Absent leaves the refusal as it was. §FS-rhei-budgets.7.1
    pub origin: Option<&'a str>,
}

/// What admission decided about the ancestry a caller claimed.
///
/// Returned rather than folded away because the receipt must record what was
/// **decided** and not what was asked, and because a caller owes the operator a
/// note about the one outcome nobody asked for.
/// §FS-rhei-budgets.7.1 §FS-rhei-budgets.7.2
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ancestry {
    /// No descriptor was offered: an ordinary unparented admission.
    Unclaimed,
    /// A descriptor of this very account, resolved to a live ancestor that
    /// permits `envelope` descendant invocations. §FS-rhei-budgets.7.1
    Placed { reservation: String, envelope: u64 },
    /// A descriptor minted for another account. It names no ancestor here, so
    /// the admission is unparented, draws against no envelope, and is said
    /// aloud once by whoever runs it. §FS-rhei-budgets.7.2
    Elsewhere { reservation: String, account: String },
}

impl Ancestry {
    /// The parent a receipt records: what was decided, never what was asked. An
    /// admission the identity test downgraded names no parent even where this
    /// journal happens to hold that name, or replay would later read the chain
    /// as corrupt. §FS-rhei-budgets.7.2 §AR-neural-admission.6
    pub fn parent(&self) -> Option<&str> {
        match self {
            Self::Placed { reservation, .. } => Some(reservation),
            Self::Unclaimed | Self::Elsewhere { .. } => None,
        }
    }

    /// The descriptor this admission declined to be a descendant of, as
    /// `(reservation, minting account)`. §FS-rhei-budgets.7.2
    pub fn minted_elsewhere(&self) -> Option<(&str, &str)> {
        match self {
            Self::Elsewhere { reservation, account } => Some((reservation, account)),
            Self::Unclaimed | Self::Placed { .. } => None,
        }
    }
}
