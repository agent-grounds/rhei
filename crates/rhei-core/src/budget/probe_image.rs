//! Copy a signed image inventory before exposing it read-only to a namespace.
//! No host library search or inherited home is part of the image.
//! §FS-rhei-budgets.12

use super::probe::refused;
use super::Result;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ImageFile {
    pub sha256: String,
    pub bytes: u64,
    pub executable: bool,
}

pub(super) fn relative(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty() || path.components().any(|p| !matches!(p, Component::Normal(_)))
    {
        return Err(refused("image paths must be nonempty relative normal components"));
    }
    Ok(())
}

pub(super) fn read_file(path: &Path, limit: u64) -> Result<Vec<u8>> {
    if !std::fs::symlink_metadata(path)?.file_type().is_file() {
        return Err(refused("probe input must be a regular file, not a symlink or device"));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)?.take(limit.saturating_add(1)).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(refused("probe input exceeds its signed byte limit"));
    }
    Ok(bytes)
}

pub(super) fn copy_image(
    source: &Path,
    destination: &Path,
    entries: &BTreeMap<String, ImageFile>,
    limit: u64,
) -> Result<()> {
    std::fs::create_dir(destination)?;
    let mut total = 0u64;
    for (name, entry) in entries {
        let path = Path::new(name);
        relative(path)?;
        total = total.checked_add(entry.bytes).ok_or_else(|| refused("image size overflow"))?;
        if total > limit {
            return Err(refused("image exceeds signed acquisition size limit"));
        }
        let mut parent = source.to_path_buf();
        for component in path.parent().into_iter().flat_map(Path::components) {
            parent.push(component);
            if !std::fs::symlink_metadata(&parent)?.file_type().is_dir() {
                return Err(refused("image parent is not a regular directory"));
            }
        }
        let bytes = read_file(&source.join(path), entry.bytes)?;
        if bytes.len() as u64 != entry.bytes || super::journal::digest(&bytes) != entry.sha256 {
            return Err(refused("image file differs from signed dependency inventory"));
        }
        let target = destination.join(path);
        std::fs::create_dir_all(target.parent().expect("image path parent"))?;
        super::probe::write_new(&target, &bytes)?;
        std::fs::set_permissions(
            target,
            std::fs::Permissions::from_mode(if entry.executable { 0o500 } else { 0o400 }),
        )?;
    }
    Ok(())
}
