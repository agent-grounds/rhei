//! `preview`, `reserve`, `record_start`, the two releases, the travel charge,
//! and ancestry authentication.
//!
//! One account held exclusively is what serializes competing processes,
//! parallel arms, and nested runtimes: two racing `rhei run` processes cannot
//! both take the last unit.
//! §FS-rhei-budgets.6 §AR-neural-admission.1

use super::journal::{Audit, Journal};
use super::types::{add, BudgetError, Contract, Dimension, Exhaustion};
use super::Result;
use serde_json::json;

/// One arm of a fanout group. Every arm costs its own invocation unit; only the
/// first carries the ticket's one travel unit. §FS-rhei-budgets.4.2
pub struct Arm<'a> {
    pub attempt_identity: &'a str,
    /// What this invocation's own descendants — a nested `rhei run`, an
    /// embedded caller of the same runtime — may together reserve, in
    /// invocation units. Zero permits no nested neural work at all.
    /// §FS-rhei-budgets.7
    pub descendant_envelope: u64,
}

/// The bounds in force, resolved and clamped by the caller. Admission checks
/// against them; it never resolves them, because settings resolve in one place
/// and it is not here. §FS-rhei-budgets.2
#[derive(Clone, Copy, Debug)]
pub struct EffectiveBounds {
    pub transition_limit: u64,
    pub invocations_per_day: u64,
}

pub struct AdmissionRequest<'a> {
    pub ticket_identity: &'a str,
    /// How the halt names the ticket: its display id, not its uuid.
    pub display_id: &'a str,
    /// How the halt names the project.
    pub project_label: &'a str,
    /// The execution root of the run that owns this reservation. Its lock is
    /// the proof of non-start. §FS-rhei-budgets.6.2
    pub execution_root: &'a str,
    pub arms: &'a [Arm<'a>],
    pub parent_reservation: Option<&'a str>,
    /// Whether this admission holds a travel unit for the edge the visit is
    /// expected to apply. A spawn that cannot move the ticket holds none.
    pub travel: bool,
}

/// A durable group: one travel unit for the ticket and one invocation unit per
/// arm, all-or-none. §FS-rhei-budgets.4.2
#[derive(Clone, Debug)]
pub struct ReservationGroup {
    pub reservation_ids: Vec<String>,
    /// The arm that carries the ticket's travel unit, where one was taken.
    pub travel_reservation_id: Option<String>,
}

impl Journal {
    /// Inspection and mutation use the same checks; `preview` cannot debit, so
    /// `rhei validate` and `--dry-run` reach exactly this code.
    /// §FS-rhei-budgets.6.3
    pub fn preview(
        &self,
        request: &AdmissionRequest<'_>,
        bounds: EffectiveBounds,
    ) -> Result<()> {
        self.validate_identity_sources()?;
        if !self.identity_installed(request.ticket_identity) {
            return Err(BudgetError::new(
                "identity_conflict",
                "ticket identity must be durably installed before reservation",
            ));
        }
        if request.arms.is_empty() {
            return Err(BudgetError::bounds("admission needs at least one arm"));
        }
        let ancestor = self.authenticate_ancestor(request)?;
        let snapshot = self.snapshot()?;
        let contract = snapshot.contract.clone();
        let invocation_bound = snapshot.invocation_bound(bounds.invocations_per_day);
        let count = u64::try_from(request.arms.len())
            .map_err(|_| BudgetError::bounds("fanout count overflow"))?;

        // Travel first: the ticket's own life is the narrower question, and a
        // ticket at its bound should be told about its bound rather than about
        // the project's day. §FS-rhei-budgets.4.1
        if request.travel {
            let travel = snapshot.travel_for(request.ticket_identity);
            if add(travel.exposure()?, 1)? > bounds.transition_limit {
                return Err(BudgetError::exhausted(
                    "travel_exhausted",
                    format!(
                        "ticket '{}' has spent its travel bound ({} applied + {} held / {})",
                        request.display_id,
                        travel.consumed,
                        travel.reserved,
                        bounds.transition_limit
                    ),
                    Exhaustion {
                        dimension: Dimension::Travel,
                        subject: request.display_id.into(),
                        counter: travel,
                        bound: bounds.transition_limit,
                        contract: contract.clone(),
                        day: snapshot.day.clone(),
                    },
                ));
            }
        }
        if add(snapshot.invocations.exposure()?, count)? > invocation_bound {
            return Err(BudgetError::exhausted(
                "invocation_exhausted",
                format!(
                    "project '{}' has no invocation capacity left ({} consumed + {} outstanding \
                     / {})",
                    request.project_label,
                    snapshot.invocations.consumed,
                    snapshot.invocations.reserved,
                    invocation_bound
                ),
                Exhaustion {
                    dimension: Dimension::Invocations,
                    subject: request.project_label.into(),
                    counter: snapshot.invocations,
                    bound: invocation_bound,
                    contract,
                    day: snapshot.day.clone(),
                },
            ));
        }
        let mut attempts = std::collections::BTreeSet::new();
        for arm in request.arms {
            if arm.attempt_identity.is_empty()
                || self.state.attempts.contains(arm.attempt_identity)
                || !attempts.insert(arm.attempt_identity)
            {
                return Err(BudgetError::corrupt("attempt identity was already reserved"));
            }
        }
        if let Some((parent, envelope)) = ancestor {
            let drawn = add(self.state.descendants_drawn(&parent)?, count)?;
            if drawn > envelope {
                return Err(BudgetError::new(
                    "ancestor_envelope_exhausted",
                    format!(
                        "ancestor {parent} permits {envelope} descendant invocations; \
                         {drawn} is claimed"
                    ),
                ));
            }
        }
        Ok(())
    }

    /// One receipt contains every arm. A partial append cannot leave a valid
    /// partial fanout behind: if any arm refuses, nothing is appended.
    /// §AR-neural-admission.3
    pub fn reserve(
        &mut self,
        request: &AdmissionRequest<'_>,
        bounds: EffectiveBounds,
        audit: &Audit,
    ) -> Result<ReservationGroup> {
        self.preview(request, bounds)?;
        let window = self.day().to_string();
        let mut ids = Vec::new();
        let mut reservations = Vec::new();
        for (index, arm) in request.arms.iter().enumerate() {
            let id = format!("reservation:{}", uuid::Uuid::new_v4());
            // The fanout rule of §FS-rhei-budgets.4.1 as arithmetic rather than
            // as a special case: one applied edge, however many arms.
            let travel_units = u64::from(request.travel && index == 0);
            reservations.push(json!({
                "reservation_id": id,
                "attempt_identity": arm.attempt_identity,
                "ticket_identity": request.ticket_identity,
                "display_id": request.display_id,
                "parent_reservation": request.parent_reservation,
                "descendant_envelope": arm.descendant_envelope,
                "execution_root": request.execution_root,
                "invocation_units": 1,
                "travel_units": travel_units,
                "window": window,
            }));
            ids.push(id);
        }
        self.append_in_window(
            "reserve",
            json!({"reservations": reservations}),
            audit,
            Some(window),
        )?;
        Ok(ReservationGroup {
            travel_reservation_id: request.travel.then(|| ids[0].clone()),
            reservation_ids: ids,
        })
    }

    /// Record ambiguity *before* capability transfer. A crash after this point
    /// can never refund the invocation; confirmation only refines it.
    /// §FS-rhei-budgets.6.2
    pub fn record_start(&mut self, reservation: &str, confirmed: bool, audit: &Audit) -> Result<()> {
        self.append(
            "start",
            json!({"reservation_id": reservation,
            "status": if confirmed { "confirmed" } else { "ambiguous" }}),
            audit,
        )
    }

    /// The one lawful release: engine-side proof that no process could have
    /// started. A failed prompt, a timeout, or a local kill is not evidence for
    /// this call. §FS-rhei-budgets.6.2
    pub fn release_unstarted(&mut self, reservation: &str, audit: &Audit) -> Result<()> {
        self.append(
            "release",
            json!({"reservation_id": reservation, "proof": "no_start_record"}),
            audit,
        )
    }

    /// A poll wait, a completion that selects no edge, or a stall gives the
    /// held travel unit back. The invocation is not affected: the process ran.
    /// §FS-rhei-budgets.4.1
    pub fn release_travel(&mut self, reservation: &str, audit: &Audit) -> Result<()> {
        self.append("release", json!({"reservation_id": reservation, "travel_only": true}), audit)
    }

    /// Charge one travel unit for an applied edge.
    ///
    /// With a reservation it converts the unit that admission already held.
    /// Without one it is a standalone charge — a person running `rhei
    /// transition`, or a callback redirect — and is checked against the bound
    /// here, because nothing checked it earlier. A manual path that moved for
    /// free would be the bypass. §FS-rhei-budgets.4.1
    pub fn charge_travel(
        &mut self,
        ticket: &str,
        display_id: &str,
        from: &str,
        to: &str,
        reservation: Option<&str>,
        transition_limit: u64,
        audit: &Audit,
    ) -> Result<()> {
        if reservation.is_none() {
            let snapshot = self.snapshot()?;
            let travel = snapshot.travel_for(ticket);
            if add(travel.exposure()?, 1)? > transition_limit {
                return Err(BudgetError::exhausted(
                    "travel_exhausted",
                    format!(
                        "ticket '{display_id}' has spent its travel bound \
                         ({} applied + {} held / {transition_limit})",
                        travel.consumed, travel.reserved
                    ),
                    Exhaustion {
                        dimension: Dimension::Travel,
                        subject: display_id.into(),
                        counter: travel,
                        bound: transition_limit,
                        contract: snapshot.contract,
                        day: snapshot.day,
                    },
                ));
            }
        }
        let mut payload = json!({
            "transition_receipt_id": format!("transition:{}", uuid::Uuid::new_v4()),
            "ticket_identity": ticket,
            "display_id": display_id,
            "from": from,
            "to": to,
        });
        if let Some(reservation) = reservation {
            payload["reservation_id"] = reservation.into();
        }
        self.append("transition", payload, audit)
    }

    /// Resolve the ancestor a nested or embedded caller claims, and return its
    /// declared descendant envelope.
    ///
    /// An ancestry token is a name, not a capability: the ledger is what says
    /// whether that reservation exists here, is still outstanding, and was
    /// admitted with room for descendants. A child that cannot be placed under
    /// a live ancestor of this very project refuses rather than opening a
    /// balance of its own. §AR-neural-admission.6
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
        if reservation.released {
            return Err(unavailable("the ancestor invocation has already been released"));
        }
        if !reservation.started {
            return Err(unavailable("the ancestor invocation has not started"));
        }
        let envelope = reservation.payload["descendant_envelope"]
            .as_u64()
            .ok_or_else(|| unavailable("it was admitted without a descendant envelope"))?;
        if envelope == 0 {
            return Err(unavailable("it permits no nested neural work"));
        }
        Ok(Some((parent.to_owned(), envelope)))
    }

    /// Release every reservation this account holds that no process could have
    /// started: no start record, and an owning run whose execution-root lock is
    /// free, which proves that run is gone.
    ///
    /// Recovery therefore needs no command and no new lock: it is one step
    /// inside an admission that already holds the account exclusively.
    /// §FS-rhei-budgets.6.2
    pub fn release_abandoned(
        &mut self,
        is_run_gone: &dyn Fn(&str) -> bool,
        audit: &Audit,
    ) -> Result<usize> {
        let abandoned: Vec<String> = self
            .state
            .reservations
            .iter()
            .filter(|(_, r)| !r.started && !r.released)
            .filter(|(_, r)| {
                r.payload["execution_root"].as_str().is_some_and(is_run_gone)
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in &abandoned {
            self.release_unstarted(id, audit)?;
        }
        Ok(abandoned.len())
    }

    /// The contract in force and the day it draws against, for a caller that
    /// needs to name the renewal instant. §FS-rhei-budgets.8
    pub fn renewal_instant(&self) -> Result<Option<String>> {
        match self.contract()? {
            Contract::Window => super::window::renewal_instant(self.day()).map(Some),
            Contract::Lifetime { .. } => Ok(None),
        }
    }
}
