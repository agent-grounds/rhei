//! Offline signature and completeness regressions do not qualify any provider.
//! §AR-neural-admission.8 §FS-rhei-budgets.6.3

use ed25519_dalek::{Signer, SigningKey};
use rhei_core::budget::{inspect_qualification_bundle, EvidenceRole, Registry};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

fn hash(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn bundle() -> Value {
    let roles = [
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
    let artifact = b"synthetic proof; no provider authority";
    json!({
        "schema": "rhei.qualification.contained.v1", "grade": "contained", "currency": "USD",
        "valid_from_unix": 1, "valid_until_unix": 2000000000u64,
        "tuple": {
            "executable_sha256": hash(b"client"), "client_version": "0.153.4",
            "configuration_sha256": hash(b"configuration"), "rhei_version": "0.5.1-dev",
            "os": "linux", "architecture": "x86_64", "agent": "codex", "provider": "openai",
            "model": "gpt-5.2-codex", "billing_contract": "fixture-billing",
            "price_contract": "fixture-price", "confinement_policy": "fixture-isolation"
        },
        "request": {
            "maximum_input_tokens": 2, "maximum_output_tokens": 3,
            "input_micro_per_million": 500000, "output_micro_per_million": 500001,
            "maximum_outstanding_requests": 1, "watchdog_millis": 100,
            "capture_policy": "durable_before_next_request",
            "post_kill_policy": "full_accepted_request_reserved",
            "nested_policy": "disabled", "request_shape": "text_responses_v1"
        },
        "artifacts": roles.iter().map(|role| json!({
            "role": role, "sha256": hash(artifact), "bytes": artifact.len(),
        })).collect::<Vec<_>>()
    })
}

fn verify(
    value: &Value,
) -> Result<rhei_core::budget::BundleInspection, rhei_core::budget::BudgetError> {
    let bytes = serde_json::to_vec(value).unwrap();
    let key = SigningKey::from_bytes(&[42; 32]);
    inspect_qualification_bundle(
        &bytes,
        &hash(&bytes),
        key.verifying_key().as_bytes(),
        &key.sign(&bytes).to_bytes(),
        |_| Ok(b"synthetic proof; no provider authority".to_vec()),
    )
}

#[test]
fn signed_inspection_rounds_up_residual_without_registering_a_tuple() {
    let report = verify(&bundle()).unwrap();
    assert_eq!(report.residual_micro, 3);
    assert!(Registry::qualify(&report.bundle.tuple).is_err());
    assert!(Registry::available().is_empty());
    let mut separate_rounding = bundle();
    separate_rounding["request"]["input_micro_per_million"] = json!(1);
    separate_rounding["request"]["output_micro_per_million"] = json!(1);
    assert_eq!(verify(&separate_rounding).unwrap().residual_micro, 2);
}

#[test]
fn every_evidence_role_is_required_even_with_a_valid_signature() {
    let original = bundle();
    for index in 0..original["artifacts"].as_array().unwrap().len() {
        let mut missing = original.clone();
        missing["artifacts"].as_array_mut().unwrap().remove(index);
        assert!(verify(&missing).is_err(), "missing role at {index}");
    }
}

#[test]
fn altered_bytes_keys_signatures_and_artifacts_refuse() {
    let bytes = serde_json::to_vec(&bundle()).unwrap();
    let key = SigningKey::from_bytes(&[42; 32]);
    let other = SigningKey::from_bytes(&[43; 32]);
    let signature = key.sign(&bytes).to_bytes();
    let wrong_signature = other.sign(&bytes).to_bytes();
    for (hash, public_key, signature, artifact) in [
        (
            hash(b"wrong"),
            key.verifying_key(),
            signature,
            b"synthetic proof; no provider authority".as_slice(),
        ),
        (
            hash(&bytes),
            other.verifying_key(),
            signature,
            b"synthetic proof; no provider authority".as_slice(),
        ),
        (
            hash(&bytes),
            key.verifying_key(),
            wrong_signature,
            b"synthetic proof; no provider authority".as_slice(),
        ),
        (hash(&bytes), key.verifying_key(), signature, b"changed evidence".as_slice()),
    ] {
        assert!(inspect_qualification_bundle(
            &bytes,
            &hash,
            public_key.as_bytes(),
            &signature,
            |_| Ok(artifact.to_vec())
        )
        .is_err());
    }
}

#[test]
fn unsupported_or_overflowing_containment_never_gets_a_residual() {
    for (field, value) in [
        ("maximum_outstanding_requests", json!(2)),
        ("watchdog_millis", json!(0)),
        ("capture_policy", json!("best_effort")),
        ("post_kill_policy", json!("local_kill")),
        ("nested_policy", json!("unrestricted")),
        ("request_shape", json!("images")),
    ] {
        let mut changed = bundle();
        changed["request"][field] = value;
        assert!(verify(&changed).is_err(), "accepted {field}");
    }
    let mut overflow = bundle();
    for field in [
        "maximum_input_tokens",
        "maximum_output_tokens",
        "input_micro_per_million",
        "output_micro_per_million",
    ] {
        overflow["request"][field] = json!(u64::MAX);
    }
    assert!(verify(&overflow).is_err());
}
