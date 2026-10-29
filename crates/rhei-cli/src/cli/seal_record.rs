// The run-input seal: which files are sealed for one rhei, the generation
// files under `runtime/seal/<rhei>/`, and the changes between a generation and
// the bytes on disk. `rhei seal` and `rhei run --strict-inputs` share it.

// §FS-rhei-seal

const SEAL_SCHEMA: &str = "rhei.seal.v1";

/// The directories beside a machine that are sealed with it. §FS-rhei-seal.2.1
const SEALED_MACHINE_DIRS: [&str; 2] = ["prompt_templates", "scripts"];

/// What is sealed for one rhei, and where its generations live.
/// §FS-rhei-seal.2 §FS-rhei-seal.3.1
#[derive(Debug, Clone)]
struct SealScope {
    rhei: String,
    /// The rhei's execution root; the seal is `runtime/seal/<rhei>/` under it.
    execution_root: PathBuf,
    /// Paths under it are recorded relative to it. §FS-rhei-seal.3.3
    base: PathBuf,
    /// The machine file, `None` under the built-in machine. §FS-rhei-seal.2.3
    machine: Option<PathBuf>,
    /// The user settings file, both project candidates, and the price book.
    settings_files: Vec<PathBuf>,
}

impl SealScope {
    /// The sealed set for `rhei`: its machine (and what is beside it), the
    /// settings files at `settings_root`, and the resolved price book.
    /// §FS-rhei-seal.2.1 §FS-rhei-seal.2.2
    fn new(
        rhei: &str,
        execution_root: &Path,
        machine: Option<&Path>,
        settings_root: &Path,
        price_book: Option<&Path>,
    ) -> Self {
        let absolute = |path: &Path| std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        let machine = machine.map(absolute);
        let base = match machine.as_deref().and_then(Path::parent) {
            Some(parent) => parent.to_path_buf(),
            None => absolute(settings_root),
        };
        let mut settings_files: Vec<PathBuf> = machine_settings_file().into_iter().collect();
        settings_files.extend(
            rhei_home_paths(&absolute(settings_root), PROJECT_SETTINGS_FILE)
                .into_iter()
                .map(|candidate| candidate.path),
        );
        settings_files.extend(price_book.map(absolute));
        Self {
            rhei: rhei.to_string(),
            execution_root: absolute(execution_root),
            base,
            machine,
            settings_files,
        }
    }

    fn seal_dir(&self) -> PathBuf {
        self.execution_root.join("runtime").join("seal").join(&self.rhei)
    }

    /// A path as the record names it: relative to the base with `/`, or
    /// absolute outside it. §FS-rhei-seal.3.3
    fn label(&self, path: &Path) -> String {
        match path.strip_prefix(&self.base) {
            Ok(relative) => relative
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/"),
            Err(_) => path.display().to_string(),
        }
    }
}

/// One sealed path's recorded state. §FS-rhei-seal.3.3
#[derive(Debug, Clone, PartialEq, Eq)]
enum SealEntry {
    /// `executable` is `None` where the platform has no owner-execute bit.
    File { sha256: String, executable: Option<bool> },
    /// `sha256` names the file the link points at; `None` for a directory or nothing.
    Symlink { target: String, sha256: Option<String> },
    Absent,
}

/// The sealed set as it stands: one generation's content, or the disk now.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct SealSnapshot {
    machine: Option<String>,
    directories: BTreeMap<String, bool>,
    entries: BTreeMap<String, SealEntry>,
}

impl SealSnapshot {
    /// What `generation 1, <n> files` counts: every present entry. §FS-rhei-seal.5
    fn file_count(&self) -> usize {
        self.entries.values().filter(|entry| **entry != SealEntry::Absent).count()
    }
}

/// One difference between a generation and the disk. §FS-rhei-seal.4
#[derive(Debug, Clone, PartialEq, Eq)]
struct SealChange {
    path: String,
    kind: &'static str,
}

/// Hash the sealed set as it is on disk now. §FS-rhei-seal.2 §FS-rhei-seal.8.2
fn take_seal_snapshot(scope: &SealScope) -> MietteResult<SealSnapshot> {
    let mut snapshot = SealSnapshot::default();
    if let Some(machine) = scope.machine.as_deref() {
        snapshot.machine = Some(scope.label(machine));
        snapshot.entries.insert(scope.label(machine), seal_entry(machine)?);
        for dir in SEALED_MACHINE_DIRS {
            let path = scope.base.join(dir);
            let present = path.is_dir();
            snapshot.directories.insert(dir.to_string(), present);
            if present {
                collect_sealed_dir(scope, &path, &mut snapshot.entries)?;
            }
        }
    }
    for file in &scope.settings_files {
        snapshot.entries.insert(scope.label(file), seal_entry(file)?);
    }
    Ok(snapshot)
}

/// Every regular file and symlink under `dir`, recursively; a link to a
/// directory is recorded and not followed. §FS-rhei-seal.2.1 §FS-rhei-seal.3.3
fn collect_sealed_dir(
    scope: &SealScope,
    dir: &Path,
    entries: &mut BTreeMap<String, SealEntry>,
) -> MietteResult<()> {
    let read = fs::read_dir(dir).map_err(|err| file_io_report(dir, "could not read", err))?;
    for item in read {
        let path = item.map_err(|err| file_io_report(dir, "could not read", err))?.path();
        let meta = fs::symlink_metadata(&path)
            .map_err(|err| file_io_report(&path, "could not read", err))?;
        if meta.is_dir() {
            collect_sealed_dir(scope, &path, entries)?;
        } else if meta.is_file() || meta.file_type().is_symlink() {
            entries.insert(scope.label(&path), seal_entry(&path)?);
        }
    }
    Ok(())
}

/// The recorded state of one path; a missing path is `absent`. §FS-rhei-seal.3.3
fn seal_entry(path: &Path) -> MietteResult<SealEntry> {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(SealEntry::Absent),
        Err(err) => return Err(file_io_report(path, "could not read", err)),
    };
    let digest = |path: &Path| -> MietteResult<String> {
        let bytes = fs::read(path).map_err(|err| file_io_report(path, "could not read", err))?;
        Ok(sha256_hex(&bytes))
    };
    if meta.file_type().is_symlink() {
        let target = fs::read_link(path).map_err(|err| file_io_report(path, "could not read", err))?;
        let names_file = fs::metadata(path).map(|target| target.is_file()).unwrap_or(false);
        let sha256 = if names_file { Some(digest(path)?) } else { None };
        return Ok(SealEntry::Symlink { target: target.to_string_lossy().into_owned(), sha256 });
    }
    if !meta.is_file() {
        return Ok(SealEntry::Absent);
    }
    Ok(SealEntry::File { sha256: digest(path)?, executable: owner_executable(&meta) })
}

/// The owner-execute bit, `None` on a platform without one. §FS-rhei-seal.3.3
#[cfg(unix)]
fn owner_executable(meta: &fs::Metadata) -> Option<bool> {
    use std::os::unix::fs::PermissionsExt;
    Some(meta.permissions().mode() & 0o100 != 0)
}

#[cfg(not(unix))]
fn owner_executable(_meta: &fs::Metadata) -> Option<bool> {
    None
}

/// Each difference between `sealed` and `current`, one kind per path, sorted
/// by path. §FS-rhei-seal.4
fn seal_changes(sealed: &SealSnapshot, current: &SealSnapshot) -> Vec<SealChange> {
    let mut changes = Vec::new();
    // §FS-rhei-seal.4: a rhei that gained or lost a machine of its own.
    match (&sealed.machine, &current.machine) {
        (None, Some(path)) => changes.push(SealChange { path: path.clone(), kind: "added" }),
        (Some(path), None) => changes.push(SealChange { path: path.clone(), kind: "deleted" }),
        _ => {}
    }
    let dirs: BTreeSet<&String> = sealed.directories.keys().chain(current.directories.keys()).collect();
    for dir in dirs {
        let was = sealed.directories.get(dir).copied().unwrap_or(false);
        let is = current.directories.get(dir).copied().unwrap_or(false);
        if was != is {
            changes.push(SealChange { path: dir.clone(), kind: if is { "added" } else { "deleted" } });
        }
    }
    let paths: BTreeSet<&String> = sealed.entries.keys().chain(current.entries.keys()).collect();
    for path in paths {
        let was = sealed.entries.get(path).unwrap_or(&SealEntry::Absent);
        let is = current.entries.get(path).unwrap_or(&SealEntry::Absent);
        if let Some(kind) = seal_entry_change(was, is) {
            changes.push(SealChange { path: path.clone(), kind });
        }
    }
    changes.sort_by(|a, b| a.path.cmp(&b.path));
    changes
}

/// The one kind of change between two states of a path, if any. §FS-rhei-seal.4
fn seal_entry_change(was: &SealEntry, is: &SealEntry) -> Option<&'static str> {
    use SealEntry::{Absent, File, Symlink};
    match (was, is) {
        (Absent, Absent) => None,
        (Absent, _) => Some("added"),
        (_, Absent) => Some("deleted"),
        (File { sha256: a, executable: x }, File { sha256: b, executable: y }) => {
            if a != b {
                Some("modified")
            } else if x.is_some() && y.is_some() && x != y {
                // §REQ-cross-platform.2: an unknown bit (Windows) is never compared.
                Some("mode")
            } else {
                None
            }
        }
        (Symlink { target: a, sha256: x }, Symlink { target: b, sha256: y }) => {
            if a != b {
                Some("link")
            } else if x != y {
                Some("modified")
            } else {
                None
            }
        }
        _ => Some("link"),
    }
}

/// Who takes a generation, as the header records it. §FS-rhei-seal.3.2
#[derive(Debug, Clone, Copy)]
enum SealTaker<'a> {
    Run(&'a str),
    Verb,
}

/// The canonical bytes of one generation file. §FS-rhei-seal.3.2 §FS-rhei-seal.3.4
fn seal_record_text(
    scope: &SealScope,
    snapshot: &SealSnapshot,
    generation: u32,
    taker: SealTaker<'_>,
) -> String {
    let (taken_by, run_id) = match taker {
        SealTaker::Run(id) => ("run", Some(id)),
        SealTaker::Verb => ("rhei seal", None),
    };
    let directories: Vec<serde_json::Value> = snapshot
        .directories
        .iter()
        .map(|(path, present)| serde_json::json!({ "path": path, "present": present }))
        .collect();
    let entries: Vec<serde_json::Value> = snapshot
        .entries
        .iter()
        .map(|(path, entry)| match entry {
            SealEntry::File { sha256, executable } => serde_json::json!({
                "path": path, "kind": "file", "sha256": sha256, "executable": executable,
            }),
            SealEntry::Symlink { target, sha256 } => serde_json::json!({
                "path": path, "kind": "symlink", "target": target, "sha256": sha256,
            }),
            SealEntry::Absent => serde_json::json!({ "path": path, "kind": "absent" }),
        })
        .collect();
    let record = serde_json::json!({
        "schema": SEAL_SCHEMA,
        "generation": generation,
        "rhei": scope.rhei,
        "taken_by": taken_by,
        "run_id": run_id,
        "taken_at": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        "rhei_build": rhei_build_record(),
        "machine": snapshot.machine,
        "directories": directories,
        "entries": entries,
    });
    canonical_json_text(&record)
}

/// Read a generation file back into the snapshot it sealed. §FS-rhei-seal.3.2
fn parse_seal_record(value: &serde_json::Value) -> Option<SealSnapshot> {
    if value.get("schema")?.as_str()? != SEAL_SCHEMA {
        return None;
    }
    let mut snapshot = SealSnapshot {
        machine: value.get("machine")?.as_str().map(str::to_string),
        ..SealSnapshot::default()
    };
    for dir in value.get("directories")?.as_array()? {
        snapshot
            .directories
            .insert(dir.get("path")?.as_str()?.to_string(), dir.get("present")?.as_bool()?);
    }
    for entry in value.get("entries")?.as_array()? {
        let text = |key: &str| entry.get(key).and_then(|v| v.as_str()).map(str::to_string);
        let parsed = match entry.get("kind")?.as_str()? {
            "file" => SealEntry::File {
                sha256: text("sha256")?,
                executable: entry.get("executable").and_then(|v| v.as_bool()),
            },
            "symlink" => SealEntry::Symlink { target: text("target")?, sha256: text("sha256") },
            "absent" => SealEntry::Absent,
            _ => return None,
        };
        snapshot.entries.insert(text("path")?, parsed);
    }
    Some(snapshot)
}

/// The latest generation of a rhei's seal and what it sealed, or `None` when
/// the rhei has no seal. §FS-rhei-seal.3.1
fn latest_seal_generation(scope: &SealScope) -> MietteResult<Option<(u32, SealSnapshot)>> {
    let dir = scope.seal_dir();
    let read = match fs::read_dir(&dir) {
        Ok(read) => read,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(file_io_report(&dir, "could not read", err)),
    };
    let latest = read
        .filter_map(Result::ok)
        .filter_map(|item| item.file_name().to_str()?.strip_suffix(".json")?.parse::<u32>().ok())
        .filter(|n| *n >= 1)
        .max();
    let Some(generation) = latest else { return Ok(None) };
    let path = dir.join(format!("{generation}.json"));
    let text = fs::read_to_string(&path).map_err(|err| file_io_report(&path, "could not read", err))?;
    let snapshot = serde_json::from_str(&text).ok().as_ref().and_then(parse_seal_record);
    match snapshot {
        Some(snapshot) => Ok(Some((generation, snapshot))),
        None => Err(miette!(
            help = "restore the file from version control, or remove the rhei's runtime/seal/ to start over",
            "'{}' is not a {SEAL_SCHEMA} generation",
            path.display()
        )),
    }
}

/// Write generation `n` once: a file that already exists is never replaced.
/// §FS-rhei-seal.3.1
fn write_seal_generation(scope: &SealScope, generation: u32, text: &str) -> MietteResult<PathBuf> {
    let dir = scope.seal_dir();
    fs::create_dir_all(&dir).map_err(|err| file_io_report(&dir, "could not create", err))?;
    let path = dir.join(format!("{generation}.json"));
    let temp = dir.join(format!(".{generation}.json.{}.tmp", std::process::id()));
    fs::write(&temp, text).map_err(|err| file_io_report(&temp, "could not write", err))?;
    // A hard link publishes the complete file and refuses an existing name.
    let linked = fs::hard_link(&temp, &path);
    let _ = fs::remove_file(&temp);
    linked.map_err(|err| file_io_report(&path, "could not write seal generation", err))?;
    Ok(path)
}

/// A generation just written, with the one it follows. §FS-rhei-seal.5
struct SealTaken {
    generation: u32,
    previous: Option<(u32, SealSnapshot)>,
    snapshot: SealSnapshot,
}

/// Append the next generation of `scope`'s seal from the disk now.
/// §FS-rhei-seal.1 §FS-rhei-seal.3.1
fn take_seal(scope: &SealScope, taker: SealTaker<'_>) -> MietteResult<SealTaken> {
    let previous = latest_seal_generation(scope)?;
    let generation = previous.as_ref().map_or(1, |(n, _)| n + 1);
    let snapshot = take_seal_snapshot(scope)?;
    write_seal_generation(scope, generation, &seal_record_text(scope, &snapshot, generation, taker))?;
    Ok(SealTaken { generation, previous, snapshot })
}

/// The line a new seal prints. §FS-rhei-seal.5 §FS-rhei-seal.8.1
fn sealed_line(rhei: &str, taken: &SealTaken) -> String {
    match &taken.previous {
        None => format!(
            "Sealed rhei `{rhei}`: generation 1, {} files",
            taken.snapshot.file_count()
        ),
        Some((previous, _)) => {
            format!("Sealed rhei `{rhei}`: generation {} (generation {previous} kept)", taken.generation)
        }
    }
}
