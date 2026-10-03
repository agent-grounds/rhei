//! The witness index, and the check every resolution makes against it: which
//! canonical root **holds** each account uuid on this machine.
//!
//! Its own part because the index is no longer only read. A root presenting a
//! uuid is compared with the root the index binds it to, and the answer is one
//! of four readings — home, adopted, a copy, a move — of which one refuses and
//! two write the index. §FS-rhei-budgets.5.3 §FS-rhei-budgets.5.4.1
//! §AR-neural-admission.4

use super::types::BudgetError;
use super::Result;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// What the index says about a uuid presented from one root.
/// §FS-rhei-budgets.5.4.1
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Reading {
    /// Bound to this root: the account's home.
    Home,
    /// Bound to no root: this machine has never recorded the account.
    Adopted,
    /// Bound to another root that still holds the uuid.
    Copy { holder: PathBuf },
    /// Bound only to roots that no longer hold it.
    Move { stale: Vec<PathBuf> },
}

/// Read what the index says about `uuid` presented from `root`, without
/// writing anything.
///
/// "Holds" is a fact on disk: the bound root's own `budgets/` still contains
/// this uuid. A bound root that cannot be read is taken to hold it, because the
/// reading that refuses is the one that creates nothing. §FS-rhei-budgets.5.4.1
pub(crate) fn read(index: &BTreeMap<PathBuf, String>, root: &Path, uuid: &str) -> Reading {
    let bound: Vec<&PathBuf> =
        index.iter().filter(|(_, held)| held.as_str() == uuid).map(|(root, _)| root).collect();
    if bound.iter().any(|bound| bound.as_path() == root) {
        return Reading::Home;
    }
    if bound.is_empty() {
        return Reading::Adopted;
    }
    if let Some(holder) = bound.iter().find(|bound| holds(bound, uuid)) {
        return Reading::Copy { holder: (*holder).clone() };
    }
    Reading::Move { stale: bound.into_iter().cloned().collect() }
}

/// Settle `uuid` at `root`: the holder where this root is a copy, and `None`
/// where it may proceed.
///
/// Adoption records the root and a move rebinds it, both under the uuid's
/// authority lock and both decided again under it, so two processes racing to
/// claim one uuid cannot both write. The lock is released before the caller
/// opens the journal, which takes it again: one process may not hold it twice.
/// A move is said once, on stderr, by the charge that rebinds it; every later
/// resolution reads `Home`. §FS-rhei-budgets.5.3 §FS-rhei-budgets.5.4.1
/// §AR-neural-admission.4
pub(crate) fn settle(root: &Path, uuid: &str) -> Result<Option<PathBuf>> {
    match read(&witnessed_roots()?, root, uuid) {
        Reading::Home => return Ok(None),
        Reading::Copy { holder } => return Ok(Some(holder)),
        Reading::Adopted | Reading::Move { .. } => {}
    }
    let _authority = super::authority::Authority::lock(root, uuid)?;
    let mut index = witnessed_roots()?;
    let stale = match read(&index, root, uuid) {
        Reading::Home => return Ok(None),
        Reading::Copy { holder } => return Ok(Some(holder)),
        Reading::Adopted => Vec::new(),
        Reading::Move { stale } => stale,
    };
    for old in &stale {
        index.remove(old);
    }
    index.insert(root.to_path_buf(), uuid.to_string());
    write_roots(&index)?;
    if let Some(old) = stale.first() {
        eprintln!(
            "warning: budget account panta:{uuid} moved to {}; it was held at {}, which no \
             longer holds it",
            root.display(),
            old.display()
        );
    }
    Ok(None)
}

/// Whether `root`'s own `budgets/` holds an account directory named `uuid`.
fn holds(root: &Path, uuid: &str) -> bool {
    match super::account::local_uuid(root) {
        Ok(found) => found.as_deref() == Some(uuid),
        Err(_) => true,
    }
}

/// The refusal a copy's charge raises, before anything is appended.
///
/// It names the account, this root and the holder, says where the charge would
/// have landed, and offers both answers: `forget` where this is a copy, and
/// moving the holder where this root is meant to replace it. The reason code
/// stays `untrustworthy_ledger`, so a harness routing on it is not broken.
/// §FS-rhei-budgets.5.4.1
pub(crate) fn held_elsewhere_refusal(root: &Path, uuid: &str, holder: &Path) -> BudgetError {
    BudgetError::new(
        "untrustworthy_ledger",
        format!(
            "budget account panta:{uuid} belongs to another project root: {} presents it, but \
             {} still holds it, so this charge would be written into the history of {}; if this \
             is a copy, give it an account of its own with: {}; if it is meant to replace {}, \
             move or delete that project first",
            root.display(),
            holder.display(),
            holder.display(),
            forget_line(root),
            holder.display(),
        ),
    )
}

/// `rhei budget forget '<root>' --reason <TEXT>`, the root quoted for the shell
/// the operator holds. §FS-rhei-budgets.10
pub(crate) fn forget_line(root: &Path) -> String {
    format!(
        "rhei budget forget {} --reason <TEXT>",
        crate::platform::shell_quote(&root.display().to_string())
    )
}

/// The same base the witness itself uses, resolved once for the process: an
/// index written beside one state directory and read beside another would
/// forget a root the witness remembers. §FS-rhei-budgets.5.3
fn roots_index_path() -> Result<PathBuf> {
    Ok(super::authority::authority_base()?.join("rhei/budget-authority/roots.json"))
}

/// The canonical root the witness index holds for an account uuid, where this
/// machine has ever seen that account.
///
/// The one way to name a project this run does not own: an ancestry descriptor
/// carries a uuid, and a note that printed the uuid would name the account on
/// disk rather than a directory a reader recognizes. `None` is an account this
/// machine has never witnessed, which is not an error — it is a note that falls
/// back to the uuid. §FS-rhei-budgets.5.3 §FS-rhei-budgets.7.2
pub fn witnessed_root(uuid: &str) -> Result<Option<PathBuf>> {
    Ok(witnessed_roots()?.into_iter().find(|(_, held)| held == uuid).map(|(root, _)| root))
}

/// The witness index: every canonical root bound to an account uuid.
/// §FS-rhei-budgets.5.3
pub(crate) fn witnessed_roots() -> Result<BTreeMap<PathBuf, String>> {
    let path = roots_index_path()?;
    match std::fs::read(&path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(error) => Err(BudgetError::unreachable(&path, &error)),
    }
}

pub(crate) fn record_root(root: &Path, uuid: &str) -> Result<()> {
    let mut index = witnessed_roots()?;
    index.insert(root.to_path_buf(), uuid.to_string());
    write_roots(&index)
}

/// Drop a root from the witness index, so the path resolves to no account and
/// the next admission mints a fresh identity for it. §FS-rhei-budgets.5.3
pub(crate) fn retract_root(root: &Path) -> Result<()> {
    let mut index = witnessed_roots()?;
    if index.remove(root).is_none() {
        return Ok(());
    }
    write_roots(&index)
}

/// The same write-pending-then-rename both directions use, so a retraction is
/// exactly as durable as the record it undoes. §FS-rhei-budgets.5.1
fn write_roots(index: &BTreeMap<PathBuf, String>) -> Result<()> {
    let path = roots_index_path()?;
    super::authority::durable_directories(path.parent().expect("index has a parent"))?;
    let pending = path.with_extension(format!("{}.pending", uuid::Uuid::new_v4()));
    std::fs::write(&pending, serde_json::to_vec_pretty(index)?)
        .map_err(|error| BudgetError::unreachable(&pending, &error))?;
    std::fs::rename(&pending, &path).map_err(|error| BudgetError::unreachable(&path, &error))?;
    Ok(())
}
