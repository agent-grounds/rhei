//! Signed immutable evidence is necessary but never operator-installed authority.
//! §AR-neural-admission.8 §FS-rhei-budgets.6.3

use super::journal::digest;
use super::{BudgetError, LaunchTuple, Money, Result};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Evidence roles are closed so a missing obligation cannot become an empty
/// optional field. The signed review attests the content, not just its presence.
/// §FS-rhei-budgets.6.3
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceRole {
    ClientProvenance,
    DependencyClosure,
    LaunchConfiguration,
    ProviderBilling,
    PriceInterval,
    ExternalProbeCap,
    CaptureBarrier,
    RequestDimensions,
    PostKillBilling,
    NestedFallback,
    CredentialIsolation,
    EgressIsolation,
    PlatformExecution,
    FinalBillMatch,
}

const REQUIRED: &[EvidenceRole] = &[
    EvidenceRole::ClientProvenance,
    EvidenceRole::DependencyClosure,
    EvidenceRole::LaunchConfiguration,
    EvidenceRole::ProviderBilling,
    EvidenceRole::PriceInterval,
    EvidenceRole::ExternalProbeCap,
    EvidenceRole::CaptureBarrier,
    EvidenceRole::RequestDimensions,
    EvidenceRole::PostKillBilling,
    EvidenceRole::NestedFallback,
    EvidenceRole::CredentialIsolation,
    EvidenceRole::EgressIsolation,
    EvidenceRole::PlatformExecution,
    EvidenceRole::FinalBillMatch,
];

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceArtifact {
    pub role: EvidenceRole,
    pub sha256: String,
    pub bytes: u64,
}

/// This initial format represents one bounded, fully billed accepted request.
/// Other dimensions cannot be silently priced as text. §FS-rhei-budgets.6.2
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RequestBound {
    pub maximum_input_tokens: u64,
    pub maximum_output_tokens: u64,
    pub input_micro_per_million: u64,
    pub output_micro_per_million: u64,
    pub maximum_outstanding_requests: u64,
    pub watchdog_millis: u64,
    pub capture_policy: String,
    pub post_kill_policy: String,
    pub nested_policy: String,
    pub request_shape: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct QualificationBundle {
    pub schema: String,
    pub tuple: LaunchTuple,
    pub grade: String,
    pub currency: String,
    pub valid_from_unix: u64,
    pub valid_until_unix: u64,
    pub request: RequestBound,
    pub artifacts: Vec<EvidenceArtifact>,
}

/// Inspection verifies artifacts but carries no launch capability. Only a
/// build-pinned registry entry may turn this data into one. §AR-neural-admission.8
#[derive(Debug, Serialize)]
pub struct BundleInspection {
    pub bundle: QualificationBundle,
    pub evidence_hash: String,
    pub residual_micro: u64,
}

/// Verify exact bytes, with no JSON reserialization before signature checking.
/// The caller supplies a trusted key; this function cannot register that key
/// or qualify execution. Artifact lookup is by content hash only.
/// §AR-neural-admission.8
pub fn inspect_qualification_bundle(
    payload: &[u8],
    expected_hash: &str,
    review_key: &[u8; 32],
    signature: &[u8; 64],
    artifact: impl Fn(&str) -> Result<Vec<u8>>,
) -> Result<BundleInspection> {
    if payload.len() > 1_048_576 || digest(payload) != expected_hash {
        return Err(invalid("bundle content differs from the pinned hash"));
    }
    VerifyingKey::from_bytes(review_key)
        .and_then(|key| key.verify_strict(payload, &Signature::from_bytes(signature)))
        .map_err(|_| invalid("bundle review signature is invalid"))?;
    let bundle: QualificationBundle = serde_json::from_slice(payload)?;
    let residual_micro = bundle.validate()?;
    let mut roles = BTreeSet::new();
    for reference in &bundle.artifacts {
        if !roles.insert(reference.role) || reference.bytes == 0 || !is_hash(&reference.sha256) {
            return Err(invalid("duplicate, empty or malformed evidence artifact"));
        }
        let bytes = artifact(&reference.sha256)?;
        if bytes.len() as u64 != reference.bytes || digest(&bytes) != reference.sha256 {
            return Err(invalid("evidence artifact does not match its signed content hash"));
        }
    }
    if roles != REQUIRED.iter().copied().collect() {
        return Err(invalid("qualification is missing a required evidence role"));
    }
    Ok(BundleInspection { bundle, evidence_hash: expected_hash.into(), residual_micro })
}

impl QualificationBundle {
    fn validate(&self) -> Result<u64> {
        let r = &self.request;
        let t = &self.tuple;
        if self.schema != "rhei.qualification.contained.v1"
            || self.grade != "contained"
            || self.valid_from_unix >= self.valid_until_unix
            || r.maximum_outstanding_requests != 1
            || r.watchdog_millis == 0
            || r.capture_policy != "durable_before_next_request"
            || r.post_kill_policy != "full_accepted_request_reserved"
            || r.nested_policy != "disabled"
            || r.request_shape != "text_responses_v1"
            || r.maximum_input_tokens == 0
            || r.maximum_output_tokens == 0
            || r.input_micro_per_million == 0
            || r.output_micro_per_million == 0
        {
            return Err(invalid("bundle does not prove the supported containment contract"));
        }
        if !is_hash(&t.executable_sha256)
            || !is_hash(&t.configuration_sha256)
            || !matches!(
                (t.agent.as_str(), t.client_version.as_str()),
                ("codex", "0.153.4") | ("pi", "0.84.1")
            )
            || !matches!(t.os.as_str(), "linux" | "macos" | "windows")
            || !matches!(t.architecture.as_str(), "x86_64" | "aarch64")
            || t.provider != "openai"
            || t.model != "gpt-5.2-codex"
            || t.rhei_version.is_empty()
            || t.billing_contract.is_empty()
            || t.price_contract.is_empty()
            || t.confinement_policy.is_empty()
        {
            return Err(invalid("bundle lacks a complete supported exact launch tuple"));
        }
        let residual = price_ceiling(
            r.maximum_input_tokens,
            r.maximum_output_tokens,
            r.input_micro_per_million,
            r.output_micro_per_million,
        )?;
        Money { currency: self.currency.clone(), amount_micro: residual }.validate()?;
        Ok(residual)
    }
}

/// Independent provider rounding may cost more than rounding the aggregate.
/// Both reservation and capture use this ceiling. §FS-rhei-budgets.6.2
pub(crate) fn price_ceiling(
    input: u64,
    output: u64,
    input_rate: u64,
    output_rate: u64,
) -> Result<u64> {
    let dimension = |tokens: u64, rate: u64| -> Result<u64> {
        let rounded = (u128::from(tokens) * u128::from(rate))
            .checked_add(999_999)
            .ok_or_else(|| invalid("request price overflow"))?
            / 1_000_000;
        u64::try_from(rounded).map_err(|_| invalid("request price overflow"))
    };
    super::types::add(dimension(input, input_rate)?, dimension(output, output_rate)?)
}

fn is_hash(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

fn invalid(message: &str) -> BudgetError {
    BudgetError::new("missing_qualification", message)
}
