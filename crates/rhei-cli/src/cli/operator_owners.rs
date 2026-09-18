/// Owner tags grant exactly one bounded shared-manifest reference. §FS-rhei-recover.2
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
enum ForcedOwner { BasinProjectMetadata, ExecutionRoot }

impl ForcedFile {
    fn key(&self) -> (Option<ForcedOwner>, &str) { (self.owner, &self.path) }
}

/// Resolve only a regular immediate-parent manifest, never a caller-supplied parent path. §FS-rhei-recover.2
fn forced_basin_manifest(root: &Path) -> MietteResult<PathBuf> {
    let canonical = rhei_core::platform::canonical_path(root).map_err(|err| diagnostic!("{err}"))?;
    if canonical != root || root.file_name().is_none_or(|name| name != "basin")
        || !fs::symlink_metadata(root).is_ok_and(|meta| meta.is_dir() && !meta.file_type().is_symlink())
        || fs::symlink_metadata(root.join("index.rhei.md")).is_ok() {
        return Err(diagnostic!("basin-project-metadata requires a canonical regular basin without an authored index"));
    }
    let parent = root.parent().ok_or_else(|| diagnostic!("basin has no project owner"))?;
    let manifest = forced_image_path(parent, "index.panta.md")?;
    if !manifest.is_file() { return Err(diagnostic!("basin project owner requires a regular index.panta.md")); }
    Ok(manifest)
}

/// One resolver is shared by preparation, validation, locking and replay. §FS-rhei-recover.2
fn forced_file_path(root: &Path, file: &ForcedFile) -> MietteResult<PathBuf> {
    match file.owner {
        None | Some(ForcedOwner::ExecutionRoot) => forced_image_path(root, &file.path),
        Some(ForcedOwner::BasinProjectMetadata) => {
            if file.path != "index.panta.md" || file.roles != ["checkpoint", "metadata"]
                || file.before == ForcedImage::Absent || file.after == ForcedImage::Absent {
                return Err(diagnostic!("invalid basin-project-metadata image: expected complete index.panta.md metadata/checkpoint images"));
            }
            forced_basin_manifest(root)
        }
    }
}

/// Dependent project reads use the same filesystem-only owner discovery as ordinary access. §FS-rhei-recover.3
fn forced_owner_roots(root: &Path, files: &[ForcedFile]) -> MietteResult<Vec<PathBuf>> {
    let roots = rhei_core::root_access::input_roots(root).map_err(|err| diagnostic!("{err}"))?;
    for file in files { forced_file_path(root, file)?; }
    Ok(roots)
}

/// Run locks have already been acquired; take the complete exclusive set in order. §FS-rhei-recover.3
fn forced_root_guards(roots: &[PathBuf]) -> MietteResult<Vec<rhei_core::root_access::RootAccessGuard>> {
    let mut guards = Vec::new();
    for root in roots {
        let guard = match rhei_core::root_access::RootAccessGuard::try_exclusive(root)
            .map_err(|err| diagnostic!("{err}"))? {
            Some(guard) => guard,
            None => {
                forced_boundary("root-contended")?;
                rhei_core::root_access::RootAccessGuard::exclusive(root).map_err(|err| diagnostic!("{err}"))?
            }
        };
        guards.push(guard);
    }
    Ok(guards)
}
