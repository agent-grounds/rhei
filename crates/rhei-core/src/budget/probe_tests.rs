//! Synthetic authority tests never launch a client or register a real tuple.
//! §FS-rhei-budgets.12

use super::*;
use ed25519_dalek::{Signer, SigningKey};
use serde_json::{json, Value};

struct Case {
    directory: PathBuf,
    authorization: ProbeAuthorization,
    bill: Value,
    review: Value,
}
impl Case {
    fn new() -> Self {
        let directory =
            std::env::temp_dir().join(format!("rhei-finality-{}", uuid::Uuid::new_v4()));
        let output = directory.join("output");
        let inbox = directory.join("inbox");
        std::fs::create_dir_all(&output).unwrap();
        std::fs::create_dir_all(&inbox).unwrap();
        let now = now_unix().unwrap();
        let tuple = LaunchTuple {
            executable_sha256: "sha256:synthetic".into(),
            client_version: "fixture".into(),
            configuration_sha256: "sha256:synthetic".into(),
            rhei_version: "fixture".into(),
            os: "fixture".into(),
            architecture: "fixture".into(),
            agent: "fixture".into(),
            provider: "fixture".into(),
            model: "fixture".into(),
            billing_contract: "fixture".into(),
            price_contract: "fixture".into(),
            confinement_policy: "fixture".into(),
        };
        let grant = ProbeGrant {
            schema: "rhei.qualification-probe.v1".into(),
            tuple: tuple.clone(),
            account: "account".into(),
            currency: "USD".into(),
            external_cap_id: "fixture-cap".into(),
            external_cap_micro: 10000,
            maximum_launches: 14,
            per_case_millis: 100,
            expires_unix: now + 60,
            finality_deadline_unix: now + 120,
            maximum_artifact_bytes: 100000,
            artifacts: vec![],
        };
        let authorization = ProbeAuthorization {
            evidence: serde_json::to_vec(&grant).unwrap(),
            grant,
            signature: [0; 64],
            billing_key: SigningKey::from_bytes(&[41; 32]).verifying_key().to_bytes(),
            review_key: SigningKey::from_bytes(&[42; 32]).verifying_key().to_bytes(),
            prerequisites: vec![],
        };
        write_new(&output.join("authorization.json"), &authorization.evidence).unwrap();
        write_new(&output.join("authorization.sig"), &authorization.signature).unwrap();
        write_new(&output.join("raw.log"), b"raw capture").unwrap();
        let results = CASES
            .iter()
            .map(|case| {
                json!({
                    "case": case, "exit_code": 0, "timed_out": false, "error": null,
                    "artifacts": [["raw.log", super::super::journal::digest(b"raw capture"), 11]],
                })
            })
            .collect::<Vec<_>>();
        let results = serde_json::to_vec(&results).unwrap();
        write_new(&output.join("probe-results.json"), &results).unwrap();
        let bill = json!({
            "schema": "rhei.probe-final-bill.v1", "grant_sha256": super::super::journal::digest(&authorization.evidence),
            "results_sha256": super::super::journal::digest(&results), "tuple": tuple,
            "account": "account", "currency": "USD", "complete": true, "finalized_unix": now,
            "requests": [{"case": "allowed_request", "request_id": "request:A", "accepted_unix": now,
                "finalized_unix": now, "charge_micro": 2000}],
        });
        let review = json!({
            "schema": "rhei.probe-review.v1", "grant_sha256": bill["grant_sha256"],
            "results_sha256": bill["results_sha256"], "final_bill_sha256": "pending",
            "accepted": true, "reviewed_unix": now,
        });
        let mut case = Self { directory, authorization, bill, review };
        case.sign();
        case
    }
    fn sign(&mut self) {
        let bill = serde_json::to_vec(&self.bill).unwrap();
        self.review["final_bill_sha256"] = super::super::journal::digest(&bill).into();
        let review = serde_json::to_vec(&self.review).unwrap();
        for (name, bytes, key) in [("final-bill", bill, [41; 32]), ("review", review, [42; 32])] {
            std::fs::write(self.directory.join("inbox").join(format!("{name}.json")), &bytes)
                .unwrap();
            std::fs::write(
                self.directory.join("inbox").join(format!("{name}.sig")),
                SigningKey::from_bytes(&key).sign(&bytes).to_bytes(),
            )
            .unwrap();
        }
    }
    fn collect(&self) -> Result<()> {
        super::super::collect_probe_finality(
            &self.authorization,
            &self.directory.join("output"),
            &self.directory.join("inbox"),
        )
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn finality_requires_both_independent_signatures_and_unchanged_raw_artifacts() {
    let case = Case::new();
    case.collect().unwrap();
    case.collect().unwrap();
    assert!(case.directory.join("output/acquisition-complete.json").is_file());
    std::fs::write(case.directory.join("output/raw.log"), b"replacement").unwrap();
    assert!(case.collect().is_err());
    let case = Case::new();
    std::fs::write(case.directory.join("inbox/review.sig"), [0; 64]).unwrap();
    assert!(case.collect().is_err());
    assert!(!case.directory.join("output/acquisition-complete.json").exists());
}

#[test]
fn signed_wrong_scope_partial_finality_and_cap_breach_do_not_complete_acquisition() {
    for (field, value) in [
        ("account", json!("other")),
        ("complete", json!(false)),
        ("results_sha256", json!("sha256:other")),
        ("currency", json!("EUR")),
    ] {
        let mut case = Case::new();
        case.bill[field] = value;
        case.sign();
        assert!(case.collect().is_err(), "{field}");
    }
    let mut case = Case::new();
    case.bill["requests"][0]["charge_micro"] = 10001.into();
    case.sign();
    assert!(case.collect().is_err());
    assert!(case.directory.join("output/final-bill.json").is_file());
    assert!(!case.directory.join("output/acquisition-complete.json").exists());
}
