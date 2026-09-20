//! All-or-none invocation, money, and travel reservation.
//! §FS-rhei-budgets.4 §FS-rhei-budgets.5

use super::types::add;
use super::{Audit, BudgetError, Journal, Money, QualifiedLaunch, Result};
use serde_json::json;

/// A resolved arm carries proof rather than a declared monetary maximum.
/// §FS-rhei-budgets.6.2
pub struct Arm<'a> {
    pub attempt_identity: &'a str,
    pub threshold: &'a Money,
    pub qualification: &'a QualifiedLaunch,
    /// What this invocation's own descendants — a nested `rhei run`, an
    /// embedded caller of the same runtime — may together reserve. Zero
    /// permits no nested neural work at all, which is what an unqualified
    /// delegation graph means. §FS-rhei-budgets.5 §AR-neural-admission.7
    pub descendant_envelope_micro: u64,
}

/// Inner bounds have already been checked by the scheduler; finite time and
/// travel are checked again at the capability boundary. §FS-rhei-budgets.4
pub struct AdmissionRequest<'a> {
    pub ticket_identity: &'a str,
    pub transition_limit: u64,
    pub deadline_unix: u64,
    pub arms: &'a [Arm<'a>],
    pub parent_reservation: Option<&'a str>,
}

/// A durable group claims one travel unit and one invocation/FWC per arm.
/// It conveys financial ownership, never an unconfined process capability.
/// §AR-neural-admission.3
#[derive(Debug)]
pub struct ReservationGroup {
    pub reservation_ids: Vec<String>,
    pub travel_reservation_id: String,
    pub transition_receipt_id: String,
}

impl Journal {
    /// Inspection and mutation use the same checks. `preview` cannot debit.
    /// §FS-rhei-budgets.4
    pub fn preview(&self, request: &AdmissionRequest<'_>) -> Result<u64> {
        self.validate_identity_sources()?;
        if !self
            .state
            .identities
            .get(request.ticket_identity)
            .is_some_and(|binding| binding["status"] == "installed")
        {
            return Err(BudgetError::new(
                "identity_conflict",
                "ticket identity must be durably installed before reservation",
            ));
        }
        let ancestor = self.authenticate_ancestor(request)?;
        if request.transition_limit == 0 || request.arms.is_empty() {
            return Err(BudgetError::bounds(
                "a finite transition_limit and at least one qualified arm are required",
            ));
        }
        let snapshot = self.snapshot()?;
        if snapshot.breached {
            return Err(BudgetError::new("breach", "qualification breach retained; increasing the allowance does not requalify execution"));
        }
        let count = u64::try_from(request.arms.len())
            .map_err(|_| BudgetError::bounds("fanout count overflow"))?;
        if count > snapshot.remaining.invocations {
            let message = if count > 1 {
                format!(
                    "fanout requires {count} invocation units; only {} remains",
                    snapshot.remaining.invocations
                )
            } else {
                format!(
                    "Panta invocation allowance exhausted ({} consumed + {} reserved / {})",
                    snapshot.consumed.invocations,
                    snapshot.reserved.invocations,
                    snapshot.allowance.invocations
                )
            };
            return Err(BudgetError::new("invocation_exhausted", message));
        }
        let travel = snapshot.travel.get(request.ticket_identity).cloned().unwrap_or_default();
        if travel.exposure()? >= request.transition_limit {
            return Err(BudgetError::new(
                "travel_exhausted",
                format!(
                    "ticket travel exhausted ({} applied + {} reserved / {})",
                    travel.consumed, travel.reserved, request.transition_limit
                ),
            ));
        }
        let mut total = 0;
        let mut attempts = std::collections::BTreeSet::new();
        for arm in request.arms {
            self.check_tuple(&serde_json::to_value(arm.qualification.tuple())?)?;
            arm.threshold.validate()?;
            arm.qualification.check_validity(request.deadline_unix)?;
            if arm.threshold.currency != snapshot.allowance.spend.currency
                || arm.threshold.currency != arm.qualification.currency
            {
                return Err(BudgetError::bounds(
                    "threshold, qualification, and project currencies must match",
                ));
            }
            if arm.attempt_identity.is_empty()
                || self.state.attempts.contains(arm.attempt_identity)
                || !attempts.insert(arm.attempt_identity)
            {
                return Err(BudgetError::corrupt("attempt identity was already reserved"));
            }
            total = add(total, add(arm.threshold.amount_micro, arm.qualification.residual_micro)?)?;
        }
        if let Some((parent, envelope)) = ancestor {
            let drawn = add(self.descendants_drawn(&parent)?, total)?;
            if drawn > envelope {
                return Err(BudgetError::new(
                    "ancestor_envelope_exhausted",
                    format!(
                        "ancestor {parent} permits {envelope} micro-{} of descendant work; {drawn} is claimed",
                        snapshot.allowance.spend.currency
                    ),
                ));
            }
        }
        if total > snapshot.remaining.spend.amount_micro {
            return Err(BudgetError::new(
                "spend_exhausted",
                format!(
                    "Panta spend allowance exhausted ({} consumed + {} reserved / {} micro-{})",
                    snapshot.consumed.spend.amount_micro,
                    snapshot.reserved.spend.amount_micro,
                    snapshot.allowance.spend.amount_micro,
                    snapshot.allowance.spend.currency
                ),
            ));
        }
        Ok(total)
    }

    /// One receipt contains every arm. A partial append cannot release any
    /// capabilities or leave a valid partial fanout behind. §FS-rhei-budgets.5
    pub fn reserve(
        &mut self,
        request: &AdmissionRequest<'_>,
        audit: &Audit,
    ) -> Result<ReservationGroup> {
        let tuples = request
            .arms
            .iter()
            .map(|arm| serde_json::to_value(arm.qualification.tuple()))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let _tuple_locks = self.lock_tuples(&tuples)?;
        self.preview(request)?;
        let transition_receipt_id = format!("transition:{}", uuid::Uuid::new_v4());
        let mut ids = Vec::new();
        let mut reservations = Vec::new();
        for (index, arm) in request.arms.iter().enumerate() {
            let id = format!("reservation:{}", uuid::Uuid::new_v4());
            reservations.push(json!({
                "reservation_id": id, "attempt_identity": arm.attempt_identity,
                "transition_receipt_id": transition_receipt_id,
                "ticket_identity": request.ticket_identity,
                "parent_reservation": request.parent_reservation,
                "descendant_envelope_micro": arm.descendant_envelope_micro,
                "qualification": arm.qualification.tuple,
                "qualification_evidence_hash": arm.qualification.evidence_hash,
                "provider_account": arm.qualification.provider_account,
                "qualification_grade": "contained", "watchdog_millis": arm.qualification.watchdog_millis,
                "invocation_units": 1, "threshold_micro": arm.threshold.amount_micro,
                "residual_micro": arm.qualification.residual_micro,
                "fwc_micro": add(arm.threshold.amount_micro, arm.qualification.residual_micro)?,
                "travel_units": if index == 0 { 1 } else { 0 },
                "transition_limit": request.transition_limit,
                "deadline_unix": request.deadline_unix,
            }));
            ids.push(id);
        }
        for reservation in &mut reservations {
            reservation["travel_reservation_id"] = ids[0].clone().into();
        }
        self.append("reserve", json!({"reservations": reservations}), audit)?;
        // Capture headers exist durably before any capability can leave the
        // transaction. A missing capture later is corruption, never a reset.
        // §FS-rhei-budgets.6.3 §FS-rhei-budgets.7
        for id in &ids {
            super::broker::initialize_capture(self, id, audit)?;
        }
        Ok(ReservationGroup {
            travel_reservation_id: ids[0].clone(),
            reservation_ids: ids,
            transition_receipt_id,
        })
    }

    /// Record ambiguity *before* capability transfer. A crash after this
    /// point can never refund the invocation; confirmation only refines it.
    /// §AR-neural-admission.4
    pub fn record_start(
        &mut self,
        reservation: &str,
        confirmed: bool,
        audit: &Audit,
    ) -> Result<()> {
        self.append(
            "start",
            json!({"reservation_id": reservation,
            "status": if confirmed { "confirmed" } else { "ambiguous" }}),
            audit,
        )
    }

    /// Release only a reservation whose capability was never transferred.
    /// A failed prompt, timeout, or local kill is not evidence for this call.
    /// §FS-rhei-budgets.7
    pub fn release_unstarted(&mut self, reservation: &str, audit: &Audit) -> Result<()> {
        self.append(
            "release",
            json!({"reservation_id": reservation,
            "proof": "no_capability_transferred"}),
            audit,
        )
    }

    /// Poll waits and completions without an edge release travel alone.
    /// §FS-rhei-budgets.2.2 §FS-rhei-budgets.5
    pub fn release_travel(&mut self, reservation: &str, audit: &Audit) -> Result<()> {
        self.append("release", json!({"reservation_id": reservation, "travel_only": true}), audit)
    }
}

impl Journal {
    /// Resolve the ancestor a nested or embedded caller claims, and return its
    /// declared descendant envelope.
    ///
    /// An ancestry token is a name, not a capability: the ledger is what says
    /// whether that reservation exists here, is still outstanding, and was
    /// admitted with room for descendants. A child that cannot be placed under
    /// a live ancestor of this very project refuses rather than opening a
    /// balance of its own — which is the laundering route the ancestry rule
    /// exists to close. §FS-rhei-budgets.5 §AR-neural-admission.7
    fn authenticate_ancestor(
        &self,
        request: &AdmissionRequest<'_>,
    ) -> Result<Option<(String, u64)>> {
        let Some(parent) = request.parent_reservation else {
            return Ok(None);
        };
        let unavailable = |why: &str| {
            BudgetError::new(
                "ancestor_unavailable",
                format!("nested admission cannot use ancestor {parent}: {why}"),
            )
        };
        let reservation = self
            .state
            .reservations
            .get(parent)
            .ok_or_else(|| unavailable("no such reservation"))?;
        if reservation.released || reservation.settled.is_some() {
            return Err(unavailable("the ancestor invocation has already been released"));
        }
        if !reservation.started {
            return Err(unavailable("the ancestor invocation has not started"));
        }
        let envelope = reservation.payload["descendant_envelope_micro"]
            .as_u64()
            .ok_or_else(|| unavailable("it was admitted without a descendant envelope"))?;
        if envelope == 0 {
            return Err(unavailable("its qualification permits no nested neural work"));
        }
        let deadline = reservation.payload["deadline_unix"]
            .as_u64()
            .ok_or_else(|| unavailable("it carries no finite deadline"))?;
        if request.deadline_unix > deadline {
            return Err(unavailable("a descendant may not outlive its ancestor's deadline"));
        }
        Ok(Some((parent.to_owned(), envelope)))
    }

    /// What this ancestor's descendants already hold against its envelope.
    /// A released child gives its share back; a started one never does.
    /// §FS-rhei-budgets.5
    fn descendants_drawn(&self, parent: &str) -> Result<u64> {
        let mut drawn = 0;
        for reservation in self.state.reservations.values() {
            if reservation.payload["parent_reservation"].as_str() != Some(parent) {
                continue;
            }
            if reservation.released {
                continue;
            }
            drawn = add(drawn, reservation.settled.unwrap_or(reservation.fwc))?;
        }
        Ok(drawn)
    }
}
