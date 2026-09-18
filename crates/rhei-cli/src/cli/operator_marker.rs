/// Complete file images and the exact ledger evidence for one hop. §FS-rhei-recover.2
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ForcedMarker {
    version: u8,
    recovery_id: String,
    hop: ForcedHop,
    files: Vec<ForcedFile>,
    ledger: ForcedLedger,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
/// §FS-rhei-recover.2
struct ForcedHop { task_id: String, from: String, to: String }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ForcedFile { path: String, roles: Vec<String>, before: ForcedImage, after: ForcedImage }

/// Absence remains distinct from a present zero-byte file. §FS-rhei-recover.2
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
enum ForcedImage { Absent, Present { bytes: String } }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ForcedLedger { path: String, offset: u64, prefix_sha256: String, metadata_line: String, movement_line: String }

/// The marker itself must not escape through a symlinked .rhei or marker path. §FS-rhei-recover.2
fn forced_marker_location(root: &Path) -> MietteResult<()> {
    for (path, directory) in [(root.join(".rhei"), true), (root.join(rhei_core::root_access::MARKER), false)] {
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() || (directory && !meta.is_dir()) || (!directory && !meta.is_file()) =>
                return Err(miette!("forced-recovery marker location is not a regular contained path: {}", path.display())),
            Ok(_) => (),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => (),
            Err(err) => return Err(file_io_report(&path, "failed to inspect recovery marker location", err)),
        }
    }
    Ok(())
}

impl ForcedImage {
    /// §FS-rhei-recover.2
    fn present(bytes: &[u8]) -> Self {
        Self::Present { bytes: rhei_core::transition_history::encode(bytes) }
    }
    fn read(path: &Path) -> MietteResult<Self> {
        match fs::read(path) {
            Ok(bytes) => Ok(Self::present(&bytes)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Self::Absent),
            Err(err) => Err(file_io_report(path, "failed to read recovery image", err)),
        }
    }
    /// §FS-rhei-recover.2
    fn bytes(&self) -> MietteResult<Option<Vec<u8>>> {
        match self {
            Self::Absent => Ok(None),
            Self::Present { bytes } => rhei_core::transition_history::decode(bytes)
                .map(Some).map_err(|err| miette!("invalid recovery image: {err}")),
        }
    }
}

/// §FS-rhei-recover.2
fn forced_digest(bytes: &[u8]) -> String {
    use sha2::Digest;
    format!("{:x}", sha2::Sha256::digest(bytes))
}

/// Reject lexical escapes, symlinks and nonregular images before any replay. §FS-rhei-recover.2
fn forced_image_path(root: &Path, relative: &str) -> MietteResult<PathBuf> {
    if relative.is_empty() || relative.contains('\\') || relative.contains(':')
        || relative.split('/').any(|part| part.is_empty() || part == "." || part == "..")
        || Path::new(relative).is_absolute() {
        return Err(miette!("invalid recovery image path '{relative}'"));
    }
    let mut path = root.to_path_buf();
    let parts = relative.split('/').collect::<Vec<_>>();
    for (index, part) in parts.iter().enumerate() {
        path.push(part);
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink()
                || (index + 1 == parts.len() && !meta.is_file())
                || (index + 1 != parts.len() && !meta.is_dir()) => {
                return Err(miette!("recovery image path is not a regular contained file: {}", path.display()));
            }
            Ok(_) => (),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => (),
            Err(err) => return Err(file_io_report(&path, "failed to inspect recovery image path", err)),
        }
    }
    Ok(path)
}

impl ForcedMarker {
    /// §FS-rhei-recover.2
    fn bytes(&self) -> MietteResult<Vec<u8>> {
        let mut bytes = rhei_core::transition_history::canonical_json(self).map_err(|err| miette!("{err}"))?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    /// Canonical reserialization also catches duplicate keys and alternate JSON. §FS-rhei-recover.2
    fn parse(root: &Path, bytes: &[u8]) -> MietteResult<Self> {
        let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|err| miette!("{err}"))?;
        if let Some(version) = value["version"].as_u64().filter(|version| *version != 1) {
            return Err(miette!("unsupported forced-recovery marker version {version}"));
        }
        let marker: Self = serde_json::from_slice(bytes).map_err(|err| miette!("{err}"))?;
        let id = uuid::Uuid::parse_str(&marker.recovery_id).map_err(|err| miette!("{err}"))?;
        if id.get_version_num() != 7 || id.to_string() != marker.recovery_id || marker.bytes()? != bytes {
            return Err(miette!("noncanonical marker or recovery id"));
        }
        let mut previous = None;
        for file in &marker.files {
            forced_image_path(root, &file.path)?;
            if previous.is_some_and(|p: &str| p >= file.path.as_str())
                || file.path == marker.ledger.path || file.path.starts_with(".rhei/")
                || file.roles.is_empty() || file.roles.windows(2).any(|p| p[0] >= p[1])
                || file.roles.iter().any(|r| !["task", "metadata", "checkpoint", "result"].contains(&r.as_str())) {
                return Err(miette!("invalid recovery image inventory"));
            }
            file.before.bytes()?;
            file.after.bytes()?;
            previous = Some(&file.path);
        }
        forced_image_path(root, &marker.ledger.path)?;
        if marker.files.is_empty() || !marker.files.iter().any(|file| file.roles.iter().any(|r| r == "task"))
            || marker.ledger.path != "runtime/state-transitions.log"
            || !rhei_core::transition_history::is_digest(&marker.ledger.prefix_sha256) {
            return Err(miette!("incomplete recovery marker"));
        }
        let pair = format!("{}{}", marker.ledger.metadata_line, marker.ledger.movement_line);
        let entries = rhei_core::transition_history::parse(&pair).map_err(|err| miette!("{err}"))?;
        if entries.len() != 1 { return Err(miette!("marker needs exactly one audit pair")); }
        let entry = &entries[0];
        let audit = entry.audit.as_ref().ok_or_else(|| miette!("marker has no exceptional audit"))?;
        let (metadata, movement) = audit.pair().map_err(|err| miette!("{err}"))?;
        if entry.task_id != marker.hop.task_id || entry.from != marker.hop.from || entry.to != marker.hop.to
            || audit.recovery_id != marker.recovery_id || metadata != marker.ledger.metadata_line
            || movement != marker.ledger.movement_line {
            return Err(miette!("marker hop contradicts audit pair"));
        }
        Ok(marker)
    }
}
