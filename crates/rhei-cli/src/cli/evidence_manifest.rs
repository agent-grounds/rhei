// The run manifest: written once when a run ends, or by the next run for one
// killed outright, over every file the run's evidence names. §FS-rhei-evidence.9

const EVIDENCE_MANIFEST_SCHEMA: &str = "rhei.evidence.manifest.v1";

/// What a run's records say, gathered for its manifest. §FS-rhei-evidence.9.1
#[derive(Default)]
struct EvidenceContents {
    invocations: Vec<serde_json::Value>,
    files: BTreeMap<String, String>,
    snapshots: Vec<serde_json::Value>,
    /// Each seal generation file a record names, with its hash now.
    seals: BTreeMap<String, String>,
    earliest_start: Option<String>,
}

/// The objects a retained value keeps, as `(path, sha256)`. §FS-rhei-evidence.5
fn retained_object(value: &serde_json::Value) -> Option<(String, String)> {
    let hex = match value.get("stored")?.as_str()? {
        "original" => value.get("sha256")?.as_str()?,
        "redacted" => value.get("stored_sha256")?.as_str()?,
        _ => return None,
    };
    Some((format!("runtime/evidence/objects/sha256/{hex}"), hex.to_string()))
}

/// Read every record of `run_dir`: the records themselves, the logs and
/// objects they name, and each process's completion. A log with no end record
/// is hashed as found now. §FS-rhei-evidence.9.1 §FS-rhei-evidence.9.3
fn evidence_contents(root: &Path, run_dir: &Path) -> EvidenceContents {
    let mut contents = EvidenceContents::default();
    let mut names: Vec<String> = fs::read_dir(run_dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
                .filter(|name| name.ends_with(".start.json") || name.ends_with(".end.json"))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    for name in &names {
        let path = run_dir.join(name);
        let Ok(bytes) = fs::read(&path) else { continue };
        contents.files.insert(evidence_relative(root, &path), sha256_hex(&bytes));
        let Ok(record) = serde_json::from_slice::<serde_json::Value>(&bytes) else { continue };
        let objects = ["prompt", "config"]
            .iter()
            .filter_map(|field| record.get(*field))
            .chain(record.get("interventions").and_then(|v| v.as_array()).into_iter().flatten());
        for (object, hex) in objects.filter_map(retained_object) {
            contents.files.insert(object, hex);
        }
        if let Some(log) = record.get("log").filter(|log| log.is_object()) {
            if let (Some(path), Some(hex)) = (log["path"].as_str(), log["sha256"].as_str()) {
                contents.files.insert(path.to_string(), hex.to_string());
            }
        }
        let Some(key) = name.strip_suffix(".start.json") else { continue };
        let completed = names.contains(&format!("{key}.end.json"));
        if !completed {
            if let Some(log) = record.get("log_path").and_then(|v| v.as_str()) {
                if let Ok(found) = fs::read(evidence_join(root, log)) {
                    contents.files.insert(log.to_string(), sha256_hex(&found));
                }
            }
        }
        let kind = record["kind"].as_str().unwrap_or_default();
        let id_field = if kind == "callback" { "callback_id" } else { "invocation_id" };
        contents.invocations.push(serde_json::json!({
            "kind": kind,
            "id": record[id_field],
            "key": key,
            "completed": completed,
        }));
        // A generation file is written once, so its hash now is its hash. §FS-rhei-seal.3.1
        if let Some(seal) = record.get("seal").and_then(|seal| seal["path"].as_str()) {
            if let Ok(found) = fs::read(evidence_join(root, seal)) {
                contents.seals.insert(seal.to_string(), sha256_hex(&found));
            }
        }
        if let Some(snapshot) = record.get("snapshot").filter(|s| s.is_object()) {
            if !contents.snapshots.contains(snapshot) {
                contents.snapshots.push(snapshot.clone());
            }
        }
        if let Some(started) = record.get("started_at").and_then(|v| v.as_str()) {
            if contents.earliest_start.as_deref().is_none_or(|earliest| started < earliest) {
                contents.earliest_start = Some(started.to_string());
            }
        }
    }
    contents
}

/// The fields a manifest takes from the run itself rather than its records.
struct ManifestRun<'a> {
    run_id: &'a str,
    status: &'a str,
    exit_code: Option<i32>,
    started_at: Option<String>,
    ended_at: Option<String>,
    finalized_by: Option<&'a str>,
    rhei_build: serde_json::Value,
    checkout_head_at_start: serde_json::Value,
    accounting: Vec<PathBuf>,
}

/// The manifest document of one run. §FS-rhei-evidence.9.1
fn evidence_manifest_value(root: &Path, run: ManifestRun<'_>) -> serde_json::Value {
    let run_dir = evidence_dir(root).join("runs").join(run.run_id);
    let contents = evidence_contents(root, &run_dir);
    let files: Vec<serde_json::Value> = contents
        .files
        .iter()
        .map(|(path, hex)| serde_json::json!({ "path": path, "sha256": hex }))
        .collect();
    let mut accounting: Vec<serde_json::Value> = run
        .accounting
        .iter()
        .filter_map(|path| {
            let bytes = fs::read(path).ok()?;
            Some(serde_json::json!({
                "path": evidence_relative(root, path),
                "sha256": sha256_hex(&bytes),
            }))
        })
        .collect();
    accounting.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    let seals: Vec<serde_json::Value> = contents
        .seals
        .iter()
        .map(|(path, hex)| serde_json::json!({ "path": path, "sha256": hex }))
        .collect();
    serde_json::json!({
        "schema": EVIDENCE_MANIFEST_SCHEMA,
        "run_id": run.run_id,
        "status": run.status,
        "exit_code": run.exit_code,
        "started_at": run.started_at.or(contents.earliest_start).unwrap_or_default(),
        "ended_at": run.ended_at,
        "finalized_by": run.finalized_by,
        "rhei_build": run.rhei_build,
        "checkout_head_at_start": run.checkout_head_at_start,
        "invocations": contents.invocations,
        "files": files,
        "accounting": accounting,
        "seals": seals,
        "snapshots": contents.snapshots,
    })
}

/// Write `manifest` once and return `{path, sha256}`, the digest of the file.
/// §FS-rhei-evidence.9 §FS-rhei-evidence.9.2
fn write_evidence_manifest(root: &Path, run_id: &str, manifest: &serde_json::Value) -> Option<serde_json::Value> {
    let path = evidence_dir(root).join("runs").join(run_id).join("manifest.json");
    if let Err(err) = write_evidence_exclusively(&path, manifest) {
        diag_warn!("evidence: could not write the manifest {}: {err}", path.display());
        return None;
    }
    Some(serde_json::json!({
        "path": evidence_relative(root, &path),
        "sha256": sha256_hex(&evidence_bytes(manifest)),
    }))
}

/// Finalize every run directory left with no manifest as `incomplete`,
/// naming the run that does it. Runs under the run lock. §FS-rhei-evidence.9.3
fn finalize_killed_runs(run: &EvidenceRun) {
    let Ok(entries) = fs::read_dir(evidence_dir(&run.root).join("runs")) else { return };
    let previous = read_descriptor(&run_descriptor_path(&run.root));
    for entry in entries.filter_map(Result::ok) {
        let dir = entry.path();
        let Some(dead) = entry.file_name().to_str().map(str::to_string) else { continue };
        if dead == run.run_id || !dir.is_dir() || dir.join("manifest.json").exists() {
            continue;
        }
        let described = previous.as_ref().filter(|descriptor| descriptor.id == dead);
        let manifest = evidence_manifest_value(
            &run.root,
            ManifestRun {
                run_id: &dead,
                status: "incomplete",
                exit_code: None,
                started_at: described.map(|descriptor| descriptor.started_at.clone()),
                ended_at: None,
                finalized_by: Some(&run.run_id),
                rhei_build: described
                    .and_then(|descriptor| serde_json::to_value(&descriptor.rhei_build).ok())
                    .filter(|build| build.is_object())
                    .unwrap_or_else(|| serde_json::json!({})),
                checkout_head_at_start: described
                    .and_then(|descriptor| serde_json::to_value(&descriptor.checkout_head_at_start).ok())
                    .unwrap_or(serde_json::Value::Null),
                accounting: Vec::new(),
            },
        );
        write_evidence_manifest(&run.root, &dead, &manifest);
    }
}

/// End the recording run: write its manifest once and return where it is.
/// Later calls return the same reference. §FS-rhei-evidence.9
fn finish_evidence_run(exit_code: i32) -> Option<serde_json::Value> {
    let run = evidence_run()?;
    let mut written = run.manifest.lock().unwrap_or_else(|poison| poison.into_inner());
    if written.is_none() {
        let status = if interrupt_exit_code().is_some() {
            "interrupted"
        } else if exit_code == 0 {
            "finished"
        } else {
            "failed"
        };
        let accounting =
            run.accounting.lock().unwrap_or_else(|poison| poison.into_inner()).iter().cloned().collect();
        let manifest = evidence_manifest_value(
            &run.root,
            ManifestRun {
                run_id: &run.run_id,
                status,
                exit_code: Some(exit_code),
                started_at: Some(run.started_at.clone()),
                ended_at: Some(rhei_tui::format_rfc3339(std::time::SystemTime::now())),
                finalized_by: None,
                rhei_build: serde_json::to_value(&run.provenance.rhei_build)
                    .unwrap_or_else(|_| serde_json::json!({})),
                checkout_head_at_start: serde_json::to_value(&run.provenance.checkout_head_at_start)
                    .unwrap_or(serde_json::Value::Null),
                accounting,
            },
        );
        *written = write_evidence_manifest(&run.root, &run.run_id, &manifest);
    }
    written.clone()
}

/// Write the manifest as the run loop ends, before `run_finished` carries its
/// digest, with the exit code the run is about to return. §FS-rhei-evidence.9.2
/// `read_at` is the instant of the run's last ready scan, which the run's own
/// halt verdict is judged at. §FS-rhei-run.5.1
#[allow(clippy::too_many_arguments)]
fn finish_evidence_at_loop_end(
    input: &Path,
    workspace_root: &Path,
    runtime_dir: &Path,
    machines: &ExecutionMachines,
    settings: &RheiSettings,
    scope: &RheiScope,
    interrupted: bool,
    read_at: u64,
    opts: &RunOptions,
) -> Option<serde_json::Value> {
    if opts.dry_run() {
        return None;
    }
    let halts = !interrupted
        && load_run_plan(input).is_ok_and(|loaded| {
            // The run's own halt verdict, at its instant. §FS-rhei-run.5.1
            let spawns = SpawnSiteRoots::new(workspace_root, runtime_dir, &loaded, machines, Some(settings), opts)
                .judged_at(read_at);
            scoped_unfinished_task_exists(&loaded.rhei, &machines.set, scope)
                && !remaining_work_is_only_gating_or_poll_blocked(&loaded.rhei, &machines.set, scope, &spawns)
        });
    let code = interrupt_exit_code().or_else(idle_exit_code).unwrap_or(i32::from(halts));
    finish_evidence_run(code)
}
