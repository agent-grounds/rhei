//! Reading a plan, a workspace index, or a project manifest off disk.
//!
//! One function rather than `fs::read_to_string` at each site preserves the
//! public reader override for embedders. The CLI's writer protocol leaves plan
//! destinations unlocked and therefore uses the default current-path reader.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

type Reader = fn(&Path) -> std::io::Result<String>;

static READER: std::sync::OnceLock<Reader> = std::sync::OnceLock::new();

thread_local! {
    /// The texts [`read_staged`] reads in place of the files at their paths.
    static STAGED: RefCell<Option<HashMap<PathBuf, String>>> = const { RefCell::new(None) };
}

/// Install an embedding reader every plan source goes through. First call wins,
/// and a second is ignored rather than fatal: this is a process-wide
/// convenience, not a contract between two callers.
// §FS-rhei-new.4
pub fn set_reader(reader: Reader) {
    let _ = READER.set(reader);
}

/// Read `path`, through the installed reader when there is one.
// §FS-rhei-new.4
pub fn read_to_string(path: &Path) -> std::io::Result<String> {
    // §FS-rhei-recover.4: direct source readers also refuse a marked root.
    let _guard = crate::root_access::for_file(path)?;
    if let Some(text) = staged(path) {
        return Ok(text);
    }
    match READER.get() {
        Some(reader) => reader(path),
        None => std::fs::read_to_string(path),
    }
}

/// Run `read` on this thread with every file named in `texts` read as that text
/// rather than from disk, so a command can load a plan it has not written yet
/// through the loader every command uses: `rhei reset` asks the validator about
/// the plan it would leave this way, and so cannot ask about a different one
/// than it writes. Every other file reads as it stands. §FS-rhei-reset.2.3
pub fn read_staged<R>(texts: HashMap<PathBuf, String>, read: impl FnOnce() -> R) -> R {
    /// Puts back what was staged before, on a panic too.
    struct Restore(Option<HashMap<PathBuf, String>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let previous = self.0.take();
            STAGED.with(|staged| *staged.borrow_mut() = previous);
        }
    }
    let _restore = Restore(STAGED.with(|staged| staged.replace(Some(texts))));
    read()
}

/// The text staged for `path` while [`read_staged`] runs, if any. A reader that
/// does not go through [`read_to_string`] asks this first. §FS-rhei-reset.2.3
pub fn staged(path: &Path) -> Option<String> {
    STAGED.with(|staged| staged.borrow().as_ref()?.get(path).cloned())
}
