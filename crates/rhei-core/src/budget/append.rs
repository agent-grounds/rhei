//! How one receipt line reaches the journal file, and what a failed append
//! undoes there. The account's first receipt creates the journal or takes over
//! an empty one; every later receipt appends to the journal its open replayed.
//! §AR-neural-admission.4

use super::authority::{cut, shorten, Writer};
use super::types::BudgetError;
use super::Result;
use std::fs::{File, OpenOptions};
use std::path::Path;

/// What a failed append leaves to undo in the journal. §AR-neural-admission.4
pub(super) enum Undo {
    /// The append created the journal, so it is removed.
    Remove,
    /// The journal was there, so it is cut back to its old length in place:
    /// zero bytes, for one the first receipt took over.
    Shorten(u64),
}

impl Undo {
    /// Put the journal back as it was before the append. §AR-neural-admission.4
    pub(super) fn apply(self, path: &Path) -> Result<()> {
        match self {
            Self::Remove => cut(path, 0),
            Self::Shorten(length) => shorten(path, length),
        }
    }
}

/// Append one line to the journal. The account's first receipt creates it, or
/// takes over one that is there and empty, the absent state its open replayed.
/// A journal that holds bytes the open did not replay fails the append before
/// anything is written to it.
///
/// A failure carries what to undo: nothing where the append never opened the
/// journal or it is still at its old length, because then it holds nothing of
/// the line. §AR-neural-admission.4
pub(super) fn append_line(
    path: &Path,
    first: bool,
    line: &[u8],
    write: Writer,
) -> std::result::Result<(), (BudgetError, Option<Undo>)> {
    let (file, created) = open_journal(path, first).map_err(|error| (error, None))?;
    // The opened handle's length, not the path's: what decides is the file this
    // append writes to. §AR-neural-admission.4
    let before = match created {
        true => 0,
        false => file.metadata().map_err(|error| (error.into(), None))?.len(),
    };
    if first && before != 0 {
        return Err((
            BudgetError::corrupt(format!(
                "the journal at {} holds {before} bytes the account's open did not replay; \
                 its first receipt is not written behind them",
                path.display()
            )),
            None,
        ));
    }
    write(&file, line).and_then(|()| file.sync_all()).map_err(|error| {
        let now = file.metadata().map(|metadata| metadata.len()).ok();
        let undo = match created {
            true => Some(Undo::Remove),
            false => (now != Some(before)).then_some(Undo::Shorten(before)),
        };
        (error.into(), undo)
    })
}

/// Open the journal to append, and say whether this open created it: only the
/// first receipt may, and it takes over a journal that is already there.
/// §AR-neural-admission.4
fn open_journal(path: &Path, first: bool) -> Result<(File, bool)> {
    let open = |create_new: bool| OpenOptions::new().append(true).create_new(create_new).open(path);
    match open(first) {
        Ok(file) => Ok((file, first)),
        Err(error) if first && error.kind() == std::io::ErrorKind::AlreadyExists => {
            open(false).map(|file| (file, false))
        }
        Err(error) => Err(error),
    }
    .map_err(|error| BudgetError::unreachable(path, &error))
}
