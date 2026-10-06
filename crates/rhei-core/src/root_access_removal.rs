//! The pending-removal marker: the recover interlock's second marker, cleared
//! only by the `rhei remove` that left it. §FS-rhei-recover.4 §FS-rhei-remove.6.2

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Where an unfinished removal is recorded, at the project execution root.
/// §FS-rhei-remove.6.2
pub const REMOVAL_MARKER: &str = ".rhei/pending-removal.json";

// Roots whose marker this process wrote or is resuming: its own loads and
// validation must read through it. §FS-rhei-remove.6.2
static OWNED: Mutex<BTreeSet<PathBuf>> = Mutex::new(BTreeSet::new());

/// This process's claim on one root's removal marker, released on drop.
#[derive(Debug)]
pub struct OwnedRemoval(PathBuf);

impl Drop for OwnedRemoval {
    fn drop(&mut self) {
        OWNED.lock().unwrap_or_else(|poison| poison.into_inner()).remove(&self.0);
    }
}

fn identity(root: &Path) -> PathBuf {
    crate::platform::canonical_path(root).unwrap_or_else(|_| root.to_path_buf())
}

/// Let this process read through the removal marker at `root` — only the
/// removal that writes or resumes it calls this. §FS-rhei-remove.6.2
pub fn own_pending_removal(root: &Path) -> OwnedRemoval {
    let root = identity(root);
    OWNED.lock().unwrap_or_else(|poison| poison.into_inner()).insert(root.clone());
    OwnedRemoval(root)
}

/// The ticket a marker records, `?` when its contents cannot be read.
pub fn pending_removal_ticket(marker: &Path) -> String {
    fs::read(marker)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|value| value["ticket"].as_str().map(str::to_string))
        .unwrap_or_else(|| "?".to_string())
}

/// Refuse while another removal is unfinished at `root`. §FS-rhei-recover.4
pub(super) fn check_removal_marker(root: &Path) -> io::Result<()> {
    let marker = root.join(REMOVAL_MARKER);
    match fs::symlink_metadata(&marker) {
        Ok(_) => (),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(io::Error::new(
                error.kind(),
                format!("cannot check removal marker {}: {error}", marker.display()),
            ))
        }
    }
    let owned = OWNED.lock().unwrap_or_else(|poison| poison.into_inner()).contains(&identity(root));
    if owned {
        return Ok(());
    }
    let ticket = pending_removal_ticket(&marker);
    Err(io::Error::other(format!(
        "removal pending: {ticket}; marker {}\nrhei remove {}",
        marker.display(),
        crate::platform::shell_quote(&ticket)
    )))
}
