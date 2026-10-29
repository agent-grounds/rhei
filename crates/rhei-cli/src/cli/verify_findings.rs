// What `rhei verify` checks: every file a run manifest names, the records a
// finalized run directory holds, and the snapshot generations it used, read
// through one evidence source. It reads only. §FS-rhei-verify.2 §FS-rhei-verify.3

/// One disagreement or omission, with its stable reason. §FS-rhei-verify.3
#[derive(Clone, Debug)]
struct VerifyFinding {
    reason_code: &'static str,
    run_id: Option<String>,
    path: Option<String>,
    recorded_sha256: Option<String>,
    found_sha256: Option<String>,
}

impl VerifyFinding {
    fn new(reason_code: &'static str, run_id: Option<&str>, path: Option<String>) -> Self {
        VerifyFinding {
            reason_code,
            run_id: run_id.map(str::to_string),
            path,
            recorded_sha256: None,
            found_sha256: None,
        }
    }

    /// Whether this finding fails the verification. §FS-rhei-verify.3
    fn fails(&self) -> bool {
        matches!(
            self.reason_code,
            "mismatch" | "missing" | "unlisted" | "unknown_digest" | "nothing_to_verify"
        )
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "reason_code": self.reason_code,
            "run_id": self.run_id,
            "path": self.path,
            "recorded_sha256": self.recorded_sha256,
            "found_sha256": self.found_sha256,
        })
    }
}

/// One run as verify saw it. A run with no manifest has no digest.
#[derive(Clone, Debug, Default)]
struct VerifiedRun {
    run_id: String,
    status: String,
    exit_code: Option<i64>,
    manifest: Option<String>,
    digest: Option<String>,
    files_verified: usize,
    snapshots_verified: usize,
    invocations: Vec<(String, bool)>,
    started_at: String,
}

/// Everything one `rhei verify` found under a run's root.
#[derive(Default)]
struct VerifyOutcome {
    runs: Vec<VerifiedRun>,
    findings: Vec<VerifyFinding>,
}

impl VerifyOutcome {
    /// Exit 0 only when a manifest verified and nothing failed. §FS-rhei-verify.5
    fn verified(&self) -> bool {
        self.runs.iter().any(|run| run.digest.is_some()) && !self.findings.iter().any(VerifyFinding::fails)
    }
}

/// The paths each reset event prunes, by the run ids it lists.
/// §FS-rhei-evidence.10.1
#[cfg(test)]
fn pruned_paths(evidence: &Path) -> BTreeMap<String, BTreeSet<String>> {
    pruned_by_events(&workspace_events(evidence))
}

/// [`pruned_paths`] over events a source read. §FS-rhei-evidence.10.1
fn pruned_by_events(events: &[Vec<u8>]) -> BTreeMap<String, BTreeSet<String>> {
    let mut pruned: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for bytes in events {
        let Ok(event) = serde_json::from_slice::<serde_json::Value>(bytes) else { continue };
        let strings = |field: &str| -> Vec<String> {
            event[field]
                .as_array()
                .map(|items| items.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
                .unwrap_or_default()
        };
        let removed = strings("removed");
        for run in strings("runs") {
            pruned.entry(run).or_default().extend(removed.iter().cloned());
        }
    }
    pruned
}

/// Check one manifest-named file against its recorded hash.
fn check_named_file(source: &dyn EvidenceSource, run: &mut VerifiedRun, entry: &serde_json::Value, pruned: &BTreeSet<String>, findings: &mut Vec<VerifyFinding>) {
    let (Some(path), Some(recorded)) = (entry["path"].as_str(), entry["sha256"].as_str()) else {
        return;
    };
    // A pruned path is never also checked. §FS-rhei-verify.3
    if pruned.contains(path) {
        findings.push(VerifyFinding::new("pruned", Some(&run.run_id), Some(path.to_string())));
        return;
    }
    match source.read(path) {
        Some(bytes) => {
            let found = sha256_hex(&bytes);
            if found == recorded {
                run.files_verified += 1;
            } else {
                let mut finding = VerifyFinding::new("mismatch", Some(&run.run_id), Some(path.to_string()));
                finding.recorded_sha256 = Some(recorded.to_string());
                finding.found_sha256 = Some(found);
                findings.push(finding);
            }
        }
        None => {
            let mut finding = VerifyFinding::new("missing", Some(&run.run_id), Some(path.to_string()));
            finding.recorded_sha256 = Some(recorded.to_string());
            findings.push(finding);
        }
    }
}

/// The snapshot generations a manifest names, checked where the source has
/// them: each transcript's bytes against the recorded `transcript_sha256`.
/// §FS-rhei-verify.2 §FS-rhei-evidence.8
fn check_snapshots(source: &dyn EvidenceSource, run: &mut VerifiedRun, manifest: &serde_json::Value, findings: &mut Vec<VerifyFinding>) {
    for snapshot in manifest["snapshots"].as_array().cloned().unwrap_or_default() {
        let recorded = snapshot["transcript_sha256"].as_str().unwrap_or_default().to_string();
        let Some(generation) = source.generation(&snapshot["ref"]) else {
            let mut finding = VerifyFinding::new("snapshot_not_cached", Some(&run.run_id), None);
            finding.recorded_sha256 = Some(recorded);
            findings.push(finding);
            continue;
        };
        // The transcript's bytes are hashed, never its generation's own claim.
        let Some(bytes) = generation.transcript else {
            let mut finding = VerifyFinding::new("missing", Some(&run.run_id), Some(generation.path));
            finding.recorded_sha256 = Some(recorded);
            findings.push(finding);
            continue;
        };
        let actual = sha256_hex(&bytes);
        if actual == recorded {
            run.snapshots_verified += 1;
        } else {
            let mut finding = VerifyFinding::new("mismatch", Some(&run.run_id), Some(generation.path));
            finding.recorded_sha256 = Some(recorded);
            finding.found_sha256 = Some(actual);
            findings.push(finding);
        }
    }
}

/// The `{path, sha256}` entries a manifest names. §FS-rhei-evidence.9.1
fn manifest_named(manifest: &serde_json::Value) -> Vec<serde_json::Value> {
    ["files", "accounting", "seals"]
        .iter()
        .flat_map(|field| manifest[*field].as_array().cloned().unwrap_or_default())
        .collect()
}

/// Verify one finalized run directory against its manifest.
fn verify_manifest(source: &dyn EvidenceSource, source_run: &SourceRun, run: &mut VerifiedRun, manifest: &serde_json::Value, pruned: &BTreeSet<String>, findings: &mut Vec<VerifyFinding>) {
    run.status = manifest["status"].as_str().unwrap_or_default().to_string();
    run.exit_code = manifest["exit_code"].as_i64();
    run.started_at = manifest["started_at"].as_str().unwrap_or_default().to_string();
    run.invocations = manifest["invocations"]
        .as_array()
        .map(|all| {
            all.iter()
                .map(|i| (i["kind"].as_str().unwrap_or_default().to_string(), i["completed"] == true))
                .collect()
        })
        .unwrap_or_default();
    let named = manifest_named(manifest);
    for entry in &named {
        check_named_file(source, run, entry, pruned, findings);
    }
    // A finalized run directory holds no record its manifest does not name.
    let listed: BTreeSet<&str> = named.iter().filter_map(|entry| entry["path"].as_str()).collect();
    for path in source.records(source_run) {
        if !listed.contains(path.as_str()) {
            findings.push(VerifyFinding::new("unlisted", Some(&run.run_id), Some(path)));
        }
    }
    check_snapshots(source, run, manifest, findings);
}

/// Verify every manifest under `root` in place. §FS-rhei-verify.1
fn verify_evidence(root: &Path, digest: Option<&str>) -> VerifyOutcome {
    verify_source(&WorkspaceSource::new(root), digest)
}

/// Verify every manifest `source` holds, oldest run first, or only the one
/// whose file has `digest`. §FS-rhei-verify.1 §FS-rhei-verify.3
fn verify_source(source: &dyn EvidenceSource, digest: Option<&str>) -> VerifyOutcome {
    let pruned = pruned_by_events(&source.events());
    let mut outcome = VerifyOutcome::default();
    let empty = BTreeSet::new();
    for source_run in source.runs() {
        let run_id = source_run.run_id.clone();
        let mut run = VerifiedRun { run_id: run_id.clone(), ..VerifiedRun::default() };
        let Some((manifest_path, bytes)) = &source_run.manifest else {
            if digest.is_none() {
                run.status = "not finalized".to_string();
                outcome.findings.push(VerifyFinding::new("not_finalized", Some(&run_id), Some(source_run.dir.clone())));
                outcome.runs.push(run);
            }
            continue;
        };
        let found = sha256_hex(bytes);
        if digest.is_some_and(|wanted| wanted != found) {
            continue;
        }
        run.manifest = Some(manifest_path.clone());
        run.digest = Some(found);
        let manifest = serde_json::from_slice(bytes).unwrap_or(serde_json::Value::Null);
        let pruned = pruned.get(&run_id).unwrap_or(&empty);
        verify_manifest(source, &source_run, &mut run, &manifest, pruned, &mut outcome.findings);
        outcome.runs.push(run);
    }
    // Oldest first: by the manifest's start, a run with none last. §FS-rhei-verify.1
    outcome.runs.sort_by(|a, b| {
        (a.started_at.is_empty(), &a.started_at, &a.run_id).cmp(&(b.started_at.is_empty(), &b.started_at, &b.run_id))
    });
    if digest.is_some() && outcome.runs.is_empty() {
        outcome.findings.push(VerifyFinding::new("unknown_digest", None, None));
    } else if !outcome.runs.iter().any(|run| run.digest.is_some()) {
        outcome.findings.push(VerifyFinding::new("nothing_to_verify", None, None));
    }
    outcome
}
