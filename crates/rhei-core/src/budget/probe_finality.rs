//! Collect independent account finality and review without granting launch
//! authority. Provider exports arrive out of band; local totals are not bills.
//! §FS-rhei-budgets.12 §AR-neural-admission.8

use super::probe::{now_unix, refused, write_new};
use super::{LaunchTuple, ProbeAuthorization, Result};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::io::Read;
use std::path::Path;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FinalBill {
    schema: String,
    grant_sha256: String,
    results_sha256: String,
    tuple: LaunchTuple,
    account: String,
    currency: String,
    complete: bool,
    finalized_unix: u64,
    requests: Vec<FinalRequest>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FinalRequest {
    case: String,
    request_id: String,
    accepted_unix: u64,
    finalized_unix: u64,
    charge_micro: u64,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Review {
    schema: String,
    grant_sha256: String,
    results_sha256: String,
    final_bill_sha256: String,
    accepted: bool,
    reviewed_unix: u64,
}

/// Read one finite set of authority-signed inputs. Missing finality is an
/// incomplete acquisition, never a zero bill. This function cannot change the
/// registry or financially settle a production reservation.
/// §FS-rhei-budgets.12
pub fn collect_probe_finality(
    authorization: &ProbeAuthorization,
    output: &Path,
    inbox: &Path,
) -> Result<()> {
    let grant = &authorization.grant;
    let now = now_unix()?;
    if now > grant.finality_deadline_unix {
        return Err(refused("probe finality deadline expired; evidence remains incomplete"));
    }
    let limit = grant.maximum_artifact_bytes;
    if bounded_read(&output.join("authorization.json"), limit)? != authorization.evidence
        || bounded_read(&output.join("authorization.sig"), 64)? != authorization.signature
    {
        return Err(refused("capture directory belongs to another authorization"));
    }
    let results = bounded_read(&output.join("probe-results.json"), limit)?;
    for (entry, _) in &authorization.prerequisites {
        let path = output.join("prerequisites").join(entry.sha256.trim_start_matches("sha256:"));
        let data = bounded_read(&path, entry.bytes)?;
        if super::journal::digest(&data) != entry.sha256 || data.len() as u64 != entry.bytes {
            return Err(refused("retained prerequisite differs from the grant"));
        }
    }
    let cases: Vec<serde_json::Value> = serde_json::from_slice(&results)?;
    let names: BTreeSet<&str> = cases.iter().filter_map(|r| r["case"].as_str()).collect();
    let expected = super::probe::CASES
        .iter()
        .map(|case| {
            serde_json::to_value(case)
                .expect("case serialization")
                .as_str()
                .expect("case name")
                .to_owned()
        })
        .collect::<BTreeSet<_>>();
    if names != expected.iter().map(String::as_str).collect() || cases.len() != names.len() {
        return Err(refused("native probe results are incomplete"));
    }
    // Verify captured bytes again; a result hash cannot excuse missing raw data.
    for case in &cases {
        for row in case["artifacts"].as_array().ok_or_else(|| refused("missing artifact list"))? {
            let path = Path::new(row[0].as_str().ok_or_else(|| refused("invalid artifact path"))?);
            if path.components().any(|p| !matches!(p, std::path::Component::Normal(_))) {
                return Err(refused("artifact path escapes acquisition directory"));
            }
            let data = bounded_read(&output.join(path), limit)?;
            if row[1] != super::journal::digest(&data) || row[2] != data.len() as u64 {
                return Err(refused("raw native evidence differs from result manifest"));
            }
        }
    }
    let bill_bytes = bounded_read(&inbox.join("final-bill.json"), limit)?;
    let bill_signature = bounded_read(&inbox.join("final-bill.sig"), 64)?;
    verify(&bill_bytes, &bill_signature, &authorization.billing_key)?;
    let bill: FinalBill = serde_json::from_slice(&bill_bytes)?;
    let grant_hash = super::journal::digest(&authorization.evidence);
    let result_hash = super::journal::digest(&results);
    if bill.schema != "rhei.probe-final-bill.v1"
        || bill.grant_sha256 != grant_hash
        || bill.results_sha256 != result_hash
        || bill.tuple != grant.tuple
        || bill.account != grant.account
        || bill.currency != grant.currency
        || !bill.complete
        || bill.finalized_unix > now
        || bill.requests.is_empty()
    {
        return Err(refused("provider finality does not cover this acquisition"));
    }
    let mut identities = BTreeSet::new();
    let mut total = 0u64;
    for request in &bill.requests {
        if request.request_id.is_empty()
            || !identities.insert(&request.request_id)
            || !names.contains(request.case.as_str())
            || request.accepted_unix > grant.expires_unix
            || request.accepted_unix > request.finalized_unix
            || request.finalized_unix > bill.finalized_unix
        {
            return Err(refused("provider request identity or finality is invalid"));
        }
        total = total
            .checked_add(request.charge_micro)
            .ok_or_else(|| refused("final bill overflow"))?;
    }
    retain(&output.join("final-bill.json"), &bill_bytes)?;
    retain(&output.join("final-bill.sig"), &bill_signature)?;
    if total > grant.external_cap_micro {
        return Err(refused(
            "authoritative bill exceeds external probe cap; retained for investigation",
        ));
    }
    let review_bytes = bounded_read(&inbox.join("review.json"), limit)?;
    let review_signature = bounded_read(&inbox.join("review.sig"), 64)?;
    verify(&review_bytes, &review_signature, &authorization.review_key)?;
    let review: Review = serde_json::from_slice(&review_bytes)?;
    if review.schema != "rhei.probe-review.v1"
        || review.grant_sha256 != grant_hash
        || review.results_sha256 != result_hash
        || review.final_bill_sha256 != super::journal::digest(&bill_bytes)
        || !review.accepted
        || review.reviewed_unix < bill.finalized_unix
        || review.reviewed_unix > now
    {
        return Err(refused("independent review does not accept this complete acquisition"));
    }
    retain(&output.join("review.json"), &review_bytes)?;
    retain(&output.join("review.sig"), &review_signature)?;
    retain(&output.join("acquisition-complete.json"), &serde_json::to_vec(&review)?)
}

fn bounded_read(path: &Path, limit: u64) -> Result<Vec<u8>> {
    if !std::fs::symlink_metadata(path)?.file_type().is_file() {
        return Err(refused("acquisition input must be a regular file"));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)?.take(limit.saturating_add(1)).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(refused("acquisition input exceeds finite cap"));
    }
    Ok(bytes)
}
fn verify(bytes: &[u8], signature: &[u8], key: &[u8; 32]) -> Result<()> {
    let signature: &[u8; 64] =
        signature.try_into().map_err(|_| refused("invalid signature size"))?;
    VerifyingKey::from_bytes(key)
        .and_then(|k| k.verify_strict(bytes, &Signature::from_bytes(signature)))
        .map_err(|_| refused("acquisition signature failed"))
}
fn retain(path: &Path, bytes: &[u8]) -> Result<()> {
    if path.exists() {
        if bounded_read(path, bytes.len() as u64)? == bytes {
            return Ok(());
        }
        return Err(refused("conflicting acquired evidence already retained"));
    }
    write_new(path, bytes)
}
