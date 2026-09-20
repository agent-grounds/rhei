//! Finite qualification runner. An external cap is a separate trust root from
//! the transport being qualified; synthetic evidence never opens this path.
//! §FS-rhei-budgets.6.3 §FS-rhei-budgets.12 §AR-neural-admission.8

use super::{BudgetError, LaunchTuple, Result};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeCase {
    AllowedRequest,
    AlternateProvider,
    InheritedCredential,
    AlternateProxy,
    DirectEgress,
    Extension,
    NestedClient,
    FallbackModel,
    ConcurrentRequests,
    MissingUsage,
    KillBeforeCommit,
    KillDuringRequest,
    KillAfterCapture,
    EscapedDescendant,
}
pub(super) const CASES: &[ProbeCase] = &[
    ProbeCase::AllowedRequest,
    ProbeCase::AlternateProvider,
    ProbeCase::InheritedCredential,
    ProbeCase::AlternateProxy,
    ProbeCase::DirectEgress,
    ProbeCase::Extension,
    ProbeCase::NestedClient,
    ProbeCase::FallbackModel,
    ProbeCase::ConcurrentRequests,
    ProbeCase::MissingUsage,
    ProbeCase::KillBeforeCommit,
    ProbeCase::KillDuringRequest,
    ProbeCase::KillAfterCapture,
    ProbeCase::EscapedDescendant,
];

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProbeGrant {
    pub schema: String,
    pub tuple: LaunchTuple,
    pub account: String,
    pub currency: String,
    pub external_cap_id: String,
    pub external_cap_micro: u64,
    pub maximum_launches: u64,
    pub per_case_millis: u64,
    pub expires_unix: u64,
    pub finality_deadline_unix: u64,
    pub maximum_artifact_bytes: u64,
    pub artifacts: Vec<super::EvidenceArtifact>,
}

/// Only independently pinned account/cap authority can produce this token.
/// A project document cannot authorize a real qualification request.
/// §FS-rhei-budgets.12
pub struct ProbeAuthorization {
    pub(super) grant: ProbeGrant,
    pub(super) evidence: Vec<u8>,
    pub(super) signature: [u8; 64],
    pub(super) billing_key: [u8; 32],
    pub(super) review_key: [u8; 32],
    pub(super) prerequisites: Vec<(super::EvidenceArtifact, Vec<u8>)>,
}

struct ProbeAuthority {
    account: &'static str,
    key: [u8; 32],
    billing_key: [u8; 32],
    review_key: [u8; 32],
}
// No independently enforced external cap or owner key has been supplied.
const PROBE_AUTHORITIES: &[ProbeAuthority] = &[];

pub fn authorize_probe(
    bytes: &[u8],
    signature: &[u8; 64],
    artifact: impl Fn(&str) -> Result<Vec<u8>>,
) -> Result<ProbeAuthorization> {
    let grant: ProbeGrant = serde_json::from_slice(bytes)?;
    let authority =
        PROBE_AUTHORITIES.iter().find(|a| a.account == grant.account).ok_or_else(|| {
            refused("missing independently pinned provider/account probe-cap authority")
        })?;
    if authority.review_key == authority.billing_key || authority.review_key == authority.key {
        return Err(refused("qualification review must have an independent pinned authority"));
    }
    VerifyingKey::from_bytes(&authority.key)
        .map_err(|_| refused("invalid probe authority key"))?
        .verify_strict(bytes, &Signature::from_bytes(signature))
        .map_err(|_| refused("invalid external-cap signature"))?;
    validate_grant(&grant, &artifact)?;
    let mut prerequisites = Vec::new();
    for entry in &grant.artifacts {
        let data = artifact(&entry.sha256)?;
        if data.len() as u64 != entry.bytes || super::journal::digest(&data) != entry.sha256 {
            return Err(refused("probe prerequisite changed during acquisition"));
        }
        prerequisites.push((entry.clone(), data));
    }
    Ok(ProbeAuthorization {
        grant,
        evidence: bytes.to_vec(),
        signature: *signature,
        billing_key: authority.billing_key,
        review_key: authority.review_key,
        prerequisites,
    })
}

fn validate_grant(grant: &ProbeGrant, artifact: impl Fn(&str) -> Result<Vec<u8>>) -> Result<()> {
    use super::EvidenceRole;
    let now = now_unix()?;
    if grant.schema != "rhei.qualification-probe.v1"
        || grant.external_cap_id.is_empty()
        || grant.external_cap_micro == 0
        || grant.maximum_launches < CASES.len() as u64
        || grant.per_case_millis == 0
        || grant.maximum_artifact_bytes == 0
        || grant.expires_unix <= now
        || grant.finality_deadline_unix < grant.expires_unix
        || grant.tuple.os != std::env::consts::OS
        || grant.tuple.architecture != std::env::consts::ARCH
        || grant.tuple.provider != "openai"
        || grant.tuple.model != "gpt-5.2-codex"
        || !matches!(
            (grant.tuple.agent.as_str(), grant.tuple.client_version.as_str()),
            ("codex", "0.153.4") | ("pi", "0.84.1")
        )
    {
        return Err(refused(
            "probe grant lacks the exact tuple, finite cap, deadlines, or native platform",
        ));
    }
    super::Money { currency: grant.currency.clone(), amount_micro: grant.external_cap_micro }
        .validate()?;
    let required = [
        EvidenceRole::ExternalProbeCap,
        EvidenceRole::ProviderBilling,
        EvidenceRole::PriceInterval,
        EvidenceRole::ClientProvenance,
        EvidenceRole::DependencyClosure,
        EvidenceRole::LaunchConfiguration,
    ];
    for role in required {
        let records = grant.artifacts.iter().filter(|a| a.role == role).collect::<Vec<_>>();
        if records.len() != 1 {
            return Err(refused("missing or duplicate probe prerequisite artifact"));
        }
        let entry = records[0];
        let data = artifact(&entry.sha256)?;
        if data.len() as u64 != entry.bytes || super::journal::digest(&data) != entry.sha256 {
            return Err(refused("probe prerequisite artifact failed byte/hash verification"));
        }
    }
    Ok(())
}

/// Native implementations own confinement and the complete child tree. The
/// interface requires a nonblocking poll and idempotent teardown; an ordinary
/// Command/Child pair is insufficient. §FS-rhei-budgets.6.4
pub trait NativeProbeBackend {
    fn tuple(&self) -> &LaunchTuple;
    fn start(
        &mut self,
        case: ProbeCase,
        directory: &Path,
        deadline: Instant,
        permit: ProbeLaunchPermit,
    ) -> Result<Box<dyn RunningProbe>>;
}

/// A single case launch granted only after the runner durably consumes the
/// external-cap grant. Callers cannot construct or clone this permit.
/// §FS-rhei-budgets.12
pub struct ProbeLaunchPermit {
    pub(super) grant_hash: String,
    pub(super) case: ProbeCase,
    pub(super) deadline: Instant,
}

impl ProbeLaunchPermit {
    pub fn grant_hash(&self) -> &str {
        &self.grant_hash
    }
    pub fn case(&self) -> ProbeCase {
        self.case
    }
    pub fn deadline(&self) -> Instant {
        self.deadline
    }
}
pub trait RunningProbe {
    fn poll(&mut self) -> Result<Option<i32>>;
    fn terminate_tree(&mut self) -> Result<()>;
    fn artifact_paths(&self) -> Vec<PathBuf>;
}

struct ProbeOwner(Box<dyn RunningProbe>);
impl Drop for ProbeOwner {
    fn drop(&mut self) {
        let _ = self.0.terminate_tree();
    }
}

#[derive(Serialize)]
struct ProbeResult {
    case: ProbeCase,
    exit_code: Option<i32>,
    timed_out: bool,
    artifacts: Vec<(String, String, u64)>,
    error: Option<String>,
}

/// No retries, finite case count, finite per-case/global deadline, bounded
/// artifact size checks, teardown on every path. Outputs require independent
/// review and final billing; running this cannot register a tuple.
/// §FS-rhei-budgets.12 §AR-neural-admission.8
pub fn run_qualification_probes(
    authorization: &ProbeAuthorization,
    backend: &mut dyn NativeProbeBackend,
    output: &Path,
) -> Result<()> {
    let grant = &authorization.grant;
    if backend.tuple() != &grant.tuple {
        return Err(refused("native backend tuple mismatch"));
    }
    if now_unix()? >= grant.expires_unix {
        return Err(refused("external probe-cap authorization expired"));
    }
    claim_grant(&authorization.evidence)?;
    std::fs::create_dir(output)?;
    let output = std::fs::canonicalize(output)?;
    write_new(&output.join("authorization.json"), &authorization.evidence)?;
    write_new(&output.join("authorization.sig"), &authorization.signature)?;
    std::fs::create_dir(output.join("prerequisites"))?;
    for (entry, data) in &authorization.prerequisites {
        let name = entry
            .sha256
            .strip_prefix("sha256:")
            .ok_or_else(|| refused("invalid prerequisite hash"))?;
        let path = output.join("prerequisites").join(name);
        if !path.exists() {
            write_new(&path, data)?;
        }
    }
    let mut results = Vec::new();
    for (index, case) in CASES.iter().enumerate() {
        let remaining = grant
            .expires_unix
            .checked_sub(now_unix()?)
            .filter(|n| *n > 0)
            .ok_or_else(|| refused("external probe-cap authorization expired"))?;
        let timeout =
            Duration::from_millis(grant.per_case_millis).min(Duration::from_secs(remaining));
        let deadline =
            Instant::now().checked_add(timeout).ok_or_else(|| refused("probe timeout overflow"))?;
        let directory = output.join(format!("case-{index:02}"));
        std::fs::create_dir(&directory)?;
        let permit = ProbeLaunchPermit {
            grant_hash: super::journal::digest(&authorization.evidence),
            case: *case,
            deadline,
        };
        write_new(
            &directory.join("launch-intent.json"),
            &serde_json::to_vec(&serde_json::json!({
                "case": case, "grant_sha256": permit.grant_hash,
            }))?,
        )?;
        let started = backend.start(*case, &directory, deadline, permit);
        let mut owner = match started {
            Ok(running) => ProbeOwner(running),
            Err(error) => {
                let result = ProbeResult {
                    case: *case,
                    exit_code: None,
                    timed_out: false,
                    artifacts: vec![],
                    error: Some(error.to_string()),
                };
                write_new(
                    &output.join(format!("receipt-{index:02}.json")),
                    &serde_json::to_vec_pretty(&result)?,
                )?;
                return Err(error);
            }
        };
        let mut result = ProbeResult {
            case: *case,
            exit_code: None,
            timed_out: false,
            artifacts: vec![],
            error: None,
        };
        loop {
            match owner.0.poll() {
                Ok(Some(exit)) => {
                    result.exit_code = Some(exit);
                    break;
                }
                Ok(None) => {}
                Err(error) => {
                    result.error = Some(error.to_string());
                    break;
                }
            }
            if Instant::now() >= deadline {
                result.timed_out = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        if let Err(error) = owner.0.terminate_tree() {
            result.error = Some(error.to_string());
        }
        let mut total = 0u64;
        for path in owner.0.artifact_paths() {
            let path = std::fs::canonicalize(path)?;
            if !path.starts_with(std::fs::canonicalize(&directory)?) {
                return Err(refused("native probe artifact escaped its capture directory"));
            }
            let bytes = std::fs::metadata(&path)?.len();
            total =
                total.checked_add(bytes).ok_or_else(|| refused("probe artifact size overflow"))?;
            if total > grant.maximum_artifact_bytes {
                return Err(refused("probe artifacts exceeded finite cap"));
            }
            use std::io::Read;
            let mut data = Vec::new();
            std::fs::File::open(&path)?.take(bytes.saturating_add(1)).read_to_end(&mut data)?;
            if data.len() as u64 != bytes {
                return Err(refused("probe artifact changed while being retained"));
            }
            result.artifacts.push((
                path.strip_prefix(&output)
                    .map_err(BudgetError::corrupt)?
                    .to_string_lossy()
                    .into_owned(),
                super::journal::digest(&data),
                bytes,
            ));
        }
        let failed = result.error.is_some();
        results.push(result);
        write_new(
            &output.join(format!("receipt-{index:02}.json")),
            &serde_json::to_vec_pretty(results.last().expect("case result"))?,
        )?;
        if failed {
            return Err(refused("native probe failed; retained artifacts require inspection"));
        }
    }
    write_new(&output.join("probe-results.json"), &serde_json::to_vec_pretty(&results)?)?;
    Ok(())
}
pub(super) fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
pub(super) fn now_unix() -> Result<u64> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(BudgetError::corrupt)?
        .as_secs())
}
pub(super) fn refused(message: &str) -> BudgetError {
    BudgetError::new("qualification_probe_refused", message)
}

fn claim_grant(bytes: &[u8]) -> Result<()> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(|home| PathBuf::from(home).join(".local/state"))
        })
        .ok_or_else(|| refused("probe authority directory unavailable"))?;
    if !base.is_absolute() {
        return Err(refused("probe authority must be an absolute path"));
    }
    let directory = base.join("rhei/probe-authority");
    super::authority::durable_directories(&directory)?;
    let hash = super::journal::digest(bytes);
    let path = directory.join(format!("{}.spent", &hash[7..]));
    write_new(&path, bytes).map_err(|_| {
        refused("probe grant already consumed or cannot be recorded; no retry authorized")
    })?;
    super::journal::sync_directory(&directory)
}

#[cfg(test)]
#[path = "probe_tests.rs"]
mod tests;
