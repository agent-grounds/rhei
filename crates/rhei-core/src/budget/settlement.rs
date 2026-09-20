//! Settlement and central-edge receipt matching never infer missing evidence.
//! §AR-neural-admission.6

use super::journal::digest;
use super::{Audit, BudgetError, Journal, Result};
use serde_json::json;

/// Final bill evidence produced only by a qualified provider adapter. Neither
/// operator JSON nor a price ceiling can construct it. §FS-rhei-budgets.7
pub struct ProviderSettlement {
    project_id: String,
    reservation_id: String,
    charge_micro: u64,
    evidence: Vec<u8>,
}

impl ProviderSettlement {
    pub(crate) fn verified(
        project_id: String,
        reservation_id: String,
        charge_micro: u64,
        evidence: Vec<u8>,
    ) -> Self {
        Self { project_id, reservation_id, charge_micro, evidence }
    }
}

impl Journal {
    /// An arm's end is durable, but its siblings still own the group's travel.
    /// §FS-rhei-budgets.5 §AR-neural-admission.6
    pub fn record_invocation_end(&mut self, reservation: &str, audit: &Audit) -> Result<()> {
        if self.state.invocation_ends.contains(reservation) {
            return Ok(());
        }
        self.append("invocation_end", json!({"reservation_id": reservation}), audit)
    }

    pub fn release_completed_group_travel(&mut self, owner: &str, audit: &Audit) -> Result<()> {
        let members = self
            .state
            .reservations
            .iter()
            .filter(|(_, r)| r.payload["travel_reservation_id"] == owner)
            .map(|(id, _)| id)
            .collect::<Vec<_>>();
        if !members.is_empty()
            && members.iter().all(|id| self.state.invocation_ends.contains(*id))
            && self.state.reservations.get(owner).is_some_and(|r| r.travel)
        {
            self.release_travel(owner, audit)?;
        }
        Ok(())
    }

    /// Reconcile only an opaque token produced by the pinned provider/account
    /// verifier. Operator-authored totals cannot reach this path.
    /// §FS-rhei-budgets.8.1
    pub fn reconcile_verified(
        &mut self,
        proof: super::VerifiedReconciliation,
        audit: &Audit,
    ) -> Result<()> {
        let reservation = proof.reservation_id.clone();
        let state = self
            .state
            .reservations
            .get(&reservation)
            .ok_or_else(|| BudgetError::corrupt("reconciliation reservation is absent"))?;
        let scope = &proof.scope;
        if scope.project_id != self.project_id
            || scope.currency != self.snapshot()?.allowance.spend.currency
            || serde_json::to_value(&scope.qualification)? != state.payload["qualification"]
            || scope.qualification_evidence_hash != state.payload["qualification_evidence_hash"]
            || scope.account != state.payload["provider_account"]
        {
            return Err(super::provider_evidence::refused(
                "evidence authority does not own this reservation",
            ));
        }
        let requests = self
            .state
            .broker_requests
            .values()
            .filter(|r| r["reservation_id"] == reservation)
            .collect::<Vec<_>>();
        if requests.is_empty() || requests.len() != proof.envelope.records.len() {
            return Err(super::provider_evidence::refused(
                "final bill does not cover every durable broker request",
            ));
        }
        for record in &proof.envelope.records {
            let request = requests
                .iter()
                .find(|r| r["request_id"] == record.provider_request_id)
                .ok_or_else(|| {
                super::provider_evidence::refused("final bill names an uncommitted request")
            })?;
            let committed = super::provider_evidence::timestamp(super::replay::string(
                request,
                "committed_at",
            )?)?;
            if record.attempt_identity != state.payload["attempt_identity"]
                || record.invocation_id != state.payload["attempt_identity"]
                || super::provider_evidence::timestamp(&record.accepted_at)? < committed
            {
                return Err(super::provider_evidence::refused(
                    "final bill does not match broker request ownership/time",
                ));
            }
        }
        let settlement = ProviderSettlement::verified(
            self.project_id.clone(),
            reservation.clone(),
            proof.charge_micro,
            proof.evidence,
        );
        self.settle(&reservation, settlement, "reconcile", audit)
    }

    /// The broker's complete capture is the only in-process settlement source.
    /// Legacy accounting totals and an operator price override cannot call it.
    /// §FS-rhei-budgets.7
    pub fn settle_captured(
        &mut self,
        reservation: &str,
        proof: ProviderSettlement,
        audit: &Audit,
    ) -> Result<()> {
        self.settle(reservation, proof, "settle", audit)
    }

    fn settle(
        &mut self,
        reservation: &str,
        proof: ProviderSettlement,
        receipt_kind: &str,
        audit: &Audit,
    ) -> Result<()> {
        audit.validate()?;
        if !self.writable {
            return Err(BudgetError::corrupt("read-only budget transaction cannot settle"));
        }
        if proof.project_id != self.project_id || proof.reservation_id != reservation {
            return Err(super::provider_evidence::refused(
                "settlement belongs to another reservation or project",
            ));
        }
        let charge = proof.charge_micro;
        let evidence = proof.evidence.as_slice();
        let r = self
            .state
            .reservations
            .get(reservation)
            .ok_or_else(|| BudgetError::corrupt("settlement reservation is absent"))?;
        let hash = digest(evidence);
        if let Some(previous) = r.settled {
            if previous == charge
                && self.receipts.iter().any(|receipt| {
                    matches!(receipt.kind.as_str(), "settle" | "reconcile" | "breach")
                        && receipt.payload["reservation_id"] == reservation
                        && receipt.payload["evidence_hash"] == hash
                })
            {
                return Ok(());
            }
            return Err(BudgetError::corrupt(
                "settlement evidence conflicts with an existing receipt",
            ));
        }
        if charge > r.fwc {
            let payload = json!({"reservation_id": reservation,
                "actual_charge_micro": charge, "reserved_charge_micro": r.fwc,
                "qualification": r.payload["qualification"], "evidence_hash": hash});
            let locks = self.lock_tuples(&[payload["qualification"].clone()])?;
            locks[0].invalidate(&self.project_id, &payload, audit)?;
            return self.append("breach", payload, audit);
        }
        self.append(
            receipt_kind,
            json!({"reservation_id": reservation,
            "charge_micro": charge, "evidence_hash": hash,
            "unused_micro": r.fwc - charge}),
            audit,
        )
    }

    /// Called only with the durable central entry. Identical recovery is a
    /// no-op; neither timestamps nor task display IDs are receipt identities.
    /// §FS-rhei-budgets.3.3 §FS-rhei-budgets.7
    pub fn record_transition(
        &mut self,
        reservation: &str,
        central_receipt_id: &str,
        from: &str,
        to: &str,
        audit: &Audit,
    ) -> Result<()> {
        let r = self
            .state
            .reservations
            .get(reservation)
            .ok_or_else(|| BudgetError::corrupt("transition reservation is absent"))?;
        let payload = json!({"reservation_id": reservation, "ticket_identity": r.ticket,
            "transition_receipt_id": central_receipt_id, "central_receipt_id": central_receipt_id,
            "from": from, "to": to});
        if let Some(previous) = self.state.transitions.get(central_receipt_id) {
            return if previous == &payload {
                Ok(())
            } else {
                Err(BudgetError::corrupt("conflicting earned-edge recovery"))
            };
        }
        self.append("transition", payload, audit)
    }
}
