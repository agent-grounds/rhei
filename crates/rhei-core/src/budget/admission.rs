//! `preview`, `reserve`, `record_start`, the two releases, the travel charge,
//! and ancestry authentication.
//!
//! One account held exclusively is what serializes competing processes,
//! parallel arms, and nested runtimes: two racing `rhei run` processes cannot
//! both take the last unit.
//! §FS-rhei-budgets.6 §AR-neural-admission.1

use super::ancestry::{Ancestry, AncestryDescriptor};
use super::journal::{Audit, Journal};
use super::types::{add, BudgetError, Contract, Dimension, Exhaustion, SpendBasis};
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
    /// Measured spend the project may be charged this UTC day, in micro-units
    /// of its account's currency. §FS-rhei-budgets.3.4
    pub spend_per_day: u64,
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
    pub parent_reservation: Option<AncestryDescriptor<'a>>,
    /// Whether this admission holds a travel unit for the edge the visit is
    /// expected to apply. A spawn that cannot move the ticket holds none.
    pub travel: bool,
    /// The worst case each arm may spend, reserved against the day and
    /// replaced by the settle that says what it cost. §FS-rhei-budgets.6.2
    pub spend_reserve_micro: u64,
    /// The currency the composed price book will report this spend in, which
    /// is what fixes an account's own on its first amount.
    /// §FS-rhei-budgets.5.5
    pub spend_currency: &'a str,
}

/// One applied edge, as the shared transition path describes it.
///
/// A struct rather than six positional arguments because `from` and `to` are
/// both `&str` and both name a state: swapped, they would compile and lie.
/// §FS-rhei-budgets.4.1
pub struct AppliedEdge<'a> {
    pub ticket: &'a str,
    pub display_id: &'a str,
    pub from: &'a str,
    pub to: &'a str,
    /// The travel unit admission already holds for this edge, where the move
    /// came from a spawn this run admitted.
    pub reservation: Option<&'a str>,
    pub transition_limit: u64,
}

/// A durable group: one travel unit for the ticket and one invocation unit per
/// arm, all-or-none. §FS-rhei-budgets.4.2
#[derive(Clone, Debug)]
pub struct ReservationGroup {
    pub reservation_ids: Vec<String>,
    /// The arm that carries the ticket's travel unit, where one was taken.
    pub travel_reservation_id: Option<String>,
    /// What became of the ancestry the request claimed, so that a caller can
    /// see a downgrade it did not ask for and say so. §FS-rhei-budgets.7.2
    pub ancestry: Ancestry,
}

impl Journal {
    /// Inspection and mutation use the same checks; `preview` cannot debit, so
    /// `rhei validate` and `--dry-run` reach exactly this code.
    ///
    /// It answers with the ancestry it authenticated rather than with nothing,
    /// so an inspection reports the downgrade of §FS-rhei-budgets.7.2 with the
    /// bounds — still taking no mutating lock, appending no receipt and
    /// debiting nothing. §FS-rhei-budgets.6.3
    pub fn preview(
        &self,
        request: &AdmissionRequest<'_>,
        bounds: EffectiveBounds,
    ) -> Result<Ancestry> {
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
        let ancestry = self.authenticate_ancestor(request)?;
        let snapshot = self.snapshot()?;
        // Before any bound, because this is not a bound: an account
        // denominated in one currency cannot be charged in another, and
        // nothing here converts. §FS-rhei-budgets.5.5
        if request.spend_reserve_micro > 0 {
            if let Some(held) = snapshot.currency.as_deref() {
                if held != request.spend_currency {
                    return Err(BudgetError::new(
                        "currency_conflict",
                        format!(
                            "project '{}' keeps its account in {held} and this run prices in \
                             {}; nothing here converts one to the other, so the run is refused \
                             before any agent starts",
                            request.project_label, request.spend_currency
                        ),
                    ));
                }
            }
        }
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
                        currency: None,
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
                    currency: None,
                },
            ));
        }
        // Third and last, so that a spawn standing at two bounds at once is
        // refused on the count and reports the text it already reported.
        // §FS-rhei-budgets.6.1
        let reserving = request.spend_reserve_micro.checked_mul(count).ok_or_else(|| {
            BudgetError::corrupt("the worst case for this fanout overflows the ledger")
        })?;
        if add(snapshot.spend.exposure()?, reserving)? > bounds.spend_per_day {
            let mut counter = snapshot.spend;
            // What reaches a spend ceiling is consumed plus the *next*
            // reserve, so the refused request's own worst case is in the
            // number a reader adds up. §FS-rhei-budgets.8
            counter.reserved = add(counter.reserved, reserving)?;
            return Err(BudgetError::exhausted(
                "spend_exhausted",
                format!(
                    "project '{}' has spent today's measured budget ({} consumed + {} \
                     outstanding / {})",
                    request.project_label,
                    crate::money::format_micro(counter.consumed, snapshot.currency.as_deref()),
                    crate::money::format_micro(counter.reserved, snapshot.currency.as_deref()),
                    crate::money::format_micro(bounds.spend_per_day, snapshot.currency.as_deref()),
                ),
                Exhaustion {
                    dimension: Dimension::Spend,
                    subject: request.project_label.into(),
                    counter,
                    bound: bounds.spend_per_day,
                    contract: snapshot.contract.clone(),
                    day: snapshot.day.clone(),
                    currency: Some(
                        snapshot
                            .currency
                            .clone()
                            .unwrap_or_else(|| request.spend_currency.to_string()),
                    ),
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
        // Only an ancestry this journal actually placed the child under draws
        // on an envelope: one minted elsewhere bounds nothing here, because the
        // child was never inside it. §FS-rhei-budgets.7.2
        if let Ancestry::Placed { reservation: parent, envelope } = &ancestry {
            let drawn = add(self.state.descendants_drawn(parent)?, count)?;
            if drawn > *envelope {
                return Err(BudgetError::new(
                    "ancestor_envelope_exhausted",
                    format!(
                        "ancestor {parent} permits {envelope} descendant invocations; \
                         {drawn} is claimed"
                    ),
                ));
            }
        }
        Ok(ancestry)
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
        let ancestry = self.preview(request, bounds)?;
        let window = self.day().to_string();
        let mut ids = Vec::new();
        let mut reservations = Vec::new();
        for (index, arm) in request.arms.iter().enumerate() {
            let id = format!("reservation:{}", uuid::Uuid::new_v4());
            // The fanout rule of §FS-rhei-budgets.4.1 as arithmetic rather than
            // as a special case: one applied edge, however many arms.
            let travel_units = u64::from(request.travel && index == 0);
            let mut arm_payload = json!({
                "reservation_id": id,
                "attempt_identity": arm.attempt_identity,
                "ticket_identity": request.ticket_identity,
                "display_id": request.display_id,
                // What the identity test and the ledger decided, not what the
                // caller asked for. §FS-rhei-budgets.7.2
                "parent_reservation": ancestry.parent(),
                "descendant_envelope": arm.descendant_envelope,
                "execution_root": request.execution_root,
                "invocation_units": 1,
                "travel_units": travel_units,
                "window": window,
            });
            // Riding the existing kind, because one transaction decides both
            // at one instant and a build that predates them ignores payload
            // keys it does not know. §FS-rhei-budgets.5.2
            if request.spend_reserve_micro > 0 {
                arm_payload["spend_reserve_micro"] = request.spend_reserve_micro.into();
                arm_payload["spend_currency"] = request.spend_currency.into();
            }
            reservations.push(arm_payload);
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
            ancestry,
        })
    }

    /// Replace one reservation's reserved worst case with what the
    /// invocation actually cost.
    ///
    /// The only write in this ledger allowed to lower a number, and it lowers
    /// exactly one: it releases no travel unit and no invocation unit, so
    /// every count is what it was. A basis other than `measured` charges the
    /// reserve, because an amount nobody could measure is not an amount of
    /// nothing. §FS-rhei-budgets.6.2 §REQ-bounded-neural-work.4
    pub fn settle_spend(
        &mut self,
        reservation: &str,
        amount_micro: u64,
        currency: &str,
        basis: SpendBasis,
        audit: &Audit,
    ) -> Result<()> {
        self.append(
            "spend",
            json!({
                "reservation_id": reservation,
                "amount_micro": amount_micro,
                "currency": currency,
                "basis": basis.as_str(),
            }),
            audit,
        )
    }

    /// Record ambiguity *before* capability transfer. A crash after this point
    /// can never refund the invocation; confirmation only refines it.
    /// §FS-rhei-budgets.6.2
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
    pub fn charge_travel(&mut self, edge: &AppliedEdge<'_>, audit: &Audit) -> Result<()> {
        let AppliedEdge { ticket, display_id, from, to, reservation, transition_limit } = *edge;
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
                        currency: None,
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
    /// Two questions, in this order, and the order is the whole of it: whose
    /// descriptor this is, and then what the journal holds. A descriptor minted
    /// for another account names **no ancestor here** — nothing was unavailable,
    /// because there was no ancestry to authenticate — so the caller is admitted
    /// unparented against its own ledger rather than refused for forgery. For a
    /// descriptor of this very account an ancestry token is a name and not a
    /// capability: the ledger is what says whether that reservation exists here,
    /// is still outstanding, and was admitted with room for descendants, and a
    /// child that cannot be placed under a live one refuses rather than opening
    /// a balance of its own. §FS-rhei-budgets.7.1 §AR-neural-admission.6
    fn authenticate_ancestor(&self, request: &AdmissionRequest<'_>) -> Result<Ancestry> {
        let Some(descriptor) = request.parent_reservation else {
            return Ok(Ancestry::Unclaimed);
        };
        let parent = descriptor.reservation;
        // A descriptor that names no account at all is taken as this project's,
        // so a caller too old to say whose it is loses nothing.
        // §FS-rhei-budgets.7.1
        if let Some(account) = descriptor.account.filter(|a| *a != self.account_uuid()) {
            return Ok(Ancestry::Elsewhere {
                reservation: parent.to_owned(),
                account: account.to_owned(),
            });
        }
        // Provenance is the caller's to supply and this layer's only to
        // interpolate: nothing here learns that any caller reads an
        // environment. §FS-rhei-budgets.7.1 §AR-neural-admission.6
        let origin = descriptor.origin.map(|origin| format!(" {origin}")).unwrap_or_default();
        let unavailable = |why: &str| {
            BudgetError::new(
                "ancestor_unavailable",
                format!("nested admission cannot use ancestor {parent}{origin}: {why}"),
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
        Ok(Ancestry::Placed { reservation: parent.to_owned(), envelope })
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
            .filter(|(_, r)| r.payload["execution_root"].as_str().is_some_and(is_run_gone))
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
