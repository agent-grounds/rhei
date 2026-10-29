// The per-invocation evidence store: `runtime/evidence/` in the run's root,
// written once per fact and never rewritten. §FS-rhei-evidence.1

/// The `evidence` settings block. §FS-rhei-agents.1.1.8
#[derive(Debug, Clone, Default, Deserialize)]
struct EvidenceSettings {
    #[serde(default)]
    retention: EvidenceRetention,
}

/// Whether retained values keep their bytes. §FS-rhei-evidence.5.2
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum EvidenceRetention {
    #[default]
    Retain,
    HashOnly,
}

impl<'de> Deserialize<'de> for EvidenceRetention {
    /// An unknown value is refused by name, never read as the default.
    /// §FS-rhei-agents.1.1.8
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "retain" => Ok(EvidenceRetention::Retain),
            "hash-only" => Ok(EvidenceRetention::HashOnly),
            other => Err(serde::de::Error::custom(format!(
                "evidence.retention must be `retain` or `hash-only`, not `{other}`"
            ))),
        }
    }
}

/// The evidence of the one run this process is: its id, its root, and what
/// it has collected for the manifest. §FS-rhei-evidence.1 §FS-rhei-evidence.9
struct EvidenceRun {
    run_id: String,
    /// The directory whose `runtime/run.json` describes the run.
    root: PathBuf,
    retention: EvidenceRetention,
    /// Read for `snapshots.redactor`, the one privacy hook. §FS-rhei-evidence.5.3
    settings: RheiSettings,
    started_at: String,
    provenance: rhei_tui::RunProvenance,
    probes: Mutex<BTreeMap<Vec<String>, VersionProbe>>,
    accounting: Mutex<BTreeSet<PathBuf>>,
    /// `{path, sha256}` once the manifest is written; it is written once.
    manifest: Mutex<Option<serde_json::Value>>,
}

static EVIDENCE_RUN: Mutex<Option<Arc<EvidenceRun>>> = Mutex::new(None);

/// The run this process is recording, if it is recording one.
fn evidence_run() -> Option<Arc<EvidenceRun>> {
    EVIDENCE_RUN.lock().unwrap_or_else(|poison| poison.into_inner()).clone()
}

/// The recording run, when `log_path` lies in its root: a process is recorded
/// by the run whose workspace holds it. §FS-rhei-evidence.1
fn evidence_run_holding(log_path: &Path) -> Option<Arc<EvidenceRun>> {
    let run = evidence_run()?;
    let dir = log_path.parent().unwrap_or(log_path);
    let dir = rhei_core::platform::canonical_path(dir).unwrap_or_else(|_| dir.to_path_buf());
    dir.starts_with(&run.root).then_some(run)
}

/// Begin recording a run: finalize any run the last process left without a
/// manifest, then hold this one. A dry run never gets here. §FS-rhei-evidence.9.3
fn start_evidence_run(
    root: &Path,
    run_id: &str,
    started_at: &str,
    provenance: &rhei_tui::RunProvenance,
) -> MietteResult<()> {
    let run = EvidenceRun::new(root, run_id, started_at, provenance, load_merged_settings(root)?);
    finalize_killed_runs(&run);
    *EVIDENCE_RUN.lock().unwrap_or_else(|poison| poison.into_inner()) = Some(Arc::new(run));
    Ok(())
}

/// The record key of an id: the first 32 hex digits of its sha256, so a raw
/// id is never a file name. §FS-rhei-evidence.2.1
fn evidence_key(id: &str) -> String {
    sha256_hex(id.as_bytes())[..32].to_string()
}

/// The one canonical encoding, with its trailing newline. §FS-rhei-evidence.1.2
fn evidence_bytes(value: &serde_json::Value) -> Vec<u8> {
    let mut text = canonical_json_text(value);
    text.push('\n');
    text.into_bytes()
}

/// Create `path` holding `value`, refusing one that already exists.
/// §FS-rhei-evidence.1.1
fn write_evidence_exclusively(path: &Path, value: &serde_json::Value) -> std::io::Result<()> {
    use std::io::Write as _;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(&evidence_bytes(value))?;
    file.sync_all()
}

/// `path` relative to `root`, `/`-separated on every platform; an absolute
/// path where it is not under `root`. §FS-rhei-evidence.1.2 §REQ-cross-platform.5
fn evidence_relative(root: &Path, path: &Path) -> String {
    let absolute = if path.is_absolute() { path.to_path_buf() } else { root.join(path) };
    let absolute = rhei_core::platform::canonical_path(&absolute).unwrap_or(absolute);
    match absolute.strip_prefix(root) {
        Ok(relative) => relative
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/"),
        Err(_) => absolute.display().to_string(),
    }
}

/// Join a recorded `/`-separated path from its components. §REQ-cross-platform.5
fn evidence_join(root: &Path, path: &str) -> PathBuf {
    path.split('/').fold(root.to_path_buf(), |joined, part| joined.join(part))
}

fn evidence_dir(root: &Path) -> PathBuf {
    root.join("runtime").join("evidence")
}

impl EvidenceRun {
    fn new(
        root: &Path,
        run_id: &str,
        started_at: &str,
        provenance: &rhei_tui::RunProvenance,
        settings: RheiSettings,
    ) -> Self {
        EvidenceRun {
            run_id: run_id.to_string(),
            root: rhei_core::platform::canonical_path(root).unwrap_or_else(|_| root.to_path_buf()),
            retention: settings.evidence.as_ref().map(|e| e.retention).unwrap_or_default(),
            settings,
            started_at: started_at.to_string(),
            provenance: provenance.clone(),
            probes: Mutex::new(BTreeMap::new()),
            accounting: Mutex::new(BTreeSet::new()),
            manifest: Mutex::new(None),
        }
    }

    fn run_dir(&self) -> PathBuf {
        evidence_dir(&self.root).join("runs").join(&self.run_id)
    }

    fn relative(&self, path: &Path) -> String {
        evidence_relative(&self.root, path)
    }

    /// Store `bytes` under their hash: a temporary name, then a rename, and
    /// an object already present is left alone. §FS-rhei-evidence.1.1
    fn put_object(&self, bytes: &[u8]) -> std::io::Result<String> {
        let hex = sha256_hex(bytes);
        let dir = evidence_dir(&self.root).join("objects").join("sha256");
        let path = dir.join(&hex);
        if path.is_file() {
            return Ok(hex);
        }
        fs::create_dir_all(&dir)?;
        let temporary = dir.join(format!(".{hex}.{}.tmp", uuid::Uuid::new_v4()));
        fs::write(&temporary, bytes)?;
        if let Err(err) = fs::rename(&temporary, &path) {
            let _ = fs::remove_file(&temporary);
            if !path.is_file() {
                return Err(err);
            }
        }
        Ok(hex)
    }

    /// A retained value: the original's hash and size, and which copy, if
    /// any, the store keeps. §FS-rhei-evidence.5 §FS-rhei-evidence.5.3
    fn retain(&self, bytes: &[u8], label: &str) -> serde_json::Value {
        let mut value = serde_json::json!({ "sha256": sha256_hex(bytes), "bytes": bytes.len() });
        let redacting = self.settings.snapshots.as_ref().is_some_and(|s| s.redactor.is_some());
        let stored = match self.retention {
            EvidenceRetention::HashOnly => "none",
            EvidenceRetention::Retain if !redacting => match self.put_object(bytes) {
                Ok(_) => "original",
                Err(err) => {
                    diag_warn!("evidence: could not store the {label}: {err}");
                    "withheld"
                }
            },
            EvidenceRetention::Retain => {
                let redacted =
                    apply_snapshot_redactor(&self.settings, &self.root, bytes.to_vec(), None, label);
                match redacted.map(|copy| self.put_object(&copy)) {
                    Ok(Ok(hex)) => {
                        value["stored_sha256"] = serde_json::json!(hex);
                        "redacted"
                    }
                    Ok(Err(err)) => {
                        diag_warn!("evidence: could not store the redacted {label}: {err}");
                        "withheld"
                    }
                    // The spawn still runs; only the copy is lost. §FS-rhei-evidence.5.3
                    Err(err) => {
                        diag_warn!("evidence: the redactor failed on the {label}, so no copy is kept: {err}");
                        "withheld"
                    }
                }
            }
        };
        value["stored"] = serde_json::json!(stored);
        value
    }

    /// Note an accounting record for the manifest. §FS-rhei-evidence.9.1
    fn note_accounting(&self, path: &Path) {
        self.accounting.lock().unwrap_or_else(|poison| poison.into_inner()).insert(path.to_path_buf());
    }
}

/// Note an accounting record the live run wrote, if a run is recording.
fn note_evidence_accounting(path: &Path) {
    if let Some(run) = evidence_run() {
        run.note_accounting(path);
    }
}

/// Every `env` object in `value`, at any depth, reduced to its sorted names.
/// §FS-rhei-evidence.6.1
fn evidence_env_names(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if key == "env" {
                    if let serde_json::Value::Object(env) = child {
                        let mut names: Vec<String> = env.keys().cloned().collect();
                        names.sort();
                        *child = serde_json::json!(names);
                        continue;
                    }
                }
                evidence_env_names(child);
            }
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(evidence_env_names),
        _ => {}
    }
}

/// The configuration object's retained value: env names only, then one
/// canonical document. §FS-rhei-evidence.6 §FS-rhei-evidence.6.2
fn evidence_config(run: &EvidenceRun, mut config: serde_json::Value) -> serde_json::Value {
    evidence_env_names(&mut config);
    run.retain(&evidence_bytes(&config), "configuration")
}
