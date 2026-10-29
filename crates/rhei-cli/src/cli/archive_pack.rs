// `rhei archive`'s packer: check the run through a source that remembers what
// it read, then hash each member again as it is written and move the tar into
// place without replacing anything. §FS-rhei-archive.2 §FS-rhei-archive.3

/// One member to write: the directory it is read under, its path there, and
/// the sha256 it must hash to while written. §FS-rhei-archive.3.2
struct PackedMember {
    base: PathBuf,
    relative: String,
    sha256: String,
}

/// Read `relative` under `base` for packing: held to the member-path rule and
/// never followed through a symlink, so nothing resolves outside `base`.
/// §FS-rhei-archive.3.3
fn read_for_packing(base: &Path, relative: &str) -> SourceRead {
    if !archive_member_path_safe(relative) {
        return SourceRead::Unsafe;
    }
    let mut at = base.to_path_buf();
    for part in relative.split('/') {
        at.push(part);
        match fs::symlink_metadata(&at) {
            Ok(meta) if meta.file_type().is_symlink() => return SourceRead::Unsafe,
            Ok(_) => {}
            Err(_) => return SourceRead::Absent,
        }
    }
    fs::read(&at).map_or(SourceRead::Absent, SourceRead::Found)
}

/// The workspace as the packer reads it: only the one run, each path through
/// [`read_for_packing`], and every file read remembered as a member, with the
/// reset events and snapshot generations the index lists. §FS-rhei-archive.2
struct PackingSource {
    workspace: WorkspaceSource,
    run_id: String,
    members: std::cell::RefCell<BTreeMap<String, PackedMember>>,
    events: std::cell::RefCell<BTreeSet<String>>,
    generations: std::cell::RefCell<Vec<serde_json::Value>>,
    refused: std::cell::RefCell<Vec<String>>,
}

impl PackingSource {
    fn new(root: &Path, run_id: &str) -> Self {
        PackingSource {
            workspace: WorkspaceSource::new(root),
            run_id: run_id.to_string(),
            members: std::cell::RefCell::default(),
            events: std::cell::RefCell::default(),
            generations: std::cell::RefCell::default(),
            refused: std::cell::RefCell::default(),
        }
    }

    /// [`read_for_packing`], remembering what it found as `member`.
    fn take(&self, base: &Path, relative: &str, member: &str) -> SourceRead {
        let read = read_for_packing(base, relative);
        if let SourceRead::Found(bytes) = &read {
            let packed = PackedMember {
                base: base.to_path_buf(),
                relative: relative.to_string(),
                sha256: sha256_hex(bytes),
            };
            self.members.borrow_mut().insert(member.to_string(), packed);
        }
        read
    }
}

impl EvidenceSource for PackingSource {
    fn runs(&self) -> Vec<SourceRun> {
        let dir = format!("runtime/evidence/runs/{}", self.run_id);
        let path = format!("{dir}/manifest.json");
        let manifest = match self.take(&self.workspace.root, &path, &path) {
            SourceRead::Found(bytes) => Some((path, bytes)),
            SourceRead::Unsafe => {
                self.refused.borrow_mut().push(path);
                None
            }
            SourceRead::Absent => None,
        };
        vec![SourceRun { run_id: self.run_id.clone(), dir, manifest }]
    }

    /// Only the reset events that list the run, each packed. §FS-rhei-archive.2
    fn events(&self) -> Vec<Vec<u8>> {
        let root = &self.workspace.root;
        let mut names: Vec<String> = fs::read_dir(evidence_dir(root).join("events"))
            .map(|all| {
                all.filter_map(Result::ok)
                    .filter_map(|e| e.file_name().into_string().ok())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        let mut out = Vec::new();
        for name in names {
            let path = format!("runtime/evidence/events/{name}");
            let bytes = match read_for_packing(root, &path) {
                SourceRead::Found(bytes) => bytes,
                SourceRead::Unsafe => {
                    self.refused.borrow_mut().push(path);
                    continue;
                }
                SourceRead::Absent => continue,
            };
            let event: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_default();
            let lists = event["runs"]
                .as_array()
                .is_some_and(|runs| runs.iter().any(|r| *r == *self.run_id));
            if lists {
                self.take(root, &path, &path);
                self.events.borrow_mut().insert(path);
                out.push(bytes);
            }
        }
        out
    }

    fn read(&self, path: &str) -> SourceRead {
        self.take(&self.workspace.root, path, path)
    }

    fn records(&self, run: &SourceRun) -> Vec<String> {
        self.workspace.records(run)
    }

    /// A generation from the cache the settings name, wherever it is, packed
    /// under `snapshots/` at its cache path. §FS-rhei-archive.2.1 §FS-rhei-archive.4.1
    fn generation(&self, reference: &serde_json::Value) -> Option<SourceGeneration> {
        let (cache, records) = self.workspace.cached();
        let record = records.iter().find(|record| snapshot_parent_ref(record) == *reference)?;
        let path = evidence_relative(&self.workspace.root, &record.transcript_path());
        let relative = |full: &Path| -> Option<String> {
            let parts: Vec<String> = full
                .strip_prefix(cache)
                .ok()?
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            Some(parts.join("/"))
        };
        let (Some(own), Some(transcript)) =
            (relative(&record.path.join("manifest.json")), relative(&record.transcript_path()))
        else {
            return Some(SourceGeneration { path, transcript: SourceRead::Unsafe });
        };
        let members = [format!("snapshots/{own}"), format!("snapshots/{transcript}")];
        let read = match self.take(cache, &own, &members[0]) {
            SourceRead::Found(_) => self.take(cache, &transcript, &members[1]),
            other => other,
        };
        if matches!(read, SourceRead::Found(_)) {
            let entry = serde_json::json!({ "ref": reference, "members": members });
            self.generations.borrow_mut().push(entry);
        }
        Some(SourceGeneration { path, transcript: read })
    }
}

/// A writer that hashes what passes through it, for the archive's digest.
struct HashingWriter<W: Write> {
    inner: W,
    hasher: Sha256,
}

impl<W: Write> Write for HashingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let written = self.inner.write(buf)?;
        self.hasher.update(&buf[..written]);
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// The file written: where, its full sha256, and what it holds.
struct WrittenArchive {
    path: PathBuf,
    sha256: String,
    members: usize,
    generations: usize,
}

/// One member, mode `0644`, owner `0`, mtime `0`: a ustar header, or a GNU
/// long name before it when ustar cannot hold the path. §FS-rhei-archive.4
fn append_archive_member<W: Write>(
    builder: &mut tar::Builder<W>,
    path: &str,
    bytes: &[u8],
) -> std::io::Result<()> {
    let mut header = tar::Header::new_ustar();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o644);
    header.set_uid(0);
    header.set_gid(0);
    header.set_mtime(0);
    header.set_entry_type(tar::EntryType::Regular);
    builder.append_data(&mut header, path, bytes)
}

/// Write the archive `source` read beside `output`, every member hashed again
/// as it is written, and move it into place without replacing anything.
/// §FS-rhei-archive.3.2 §FS-rhei-archive.4.2
fn write_archive(source: &PackingSource, output: &Path) -> Result<WrittenArchive, VerifyFinding> {
    let run = Some(source.run_id.as_str());
    let failed = |code| VerifyFinding::new(code, run, Some(output.display().to_string()));
    let parent = output.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let temp = tempfile::Builder::new()
        .prefix(".rhei-archive-")
        .suffix(".tmp")
        .tempfile_in(parent)
        .map_err(|_| failed("write_failed"))?;
    let members = source.members.borrow();
    let manifest = format!("runtime/evidence/runs/{}/manifest.json", source.run_id);
    let generations = source.generations.borrow();
    let index = serde_json::json!({
        "schema": ARCHIVE_SCHEMA,
        "run_id": source.run_id,
        "manifest": { "path": manifest, "sha256": members.get(&manifest).map(|m| m.sha256.clone()) },
        "events": *source.events.borrow(),
        "snapshots": *generations,
    });
    let file = temp.as_file().try_clone().map_err(|_| failed("write_failed"))?;
    let writer = HashingWriter { inner: std::io::BufWriter::new(file), hasher: Sha256::new() };
    let mut builder = tar::Builder::new(writer);
    append_archive_member(&mut builder, ARCHIVE_INDEX, &evidence_bytes(&index))
        .map_err(|_| failed("write_failed"))?;
    for (path, member) in members.iter() {
        let bytes = match read_for_packing(&member.base, &member.relative) {
            SourceRead::Found(bytes) => bytes,
            other => {
                let code = if matches!(other, SourceRead::Unsafe) {
                    "archive_unsafe_member"
                } else {
                    "missing"
                };
                let mut finding = VerifyFinding::new(code, run, Some(path.clone()));
                finding.recorded_sha256 = Some(member.sha256.clone());
                return Err(finding);
            }
        };
        let found = sha256_hex(&bytes);
        if found != member.sha256 {
            let mut finding = VerifyFinding::new("mismatch", run, Some(path.clone()));
            finding.recorded_sha256 = Some(member.sha256.clone());
            finding.found_sha256 = Some(found);
            return Err(finding);
        }
        append_archive_member(&mut builder, path, &bytes).map_err(|_| failed("write_failed"))?;
    }
    let mut writer = builder.into_inner().map_err(|_| failed("write_failed"))?;
    writer.flush().map_err(|_| failed("write_failed"))?;
    let HashingWriter { inner, hasher } = writer;
    drop(inner);
    let mut sha256 = String::with_capacity(64);
    for byte in hasher.finalize() {
        sha256.push_str(&format!("{byte:02x}"));
    }
    temp.as_file().sync_all().map_err(|_| failed("write_failed"))?;
    temp.persist_noclobber(output).map_err(|err| {
        failed(if err.error.kind() == std::io::ErrorKind::AlreadyExists {
            "output_exists"
        } else {
            "write_failed"
        })
    })?;
    Ok(WrittenArchive {
        path: output.to_path_buf(),
        sha256,
        members: members.len() + 1,
        generations: generations.len(),
    })
}

/// Check the run in place and write its archive, or say why not: an unknown
/// run, a run that is not finalized or fails, an unsafe path, an output that
/// exists, a failed write. §FS-rhei-archive.3 §FS-rhei-archive.5
fn pack_archive(
    root: &Path,
    run_id: &str,
    output: &Path,
) -> (VerifyOutcome, Option<WrittenArchive>) {
    let one_component = archive_member_path_safe(run_id) && !run_id.contains('/');
    if !one_component || !evidence_dir(root).join("runs").join(run_id).is_dir() {
        let mut outcome = VerifyOutcome::default();
        outcome.findings.push(VerifyFinding::new("unknown_run", Some(run_id), None));
        return (outcome, None);
    }
    let source = PackingSource::new(root, run_id);
    let mut outcome = verify_source(&source, None);
    for path in source.refused.take() {
        outcome.findings.push(VerifyFinding::new(
            "archive_unsafe_member",
            Some(run_id),
            Some(path),
        ));
    }
    if !outcome.verified() {
        return (outcome, None);
    }
    if fs::symlink_metadata(output).is_ok() {
        let finding =
            VerifyFinding::new("output_exists", Some(run_id), Some(output.display().to_string()));
        outcome.findings.push(finding);
        return (outcome, None);
    }
    match write_archive(&source, output) {
        Ok(written) => (outcome, Some(written)),
        Err(finding) => {
            outcome.findings.push(finding);
            (outcome, None)
        }
    }
}
