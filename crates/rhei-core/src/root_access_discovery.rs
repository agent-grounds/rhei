//! Discover identities before any dependent reads or lock acquisition. §FS-rhei-panta.6.6

use super::{check_pending, RootAccessGuard, MARKER};
use crate::workspace::{PANTA_INDEX_FILE, RHEI_INDEX_FILE};
use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

fn has_entry(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn is_project(root: &Path) -> bool {
    has_entry(&root.join(PANTA_INDEX_FILE)) || has_entry(&root.join("basin").join(MARKER))
}

fn project_owner(root: &Path) -> Option<&Path> {
    if is_project(root) {
        Some(root)
    } else {
        root.parent().filter(|parent| is_project(parent))
    }
}

/// Also find a basin marker when the manifest itself is unreadable. §FS-rhei-recover.4
pub(super) fn pending_roots(root: &Path) -> Vec<PathBuf> {
    let mut roots = vec![root.to_path_buf()];
    if let Some(project) = project_owner(root) {
        roots.push(project.to_path_buf());
        roots.push(project.join("basin"));
    }
    roots.sort();
    roots.dedup();
    roots
}

fn file_root(path: &Path) -> PathBuf {
    let parent = crate::workspace::plan_parent_dir(path);
    parent
        .ancestors()
        .find(|dir| {
            has_entry(&dir.join(MARKER))
                || has_entry(&dir.join(RHEI_INDEX_FILE))
                || is_project(dir)
                || (dir.file_name().is_some_and(|name| name == "basin")
                    && dir.parent().is_some_and(is_project))
                || dir.join(".rhei/run.lock").is_file()
                || fs::read_dir(dir).is_ok_and(|entries| {
                    entries.flatten().any(|entry| {
                        entry.file_name().to_str().is_some_and(|name| name.ends_with(".rhei.md"))
                    })
                })
        })
        .unwrap_or(parent)
        .to_path_buf()
}

/// Identities only: no parsing of a possibly in-doubt project manifest. §FS-rhei-recover.4
pub fn input_roots(path: &Path) -> io::Result<Vec<PathBuf>> {
    let root = if path.is_dir() { path.to_path_buf() } else { file_root(path) };
    let root = crate::platform::canonical_path(&root)?;
    let mut roots = vec![root.clone()];
    if let Some(project) = project_owner(&root) {
        roots.push(project.to_path_buf());
        for entry in fs::read_dir(project)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir()
                && (has_entry(&path.join(RHEI_INDEX_FILE))
                    || path.file_name().is_some_and(|name| name == "basin"))
            {
                roots.push(crate::platform::canonical_path(&path)?);
            }
        }
    }
    roots.sort();
    roots.dedup();
    Ok(roots)
}

/// Acquire once in canonical order, including owners needed by nested loads. §FS-rhei-panta.6.6
pub fn shared_roots(roots: impl IntoIterator<Item = PathBuf>) -> io::Result<Vec<RootAccessGuard>> {
    let mut complete = BTreeSet::new();
    for root in roots {
        let root = crate::platform::canonical_path(&root)?;
        if !complete.contains(&root) {
            complete.extend(input_roots(&root)?);
        }
    }
    for root in &complete {
        check_pending(root)?;
    }
    let guards = complete
        .iter()
        .map(|root| RootAccessGuard::shared(root))
        .collect::<io::Result<Vec<_>>>()?;
    for root in &complete {
        check_pending(root)?;
    }
    Ok(guards)
}

/// Retain the complete set through all reads/writes derived from an input. §FS-rhei-panta.6.6
pub fn for_input(path: &Path) -> io::Result<Vec<RootAccessGuard>> {
    shared_roots(input_roots(path)?)
}

/// Direct shared-manifest writers obey the same boundary as loaders. §FS-rhei-recover.4
pub fn for_file(path: &Path) -> io::Result<Vec<RootAccessGuard>> {
    for_input(path)
}
