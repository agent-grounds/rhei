// What `rhei verify`'s one checker reads a run's evidence through: a workspace
// in place, and later an archive, behind one interface, so the checks, the
// findings and the output are the same code for both. §FS-rhei-verify.2

/// One run directory as a source holds it: its id, its root-relative path,
/// and its manifest's path and bytes when it has one.
struct SourceRun {
    run_id: String,
    dir: String,
    manifest: Option<(String, Vec<u8>)>,
}

/// A snapshot generation as a source holds it: the path a finding names, and
/// the `transcript_sha256` it is held to. §FS-rhei-evidence.8
struct SourceGeneration {
    path: String,
    transcript_sha256: String,
}

/// Where the checker reads from. Every path is relative to the run's root and
/// `/`-separated. §FS-rhei-verify.2
trait EvidenceSource {
    /// Every run directory, sorted by id.
    fn runs(&self) -> Vec<SourceRun>;
    /// The bytes of each reset event. §FS-rhei-evidence.10.1
    fn events(&self) -> Vec<Vec<u8>>;
    /// The bytes at `path`, or `None` when nothing is there.
    fn read(&self, path: &str) -> Option<Vec<u8>>;
    /// The start and end records a run directory holds, sorted.
    fn records(&self, run: &SourceRun) -> Vec<String>;
    /// The generation `reference` names, or `None` when it is not cached.
    fn generation(&self, reference: &serde_json::Value) -> Option<SourceGeneration>;
}

/// A workspace read in place: the run's root, and the snapshot cache its
/// settings name, read once and only when a manifest names a generation.
/// §FS-rhei-verify.2 §FS-rhei-snapshots.7
struct WorkspaceSource {
    root: PathBuf,
    cached: std::cell::OnceCell<Vec<SnapshotRecord>>,
}

impl WorkspaceSource {
    fn new(root: &Path) -> Self {
        WorkspaceSource { root: root.to_path_buf(), cached: std::cell::OnceCell::new() }
    }

    fn cached(&self) -> &[SnapshotRecord] {
        self.cached.get_or_init(|| {
            let cache =
                snapshot_cache_dir(&load_merged_settings_for_completion(&self.root), &self.root);
            read_snapshot_records(&cache).unwrap_or_default()
        })
    }
}

/// The bytes of every reset event under `evidence`. §FS-rhei-evidence.10.1
fn workspace_events(evidence: &Path) -> Vec<Vec<u8>> {
    let Ok(entries) = fs::read_dir(evidence.join("events")) else { return Vec::new() };
    entries.filter_map(Result::ok).filter_map(|entry| fs::read(entry.path()).ok()).collect()
}

impl EvidenceSource for WorkspaceSource {
    fn runs(&self) -> Vec<SourceRun> {
        let mut dirs: Vec<PathBuf> = fs::read_dir(evidence_dir(&self.root).join("runs"))
            .map(|entries| {
                entries.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.is_dir()).collect()
            })
            .unwrap_or_default();
        dirs.sort();
        dirs.into_iter()
            .map(|dir| {
                let manifest_path = dir.join("manifest.json");
                SourceRun {
                    run_id: dir
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or_default()
                        .to_string(),
                    dir: evidence_relative(&self.root, &dir),
                    manifest: fs::read(&manifest_path)
                        .ok()
                        .map(|bytes| (evidence_relative(&self.root, &manifest_path), bytes)),
                }
            })
            .collect()
    }

    fn events(&self) -> Vec<Vec<u8>> {
        workspace_events(&evidence_dir(&self.root))
    }

    fn read(&self, path: &str) -> Option<Vec<u8>> {
        fs::read(evidence_join(&self.root, path)).ok()
    }

    fn records(&self, run: &SourceRun) -> Vec<String> {
        let mut records: Vec<PathBuf> = fs::read_dir(evidence_join(&self.root, &run.dir))
            .map(|entries| entries.filter_map(Result::ok).map(|entry| entry.path()).collect())
            .unwrap_or_default();
        records.sort();
        records
            .into_iter()
            .filter(|record| {
                let name = record.file_name().and_then(|n| n.to_str()).unwrap_or_default();
                name.ends_with(".start.json") || name.ends_with(".end.json")
            })
            .map(|record| evidence_relative(&self.root, &record))
            .collect()
    }

    fn generation(&self, reference: &serde_json::Value) -> Option<SourceGeneration> {
        let record =
            self.cached().iter().find(|record| snapshot_parent_ref(record) == *reference)?;
        Some(SourceGeneration {
            path: evidence_relative(&self.root, &record.path),
            transcript_sha256: record.manifest["transcript_sha256"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
        })
    }
}
