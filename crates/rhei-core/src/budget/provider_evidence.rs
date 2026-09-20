//! Signed evidence retains its authority through the financial mutation.
//! §FS-rhei-budgets.8.1 §AR-neural-admission.6

use super::journal::digest;
use super::{BudgetError, LaunchTuple, Result};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderEvidence {
    pub schema: String,
    pub project_id: String,
    pub provider: String,
    pub account: String,
    pub currency: String,
    pub billing_contract: String,
    pub price_contract: String,
    pub qualification: LaunchTuple,
    pub qualification_evidence_hash: String,
    pub coverage_start: String,
    pub coverage_end: String,
    pub complete: bool,
    pub records: Vec<EvidenceRecord>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceRecord {
    pub invocation_id: String,
    pub attempt_identity: String,
    pub provider_request_id: String,
    pub reservation_id: Option<String>,
    pub accepted_at: String,
    pub finalized_at: String,
    pub charge_micro: u64,
    pub final_charge: bool,
}

/// Only the pinned registry constructs production authority. In particular an
/// embedded caller cannot nominate a signing key. §FS-rhei-budgets.8.1
/// ```compile_fail
/// use rhei_core::budget::ProviderEvidenceAuthority;
/// // Caller-selected keys cannot construct production financial authority.
/// let _ = ProviderEvidenceAuthority::new([42; 32], String::new(), String::new(),
///     String::new(), String::new(), String::new(), String::new());
/// ```
pub struct ProviderEvidenceAuthority {
    key: VerifyingKey,
    scope: EvidenceScope,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct EvidenceScope {
    pub project_id: String,
    pub qualification: LaunchTuple,
    pub qualification_evidence_hash: String,
    pub account: String,
    pub currency: String,
    pub authority_key_sha256: String,
    pub association_start: String,
}

pub struct VerifiedHistory {
    pub(crate) scope: EvidenceScope,
    pub(crate) envelope: ProviderEvidence,
    pub(crate) evidence_hash: String,
    pub(crate) invocations: u64,
    pub(crate) charge_micro: u64,
}

pub struct VerifiedReconciliation {
    pub(crate) scope: EvidenceScope,
    pub(crate) envelope: ProviderEvidence,
    pub(crate) reservation_id: String,
    pub(crate) charge_micro: u64,
    pub(crate) evidence: Vec<u8>,
}

impl ProviderEvidenceAuthority {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_registry(
        key: [u8; 32],
        project_id: String,
        qualification: LaunchTuple,
        qualification_evidence_hash: String,
        account: String,
        currency: String,
        association_start: String,
    ) -> Result<Self> {
        timestamp(&association_start)?;
        let scope = EvidenceScope {
            project_id,
            qualification,
            qualification_evidence_hash,
            account,
            currency,
            authority_key_sha256: digest(&key),
            association_start,
        };
        let key = VerifyingKey::from_bytes(&key).map_err(|_| refused("invalid authority key"))?;
        Ok(Self { key, scope })
    }

    pub fn verify_history(
        &self,
        bytes: &[u8],
        signature: &[u8; 64],
        required_start: &str,
        cutoff: &str,
    ) -> Result<VerifiedHistory> {
        let envelope = self.verify(bytes, signature)?;
        if timestamp(required_start)? != timestamp(&self.scope.association_start)?
            || timestamp(&envelope.coverage_start)? != timestamp(required_start)?
            || timestamp(&envelope.coverage_end)? != timestamp(cutoff)?
        {
            return Err(refused("history does not cover the complete required lifetime"));
        }
        let invocations =
            envelope.records.iter().map(|r| &r.invocation_id).collect::<BTreeSet<_>>().len() as u64;
        let charge_micro = total(&envelope.records)?;
        Ok(VerifiedHistory {
            scope: self.scope.clone(),
            envelope,
            evidence_hash: digest(bytes),
            invocations,
            charge_micro,
        })
    }

    /// Request ownership is checked against durable broker receipts at mutation,
    /// never against an expected identifier copied from this envelope.
    /// §FS-rhei-budgets.8.1
    pub fn verify_reconciliation(
        &self,
        bytes: &[u8],
        signature: &[u8; 64],
        reservation_id: &str,
        attempt_identity: &str,
    ) -> Result<VerifiedReconciliation> {
        let envelope = self.verify(bytes, signature)?;
        if envelope.records.is_empty()
            || envelope.records.iter().any(|r| {
                r.reservation_id.as_deref() != Some(reservation_id)
                    || r.attempt_identity != attempt_identity
                    || r.invocation_id != attempt_identity
            })
        {
            return Err(refused("final records do not match the reservation and invocation"));
        }
        Ok(VerifiedReconciliation {
            scope: self.scope.clone(),
            charge_micro: total(&envelope.records)?,
            envelope,
            reservation_id: reservation_id.into(),
            evidence: bytes.to_vec(),
        })
    }

    fn verify(&self, bytes: &[u8], signature: &[u8; 64]) -> Result<ProviderEvidence> {
        self.key
            .verify(bytes, &Signature::from_bytes(signature))
            .map_err(|_| refused("provider evidence signature failed"))?;
        let e: ProviderEvidence = serde_json::from_slice(bytes)?;
        let s = &self.scope;
        if e.schema != "rhei.provider-evidence.v1"
            || e.project_id != s.project_id
            || e.provider != s.qualification.provider
            || e.account != s.account
            || e.currency != s.currency
            || e.billing_contract != s.qualification.billing_contract
            || e.price_contract != s.qualification.price_contract
            || e.qualification != s.qualification
            || e.qualification_evidence_hash != s.qualification_evidence_hash
            || !e.complete
        {
            return Err(refused("provider evidence scope, contract, or completeness mismatch"));
        }
        let (start, end) = (timestamp(&e.coverage_start)?, timestamp(&e.coverage_end)?);
        if start < timestamp(&s.association_start)?
            || start > end
            || end > OffsetDateTime::now_utc()
        {
            return Err(refused("invalid evidence coverage interval"));
        }
        let mut requests = BTreeSet::new();
        let mut invocations = std::collections::BTreeMap::new();
        for r in &e.records {
            let (accepted, finalized) = (timestamp(&r.accepted_at)?, timestamp(&r.finalized_at)?);
            let binding = (&r.attempt_identity, &r.reservation_id);
            if r.invocation_id.is_empty()
                || r.attempt_identity.is_empty()
                || r.provider_request_id.is_empty()
                || !r.final_charge
                || !requests.insert(&r.provider_request_id)
                || invocations.insert(&r.invocation_id, binding).is_some_and(|old| old != binding)
                || accepted < start
                || accepted > finalized
                || finalized > end
            {
                return Err(refused("non-final, duplicate, mismatched, or out-of-coverage record"));
            }
        }
        Ok(e)
    }
}

pub(crate) fn timestamp(value: &str) -> Result<OffsetDateTime> {
    OffsetDateTime::parse(value, &Rfc3339)
        .map_err(|_| refused("invalid RFC 3339 evidence timestamp"))
}
fn total(records: &[EvidenceRecord]) -> Result<u64> {
    records.iter().try_fold(0u64, |sum, record| {
        sum.checked_add(record.charge_micro).ok_or_else(|| refused("history charge overflow"))
    })
}
pub(crate) fn refused(message: &str) -> BudgetError {
    BudgetError::new("evidence_refused", message)
}
