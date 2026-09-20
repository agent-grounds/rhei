//! Semantic replay: no receipt may erase exposure without matching its claim.
//! §FS-rhei-budgets.3.3 §FS-rhei-budgets.7

use super::types::add;
use super::{Allowance, BudgetError, Counter, Money, Receipt, Result, Snapshot};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub(crate) struct Reservation {
    pub payload: Value,
    pub ticket: String,
    pub fwc: u64,
    pub started: bool,
    pub released: bool,
    pub settled: Option<u64>,
    pub travel: bool,
    pub transition: Option<String>,
}

/// Recomputed from the entire verified chain; no mutable balance cache.
/// §FS-rhei-budgets.3.1
#[derive(Clone, Default)]
pub(crate) struct State {
    pub allowance: Option<Allowance>,
    pub historical_invocations: u64,
    pub historical_charge_micro: u64,
    pub reservations: BTreeMap<String, Reservation>,
    pub travel: BTreeMap<String, u64>,
    pub travel_limits: BTreeMap<String, u64>,
    pub identities: BTreeMap<String, Value>,
    pub attempts: BTreeSet<String>,
    pub invocation_ends: BTreeSet<String>,
    pub transitions: BTreeMap<String, Value>,
    pub broker_requests: BTreeMap<String, Value>,
    pub breached: bool,
    pub audit: Vec<Value>,
}

impl State {
    pub(crate) fn apply(&mut self, receipt: &Receipt) -> Result<()> {
        let p = &receipt.payload;
        if self.allowance.is_none() && receipt.kind != "initialize" {
            return Err(BudgetError::corrupt("first receipt must initialize the allowance"));
        }
        match receipt.kind.as_str() {
            "initialize" => {
                if self.allowance.is_some() {
                    return Err(BudgetError::corrupt("duplicate initialization"));
                }
                let allowance: Allowance = serde_json::from_value(p["allowance"].clone())?;
                allowance.validate()?;
                if p["history"] == json!({"status": "complete", "records": 0}) {
                    // Fresh, explicitly empty lifetime.
                } else if p["history"]["status"] == "imported"
                    && string(&p["history"], "evidence_hash")?.starts_with("sha256:")
                    && p["history"]["records"].is_array()
                {
                    self.historical_invocations = number(&p["history"], "invocations")?;
                    self.historical_charge_micro = number(&p["history"], "charge_micro")?;
                    if self.historical_invocations
                        != p["history"]["records"]
                            .as_array()
                            .expect("checked array")
                            .iter()
                            .map(|r| string(r, "invocation_id"))
                            .collect::<Result<BTreeSet<_>>>()?
                            .len() as u64
                    {
                        return Err(BudgetError::corrupt(
                            "historical import count does not match its records",
                        ));
                    }
                } else {
                    return Err(BudgetError::corrupt("invalid historical import receipt"));
                }
                self.allowance = Some(allowance);
                self.audit.push(p.clone());
            }
            "identity" => {
                let ticket = string(p, "ticket_identity")?;
                string(p, "display_id")?;
                let binding = super::identity::replay_binding(self.identities.get(ticket), p)?;
                self.identities.insert(ticket.into(), binding);
            }
            "reserve" => {
                if self.breached {
                    return Err(BudgetError::corrupt("reservation after a breach"));
                }
                if let Some(arms) = p.get("reservations").and_then(Value::as_array) {
                    if arms.is_empty() {
                        return Err(BudgetError::corrupt("empty fanout reservation"));
                    }
                    for arm in arms {
                        self.reserve(arm, &receipt.project_id)?;
                    }
                } else {
                    self.reserve(p, &receipt.project_id)?;
                }
            }
            "start" => {
                let r = self.reservation(p)?;
                if r.released || !matches!(string(p, "status")?, "confirmed" | "ambiguous") {
                    return Err(BudgetError::corrupt("invalid start receipt"));
                }
                r.started = true;
            }
            "invocation_end" => {
                let r = self.reservation(p)?;
                if !r.started || r.released {
                    return Err(BudgetError::corrupt("invocation end without a possible start"));
                }
                self.invocation_ends.insert(string(p, "reservation_id")?.into());
            }
            "containment" => {
                let r = self.reservation(p)?;
                if !r.started || r.released {
                    return Err(BudgetError::corrupt("containment has no active invocation"));
                }
                string(p, "reason")?;
            }
            "broker_request" => {
                let id = string(p, "request_id")?;
                let r = self.reservation(p)?;
                if !r.started
                    || r.released
                    || r.settled.is_some()
                    || p["attempt_identity"] != r.payload["attempt_identity"]
                    || !id.starts_with("request:")
                {
                    return Err(BudgetError::corrupt("broker request has no active reservation"));
                }
                if self.broker_requests.insert(id.into(), p.clone()).is_some() {
                    return Err(BudgetError::corrupt("duplicate committed broker request"));
                }
            }
            "release" => {
                let r = self.reservation(p)?;
                if p["travel_only"] == true {
                    if r.transition.is_some() {
                        return Err(BudgetError::corrupt("applied travel cannot be released"));
                    }
                    r.travel = false;
                } else {
                    if r.started || p["proof"] != "no_capability_transferred" {
                        return Err(BudgetError::corrupt(
                            "a possible start cannot refund invocation or money",
                        ));
                    }
                    r.released = true;
                    r.travel = false;
                }
            }
            "settle" | "reconcile" => {
                let charge = number(p, "charge_micro")?;
                let evidence = string(p, "evidence_hash")?;
                if !evidence.starts_with("sha256:") {
                    return Err(BudgetError::corrupt("settlement evidence has no hash"));
                }
                let r = self.reservation(p)?;
                if r.released || !r.started || r.settled.is_some_and(|old| old != charge) {
                    return Err(BudgetError::corrupt("conflicting settlement identity or charge"));
                }
                if charge > r.fwc {
                    return Err(BudgetError::corrupt("an excess charge requires a breach receipt"));
                }
                r.settled = Some(charge);
            }
            "transition" => {
                let id = string(p, "transition_receipt_id")?;
                if !id.starts_with("transition:") || p["central_receipt_id"] != id {
                    return Err(BudgetError::corrupt("unmatched central transition receipt"));
                }
                if let Some(previous) = self.transitions.get(id) {
                    return if previous == p {
                        Ok(())
                    } else {
                        Err(BudgetError::corrupt("conflicting transition replay"))
                    };
                }
                string(p, "from")?;
                string(p, "to")?;
                let r = self.reservation(p)?;
                if !r.started
                    || !r.travel
                    || r.transition.is_some()
                    || p["ticket_identity"] != r.ticket
                {
                    return Err(BudgetError::corrupt("transition has no matching reserved travel"));
                }
                r.travel = false;
                r.transition = Some(id.into());
                let ticket = r.ticket.clone();
                let count = self.travel.entry(ticket).or_default();
                *count = add(*count, 1)?;
                self.transitions.insert(id.into(), p.clone());
            }
            "adjust" => {
                let next: Allowance = serde_json::from_value(p["new"].clone())?;
                next.validate()?;
                let snapshot = self.snapshot(&receipt.project_id)?;
                if p["old"] != serde_json::to_value(&snapshot.allowance)?
                    || next.spend.currency != snapshot.allowance.spend.currency
                    || next.invocations
                        < add(snapshot.consumed.invocations, snapshot.reserved.invocations)?
                    || next.spend.amount_micro
                        < add(
                            snapshot.consumed.spend.amount_micro,
                            snapshot.reserved.spend.amount_micro,
                        )?
                {
                    return Err(BudgetError::corrupt(
                        "adjustment is below settled plus outstanding exposure",
                    ));
                }
                self.allowance = Some(next);
                self.audit.push(p.clone());
            }
            "breach" => {
                let actual = number(p, "actual_charge_micro")?;
                let r = self.reservation(p)?;
                if actual <= r.fwc || !r.started || r.released {
                    return Err(BudgetError::corrupt("invalid breach evidence"));
                }
                r.settled = Some(actual.max(r.settled.unwrap_or(0)));
                self.breached = true;
            }
            // A recovery may carry evidence, but cannot change any balance.
            // §FS-rhei-budgets.8
            "recover" => {
                self.audit.push(p.clone());
            }
            kind => {
                return Err(BudgetError::corrupt(format!(
                    "unsupported receipt kind '{kind}'; this build cannot mutate it"
                )))
            }
        }
        let snapshot = self.snapshot(&receipt.project_id)?;
        if !self.breached
            && (add(snapshot.consumed.invocations, snapshot.reserved.invocations)?
                > snapshot.allowance.invocations
                || add(snapshot.consumed.spend.amount_micro, snapshot.reserved.spend.amount_micro)?
                    > snapshot.allowance.spend.amount_micro)
        {
            return Err(BudgetError::corrupt("receipt exceeds the allowance"));
        }
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
        // Replay re-derives ancestry from the chain itself: a receipt naming a
        // parent that is absent, finished, or without an envelope is corrupt,
        // not a fresh allowance. §FS-rhei-budgets.5 §AR-neural-admission.7
        if let Some(parent) = p.get("parent_reservation").and_then(Value::as_str) {
            let ancestor = self.reservations.get(parent).ok_or_else(|| {
                BudgetError::corrupt("nested reservation names an unknown ancestor")
            })?;
            if !ancestor.started || ancestor.released {
                return Err(BudgetError::corrupt(
                    "nested reservation names an ancestor that was not outstanding",
                ));
            }
            let envelope = ancestor.payload["descendant_envelope_micro"].as_u64().unwrap_or(0);
            let drawn = self
                .reservations
                .values()
                .filter(|row| {
                    row.payload["parent_reservation"].as_str() == Some(parent) && !row.released
                })
                .try_fold(0u64, |total, row| add(total, row.fwc))?;
            if envelope == 0 || add(drawn, number(p, "fwc_micro")?)? > envelope {
                return Err(BudgetError::corrupt(
                    "nested reservation exceeds its ancestor's descendant envelope",
                ));
            }
        } else if p.get("parent_reservation").is_some_and(|v| !v.is_null()) {
            return Err(BudgetError::corrupt("ancestry must be a reservation id or null"));
        }
        let fwc = number(p, "fwc_micro")?;
        let threshold = number(p, "threshold_micro")?;
        if threshold == 0
            || fwc != add(threshold, number(p, "residual_micro")?)?
            || number(p, "invocation_units")? != 1
            || number(p, "travel_units")? > 1
            || self.reservations.contains_key(id)
            || !self.attempts.insert(string(p, "attempt_identity")?.into())
        {
            return Err(BudgetError::corrupt("invalid or duplicate reservation"));
        }
        if !p["qualification"].is_object() && p["qualification"].as_str().is_none_or(str::is_empty)
        {
            return Err(BudgetError::corrupt("qualification tuple is absent"));
        }
        string(p, "qualification_evidence_hash")?;
        if let Some(limit) = p["transition_limit"].as_u64() {
            self.travel_limits.insert(ticket.into(), limit);
        }
        self.reservations.insert(
            id.into(),
            Reservation {
                payload: p.clone(),
                ticket: ticket.into(),
                fwc,
                started: false,
                released: false,
                settled: None,
                travel: number(p, "travel_units")? == 1,
                transition: None,
            },
        );
        Ok(())
    }

    pub(crate) fn reservation(&mut self, p: &Value) -> Result<&mut Reservation> {
        self.reservations
            .get_mut(string(p, "reservation_id")?)
            .ok_or_else(|| BudgetError::corrupt("receipt refers to an absent reservation"))
    }

    pub(crate) fn snapshot(&self, project: &str) -> Result<Snapshot> {
        let allowance =
            self.allowance.clone().ok_or_else(|| BudgetError::corrupt("missing initialization"))?;
        let (mut inv, mut spend) = (
            Counter { consumed: self.historical_invocations, reserved: 0 },
            Counter { consumed: self.historical_charge_micro, reserved: 0 },
        );
        let mut travel = BTreeMap::<String, Counter>::new();
        let mut reservations = BTreeMap::new();
        let travel_limits = self.travel_limits.clone();
        for (ticket, applied) in &self.travel {
            travel.entry(ticket.clone()).or_default().consumed = *applied;
        }
        for (id, r) in &self.reservations {
            let mut row = r.payload.clone();
            row["started"] = r.started.into();
            row["released"] = r.released.into();
            row["settled_charge_micro"] = r.settled.into();
            row["containment_state"] = if r.released {
                "released_before_start"
            } else if r.settled.is_some() {
                "settled"
            } else if r.started {
                "exposure_retained"
            } else {
                "reserved"
            }
            .into();
            reservations.insert(id.clone(), row);
            if r.released {
                continue;
            }
            if r.started {
                inv.consumed = add(inv.consumed, 1)?;
            } else {
                inv.reserved = add(inv.reserved, 1)?;
            }
            if let Some(charge) = r.settled {
                spend.consumed = add(spend.consumed, charge)?;
            } else {
                spend.reserved = add(spend.reserved, r.fwc)?;
            }
            if r.travel {
                let t = travel.entry(r.ticket.clone()).or_default();
                t.reserved = add(t.reserved, 1)?;
            }
        }
        let units = |invocations, amount_micro| Allowance {
            invocations,
            spend: Money { currency: allowance.spend.currency.clone(), amount_micro },
        };
        // Breach overage is retained in consumed. Remaining is zero, never a
        // wrapped credit; ordinary arithmetic above remains checked.
        // §FS-rhei-budgets.9
        let remaining = units(
            allowance.invocations.saturating_sub(inv.exposure()?),
            allowance.spend.amount_micro.saturating_sub(spend.exposure()?),
        );
        let travel_remaining = travel_limits
            .iter()
            .map(|(ticket, limit)| {
                let exposure = travel.get(ticket).map(Counter::exposure).transpose()?.unwrap_or(0);
                Ok((ticket.clone(), limit.saturating_sub(exposure)))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
        Ok(Snapshot {
            ledger_health: "verified".into(),
            project_id: project.into(),
            allowance: allowance.clone(),
            consumed: units(inv.consumed, spend.consumed),
            reserved: units(inv.reserved, spend.reserved),
            remaining,
            travel,
            travel_limits,
            travel_remaining,
            reservations,
            breached: self.breached,
            audit: self.audit.clone(),
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
