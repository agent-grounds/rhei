//! Durable one-request barrier. Network adapters may forward only a returned
//! permit, and may not retry a request whose outcome is unknown.
//! §AR-neural-admission.5 §FS-rhei-budgets.6.4

use super::journal::{digest, sync_directory};
use super::replay::{number, string};
use super::types::add;
use super::{Audit, BudgetError, Journal, QualifiedLaunch, Result};
use serde_json::{json, Value};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;

/// The financial barrier is separate from HTTP and OS confinement. No
/// network credential or process capability is issued by this type.
/// §AR-neural-admission.5
#[derive(Clone)]
pub struct Broker {
    pub(super) sink: Option<std::sync::Arc<dyn super::BudgetEventSink>>,
    pub(super) root: PathBuf,
    pub(super) project_uuid: String,
    pub(super) reservation: String,
    pub(super) qualification: QualifiedLaunch,
}

/// A single committed request, intentionally neither Clone nor Deserialize.
/// Dropping it leaves the complete request exposure outstanding.
/// §FS-rhei-budgets.6.3
pub struct RequestPermit {
    request_id: String,
    reservation: String,
    maximum_micro: u64,
    prepared_at: std::time::Instant,
    watchdog_millis: u64,
}

impl RequestPermit {
    pub fn request_id(&self) -> &str {
        &self.request_id
    }
}

impl Broker {
    /// Reopening a broker never clears the previous request's uncertainty.
    /// §FS-rhei-budgets.7
    pub fn open(
        root: PathBuf,
        project_uuid: String,
        reservation: String,
        qualification: QualifiedLaunch,
    ) -> Result<Self> {
        let journal = Journal::open(&root, &project_uuid, false)?;
        let r = journal
            .state
            .reservations
            .get(&reservation)
            .ok_or_else(|| BudgetError::corrupt("broker reservation is absent"))?;
        if !r.started
            || r.released
            || r.settled.is_some()
            || r.payload["qualification_evidence_hash"] != qualification.evidence_hash
        {
            return Err(BudgetError::corrupt(
                "broker does not own an active qualified reservation",
            ));
        }
        Ok(Self { root, project_uuid, reservation, qualification, sink: None })
    }

    /// Reserve the complete request charge durably before forwarding bytes.
    /// The provider maximum, not a local tokenizer, bounds input exposure.
    /// §FS-rhei-budgets.6.2 §FS-rhei-budgets.6.4
    pub fn prepare(&self, body: &Value, audit: &Audit) -> Result<RequestPermit> {
        audit.validate()?;
        let mut journal = self.journal_with_sink()?;
        let r = journal
            .state
            .reservations
            .get(&self.reservation)
            .ok_or_else(|| BudgetError::corrupt("broker reservation is absent"))?;
        if r.settled.is_some() || r.released || journal.state.breached {
            return Err(BudgetError::new("containment_stop", "broker invocation is closed"));
        }
        self.qualification.check_validity(number(&r.payload, "deadline_unix")?)?;
        self.validate_request(body)?;
        let (mut rows, _, bytes) = self.read_capture(&journal)?;
        if rows.last().is_some_and(|v| v["kind"] == "committed") {
            return Err(BudgetError::new(
                "containment_stop",
                "unsettled provider request retains its complete maximum; retry refused",
            ));
        }
        let spent = captured_charge(&rows)?;
        if spent >= number(&r.payload, "threshold_micro")? {
            return Err(BudgetError::new("containment_stop", "live provider threshold reached"));
        }
        let maximum = price(
            self.qualification.maximum_input_tokens,
            self.qualification.maximum_output_tokens,
            &self.qualification,
        )?;
        if add(spent, maximum)? > r.fwc {
            return Err(BudgetError::new(
                "missing_qualification",
                "request maximum does not fit the qualified residual",
            ));
        }
        let request_id = format!("request:{}", uuid::Uuid::new_v4());
        let row = json!({"kind": "committed", "request_id": request_id,
            "request_hash": digest(&serde_json::to_vec(body)?), "maximum_micro": maximum,
            "audit": audit});
        let attempt = r.payload["attempt_identity"].clone();
        self.append_capture(&journal, &mut rows, &bytes, row)?;
        // The witnessed receipt, not caller evidence or a mutable capture file,
        // establishes ownership before forwarding. §FS-rhei-budgets.8.1
        journal.append(
            "broker_request",
            json!({
                "reservation_id": self.reservation, "request_id": request_id,
                "attempt_identity": attempt, "request_hash": digest(&serde_json::to_vec(body)?),
                "committed_at": audit.written_at,
            }),
            audit,
        )?;
        Ok(RequestPermit {
            request_id,
            reservation: self.reservation.clone(),
            maximum_micro: maximum,
            prepared_at: std::time::Instant::now(),
            watchdog_millis: self.qualification.watchdog_millis,
        })
    }

    /// Normalize the two client-compatible Responses transports into the one
    /// authenticated final response consumed by the durable capture barrier.
    /// Missing completion, provider error events, trailing events, and a late
    /// watchdog all retain the committed maximum. §FS-rhei-budgets.6.4
    pub fn capture_http_response(
        &self,
        permit: RequestPermit,
        status: u16,
        content_type: &str,
        body: &[u8],
        audit: &Audit,
    ) -> Result<()> {
        if permit.prepared_at.elapsed().as_millis() > u128::from(permit.watchdog_millis) {
            return Err(BudgetError::new(
                "containment_stop",
                "provider capture watchdog expired; full request exposure retained",
            ));
        }
        if status != 200 {
            return Err(BudgetError::new(
                "containment_stop",
                format!("provider returned HTTP {status}; committed exposure retained"),
            ));
        }
        let normalized = if content_type
            .split(';')
            .next()
            .is_some_and(|value| value.trim().eq_ignore_ascii_case("text/event-stream"))
        {
            normalize_responses_sse(body)?
        } else if content_type
            .split(';')
            .next()
            .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
        {
            body.to_vec()
        } else {
            return Err(BudgetError::new(
                "containment_stop",
                "provider response has an unqualified content type",
            ));
        };
        self.capture(permit, &normalized, audit)
    }

    /// Capture only a response received by the qualified provider adapter.
    /// Client stdout is not provider evidence. This method deliberately does
    /// not settle the invocation from a price ceiling: it retains FWC until
    /// the adapter establishes the final provider charge for every request.
    /// §FS-rhei-budgets.7
    pub fn capture(&self, permit: RequestPermit, response: &[u8], audit: &Audit) -> Result<()> {
        audit.validate()?;
        let journal = Journal::open(&self.root, &self.project_uuid, true)?;
        if permit.reservation != self.reservation {
            return Err(BudgetError::corrupt("request permit belongs to another reservation"));
        }
        let (mut rows, _, bytes) = self.read_capture(&journal)?;
        let pending =
            rows.last().ok_or_else(|| BudgetError::corrupt("request was not committed"))?;
        if pending["kind"] != "committed" || pending["request_id"] != permit.request_id {
            return Err(BudgetError::corrupt("request capture identity conflict"));
        }
        let body: Value = serde_json::from_slice(response)?;
        if body["object"] != "response"
            || body["status"] != "completed"
            || body["model"] != self.qualification.tuple.model
        {
            return Err(BudgetError::new(
                "containment_stop",
                "provider response is not complete for the qualified model",
            ));
        }
        let usage = &body["usage"];
        let input = number(usage, "input_tokens")?;
        let output = number(usage, "output_tokens")?;
        if input > self.qualification.maximum_input_tokens
            || output > self.qualification.maximum_output_tokens
        {
            return Err(BudgetError::new("breach", "provider response exceeds qualified token dimensions; retain full exposure and requalify"));
        }
        let charge_bound = price(input, output, &self.qualification)?;
        if charge_bound > permit.maximum_micro {
            return Err(BudgetError::new(
                "breach",
                "captured response exceeds reserved request maximum",
            ));
        }
        // Save the complete response before acknowledging the barrier. A
        // missing final price still retains invocation FWC at settlement.
        // §FS-rhei-budgets.6.3
        let row = json!({"kind": "captured", "request_id": permit.request_id,
            "charge_bound_micro": charge_bound, "provider_response": body,
            "response_hash": digest(response), "audit": audit});
        self.append_capture(&journal, &mut rows, &bytes, row)
    }

    fn validate_request(&self, request: &Value) -> Result<()> {
        let fields = request
            .as_object()
            .ok_or_else(|| BudgetError::bounds("broker expects a Responses request object"))?;
        // Closed initial subset: no tools, hidden conversation, server-side
        // compaction, background work, subscription route, or fallback.
        // §FS-rhei-budgets.6.4
        const ALLOWED: &[&str] = &[
            "model",
            "input",
            "instructions",
            "max_output_tokens",
            "stream",
            "store",
            "background",
            "service_tier",
            "reasoning",
        ];
        if fields.keys().any(|key| !ALLOWED.contains(&key.as_str()))
            || request["model"] != self.qualification.tuple.model
            || request["background"] != false
            || request["store"] != false
            || request["service_tier"] != "default"
        {
            return Err(BudgetError::new(
                "containment_stop",
                "request is outside the qualified text-only Responses subset",
            ));
        }
        let output = number(request, "max_output_tokens")?;
        if output == 0 || output > self.qualification.maximum_output_tokens {
            return Err(BudgetError::bounds("request exceeds the qualified output maximum"));
        }
        if !request["input"].is_string()
            || fields.get("instructions").is_some_and(|v| !v.is_string())
        {
            return Err(BudgetError::new(
                "containment_stop",
                "only stateless text input is supported by this broker subset",
            ));
        }
        Ok(())
    }

    fn capture_path(&self, journal: &Journal) -> PathBuf {
        journal
            .path()
            .parent()
            .expect("journal has a parent")
            .join("broker")
            .join(format!("{}.jsonl", self.reservation.trim_start_matches("reservation:")))
    }

    pub(super) fn read_capture(
        &self,
        journal: &Journal,
    ) -> Result<(Vec<Value>, Option<String>, Vec<u8>)> {
        let path = self.capture_path(journal);
        let mut bytes = Vec::new();
        match File::open(&path) {
            Ok(mut file) => {
                file.read_to_end(&mut bytes)?;
            }
            Err(e) => return Err(e.into()),
        }
        if bytes.is_empty() || !bytes.ends_with(b"\n") {
            return Err(BudgetError::corrupt("incomplete broker capture"));
        }
        let mut rows = Vec::new();
        let mut previous = None;
        for line in bytes[..bytes.len() - 1].split(|b| *b == b'\n') {
            let row: Value = serde_json::from_slice(line)?;
            if row["schema"] != "rhei.broker.request.v1"
                || row["reservation_id"] != self.reservation
                || row["previous_hash"] != serde_json::to_value(&previous)?
                || number(&row, "sequence")? != add(rows.len() as u64, 1)?
            {
                return Err(BudgetError::corrupt("invalid broker capture chain"));
            }
            match string(&row, "kind")? {
                "initialize" if rows.is_empty() => {}
                "committed"
                    if rows.last().is_some_and(|p: &Value| {
                        p["kind"] == "captured" || p["kind"] == "initialize"
                    }) => {}
                "captured"
                    if rows.last().is_some_and(|p| {
                        p["kind"] == "committed" && p["request_id"] == row["request_id"]
                    }) => {}
                _ => return Err(BudgetError::corrupt("broker request barrier was bypassed")),
            }
            previous = Some(digest(line));
            rows.push(row);
        }
        Ok((rows, previous, bytes))
    }

    fn append_capture(
        &self,
        journal: &Journal,
        rows: &mut Vec<Value>,
        previous: &[u8],
        mut row: Value,
    ) -> Result<()> {
        let path = self.capture_path(journal);
        let dir = path.parent().expect("capture has a parent");
        std::fs::create_dir_all(dir)?;
        row["schema"] = "rhei.broker.request.v1".into();
        row["reservation_id"] = self.reservation.clone().into();
        row["sequence"] = add(rows.len() as u64, 1)?.into();
        row["previous_hash"] = if previous.is_empty() {
            Value::Null
        } else {
            digest(
                previous[..previous.len() - 1]
                    .rsplit(|b| *b == b'\n')
                    .next()
                    .expect("nonempty capture"),
            )
            .into()
        };
        let mut bytes = serde_json::to_vec(&row)?;
        bytes.push(b'\n');
        let mut options = OpenOptions::new();
        options.write(true).append(true);
        if previous.is_empty() {
            options.create_new(true);
        }
        let mut file = options.open(&path)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        sync_directory(dir)?;
        rows.push(row);
        Ok(())
    }
}

/// Closed SSE grammar for the restricted Responses subset. The terminal
/// provider object, not client stdout, is the billing capture.
/// §FS-rhei-budgets.6.4 §AR-neural-admission.5
fn normalize_responses_sse(body: &[u8]) -> Result<Vec<u8>> {
    let text = std::str::from_utf8(body)
        .map_err(|_| BudgetError::new("containment_stop", "provider SSE is not UTF-8"))?;
    let mut completed = None;
    let mut saw_done = false;
    for frame in text.replace("\r\n", "\n").split("\n\n") {
        let data = frame
            .lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .map(str::trim_start)
            .collect::<Vec<_>>()
            .join("\n");
        if data.is_empty() {
            continue;
        }
        if data == "[DONE]" {
            saw_done = true;
            continue;
        }
        if completed.is_some() || saw_done {
            return Err(BudgetError::new(
                "containment_stop",
                "provider SSE contains events after terminal completion",
            ));
        }
        let event: Value = serde_json::from_str(&data)?;
        match event["type"].as_str() {
            Some("response.completed") => {
                let response = event["response"].clone();
                if !response.is_object() {
                    return Err(BudgetError::new(
                        "containment_stop",
                        "response.completed has no provider response object",
                    ));
                }
                completed = Some(serde_json::to_vec(&response)?);
            }
            Some("error" | "response.failed" | "response.incomplete") => {
                return Err(BudgetError::new(
                    "containment_stop",
                    "provider SSE ended without a complete billable response",
                ));
            }
            Some(_) => {}
            None => {
                return Err(BudgetError::new("containment_stop", "provider SSE event has no type"));
            }
        }
    }
    if !saw_done {
        return Err(BudgetError::new(
            "containment_stop",
            "provider SSE ended before the terminal barrier",
        ));
    }
    completed.ok_or_else(|| {
        BudgetError::new("containment_stop", "provider SSE has no response.completed event")
    })
}

/// Prepared under the project lock as part of reservation, before start.
/// §AR-neural-admission.4 §AR-neural-admission.5
pub(crate) fn initialize_capture(
    journal: &Journal,
    reservation: &str,
    audit: &Audit,
) -> Result<()> {
    let id = reservation
        .strip_prefix("reservation:")
        .ok_or_else(|| BudgetError::corrupt("invalid broker reservation identity"))?;
    super::types::uuid(id)?;
    let dir = journal.path().parent().expect("journal has a parent").join("broker");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{id}.jsonl"));
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    let row = json!({"schema": "rhei.broker.request.v1", "reservation_id": reservation,
        "sequence": 1, "previous_hash": null, "kind": "initialize", "audit": audit});
    let mut bytes = serde_json::to_vec(&row)?;
    bytes.push(b'\n');
    file.write_all(&bytes)?;
    file.sync_all()?;
    sync_directory(&dir)?;
    sync_directory(dir.parent().expect("capture directory has a parent"))
}

/// Ceiling pricing is used for containment, never asserted to be the exact
/// provider bill. Checked wide arithmetic rounds in the costly direction.
/// §FS-rhei-budgets.6.2
fn price(input: u64, output: u64, q: &QualifiedLaunch) -> Result<u64> {
    super::qualification_bundle::price_ceiling(
        input,
        output,
        q.input_micro_per_million,
        q.output_micro_per_million,
    )
}

fn captured_charge(rows: &[Value]) -> Result<u64> {
    rows.iter()
        .filter(|r| r["kind"] == "captured")
        .try_fold(0, |total, row| add(total, number(row, "charge_bound_micro")?))
}
