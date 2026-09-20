//! Synthetic keys exercise mutation boundaries, never the release registry.
//! §FS-rhei-budgets.8.1 §FS-rhei-budgets.7

use super::*;
use crate::budget::*;
use ed25519_dalek::{Signer, SigningKey};

const START: &str = "2020-01-01T00:00:00Z";
const END: &str = "2026-01-02T00:00:00Z";
const ACCEPTED: &str = "2026-01-01T00:00:01Z";
const FINAL: &str = "2026-01-01T00:00:02Z";

fn tuple() -> LaunchTuple {
    LaunchTuple {
        executable_sha256: "sha256:fixture".into(),
        client_version: "fixture".into(),
        configuration_sha256: "sha256:fixture".into(),
        rhei_version: "fixture".into(),
        os: "fixture".into(),
        architecture: "fixture".into(),
        agent: "fixture".into(),
        provider: "fixture".into(),
        model: "fixture".into(),
        billing_contract: "fixture".into(),
        price_contract: "fixture".into(),
        confinement_policy: "fixture".into(),
    }
}
fn authority(project: &str) -> ProviderEvidenceAuthority {
    ProviderEvidenceAuthority::from_registry(
        SigningKey::from_bytes(&[42; 32]).verifying_key().to_bytes(),
        project.into(),
        tuple(),
        "sha256:fixture".into(),
        "account-A".into(),
        "USD".into(),
        START.into(),
    )
    .unwrap()
}
fn envelope(project: &str, reservation: &str) -> ProviderEvidence {
    ProviderEvidence {
        schema: "rhei.provider-evidence.v1".into(),
        project_id: project.into(),
        provider: "fixture".into(),
        account: "account-A".into(),
        currency: "USD".into(),
        billing_contract: "fixture".into(),
        price_contract: "fixture".into(),
        qualification: tuple(),
        qualification_evidence_hash: "sha256:fixture".into(),
        coverage_start: START.into(),
        coverage_end: END.into(),
        complete: true,
        records: vec![EvidenceRecord {
            invocation_id: "attempt-A".into(),
            attempt_identity: "attempt-A".into(),
            provider_request_id: "request:A".into(),
            reservation_id: Some(reservation.into()),
            accepted_at: ACCEPTED.into(),
            finalized_at: FINAL.into(),
            charge_micro: 1000,
            final_charge: true,
        }],
    }
}
fn signed(e: &ProviderEvidence) -> (Vec<u8>, [u8; 64]) {
    let bytes = serde_json::to_vec(e).unwrap();
    let signature = SigningKey::from_bytes(&[42; 32]).sign(&bytes).to_bytes();
    (bytes, signature)
}
fn proof(e: &ProviderEvidence) -> VerifiedReconciliation {
    let (bytes, signature) = signed(e);
    authority(&e.project_id)
        .verify_reconciliation(
            &bytes,
            &signature,
            e.records[0].reservation_id.as_deref().unwrap(),
            "attempt-A",
        )
        .unwrap()
}

pub(super) struct Case {
    pub(super) journal: Journal,
    _directory: CaseDirectory,
    pub(super) reservation: String,
    pub(super) qualification: QualifiedLaunch,
}
impl Case {
    pub(super) fn new() -> Self {
        Self::with_authority(None)
    }
    /// The same case with an ancestor admitted under a qualification that
    /// proved a finite delegation graph. §AR-neural-admission.7
    pub(super) fn with_descendant_envelope(micro: u64) -> Self {
        Self::build(None, micro)
    }
    fn with_authority(shared: Option<&Path>) -> Self {
        Self::build(shared, 0)
    }
    fn build(shared: Option<&Path>, descendant_envelope_micro: u64) -> Self {
        let uuid = uuid::Uuid::new_v4().to_string();
        let directory = std::env::temp_dir().join(format!("rhei-proof-{uuid}"));
        let root = directory.join("project");
        let dir = root.join(".agent-grounds/rhei/budgets").join(&uuid);
        std::fs::create_dir_all(&dir).unwrap();
        let authority = crate::budget::authority::Authority::lock_at(
            &root,
            shared.unwrap_or(&directory.join("authority")),
            &uuid,
            true,
        )
        .unwrap();
        let lock = File::create(dir.join("journal.jsonl.lock")).unwrap();
        lock.lock_exclusive().unwrap();
        let mut journal = Journal {
            authority,
            _lock: lock,
            path: dir.join("journal.jsonl"),
            project_id: format!("panta:{uuid}"),
            state: State::default(),
            receipts: vec![],
            last_hash: None,
            writable: true,
            event_sink: None,
        };
        journal.append("initialize", json!({"allowance": {"invocations": 3, "spend": {"currency": "USD", "amount_micro": 20000}},
            "history": {"status":"complete", "records":0}}), &audit()).unwrap();
        let ticket = format!("ticket:{uuid}:{}", uuid::Uuid::new_v4());
        let source = root.join("plan.md");
        std::fs::write(&source, "fixture").unwrap();
        journal.bind_ticket(&ticket, "task", &source, false, &audit()).unwrap();
        let now =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        let q = QualifiedLaunch {
            tuple: tuple(),
            evidence_hash: "sha256:fixture".into(),
            currency: "USD".into(),
            provider_account: "account-A".into(),
            residual_micro: 2000,
            maximum_input_tokens: 1000,
            maximum_output_tokens: 1000,
            input_micro_per_million: 1000000,
            output_micro_per_million: 1000000,
            valid_from_unix: 0,
            valid_until_unix: now + 100,
            watchdog_millis: 1000,
            descendant_envelope_micro,
        };
        let money = Money { currency: "USD".into(), amount_micro: 2000 };
        let group = journal
            .reserve(
                &AdmissionRequest {
                    ticket_identity: &ticket,
                    transition_limit: 3,
                    deadline_unix: now + 50,
                    arms: &[Arm {
                        attempt_identity: "attempt-A",
                        threshold: &money,
                        qualification: &q,
                        descendant_envelope_micro,
                    }],
                    parent_reservation: None,
                },
                &audit(),
            )
            .unwrap();
        let reservation = group.reservation_ids[0].clone();
        journal.record_start(&reservation, false, &audit()).unwrap();
        Self { journal, _directory: CaseDirectory(directory), reservation, qualification: q }
    }

    /// Re-derive the ledger state from the durable receipts alone, which is
    /// what every opener does — no balance is cached. §FS-rhei-budgets.3.1
    pub(super) fn replayed(&self) -> crate::budget::replay::State {
        let raw = std::fs::read_to_string(self.journal.path()).unwrap();
        let mut state = crate::budget::replay::State::default();
        for line in raw.lines().filter(|line| !line.trim().is_empty()) {
            state.apply(&serde_json::from_str::<Receipt>(line).unwrap()).unwrap();
        }
        state
    }
    pub(super) fn request(&mut self, id: &str) {
        self.journal
            .append(
                "broker_request",
                json!({"reservation_id":self.reservation,
            "request_id":id, "attempt_identity":"attempt-A", "request_hash":"sha256:fixture",
            "committed_at":"2026-01-01T00:00:00Z"}),
                &audit(),
            )
            .unwrap();
    }
}

/// A real financial mutation fences the same tuple in a second live project;
/// allowance increases cannot undo either the charge or the shared refusal.
/// §FS-rhei-budgets.9
#[test]
fn a_breach_refuses_admission_in_another_project_under_the_same_authority() {
    let mut first = Case::new();
    let authority_base = first._directory.0.join("authority");
    let mut second = Case::with_authority(Some(&authority_base));
    first.request("request:A");
    let mut bill = envelope(&first.journal.project_id, &first.reservation);
    bill.records[0].charge_micro = 5000;
    first.journal.reconcile_verified(proof(&bill), &audit()).unwrap();
    assert_eq!(first.journal.snapshot().unwrap().consumed.spend.amount_micro, 5000);
    assert!(first.journal.snapshot().unwrap().breached);
    first.journal.reconcile_verified(proof(&bill), &audit()).unwrap();
    let ticket = second.journal.state.reservations[&second.reservation].ticket.clone();
    let threshold = Money { currency: "USD".into(), amount_micro: 2000 };
    let request = AdmissionRequest {
        ticket_identity: &ticket,
        transition_limit: 3,
        deadline_unix: second.qualification.valid_until_unix - 1,
        arms: &[Arm {
            attempt_identity: "attempt-B",
            threshold: &threshold,
            qualification: &second.qualification,
            descendant_envelope_micro: 0,
        }],
        parent_reservation: None,
    };
    let before = std::fs::read(second.journal.path()).unwrap();
    assert_eq!(second.journal.reserve(&request, &audit()).unwrap_err().reason_code, "breach");
    assert_eq!(std::fs::read(second.journal.path()).unwrap(), before);
    let mut allowance = second.journal.snapshot().unwrap().allowance;
    allowance.spend.amount_micro += 10000;
    second.journal.adjust(allowance, &audit()).unwrap();
    assert_eq!(second.journal.reserve(&request, &audit()).unwrap_err().reason_code, "breach");
}
struct CaseDirectory(PathBuf);
impl Drop for CaseDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub(super) fn audit() -> Audit {
    Audit {
        actor: "test".into(),
        written_at: "2026-01-01T00:00:00Z".into(),
        reason: "fixture".into(),
        argv: vec![],
    }
}

#[test]
fn budget_proof_project_cannot_change_at_import() {
    let project = format!("panta:{}", uuid::Uuid::new_v4());
    let (bytes, signature) = signed(&envelope(&project, "reservation:A"));
    let history = authority(&project).verify_history(&bytes, &signature, START, END).unwrap();
    let result = Journal::initialize_imported(
        Path::new("must-not-be-created"),
        &uuid::Uuid::new_v4().to_string(),
        Allowance { invocations: 10, spend: Money { currency: "USD".into(), amount_micro: 10000 } },
        history,
        &audit(),
    );
    assert!(result.is_err());
    assert_eq!(result.err().unwrap().reason_code, "evidence_refused");
}

#[test]
fn budget_signed_invalid_times_and_scope_are_refused() {
    let original = envelope("panta:A", "reservation:A");
    for (field, value) in [
        ("coverage_start", "not-a-time"),
        ("coverage_end", START),
        ("coverage_end", "2099-01-01T00:00:00Z"),
        ("account", "account-B"),
        ("provider", "other"),
    ] {
        let mut json = serde_json::to_value(&original).unwrap();
        json[field] = value.into();
        let (bytes, signature) = signed(&serde_json::from_value(json).unwrap());
        assert!(
            authority("panta:A").verify_history(&bytes, &signature, START, END).is_err(),
            "{field}"
        );
    }
    for (accepted, finalized) in [
        ("no", FINAL),
        (FINAL, ACCEPTED),
        ("2019-01-01T00:00:00Z", FINAL),
        (ACCEPTED, "2026-02-01T00:00:00Z"),
    ] {
        let mut e = original.clone();
        e.records[0].accepted_at = accepted.into();
        e.records[0].finalized_at = finalized.into();
        let (bytes, signature) = signed(&e);
        assert!(authority("panta:A").verify_history(&bytes, &signature, START, END).is_err());
    }
    let (bytes, _) = signed(&original);
    let wrong = SigningKey::from_bytes(&[43; 32]).sign(&bytes).to_bytes();
    assert!(authority("panta:A").verify_history(&bytes, &wrong, START, END).is_err());
}

#[test]
fn budget_reconciliation_requires_every_committed_request_and_pinned_scope() {
    let mut case = Case::new();
    let e = envelope(&case.journal.project_id, &case.reservation);
    assert!(case.journal.reconcile_verified(proof(&e), &audit()).is_err());
    case.request("request:A");
    for change in ["account", "provider", "model", "project", "hash"] {
        let mut p = proof(&e);
        match change {
            "account" => p.scope.account = "account-B".into(),
            "provider" => p.scope.qualification.provider = "other".into(),
            "model" => p.scope.qualification.model = "other".into(),
            "project" => p.scope.project_id = "panta:B".into(),
            _ => p.scope.qualification_evidence_hash = "sha256:other".into(),
        }
        assert!(case.journal.reconcile_verified(p, &audit()).is_err(), "{change}");
    }
    case.request("request:B");
    assert!(case.journal.reconcile_verified(proof(&e), &audit()).is_err());
    let mut complete = e.clone();
    let mut second = complete.records[0].clone();
    second.provider_request_id = "request:B".into();
    complete.records.push(second);
    case.journal.reconcile_verified(proof(&complete), &audit()).unwrap();
    let count = case.journal.receipts().len();
    case.journal.reconcile_verified(proof(&complete), &audit()).unwrap();
    assert_eq!(case.journal.receipts().len(), count);
    let snapshot = case.journal.snapshot().unwrap();
    assert_eq!(snapshot.consumed.invocations, 1);
    assert_eq!(snapshot.consumed.spend.amount_micro, 2000);
    assert_eq!(snapshot.reserved.spend.amount_micro, 0);
    complete.records[0].charge_micro += 1;
    assert!(case.journal.reconcile_verified(proof(&complete), &audit()).is_err());
}
