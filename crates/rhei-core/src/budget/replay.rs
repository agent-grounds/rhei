//! Semantic replay: no receipt may erase exposure without matching its claim.
//!
//! No balance is stored. `consumed` and `outstanding` are derived by replaying
//! the whole chain under the active contract, which is why a window needs no
//! renewal event and therefore has nothing to forge, replay twice, or lose.
//! §AR-neural-admission.5 §FS-rhei-budgets.3.3

use super::journal::Receipt;
use super::types::{add, BudgetError, Contract, Counter, Snapshot, SpendBasis, SpendMarks};
use super::Result;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub(crate) struct Reservation {
    pub payload: Value,
    pub ticket: String,
    /// The UTC day this reservation was admitted in. It stays stamped:
    /// midnight neither enlarges it, revives it, nor moves its deadline.
    /// §FS-rhei-budgets.3.3
    pub window: String,
    pub started: bool,
    pub released: bool,
    /// A travel unit held but not yet applied to an edge.
    pub travel_held: bool,
    pub transition: Option<String>,
    /// The worst case this admission reserved against the day, in micro-units.
    /// Zero on a reservation written before the spend dimension existed, which
    /// is what makes such an account read unchanged. §FS-rhei-budgets.6.2
    pub spend_reserve: u64,
    /// The settle that replaced the reserve, with the receipt it came from so
    /// a replay of the same bytes is idempotent and a different one is
    /// corruption. §FS-rhei-budgets.5.2
    pub spend: Option<(u64, SpendBasis, Value)>,
}

/// Recomputed from the entire verified chain; no mutable balance cache.
/// §AR-neural-admission.5
#[derive(Clone, Default)]
pub(crate) struct State {
    pub contract: Option<Contract>,
    pub reservations: BTreeMap<String, Reservation>,
    /// Applied travel per ticket identity, over the whole life of the chain.
    pub travel: BTreeMap<String, u64>,
    pub identities: BTreeMap<String, Value>,
    pub attempts: BTreeSet<String>,
    pub transitions: BTreeMap<String, Value>,
    /// The highest day key the chain has ever recorded, which is the floor a
    /// clock moved backwards cannot get under. §FS-rhei-budgets.3.3
    pub highest_day: Option<String>,
    /// The account's one currency, fixed by the first receipt that carried an
    /// amount. §FS-rhei-budgets.5.5
    pub currency: Option<String>,
    /// Receipt kinds this build does not understand. One of them makes the
    /// account **read-only** rather than untrustworthy: it verified, it is
    /// simply newer than this binary, and a downgrade is owed the reading it
    /// was promised. §FS-rhei-budgets.5.2
    pub unknown_kinds: BTreeSet<String>,
    pub audit: Vec<Value>,
}

impl State {
    pub(crate) fn apply(&mut self, receipt: &Receipt) -> Result<()> {
        let p = &receipt.payload;
        if self.contract.is_none() && receipt.kind != "initialize" {
            return Err(BudgetError::corrupt("first receipt must establish the account"));
        }
        if let Some(window) = receipt.window.as_deref() {
            if self.highest_day.as_deref().is_none_or(|highest| window > highest) {
                self.highest_day = Some(window.to_string());
            }
        }
        match receipt.kind.as_str() {
            "initialize" => {
                if self.contract.is_some() {
                    return Err(BudgetError::corrupt("duplicate initialization"));
                }
                self.contract = Some(Contract::from_json(&p["contract"])?);
                self.audit.push(p.clone());
            }
            "identity" => {
                let ticket = string(p, "ticket_identity")?;
                string(p, "display_id")?;
                let binding = super::identity::replay_binding(self.identities.get(ticket), p)?;
                self.identities.insert(ticket.into(), binding);
            }
            "reserve" => {
                let arms = p
                    .get("reservations")
                    .and_then(Value::as_array)
                    .ok_or_else(|| BudgetError::corrupt("reservation receipt has no arms"))?;
                if arms.is_empty() {
                    return Err(BudgetError::corrupt("empty fanout reservation"));
                }
                for arm in arms {
                    self.reserve(arm, &receipt.project_id)?;
                }
            }
            "start" => {
                let r = self.reservation(p)?;
                if r.released || !matches!(string(p, "status")?, "confirmed" | "ambiguous") {
                    return Err(BudgetError::corrupt("invalid start receipt"));
                }
                r.started = true;
            }
            "release" => {
                let r = self.reservation(p)?;
                if p["travel_only"] == true {
                    // A poll wait or a completion that selects no edge gives
                    // the travel unit back; an *applied* edge never does.
                    // §FS-rhei-budgets.4.1
                    if r.transition.is_some() {
                        return Err(BudgetError::corrupt("applied travel cannot be released"));
                    }
                    r.travel_held = false;
                } else {
                    // The one lawful release: engine-side proof that no process
                    // could have started. §FS-rhei-budgets.6.2
                    if r.started || p["proof"] != "no_start_record" {
                        return Err(BudgetError::corrupt(
                            "a possible start cannot refund an invocation",
                        ));
                    }
                    r.released = true;
                    r.travel_held = false;
                }
            }
            "transition" => self.apply_transition(p)?,
            "adjust" => {
                let next = Contract::from_json(&p["new"])?;
                let current = self
                    .contract
                    .clone()
                    .ok_or_else(|| BudgetError::corrupt("adjustment before initialization"))?;
                if p["old"] != current.to_json() {
                    return Err(BudgetError::corrupt("adjustment names the wrong prior contract"));
                }
                if let Contract::Lifetime { allowance } = &next {
                    let spent = self.lifetime_counter()?.exposure()?;
                    if *allowance < spent {
                        return Err(BudgetError::corrupt(
                            "adjustment is below settled plus outstanding exposure",
                        ));
                    }
                }
                self.contract = Some(next);
                self.audit.push(p.clone());
            }
            // Money, and nothing else: it releases no travel unit and no
            // invocation unit, so both counts are what they were.
            // §FS-rhei-budgets.6.2 §REQ-bounded-neural-work.4
            "spend" => self.apply_spend(p)?,
            // Retained rather than refused: a build that predates a kind has
            // verified the chain and may read it, but not append to it.
            // §FS-rhei-budgets.5.2
            kind => {
                self.unknown_kinds.insert(kind.to_string());
            }
        }
        Ok(())
    }

    /// The one write in this ledger allowed to lower a number.
    ///
    /// It revises a reservation's reserved worst case to what the invocation
    /// actually cost — up or down — and touches neither count in either
    /// direction. At most one may name a reservation: a second with different
    /// content is corruption, and one naming a reservation the chain does not
    /// hold is corruption. §FS-rhei-budgets.5.2 §FS-rhei-budgets.6.2
    fn apply_spend(&mut self, p: &Value) -> Result<()> {
        let currency = string(p, "currency")?.to_string();
        let basis = SpendBasis::parse(string(p, "basis")?)
            .ok_or_else(|| BudgetError::corrupt("spend receipt names an unknown basis"))?;
        let amount = number(p, "amount_micro")?;
        self.fix_currency(&currency)?;
        let reservation = self.reservation(p)?;
        if let Some((_, _, previous)) = &reservation.spend {
            return if previous == p {
                Ok(())
            } else {
                Err(BudgetError::corrupt("conflicting spend settle for one reservation"))
            };
        }
        if reservation.released {
            return Err(BudgetError::corrupt("a released reservation spent nothing"));
        }
        reservation.spend = Some((amount, basis, p.clone()));
        Ok(())
    }

    /// One currency per account, fixed by the first receipt carrying an
    /// amount. A second is corruption here, because admission refuses it
    /// before any receipt is written: a chain that holds two was not written
    /// by this engine. §FS-rhei-budgets.5.5
    fn fix_currency(&mut self, currency: &str) -> Result<()> {
        match &self.currency {
            Some(held) if held != currency => Err(BudgetError::corrupt(format!(
                "the account is denominated in {held} and a receipt carries {currency}"
            ))),
            Some(_) => Ok(()),
            None => {
                self.currency = Some(currency.to_string());
                Ok(())
            }
        }
    }

    /// An applied edge. With a reservation it converts that reservation's held
    /// travel unit; without one it is a standalone charge — a person running
    /// `rhei transition`, or a callback redirect, which moved the ticket
    /// without an admission of its own. §FS-rhei-budgets.4.1
    fn apply_transition(&mut self, p: &Value) -> Result<()> {
        let id = string(p, "transition_receipt_id")?;
        if !id.starts_with("transition:") {
            return Err(BudgetError::corrupt("malformed transition receipt id"));
        }
        if let Some(previous) = self.transitions.get(id) {
            return if previous == p {
                Ok(())
            } else {
                Err(BudgetError::corrupt("conflicting transition replay"))
            };
        }
        let ticket = string(p, "ticket_identity")?.to_string();
        string(p, "from")?;
        string(p, "to")?;
        if let Some(reservation_id) = p.get("reservation_id").and_then(Value::as_str) {
            let r = self
                .reservations
                .get_mut(reservation_id)
                .ok_or_else(|| BudgetError::corrupt("transition names an absent reservation"))?;
            if !r.travel_held || r.released || r.transition.is_some() || r.ticket != ticket {
                return Err(BudgetError::corrupt("transition has no matching reserved travel"));
            }
            r.travel_held = false;
            r.transition = Some(id.into());
        }
        let count = self.travel.entry(ticket).or_default();
        *count = add(*count, 1)?;
        self.transitions.insert(id.into(), p.clone());
        Ok(())
    }

    fn reserve(&mut self, p: &Value, project: &str) -> Result<()> {
        let id = string(p, "reservation_id")?;
        let ticket = string(p, "ticket_identity")?;
        let expected = format!("ticket:{}:", project.trim_start_matches("panta:"));
        if !ticket.starts_with(&expected) {
            return Err(BudgetError::corrupt("reservation belongs to another project"));
        }
        super::types::uuid(&ticket[expected.len()..])?;
        self.authenticate_replayed_ancestor(p)?;
        if number(p, "invocation_units")? != 1
            || number(p, "travel_units")? > 1
            || self.reservations.contains_key(id)
            || !self.attempts.insert(string(p, "attempt_identity")?.into())
        {
            return Err(BudgetError::corrupt("invalid or duplicate reservation"));
        }
        let window = string(p, "window")?.to_string();
        // Absent on a reservation written before the spend dimension existed,
        // which is what lets an older account read unchanged: it reserved
        // nothing against a day nobody was charging. §FS-rhei-budgets.5.2
        let spend_reserve = p.get("spend_reserve_micro").and_then(Value::as_u64).unwrap_or(0);
        if spend_reserve > 0 {
            let currency = string(p, "spend_currency")?.to_string();
            self.fix_currency(&currency)?;
        }
        self.reservations.insert(
            id.into(),
            Reservation {
                payload: p.clone(),
                ticket: ticket.into(),
                window,
                started: false,
                released: false,
                travel_held: number(p, "travel_units")? == 1,
                transition: None,
                spend_reserve,
                spend: None,
            },
        );
        Ok(())
    }

    /// The day's spend, derived in the same pass and from the same receipts as
    /// the counts: a reservation's settle where it has one, its reserved worst
    /// case where it has none.
    ///
    /// No amount is stored, exactly as no count is, which is why lowering
    /// `spend_per_day` rewrites nothing. A released reservation contributes
    /// nothing at all: the one lawful release is proof that no process could
    /// have started, so there was no request to pay for.
    /// §FS-rhei-budgets.3.4 §AR-neural-admission.5
    fn spend(&self, day: &str) -> Result<(Counter, SpendMarks, Vec<String>)> {
        let mut counter = Counter::default();
        let mut marks = SpendMarks::default();
        let mut unsettled = Vec::new();
        for r in self.reservations.values() {
            if r.released || r.spend_reserve == 0 || r.window != day {
                continue;
            }
            match &r.spend {
                Some((amount, basis, _)) => {
                    counter.consumed = add(counter.consumed, *amount)?;
                    match basis {
                        SpendBasis::Measured => {}
                        SpendBasis::Unpriced => marks.unpriced = add(marks.unpriced, 1)?,
                        SpendBasis::Unmeasurable => {
                            marks.unmeasurable = add(marks.unmeasurable, 1)?
                        }
                    }
                }
                None => {
                    counter.reserved = add(counter.reserved, r.spend_reserve)?;
                    // Only a *started* reservation can be unsettled: one that
                    // never started is released whole by the step above this
                    // one, money and all. §FS-rhei-budgets.6.2
                    if r.started {
                        if let Some(root) = r.payload["execution_root"].as_str() {
                            unsettled.push(root.to_string());
                        }
                    }
                }
            }
        }
        Ok((counter, marks, unsettled))
    }

    /// Replay re-derives ancestry from the chain itself: a receipt naming a
    /// parent that is absent, finished, or without an envelope is corruption,
    /// not a fresh allowance. §AR-neural-admission.6
    fn authenticate_replayed_ancestor(&self, p: &Value) -> Result<()> {
        let Some(parent) = p.get("parent_reservation").and_then(Value::as_str) else {
            if p.get("parent_reservation").is_some_and(|v| !v.is_null()) {
                return Err(BudgetError::corrupt("ancestry must be a reservation id or null"));
            }
            return Ok(());
        };
        let ancestor = self
            .reservations
            .get(parent)
            .ok_or_else(|| BudgetError::corrupt("nested reservation names an unknown ancestor"))?;
        if !ancestor.started || ancestor.released {
            return Err(BudgetError::corrupt(
                "nested reservation names an ancestor that was not outstanding",
            ));
        }
        let envelope = ancestor.payload["descendant_envelope"].as_u64().unwrap_or(0);
        let drawn = self.descendants_drawn(parent)?;
        if envelope == 0 || add(drawn, 1)? > envelope {
            return Err(BudgetError::corrupt(
                "nested reservation exceeds its ancestor's descendant envelope",
            ));
        }
        Ok(())
    }

    /// What this ancestor's descendants already hold against its envelope, in
    /// invocation units. A released child gives its share back; a started one
    /// never does. §FS-rhei-budgets.7
    pub(crate) fn descendants_drawn(&self, parent: &str) -> Result<u64> {
        let mut drawn = 0;
        for reservation in self.reservations.values() {
            if reservation.payload["parent_reservation"].as_str() != Some(parent)
                || reservation.released
            {
                continue;
            }
            drawn = add(drawn, 1)?;
        }
        Ok(drawn)
    }

    pub(crate) fn reservation(&mut self, p: &Value) -> Result<&mut Reservation> {
        self.reservations
            .get_mut(string(p, "reservation_id")?)
            .ok_or_else(|| BudgetError::corrupt("receipt refers to an absent reservation"))
    }

    /// Every invocation the account has ever admitted, whatever the contract.
    pub(crate) fn lifetime_counter(&self) -> Result<Counter> {
        self.counter(None)
    }

    /// Invocations, optionally narrowed to one day key. §AR-neural-admission.5
    fn counter(&self, day: Option<&str>) -> Result<Counter> {
        let mut counter = Counter::default();
        for r in self.reservations.values() {
            if r.released || day.is_some_and(|day| r.window != day) {
                continue;
            }
            if r.started {
                counter.consumed = add(counter.consumed, 1)?;
            } else {
                counter.reserved = add(counter.reserved, 1)?;
            }
        }
        Ok(counter)
    }

    pub(crate) fn snapshot(&self, project: &str, day: &str) -> Result<Snapshot> {
        let contract = self
            .contract
            .clone()
            .ok_or_else(|| BudgetError::corrupt("missing account initialization"))?;
        let invocations = match contract {
            Contract::Window => self.counter(Some(day))?,
            Contract::Lifetime { .. } => self.lifetime_counter()?,
        };
        let mut travel = BTreeMap::<String, Counter>::new();
        for (ticket, applied) in &self.travel {
            travel.entry(ticket.clone()).or_default().consumed = *applied;
        }
        let (spend, spend_marks, spend_unsettled_roots) = self.spend(day)?;
        let mut reservations = BTreeMap::new();
        for (id, r) in &self.reservations {
            let mut row = r.payload.clone();
            row["started"] = r.started.into();
            row["released"] = r.released.into();
            reservations.insert(id.clone(), row);
            if r.released || !r.travel_held {
                continue;
            }
            let held = travel.entry(r.ticket.clone()).or_default();
            held.reserved = add(held.reserved, 1)?;
        }
        Ok(Snapshot {
            project_id: project.into(),
            contract,
            day: day.into(),
            invocations,
            lifetime_invocations: self.lifetime_counter()?,
            travel,
            spend,
            currency: self.currency.clone(),
            spend_marks,
            spend_unsettled_roots,
            reservations,
            health: "verified",
        })
    }
}

pub(crate) fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| BudgetError::corrupt(format!("missing string '{key}'")))
}

pub(crate) fn number(value: &Value, key: &str) -> Result<u64> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| BudgetError::corrupt(format!("missing integer '{key}'")))
}
