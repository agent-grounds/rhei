//! An append that fails partway, on either side, leaves both files as they
//! were before it.
//!
//! A write cut short reaches the file before its error comes back, so each
//! case goes through a writer that takes part of the line and then fails, the
//! way a full disk or a file-size limit does — on every platform, where the
//! end-to-end forms need a Unix file-size limit or a read-only journal.
//! §AR-neural-admission.4

use super::diagnosis::Inspection;
use super::test_support::Case;
use std::fs::{self, File};
use std::io::{self, Write};

/// A writer with room for part of what it is given: what fits reaches the
/// file, and the next write fails.
struct CutShort<'f> {
    file: &'f File,
    room: usize,
}

impl Write for CutShort<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.room == 0 {
            return Err(io::Error::other("no space left for the rest of the line"));
        }
        let written = self.file.write(&buf[..buf.len().min(self.room)])?;
        self.room -= written;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Half of the line, then an error.
pub(super) fn cut_short(file: &File, line: &[u8]) -> io::Result<()> {
    CutShort { file, room: line.len() / 2 }.write_all(line)
}

pub(super) const LINE: &[u8] =
    b"{\"schema\":\"rhei.budget.receipt.v1\",\"kind\":\"a receipt cut short\"}\n";

/// The witness's write is cut short: the witness is cut back to its length
/// before the append, its in-memory copy never took the line, and the error
/// returned is the write's own. §AR-neural-admission.4
#[test]
fn a_witness_append_cut_short_leaves_the_witness_as_it_was() {
    let case = Case::new();
    let mut journal = case.open();
    let witness = fs::read(case.witness_path()).expect("read the witness");

    let error = journal.authority.append_through(LINE, cut_short).expect_err("the write fails");

    assert!(error.message.contains("no space left"), "the write's own error: {}", error.message);
    assert_eq!(fs::read(case.witness_path()).expect("read the witness"), witness);
    assert_eq!(journal.authority.bytes(), &witness[..], "the in-memory copy follows the file");
}

/// The witness takes the whole line and the journal's write is cut short: the
/// journal is cut back, then the witness, and the account verifies as it did
/// before the edge. §AR-neural-admission.4
#[test]
fn a_journal_append_cut_short_cuts_both_files_back() {
    let case = Case::new();
    let mut journal = case.open();
    let witness = fs::read(case.witness_path()).expect("read the witness");
    let ledger = fs::read(case.journal_path()).expect("read the journal");

    let error = journal.write_receipt(LINE, cut_short).expect_err("the journal's write fails");

    assert!(error.message.contains("no space left"), "the write's own error: {}", error.message);
    assert_eq!(fs::read(case.journal_path()).expect("read the journal"), ledger);
    assert_eq!(fs::read(case.witness_path()).expect("read the witness"), witness);
    assert_eq!(journal.authority.bytes(), &witness[..], "the in-memory copy follows the file");
    drop(journal);
    assert!(
        matches!(case.account.inspect().expect("inspect"), Inspection::Verified(_)),
        "both files are as they were, so the account verifies"
    );
}
