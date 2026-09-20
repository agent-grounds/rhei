//! Restricted client HTTP adapter. Admission and OS confinement are separate
//! capabilities; opening a listener alone never authorizes process creation.
//! §FS-rhei-budgets.6.4 §AR-neural-admission.5

use super::{Audit, Broker, BudgetError, Result};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const BODY_LIMIT: u64 = 4 * 1024 * 1024;
const HEADER_LIMIT: usize = 16 * 1024;

enum Upstream {
    Api {
        key: String,
    },
    #[cfg(feature = "budget-fixtures")]
    Fixture {
        missing_usage: bool,
    },
}

/// An invocation-scoped authenticated listener with an autonomous watchdog.
/// The owner must supply teardown before clients can use it. §FS-rhei-budgets.6.4
pub struct Forwarder {
    endpoint: String,
    token: String,
    stopped: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    watchdog: Option<JoinHandle<()>>,
}

impl Forwarder {
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
    pub fn token(&self) -> &str {
        &self.token
    }

    pub(crate) fn start(
        broker: Broker,
        api_key: Option<String>,
        deadline: Instant,
        teardown: Arc<dyn Fn() + Send + Sync>,
        fixture_missing_usage: bool,
    ) -> Result<Self> {
        #[cfg(not(feature = "budget-fixtures"))]
        let _ = fixture_missing_usage;
        let upstream = match api_key {
            Some(key) if !key.is_empty() && !key.contains(['\r', '\n']) => Upstream::Api { key },
            #[cfg(feature = "budget-fixtures")]
            None if broker.is_fixture() => {
                Upstream::Fixture { missing_usage: fixture_missing_usage }
            }
            _ => {
                return Err(BudgetError::new(
                    "missing_qualification",
                    "broker credential unavailable",
                ))
            }
        };
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let endpoint = format!("http://{}/v1/responses", listener.local_addr()?);
        let token = uuid::Uuid::new_v4().to_string();
        let stopped = Arc::new(AtomicBool::new(false));
        let active = Arc::new(Mutex::new(None::<Instant>));
        let watchdog_stop = stopped.clone();
        let watchdog_active = active.clone();
        let watchdog_teardown = teardown.clone();
        let capture_bound = Duration::from_millis(broker.watchdog_millis());
        let watchdog_broker = broker.clone();
        // No journal or HTTP lock is needed to stop the process tree.
        // §AR-neural-admission.5
        let watchdog = std::thread::spawn(move || {
            while !watchdog_stop.load(Ordering::Acquire) {
                let expired = Instant::now() >= deadline
                    || watchdog_active
                        .lock()
                        .expect("watchdog state")
                        .is_some_and(|at| at.elapsed() >= capture_bound);
                if expired {
                    watchdog_stop.store(true, Ordering::Release);
                    watchdog_teardown();
                    let _ = watchdog_broker
                        .record_containment("autonomous deadline/capture watchdog expired");
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        });
        let worker_stop = stopped.clone();
        let worker_token = token.clone();
        let worker = std::thread::spawn(move || {
            while !worker_stop.load(Ordering::Acquire) {
                let (mut stream, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(_) => break,
                };
                *active.lock().expect("watchdog state") = Some(Instant::now());
                let result = serve(
                    &broker,
                    &upstream,
                    &mut stream,
                    &worker_token,
                    capture_bound.min(deadline.saturating_duration_since(Instant::now())),
                    &worker_stop,
                );
                *active.lock().expect("watchdog state") = None;
                if let Err(error) = result {
                    worker_stop.store(true, Ordering::Release);
                    teardown();
                    let _ = broker.record_containment(&error.to_string());
                    let _ = stream.write_all(
                        b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    );
                }
            }
        });
        Ok(Self { endpoint, token, stopped, worker: Some(worker), watchdog: Some(watchdog) })
    }
}

impl Drop for Forwarder {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        // Provider and socket I/O each have a finite timeout. No unfinished
        // worker can accept a new request after this flag. §FS-rhei-budgets.6.4
        if let Some(thread) = self.worker.take() {
            let _ = thread.join();
        }
        if let Some(thread) = self.watchdog.take() {
            let _ = thread.join();
        }
    }
}

fn serve(
    broker: &Broker,
    upstream: &Upstream,
    stream: &mut TcpStream,
    token: &str,
    timeout: Duration,
    stopped: &AtomicBool,
) -> Result<()> {
    if timeout.is_zero() {
        return Err(stop("invocation deadline reached"));
    }
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    let body = read_request(stream, token, Instant::now() + timeout, stopped)?;
    if stopped.load(Ordering::Acquire) {
        return Err(stop("watchdog stopped admission"));
    }
    let request: serde_json::Value = serde_json::from_slice(&body)?;
    let audit = audit("forward qualified provider request");
    let permit = broker.prepare(&request, &audit)?;
    let (status, content_type, response) = match upstream {
        Upstream::Api { key } => {
            let agent = ureq::AgentBuilder::new()
                .try_proxy_from_env(false)
                .redirects(0)
                .timeout(timeout)
                .build();
            // Fixed origin; never forward a client URL, proxy or provider key.
            // §FS-rhei-budgets.6.4
            let response = agent
                .post("https://api.openai.com/v1/responses")
                .set("Authorization", &format!("Bearer {key}"))
                .set("Content-Type", "application/json")
                .set("Idempotency-Key", permit.request_id())
                .send_bytes(&body)
                .map_err(|e| stop(&format!("provider transport failed: {e}")))?;
            let status = response.status();
            let content_type = response.header("Content-Type").unwrap_or("").to_string();
            let mut bytes = Vec::new();
            response.into_reader().take(BODY_LIMIT + 1).read_to_end(&mut bytes)?;
            if bytes.len() as u64 > BODY_LIMIT {
                return Err(stop("provider response exceeds capture bound"));
            }
            (status, content_type, bytes)
        }
        #[cfg(feature = "budget-fixtures")]
        Upstream::Fixture { missing_usage } => {
            let mut response = serde_json::json!({
                "id": permit.request_id(), "object": "response", "status": "completed",
                "model": request["model"], "output": [],
                "usage": { "input_tokens": 1000, "output_tokens": 1000 },
            });
            if *missing_usage {
                response.as_object_mut().expect("fixture response").remove("usage");
            }
            (200, "application/json".into(), serde_json::to_vec(&response)?)
        }
    };
    if stopped.load(Ordering::Acquire) {
        return Err(stop("watchdog stopped provider capture"));
    }
    broker.capture_http_response(permit, status, &content_type, &response, &audit)?;
    write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", response.len())?;
    stream.write_all(&response)?;
    stream.flush()?;
    Ok(())
}

fn read_request(
    stream: &mut TcpStream,
    token: &str,
    deadline: Instant,
    stopped: &AtomicBool,
) -> Result<Vec<u8>> {
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        read_bounded(stream, &mut byte, deadline, stopped)?;
        header.push(byte[0]);
        if header.len() > HEADER_LIMIT {
            return Err(stop("request header exceeds bound"));
        }
    }
    let header = std::str::from_utf8(&header).map_err(|_| stop("invalid HTTP header"))?;
    let mut lines = header.split("\r\n");
    if lines.next() != Some("POST /v1/responses HTTP/1.1") {
        return Err(stop("only the pinned Responses route is allowed"));
    }
    let mut fields = std::collections::BTreeMap::new();
    for line in lines.filter(|s| !s.is_empty()) {
        let (name, value) = line.split_once(':').ok_or_else(|| stop("invalid HTTP field"))?;
        if fields.insert(name.to_ascii_lowercase(), value.trim()).is_some() {
            return Err(stop("duplicate HTTP field"));
        }
    }
    if fields.get("authorization") != Some(&format!("Bearer {token}").as_str())
        || fields.contains_key("transfer-encoding")
        || fields.contains_key("proxy-authorization")
        || fields.get("content-type") != Some(&"application/json")
    {
        return Err(stop("unauthenticated or unsupported broker request"));
    }
    let length: u64 = fields
        .get("content-length")
        .ok_or_else(|| stop("missing body length"))?
        .parse()
        .map_err(|_| stop("invalid body length"))?;
    if length == 0 || length > BODY_LIMIT {
        return Err(stop("request body exceeds bound"));
    }
    let mut body = vec![0; length as usize];
    read_bounded(stream, &mut body, deadline, stopped)?;
    Ok(body)
}

fn read_bounded(
    stream: &mut TcpStream,
    mut buffer: &mut [u8],
    deadline: Instant,
    stopped: &AtomicBool,
) -> Result<()> {
    while !buffer.is_empty() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() || stopped.load(Ordering::Acquire) {
            return Err(stop("HTTP read deadline reached"));
        }
        stream.set_read_timeout(Some(remaining))?;
        let read = stream.read(buffer)?;
        if read == 0 {
            return Err(stop("incomplete HTTP request"));
        }
        buffer = &mut buffer[read..];
    }
    Ok(())
}

fn stop(message: &str) -> BudgetError {
    BudgetError::new("containment_stop", message)
}
fn audit(reason: &str) -> Audit {
    Audit {
        actor: "qualified-provider-adapter".into(),
        written_at: time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .expect("UTC formats"),
        reason: reason.into(),
        argv: vec![],
    }
}
