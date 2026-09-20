//! Forwarder and fixture finality use the same durable request ownership.
//! §AR-neural-admission.5 §FS-rhei-budgets.7

use super::{Audit, Broker, BudgetError, Forwarder, Journal, Result};
use std::sync::Arc;
use std::time::{Duration, Instant};

impl Broker {
    pub fn with_event_sink(mut self, sink: Arc<dyn super::BudgetEventSink>) -> Self {
        self.sink = Some(sink);
        self
    }

    pub(super) fn journal_with_sink(&self) -> Result<Journal> {
        let mut journal = Journal::open(&self.root, &self.project_uuid, true)?;
        if let Some(sink) = &self.sink {
            journal.set_event_sink(sink.clone())?;
        }
        Ok(journal)
    }

    pub(super) fn watchdog_millis(&self) -> u64 {
        self.qualification.watchdog_millis
    }

    #[cfg(feature = "budget-fixtures")]
    pub(super) fn is_fixture(&self) -> bool {
        self.qualification.tuple.provider == "rhei-test"
            && self.qualification.tuple.billing_contract == "synthetic-fixed"
            && self.qualification.provider_account == "deterministic-fixture"
    }

    /// Only a qualified adapter may give this server a credential. Starting the
    /// listener does not confer a process capability. §AR-neural-admission.1
    pub fn start_forwarder(
        self,
        api_key: String,
        teardown: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Forwarder> {
        if self.qualification.tuple.provider != "openai" {
            return Err(BudgetError::new("missing_qualification", "unsupported provider route"));
        }
        let deadline = self.forwarder_deadline()?;
        Forwarder::start(self, Some(api_key), deadline, teardown, false)
    }

    #[cfg(feature = "budget-fixtures")]
    pub fn start_fixture_forwarder(
        self,
        teardown: Arc<dyn Fn() + Send + Sync>,
        missing_usage: bool,
    ) -> Result<Forwarder> {
        if !self.is_fixture() {
            return Err(BudgetError::new(
                "missing_qualification",
                "fixture authority cannot forward real work",
            ));
        }
        let deadline = self.forwarder_deadline()?;
        Forwarder::start(self, None, deadline, teardown, missing_usage)
    }

    fn forwarder_deadline(&self) -> Result<Instant> {
        let journal = Journal::open(&self.root, &self.project_uuid, false)?;
        let r = journal
            .state
            .reservations
            .get(&self.reservation)
            .ok_or_else(|| BudgetError::corrupt("forwarder reservation absent"))?;
        let deadline = super::replay::number(&r.payload, "deadline_unix")?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(BudgetError::corrupt)?
            .as_secs();
        let remaining = deadline
            .checked_sub(now)
            .filter(|n| *n > 0)
            .ok_or_else(|| BudgetError::new("containment_stop", "invocation deadline reached"))?;
        Instant::now()
            .checked_add(Duration::from_secs(remaining))
            .ok_or_else(|| BudgetError::bounds("invocation deadline overflow"))
    }

    pub(super) fn record_containment(&self, reason: &str) -> Result<()> {
        let mut journal = self.journal_with_sink()?;
        let audit = Audit {
            actor: "provider-adapter".into(),
            written_at: time::OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .map_err(BudgetError::corrupt)?,
            reason: reason.into(),
            argv: vec![],
        };
        journal.append(
            "containment",
            serde_json::json!({
                "reservation_id": self.reservation, "reason": reason,
                "qualification": self.qualification.tuple,
            }),
            &audit,
        )
    }

    /// The synthetic provider has a fixed final charge for its exact response.
    /// Neither client output nor capture-file existence establishes that bill.
    /// §AR-neural-admission.8 §FS-rhei-budgets.7
    #[cfg(feature = "budget-fixtures")]
    pub fn fixture_final_bill(&self) -> Result<super::ProviderSettlement> {
        if !self.is_fixture() {
            return Err(BudgetError::new("evidence_refused", "not a synthetic provider"));
        }
        let journal = Journal::open(&self.root, &self.project_uuid, false)?;
        let (rows, _, evidence) = self.read_capture(&journal)?;
        let requests = journal
            .state
            .broker_requests
            .iter()
            .filter(|(_, r)| r["reservation_id"] == self.reservation)
            .collect::<Vec<_>>();
        let captures = rows.iter().filter(|r| r["kind"] == "captured").collect::<Vec<_>>();
        if requests.is_empty()
            || requests.len() != captures.len()
            || !rows.last().is_some_and(|r| r["kind"] == "captured")
        {
            return Err(BudgetError::new("evidence_refused", "synthetic finality is incomplete"));
        }
        for (id, _) in &requests {
            if !captures.iter().any(|r| {
                r["request_id"] == **id
                    && r["provider_response"]["id"] == **id
                    && r["provider_response"]["usage"]["input_tokens"] == 1000
                    && r["provider_response"]["usage"]["output_tokens"] == 1000
            }) {
                return Err(BudgetError::new(
                    "evidence_refused",
                    "synthetic final bill has no matching capture",
                ));
            }
        }
        let charge = (requests.len() as u64)
            .checked_mul(2000)
            .ok_or_else(|| BudgetError::bounds("synthetic charge overflow"))?;
        Ok(super::ProviderSettlement::verified(
            journal.project_id.clone(),
            self.reservation.clone(),
            charge,
            evidence,
        ))
    }
}
