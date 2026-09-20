//! Release qualification is code-owned, never operator-supplied JSON.
//! §FS-rhei-budgets.6.1 §AR-neural-admission.8

use super::{BudgetError, Result};
use serde::{Deserialize, Serialize};

/// Every field participates in exact qualification. A familiar command name,
/// user price book, or version string alone carries no authority.
/// §FS-rhei-budgets.6.1
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LaunchTuple {
    pub executable_sha256: String,
    pub client_version: String,
    pub configuration_sha256: String,
    pub rhei_version: String,
    pub os: String,
    pub architecture: String,
    pub agent: String,
    pub provider: String,
    pub model: String,
    pub billing_contract: String,
    pub price_contract: String,
    pub confinement_policy: String,
}

/// Facts the runtime can establish before it receives provider credentials.
/// Contract and confinement claims come only from the matching signed bundle.
/// §FS-rhei-budgets.6.1 §AR-neural-admission.1
pub struct ObservedLaunch {
    pub executable_sha256: String,
    pub configuration_sha256: String,
    pub rhei_version: String,
    pub os: String,
    pub architecture: String,
    pub agent: String,
    pub provider: String,
    pub model: String,
}

/// Only the registry may produce this proof. It cannot be deserialized,
/// constructed through a public field, or supplied by an environment switch.
/// §AR-neural-admission.1 §AR-neural-admission.8
#[derive(Clone, Debug)]
pub struct QualifiedLaunch {
    pub(crate) tuple: LaunchTuple,
    pub(crate) evidence_hash: String,
    pub(crate) currency: String,
    pub(crate) provider_account: String,
    pub(crate) residual_micro: u64,
    pub(crate) maximum_input_tokens: u64,
    pub(crate) maximum_output_tokens: u64,
    pub(crate) input_micro_per_million: u64,
    pub(crate) output_micro_per_million: u64,
    pub(crate) valid_from_unix: u64,
    pub(crate) valid_until_unix: u64,
    pub(crate) watchdog_millis: u64,
    /// What a validated finite delegation graph permits this invocation's
    /// descendants to reserve. The initial contained format disables nested
    /// and fallback work outright, so it is zero until a bundle proves the
    /// graph. §FS-rhei-budgets.6.3 §AR-neural-admission.7
    pub(crate) descendant_envelope_micro: u64,
}

impl QualifiedLaunch {
    pub fn tuple(&self) -> &LaunchTuple {
        &self.tuple
    }
    pub fn evidence_hash(&self) -> &str {
        &self.evidence_hash
    }
    pub fn currency(&self) -> &str {
        &self.currency
    }
    pub fn descendant_envelope_micro(&self) -> u64 {
        self.descendant_envelope_micro
    }

    /// Construct deterministic provider authority for the separately compiled
    /// engine fixture. This symbol does not exist in release builds.
    /// §AR-neural-admission.8
    #[cfg(feature = "budget-fixtures")]
    pub fn deterministic_fixture(
        agent: &str,
        provider: &str,
        model: &str,
        currency: &str,
        residual_micro: u64,
        deadline_unix: u64,
    ) -> Result<Self> {
        if !matches!(
            (provider, model),
            ("rhei-test", "fixed-2000" | "fixed-2000-a" | "fixed-2000-b")
        ) {
            return Err(BudgetError::new(
                "missing_qualification",
                "deterministic authority only qualifies the synthetic fixed-cost provider",
            ));
        }
        Ok(Self {
            tuple: LaunchTuple {
                executable_sha256: "sha256:deterministic-fixture".into(),
                client_version: "fixture-v1".into(),
                configuration_sha256: "sha256:deterministic-fixture".into(),
                rhei_version: env!("CARGO_PKG_VERSION").into(),
                os: std::env::consts::OS.into(),
                architecture: std::env::consts::ARCH.into(),
                agent: agent.into(),
                provider: provider.into(),
                model: model.into(),
                billing_contract: "synthetic-fixed".into(),
                price_contract: "synthetic-2000-micro".into(),
                confinement_policy: "in-process-no-network-fixture".into(),
            },
            evidence_hash: "sha256:deterministic-fixture-authority".into(),
            currency: currency.into(),
            provider_account: "deterministic-fixture".into(),
            residual_micro,
            maximum_input_tokens: 1_000,
            maximum_output_tokens: 1_000,
            input_micro_per_million: 1_000_000,
            output_micro_per_million: 1_000_000,
            valid_from_unix: 0,
            valid_until_unix: deadline_unix,
            watchdog_millis: 5_000,
            // The synthetic contract disables nested work exactly as the
            // reviewed contained format does. §FS-rhei-budgets.6.3
            descendant_envelope_micro: 0,
        })
    }

    pub(crate) fn check_validity(&self, deadline_unix: u64) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| BudgetError::bounds("system clock precedes the qualification epoch"))?
            .as_secs();
        if now < self.valid_from_unix
            || deadline_unix <= now
            || deadline_unix > self.valid_until_unix
        {
            return Err(BudgetError::new(
                "missing_qualification",
                "qualification does not cover the complete invocation deadline",
            ));
        }
        Ok(())
    }
}

/// Evidence must enter through a reviewed, content-addressed build registry.
/// No real bundle has yet met the three-OS and billing-contract obligations;
/// this registry deliberately issues no production capabilities.
/// §FS-rhei-budgets.6.5 §FS-rhei-budgets.12
pub struct Registry;

impl Registry {
    /// Resolve an exact build-pinned tuple from facts measured at admission.
    /// The executable hash binds the reviewed client version; the runtime
    /// never accepts a self-reported version or operator contract identifier.
    /// §FS-rhei-budgets.6.1 §FS-rhei-budgets.6.5
    pub fn qualify_observed(observed: &ObservedLaunch) -> Result<QualifiedLaunch> {
        for registered in RELEASE_BUNDLES {
            let proof = registered.verify()?;
            let tuple = &proof.bundle.tuple;
            if tuple.executable_sha256 == observed.executable_sha256
                && tuple.configuration_sha256 == observed.configuration_sha256
                && tuple.rhei_version == observed.rhei_version
                && tuple.os == observed.os
                && tuple.architecture == observed.architecture
                && tuple.agent == observed.agent
                && tuple.provider == observed.provider
                && tuple.model == observed.model
            {
                return qualified_from_proof(proof, registered.provider_account);
            }
        }
        Err(BudgetError::new(
            "missing_qualification",
            format!(
                "missing exact qualification for {} / {} / {} / {}-{}: measured executable and configuration have no verified provider billing, residual, and confinement bundle",
                observed.agent,
                observed.provider,
                observed.model,
                observed.os,
                observed.architecture,
            ),
        ))
    }

    pub fn qualify(tuple: &LaunchTuple) -> Result<QualifiedLaunch> {
        for registered in RELEASE_BUNDLES {
            let proof = registered.verify()?;
            if &proof.bundle.tuple == tuple {
                return qualified_from_proof(proof, registered.provider_account);
            }
        }
        Err(BudgetError::new("missing_qualification", format!(
            "missing qualification for {} {} / {} / {} / {}-{}: verified provider billing interval, complete residual, and credential/egress confinement evidence are required; no release tuple is qualified",
            tuple.agent, tuple.client_version, tuple.provider, tuple.model, tuple.os, tuple.architecture,
        )))
    }

    pub fn available() -> &'static [LaunchTuple] {
        static VERIFIED: std::sync::OnceLock<Vec<LaunchTuple>> = std::sync::OnceLock::new();
        VERIFIED.get_or_init(|| {
            RELEASE_BUNDLES
                .iter()
                .filter_map(|entry| entry.verify().ok().map(|v| v.bundle.tuple))
                .collect()
        })
    }

    /// Resolve provider/account billing authority from the same build-pinned
    /// release entry that qualified launch. §FS-rhei-budgets.8.1
    pub fn evidence_authority(
        tuple: &LaunchTuple,
        project_id: &str,
        currency: &str,
    ) -> Result<super::ProviderEvidenceAuthority> {
        for registered in RELEASE_BUNDLES {
            let proof = registered.verify()?;
            if &proof.bundle.tuple == tuple {
                return super::ProviderEvidenceAuthority::from_registry(
                    registered.provider_evidence_key,
                    project_id.into(),
                    tuple.clone(),
                    proof.evidence_hash.clone(),
                    registered.provider_account.into(),
                    currency.into(),
                    registered.account_association_start.into(),
                );
            }
        }
        Err(BudgetError::new(
            "evidence_refused",
            "reservation has no build-pinned provider/account evidence authority",
        ))
    }

    /// Verify a complete pre-initialization lifetime against a build-pinned
    /// provider/account association. The evidence chooses no key or scope: it
    /// must match one reviewed release entry exactly. §FS-rhei-budgets.8.1
    pub fn verify_history(
        bytes: &[u8],
        signature: &[u8; 64],
        project_id: &str,
        currency: &str,
        cutoff: &str,
    ) -> Result<super::VerifiedHistory> {
        for registered in RELEASE_BUNDLES {
            let proof = registered.verify()?;
            let tuple = &proof.bundle.tuple;
            let authority = super::ProviderEvidenceAuthority::from_registry(
                registered.provider_evidence_key,
                project_id.into(),
                tuple.clone(),
                proof.evidence_hash.clone(),
                registered.provider_account.into(),
                currency.into(),
                registered.account_association_start.into(),
            )?;
            if let Ok(history) = authority.verify_history(
                bytes,
                signature,
                registered.account_association_start,
                cutoff,
            ) {
                return Ok(history);
            }
        }
        Err(BudgetError::new(
            "evidence_refused",
            "history is not a complete signed export for a build-pinned provider/account association and import cutoff",
        ))
    }
}

fn qualified_from_proof(
    proof: super::BundleInspection,
    provider_account: &str,
) -> Result<QualifiedLaunch> {
    let bundle = proof.bundle;
    let bound = bundle.request;
    Ok(QualifiedLaunch {
        tuple: bundle.tuple,
        evidence_hash: proof.evidence_hash,
        currency: bundle.currency,
        provider_account: provider_account.into(),
        residual_micro: proof.residual_micro,
        maximum_input_tokens: bound.maximum_input_tokens,
        maximum_output_tokens: bound.maximum_output_tokens,
        input_micro_per_million: bound.input_micro_per_million,
        output_micro_per_million: bound.output_micro_per_million,
        valid_from_unix: bundle.valid_from_unix,
        valid_until_unix: bundle.valid_until_unix,
        watchdog_millis: bound.watchdog_millis,
        // The reviewed contained format disables every nested and fallback
        // route, so nothing under it may reserve. §FS-rhei-budgets.6.3
        descendant_envelope_micro: 0,
    })
}

/// These are build inputs, never read from a project path or environment.
/// A failed signature, missing artifact or changed tuple issues no capability.
/// §AR-neural-admission.8
struct ReleaseBundle {
    payload: &'static [u8],
    hash: &'static str,
    review_key: [u8; 32],
    signature: [u8; 64],
    artifacts: &'static [(&'static str, &'static [u8])],
    provider_evidence_key: [u8; 32],
    provider_account: &'static str,
    account_association_start: &'static str,
}

impl ReleaseBundle {
    fn verify(&self) -> Result<super::BundleInspection> {
        super::inspect_qualification_bundle(
            self.payload,
            self.hash,
            &self.review_key,
            &self.signature,
            |hash| {
                self.artifacts
                    .iter()
                    .find(|(id, _)| *id == hash)
                    .map(|(_, bytes)| bytes.to_vec())
                    .ok_or_else(|| {
                        BudgetError::new(
                            "missing_qualification",
                            "release evidence artifact is absent",
                        )
                    })
            },
        )
    }
}

// All six client/OS bundles remain absent until independently qualified.
// The verifier cannot turn operator evidence into an entry. §FS-rhei-budgets.12
const RELEASE_BUNDLES: &[ReleaseBundle] = &[];
