/// Every generation of the store, for a reader that refuses on the first manifest it
/// cannot read. `rhei snapshot list` and `gc` use [`scan_snapshot_records`] instead;
/// the rest of Rhei stays strict. §FS-rhei-snapshot-operations.1.6
fn read_snapshot_records(cache_root: &Path) -> MietteResult<Vec<SnapshotRecord>> {
    let mut records = Vec::new();
    for manifest_path in snapshot_manifest_paths(cache_root)? {
        records.extend(read_snapshot_manifest(cache_root, &manifest_path)?);
    }
    Ok(records)
}

/// A generation whose manifest could not be read, parsed or accepted, and what failed.
/// §FS-rhei-snapshot-operations.1.6
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

/// Read every generation, letting an unreadable one cost only itself. A failure walking
/// the cache still refuses, because it is not one generation's damage.
/// §FS-rhei-snapshot-operations.1.6
fn scan_snapshot_records(cache_root: &Path) -> MietteResult<SnapshotStoreScan> {
    let mut scan = SnapshotStoreScan::default();
    for manifest_path in snapshot_manifest_paths(cache_root)? {
        match read_snapshot_manifest(cache_root, &manifest_path) {
            Ok(record) => scan.records.extend(record),
            Err(report) => scan.unreadable.push(UnreadableGeneration {
                directory: manifest_path.parent().unwrap_or(&manifest_path).to_path_buf(),
                // The line already names the generation, so the reason does not repeat it.
                reason: report
                    .to_string()
                    .replace(&format!(" '{}'", manifest_path.display()), ""),
            }),
        }
    }
    Ok(scan)
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

/// The committed manifests under the cache, in path order so what names them is predictable.
/// §FS-rhei-snapshot-operations.1.6
fn snapshot_manifest_paths(cache_root: &Path) -> MietteResult<Vec<PathBuf>> {
    if !cache_root.exists() {
        return Ok(Vec::new());
    }
    let mut manifests = Vec::new();
    collect_manifest_paths(cache_root, cache_root, &mut manifests)
        .map_err(|err| file_io_report(cache_root, "failed to read snapshot cache", err))?;
    manifests.sort();
    Ok(manifests)
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

fn collect_manifest_paths(cache_root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            if entry.file_name().to_str().is_some_and(|name| name.contains(".tmp-")) {
                continue;
            }
            collect_manifest_paths(cache_root, &path, out)?;
        } else if entry.file_name() == OsStr::new("manifest.json")
            && is_committed_snapshot_manifest(cache_root, &path)
        {
            out.push(path);
        }
    }
    Ok(())
}

fn is_committed_snapshot_manifest(cache_root: &Path, manifest_path: &Path) -> bool {
    let relative = manifest_path.strip_prefix(cache_root).unwrap_or(manifest_path);
    if relative.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .is_some_and(|name| name.contains(".tmp-"))
    }) {
        return false;
    }
    manifest_path
        .parent()
        .and_then(Path::file_name)
        .and_then(OsStr::to_str)
        .and_then(|name| name.strip_prefix('g'))
        .is_some_and(|generation| !generation.is_empty() && generation.parse::<u64>().is_ok())
}
