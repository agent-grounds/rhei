/// Every generation of the store, for a reader that refuses on the first one it cannot
/// read. `rhei snapshot list` and `gc` use [`scan_snapshot_records`] instead; the rest of
/// Rhei stays strict. §FS-rhei-snapshot-operations.1.6
fn read_snapshot_records(cache_root: &Path) -> MietteResult<Vec<SnapshotRecord>> {
    let mut records = Vec::new();
    for generation in snapshot_store_generations(cache_root)? {
        records.extend(generation.read(cache_root)?);
    }
    Ok(records)
}

/// One generation directory the walk found: its manifest to read, or why it could not be
/// listed. §FS-rhei-snapshot-operations.1.6
enum StoreGeneration {
    Manifest(PathBuf),
    Unlistable { directory: PathBuf, cause: std::io::Error },
}

impl StoreGeneration {
    fn directory(&self) -> &Path {
        match self {
            Self::Manifest(manifest_path) => manifest_path.parent().unwrap_or(manifest_path),
            Self::Unlistable { directory, .. } => directory,
        }
    }

    /// The path a failure of [`Self::read`] names in its message.
    fn read_path(&self) -> &Path {
        match self {
            Self::Manifest(manifest_path) => manifest_path,
            Self::Unlistable { directory, .. } => directory,
        }
    }

    fn read(&self, cache_root: &Path) -> MietteResult<Option<SnapshotRecord>> {
        match self {
            Self::Manifest(manifest_path) => read_snapshot_manifest(cache_root, manifest_path),
            Self::Unlistable { directory, cause } => {
                Err(file_io_report(directory, "failed to read generation directory", cause))
            }
        }
    }
}

/// A generation that could not be listed, or whose manifest could not be read, parsed or
/// accepted, and what failed. §FS-rhei-snapshot-operations.1.6
struct UnreadableGeneration {
    directory: PathBuf,
    reason: String,
}

/// The store as far as it can be read: the readable generations, and the unreadable ones
/// in path order. §FS-rhei-snapshot-operations.1.6
#[derive(Default)]
struct SnapshotStoreScan {
    records: Vec<SnapshotRecord>,
    unreadable: Vec<UnreadableGeneration>,
}

/// Read every generation, letting an unreadable one cost only itself. A directory above a
/// generation that cannot be listed still refuses, because it may hold any number of them.
/// §FS-rhei-snapshot-operations.1.6
fn scan_snapshot_records(cache_root: &Path) -> MietteResult<SnapshotStoreScan> {
    let mut scan = SnapshotStoreScan::default();
    for generation in snapshot_store_generations(cache_root)? {
        match generation.read(cache_root) {
            Ok(record) => scan.records.extend(record),
            Err(report) => scan.unreadable.push(UnreadableGeneration {
                directory: generation.directory().to_path_buf(),
                reason: unreadable_reason(&report, generation.read_path()),
            }),
        }
    }
    Ok(scan)
}

/// The failure as one line after the generation's path: the line already names the path, so
/// the reason does not repeat it, and a manifest's value cannot break the line or forge
/// another. §FS-rhei-snapshot-operations.1.6
fn unreadable_reason(report: &Report, read_path: &Path) -> String {
    report
        .to_string()
        .replace(&format!(" '{}'", read_path.display()), "")
        .chars()
        .fold(String::new(), |mut reason, character| {
            if character.is_control() {
                reason.extend(character.escape_default());
            } else {
                reason.push(character);
            }
            reason
        })
}

/// Name each unreadable generation on one unwrapped stderr line, so its path can be pasted,
/// then end the command in the error that counts them. With none, this prints nothing and
/// succeeds. §FS-rhei-snapshot-operations.1.6
fn report_unreadable_generations(
    verb: &str,
    unreadable: &[UnreadableGeneration],
) -> MietteResult<()> {
    if unreadable.is_empty() {
        return Ok(());
    }
    for generation in unreadable {
        eprintln!("{verb} {}: {}", generation.directory.display(), generation.reason);
    }
    let count = unreadable.len();
    let noun = if count == 1 { "generation" } else { "generations" };
    Err(miette!(help = snapshot_unreadable_help(), "could not read {count} snapshot {noun}"))
}

/// The committed generations under the cache, in path order so what names them is
/// predictable. A walk failure above a generation names the directory that failed.
/// §FS-rhei-snapshot-operations.1.6
fn snapshot_store_generations(cache_root: &Path) -> MietteResult<Vec<StoreGeneration>> {
    if !cache_root.exists() {
        return Ok(Vec::new());
    }
    let mut generations = Vec::new();
    collect_store_generations(cache_root, cache_root, &mut generations)?;
    generations.sort_by(|left, right| left.directory().cmp(right.directory()));
    Ok(generations)
}

fn read_snapshot_manifest(
    cache_root: &Path,
    manifest_path: &Path,
) -> MietteResult<Option<SnapshotRecord>> {
    let raw = fs::read_to_string(manifest_path)
        .map_err(|err| file_io_report(manifest_path, "failed to read snapshot manifest", err))?;
    let manifest: serde_json::Value = serde_json::from_str(&raw).map_err(|err| {
        miette!(
            help = snapshot_corrupt_help(),
            "failed to parse snapshot manifest '{}': {err}", manifest_path.display()
        )
    })?;
    snapshot_record_from_manifest(cache_root, manifest_path, manifest)
}

fn collect_store_generations(
    cache_root: &Path,
    dir: &Path,
    out: &mut Vec<StoreGeneration>,
) -> MietteResult<()> {
    let entries = match list_directory(dir) {
        Ok(entries) => entries,
        // §FS-rhei-snapshot-operations.1.6: a generation costs itself; anything else refuses.
        Err(cause) if is_generation_directory(cache_root, dir) => {
            out.push(StoreGeneration::Unlistable { directory: dir.to_path_buf(), cause });
            return Ok(());
        }
        Err(cause) => return Err(file_io_report(dir, "failed to read snapshot cache", cause)),
    };
    for (entry, file_type) in entries {
        let path = entry.path();
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            if entry.file_name().to_str().is_some_and(|name| name.contains(".tmp-")) {
                continue;
            }
            collect_store_generations(cache_root, &path, out)?;
        } else if entry.file_name() == OsStr::new("manifest.json")
            && path.parent().is_some_and(|parent| is_generation_directory(cache_root, parent))
        {
            out.push(StoreGeneration::Manifest(path));
        }
    }
    Ok(())
}

/// Every entry of `dir` with its type, or the failure that stopped listing it.
fn list_directory(dir: &Path) -> std::io::Result<Vec<(fs::DirEntry, fs::FileType)>> {
    fs::read_dir(dir)?
        .map(|entry| {
            let entry = entry?;
            let file_type = entry.file_type()?;
            Ok((entry, file_type))
        })
        .collect()
}

/// A committed generation's directory: named `g<N>`, and not in a staging directory.
/// §FS-rhei-snapshot-operations.1.6
fn is_generation_directory(cache_root: &Path, dir: &Path) -> bool {
    let relative = dir.strip_prefix(cache_root).unwrap_or(dir);
    if relative.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .is_some_and(|name| name.contains(".tmp-"))
    }) {
        return false;
    }
    relative
        .file_name()
        .and_then(OsStr::to_str)
        .and_then(|name| name.strip_prefix('g'))
        .is_some_and(|generation| !generation.is_empty() && generation.parse::<u64>().is_ok())
}
