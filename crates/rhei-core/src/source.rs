//! Reading a plan, a workspace index, or a project manifest off disk.
//!
//! One function rather than `fs::read_to_string` at each site preserves the
//! public reader override for embedders. The CLI's writer protocol leaves plan
//! destinations unlocked and therefore uses the default current-path reader.

use std::path::Path;

type Reader = fn(&Path) -> std::io::Result<String>;

static READER: std::sync::OnceLock<Reader> = std::sync::OnceLock::new();

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
    match READER.get() {
        Some(reader) => reader(path),
        None => std::fs::read_to_string(path),
    }
}
