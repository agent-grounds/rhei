// `rhei verify`: check that what a workspace records about its runs is still
// what those runs wrote, and print what it found. §FS-rhei-verify

/// A full 64-hex digest; anything shorter is a usage error. §FS-rhei-verify.1
fn parse_verify_digest(value: &str) -> Result<String, String> {
    if value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(value.to_ascii_lowercase())
    } else {
        Err("expected the manifest's full sha256 digest, 64 hex digits".to_string())
    }
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// One block per run, then a closing line; every digest in full.
/// §FS-rhei-verify.4
fn render_verify_text(outcome: &VerifyOutcome) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    for run in &outcome.runs {
        let Some(digest) = run.digest.as_deref() else {
            let _ = writeln!(out, "run {}  not finalized", run.run_id);
            continue;
        };
        let exit = run.exit_code.map_or_else(|| "no exit".to_string(), |code| format!("exit {code}"));
        let _ = writeln!(out, "run {}  {} ({exit})  manifest sha256:{digest}", run.run_id, run.status);
        let count = |kind: &str| run.invocations.iter().filter(|(k, _)| k == kind).count();
        let open = run.invocations.iter().filter(|(_, completed)| !completed).count();
        let _ = writeln!(
            out,
            "  {} ({} agent, {} program, {} callback), {}",
            plural(run.invocations.len(), "invocation", "invocations"),
            count("agent"),
            count("program"),
            count("callback"),
            if open == 0 { "all complete".to_string() } else { format!("{open} incomplete") }
        );
        let mut verified = format!("  {} verified", plural(run.files_verified, "file", "files"));
        if run.snapshots_verified > 0 {
            let generations =
                plural(run.snapshots_verified, "snapshot generation", "snapshot generations");
            let _ = write!(verified, ", {generations} verified");
        }
        let _ = writeln!(out, "{verified}");
        for finding in outcome.findings.iter().filter(|f| f.run_id.as_deref() == Some(&run.run_id)) {
            let _ = writeln!(
                out,
                "  {} {}",
                finding.reason_code.to_ascii_uppercase(),
                finding.path.as_deref().unwrap_or("")
            );
            if let Some(recorded) = &finding.recorded_sha256 {
                let _ = writeln!(out, "    recorded sha256:{recorded}");
            }
            if let Some(found) = &finding.found_sha256 {
                let _ = writeln!(out, "    found    sha256:{found}");
            }
        }
    }
    for finding in outcome.findings.iter().filter(|f| f.run_id.is_none()) {
        let _ = writeln!(out, "{}", finding.reason_code.to_ascii_uppercase());
    }
    if outcome.verified() {
        let finalized = outcome.runs.iter().filter(|run| run.digest.is_some()).count();
        let _ = writeln!(out, "verified: {}", plural(finalized, "run", "runs"));
    } else {
        let mut failed: BTreeMap<(Option<String>, &str), usize> = BTreeMap::new();
        for finding in outcome.findings.iter().filter(|f| f.fails()) {
            *failed.entry((finding.run_id.clone(), finding.reason_code)).or_default() += 1;
        }
        let parts: Vec<String> = failed
            .iter()
            .map(|((run, code), count)| {
                let code = code.replace('_', " ");
                match run {
                    Some(run) => format!("{count} {code} in run {run}"),
                    None => code,
                }
            })
            .collect();
        let _ = writeln!(out, "verification failed: {}", parts.join(", "));
    }
    out
}

/// The `--json` object: runs, findings, and the verdict. §FS-rhei-verify.4.1
fn render_verify_json(outcome: &VerifyOutcome) -> serde_json::Value {
    let runs: Vec<serde_json::Value> = outcome
        .runs
        .iter()
        .map(|run| {
            serde_json::json!({
                "run_id": run.run_id,
                "status": run.status,
                "manifest": run.manifest,
                "digest": run.digest,
                "files_verified": run.files_verified,
                "invocations": run.invocations.len(),
            })
        })
        .collect();
    let findings: Vec<serde_json::Value> = outcome.findings.iter().map(VerifyFinding::to_json).collect();
    serde_json::json!({ "runs": runs, "findings": findings, "verified": outcome.verified() })
}

/// Resolve the run's root as `rhei summary` does, verify, print, and exit `1`
/// when verification fails. §FS-rhei-verify.1 §FS-rhei-verify.5
fn verify_command(input: Option<PathBuf>, digest: Option<&str>, json: bool) -> MietteResult<()> {
    let target = resolve_plan_target(input)?;
    let root = evidence_root_for(target.path());
    let outcome = verify_evidence(&root, digest);
    if json {
        println!("{}", render_verify_json(&outcome));
    } else {
        print!("{}", render_verify_text(&outcome));
    }
    if !outcome.verified() {
        std::process::exit(1);
    }
    Ok(())
}
