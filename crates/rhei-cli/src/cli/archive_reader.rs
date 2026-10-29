// Reading an archive `rhei archive` packed: the tar is parsed in memory and
// never unpacked, and it is a second evidence source behind verify's one
// checker. §FS-rhei-verify.2.1 §FS-rhei-archive.4

const ARCHIVE_INDEX: &str = "rhei-archive.json";
const ARCHIVE_SCHEMA: &str = "rhei.archive.v1";
const TAR_BLOCK: usize = 512;

/// Whether `path` may name a member: relative, `/`-separated, not empty, with
/// no `.` or `..` component, no drive and no `\`. §FS-rhei-verify.2.1
fn archive_member_path_safe(path: &str) -> bool {
    let drive = |part: &str| {
        part.len() >= 2 && part.as_bytes()[0].is_ascii_alphabetic() && part.as_bytes()[1] == b':'
    };
    !path.is_empty()
        && !path.contains('\\')
        && !path.split('/').next().is_some_and(drive)
        && path.split('/').all(|part| !part.is_empty() && part != "." && part != "..")
}

/// A tar number field: octal, or GNU base-256 when its high bit is set.
fn tar_number(field: &[u8]) -> Option<u64> {
    if field.first().is_some_and(|byte| byte & 0x80 != 0) {
        return field[1..]
            .iter()
            .try_fold(0u64, |value, byte| value.checked_mul(256)?.checked_add(u64::from(*byte)));
    }
    let text = std::str::from_utf8(field).ok()?;
    let digits = text.trim_matches(|c: char| c == ' ' || c == '\0');
    if digits.is_empty() {
        return Some(0);
    }
    u64::from_str_radix(digits, 8).ok()
}

/// Whether `block` is a ustar or GNU header whose checksum holds.
/// §FS-rhei-verify.1
fn tar_header_valid(block: &[u8]) -> bool {
    if block.len() < TAR_BLOCK {
        return false;
    }
    let magic = &block[257..265];
    if magic != b"ustar\x0000" && magic != b"ustar  \x00" {
        return false;
    }
    let sum: u64 = block[..TAR_BLOCK]
        .iter()
        .enumerate()
        .map(|(at, byte)| if (148..156).contains(&at) { 32 } else { u64::from(*byte) })
        .sum();
    tar_number(&block[148..156]) == Some(sum)
}

/// A header's name: ustar's prefix joined to its name, a GNU header's name.
fn tar_header_name(block: &[u8]) -> Vec<u8> {
    let field = |range: std::ops::Range<usize>| -> Vec<u8> {
        let bytes = &block[range];
        bytes[..bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len())].to_vec()
    };
    let name = field(0..100);
    let prefix = if &block[257..263] == b"ustar\0" { field(345..500) } else { Vec::new() };
    if prefix.is_empty() {
        name
    } else {
        [prefix, b"/".to_vec(), name].concat()
    }
}

/// Whether `path` is an archive rather than a plan or a workspace: a regular
/// file whose first 512 bytes are a tar header. §FS-rhei-verify.1
fn is_archive_file(path: &Path) -> bool {
    use std::io::Read as _;
    if !fs::metadata(path).is_ok_and(|meta| meta.is_file()) {
        return false;
    }
    let mut block = vec![0u8; TAR_BLOCK];
    fs::File::open(path).and_then(|mut file| file.read_exact(&mut block)).is_ok()
        && tar_header_valid(&block)
}

/// An archive read into memory: its index, each safe member once by path, and
/// the paths of the members refused on the way. §FS-rhei-verify.2.1
struct ArchiveSource {
    index: serde_json::Value,
    members: BTreeMap<String, Vec<u8>>,
    refused: Vec<String>,
}

/// Parse `bytes` as an archive, or `None` when it cannot be read at all: it is
/// truncated, its first member is not the index, the index has an unknown
/// `schema`, or anything but zeros follows the end-of-archive blocks.
/// §FS-rhei-verify.3 §FS-rhei-archive.4
fn read_archive(bytes: &[u8]) -> Option<ArchiveSource> {
    let mut source = ArchiveSource {
        index: serde_json::Value::Null,
        members: BTreeMap::new(),
        refused: Vec::new(),
    };
    let mut at = 0;
    let mut long_name: Option<Vec<u8>> = None;
    let mut first = true;
    loop {
        let block = bytes.get(at..at + TAR_BLOCK)?;
        if block.iter().all(|byte| *byte == 0) {
            // The end of the archive: nothing but zero padding may follow.
            let ended = !first && long_name.is_none() && bytes[at..].iter().all(|byte| *byte == 0);
            return ended.then_some(source);
        }
        if !tar_header_valid(block) {
            return None;
        }
        let size = usize::try_from(tar_number(&block[124..136])?).ok()?;
        let start = at + TAR_BLOCK;
        let data = bytes.get(start..start.checked_add(size)?)?;
        at = start + size.div_ceil(TAR_BLOCK) * TAR_BLOCK;
        if at > bytes.len() {
            return None;
        }
        let kind = block[156];
        if kind == b'L' {
            // The one extension: a GNU long name for the entry after it. §FS-rhei-archive.4
            let end = data.iter().rposition(|byte| *byte != 0).map_or(0, |last| last + 1);
            long_name = Some(data[..end].to_vec());
            continue;
        }
        let name = long_name.take().unwrap_or_else(|| tar_header_name(block));
        let name = String::from_utf8_lossy(&name).into_owned();
        let regular = kind == b'0' || kind == 0;
        if std::mem::take(&mut first) {
            if !regular || name != ARCHIVE_INDEX {
                return None;
            }
            let index: serde_json::Value = serde_json::from_slice(data).ok()?;
            let readable = index["schema"] == ARCHIVE_SCHEMA
                && index["run_id"].is_string()
                && index["manifest"]["path"].is_string();
            if !readable {
                return None;
            }
            source.index = index;
        }
        if !regular || !archive_member_path_safe(&name) || source.members.contains_key(&name) {
            source.refused.push(name);
            continue;
        }
        source.members.insert(name, data.to_vec());
    }
}

impl ArchiveSource {
    fn run_id(&self) -> &str {
        self.index["run_id"].as_str().unwrap_or_default()
    }

    fn manifest_path(&self) -> &str {
        self.index["manifest"]["path"].as_str().unwrap_or_default()
    }

    /// The first index entry for `reference`: the `manifest.json` member it
    /// lists, and every member it lists. §FS-rhei-verify.2.1
    fn packed_generation(&self, reference: &serde_json::Value) -> Option<(String, Vec<String>)> {
        let packed =
            self.index["snapshots"].as_array()?.iter().find(|entry| entry["ref"] == *reference)?;
        let listed = self.listed(&packed["members"]);
        let manifest =
            listed.iter().find(|path| path.rsplit('/').next() == Some("manifest.json"))?.clone();
        Some((manifest, listed))
    }

    /// The members a check reads, and so the only ones named: the index never
    /// names one on its own, since `--digest` does not cover it. §FS-rhei-verify.2.1
    fn named_members(&self, manifest: &serde_json::Value) -> BTreeSet<String> {
        let mut named: BTreeSet<String> =
            [ARCHIVE_INDEX.to_string(), self.manifest_path().to_string()].into();
        named.extend(
            manifest_named(manifest)
                .iter()
                .filter_map(|entry| entry["path"].as_str().map(str::to_string)),
        );
        for snapshot in manifest["snapshots"].as_array().into_iter().flatten() {
            let Some((own, _)) = self.packed_generation(&snapshot["ref"]) else { continue };
            named.insert(own);
            if let Some(generation) = self.generation(&snapshot["ref"]) {
                if matches!(generation.transcript, SourceRead::Found(_)) {
                    named.insert(generation.path);
                }
            }
        }
        for path in self.listed(&self.index["events"]) {
            let event: serde_json::Value = self
                .members
                .get(&path)
                .and_then(|bytes| serde_json::from_slice(bytes).ok())
                .unwrap_or_default();
            let lists = event["runs"]
                .as_array()
                .is_some_and(|runs| runs.iter().any(|r| r.as_str() == Some(self.run_id())));
            if lists {
                named.insert(path);
            }
        }
        named
    }

    /// The paths an index list names.
    fn listed(&self, value: &serde_json::Value) -> Vec<String> {
        value.as_array().map_or_else(Vec::new, |all| {
            all.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()
        })
    }
}

impl EvidenceSource for ArchiveSource {
    fn runs(&self) -> Vec<SourceRun> {
        let path = self.manifest_path();
        vec![SourceRun {
            run_id: self.run_id().to_string(),
            dir: path.rsplit_once('/').map_or("", |(dir, _)| dir).to_string(),
            manifest: self.members.get(path).map(|bytes| (path.to_string(), bytes.clone())),
        }]
    }

    /// The reset events the index lists, never a member by its prefix.
    /// §FS-rhei-archive.4.2
    fn events(&self) -> Vec<Vec<u8>> {
        self.listed(&self.index["events"])
            .iter()
            .filter_map(|path| self.members.get(path).cloned())
            .collect()
    }

    fn read(&self, path: &str) -> SourceRead {
        self.members.get(path).cloned().map_or(SourceRead::Absent, SourceRead::Found)
    }

    fn records(&self, run: &SourceRun) -> Vec<String> {
        let prefix = format!("{}/", run.dir);
        self.members
            .keys()
            .filter(|path| {
                path.strip_prefix(&prefix).is_some_and(|name| {
                    !name.contains('/')
                        && (name.ends_with(".start.json") || name.ends_with(".end.json"))
                })
            })
            .cloned()
            .collect()
    }

    /// A generation from the members the index lists for its `ref`: its
    /// `manifest.json` says where the transcript is. §FS-rhei-verify.2.1
    fn generation(&self, reference: &serde_json::Value) -> Option<SourceGeneration> {
        let (manifest, listed) = self.packed_generation(reference)?;
        let manifest = &manifest;
        let Some(bytes) = self.members.get(manifest) else {
            return Some(SourceGeneration {
                path: manifest.clone(),
                transcript: SourceRead::Absent,
            });
        };
        let own: serde_json::Value = serde_json::from_slice(bytes).unwrap_or_default();
        let dir = manifest.rsplit_once('/').map_or("", |(dir, _)| dir);
        let path = format!("{dir}/{}", snapshot_transcript_name(&own));
        let transcript = match self.members.get(&path) {
            Some(bytes) if listed.contains(&path) => SourceRead::Found(bytes.clone()),
            _ => SourceRead::Absent,
        };
        Some(SourceGeneration { path, transcript })
    }
}

/// What only an archive can get wrong, after the shared checks: a refused
/// member, a member no check reads or one at a path a packed reset event
/// prunes (`unlisted`), and a member the index lists that is absent (`missing`).
/// §FS-rhei-verify.2.1 §FS-rhei-verify.3
fn archive_member_findings(source: &ArchiveSource, findings: &mut Vec<VerifyFinding>) {
    let run = Some(source.run_id());
    for path in &source.refused {
        findings.push(VerifyFinding::new("archive_unsafe_member", run, Some(path.clone())));
    }
    let pruned = pruned_by_events(&source.events()).remove(source.run_id()).unwrap_or_default();
    let manifest: serde_json::Value = source
        .members
        .get(source.manifest_path())
        .and_then(|bytes| serde_json::from_slice(bytes).ok())
        .unwrap_or_default();
    let named = source.named_members(&manifest);
    let events = source.listed(&source.index["events"]);
    let reported: BTreeSet<String> = findings
        .iter()
        .filter(|finding| finding.reason_code == "unlisted")
        .filter_map(|finding| finding.path.clone())
        .collect();
    for path in source.members.keys() {
        if (!named.contains(path) || pruned.contains(path)) && !reported.contains(path) {
            findings.push(VerifyFinding::new("unlisted", run, Some(path.clone())));
        }
    }
    for path in events.iter().chain([&source.manifest_path().to_string()]) {
        if !source.members.contains_key(path) {
            findings.push(VerifyFinding::new("missing", run, Some(path.clone())));
        }
    }
}

/// Verify the archive at `path`: one `archive_unreadable` finding and nothing
/// else when it cannot be read, otherwise the checks a workspace gets and then
/// the archive's own. §FS-rhei-verify.2.1 §FS-rhei-verify.3
fn verify_archive(path: &Path, digest: Option<&str>) -> VerifyOutcome {
    let Some(source) = fs::read(path).ok().as_deref().and_then(read_archive) else {
        let mut outcome = VerifyOutcome::default();
        outcome.findings.push(VerifyFinding::new("archive_unreadable", None, None));
        return outcome;
    };
    let mut outcome = verify_source(&source, digest);
    archive_member_findings(&source, &mut outcome.findings);
    outcome
}
