// Durable accounting record types shared by writers, readers, and inspection.

const ACCOUNTING_INVOCATION_SCHEMA: &str = "rhei.accounting.invocation.v1";
const ACCOUNTING_USAGE_EVENT_SCHEMA: &str = "rhei.accounting.usage.v1";
static ACCOUNTING_INVOCATION_FILE_SEQUENCE: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct AccountingInvocationRecord {
    schema: String,
    invocation_id: String,
    /// The one invocation of `rhei run` that spawned this process.
    ///
    /// Optional, and the schema string does not move with it: a record written
    /// before the field existed still parses, and is *unattributed* — never
    /// dropped, never folded into a named run.
    // §FS-rhei-cost-accounting.3.5
    #[serde(default, skip_serializing_if = "Option::is_none")]
    run_id: Option<String>,
    task_id: String,
    state: String,
    visit: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    target_slug: Option<String>,
    agent: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    started_at: String,
    ended_at: String,
    // Optional on read for records written before this field was published.
    // §FS-rhei-cost-accounting.3.4
    #[serde(default, skip_serializing_if = "Option::is_none")]
    duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cli_session: Option<AccountingCliSession>,
    extraction_status: String,
    /// Distinct structured-extraction failures, in first-seen order.
    ///
    /// Empty only for historical records and non-failure statuses; current
    /// failed writers always persist at least one reason.
    // §FS-rhei-cost-accounting.3 §FS-rhei-cost-accounting.3.2
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    extraction_diagnostics: Vec<String>,
    scope: String,
    /// Which convention this record's token dimensions follow.
    ///
    /// Optional, and the schema string does not move with it, exactly as
    /// `run_id` is. A record written before the field existed still parses,
    /// and is read under the convention its own `agent` implies rather than at
    /// face value.
    // §FS-rhei-cost-accounting.3.6
    #[serde(default, skip_serializing_if = "Option::is_none")]
    token_convention: Option<String>,
    tokens: AccountingTokens,
    pricing: AccountingPricing,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
struct AccountingCliSession {
    id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    store_path: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct AccountingTokens {
    #[serde(default = "unknown_token_dimension")]
    total: AccountingTokenDimension,
    input: AccountingTokenSide,
    output: AccountingTokenSide,
}

impl Default for AccountingTokens {
    fn default() -> Self {
        Self {
            total: AccountingTokenDimension::unavailable("unknown"),
            input: AccountingTokenSide::default(),
            output: AccountingTokenSide::default(),
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct AccountingTokenSide {
    total: AccountingTokenDimension,
    cached_read: AccountingTokenDimension,
    cache_write: AccountingTokenDimension,
}

impl Default for AccountingTokenSide {
    fn default() -> Self {
        Self {
            total: AccountingTokenDimension::unavailable("unknown"),
            cached_read: AccountingTokenDimension::unavailable("unsupported"),
            cache_write: AccountingTokenDimension::unavailable("unsupported"),
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct AccountingTokenDimension {
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>,
}

impl AccountingTokenDimension {
    fn measured(value: u64) -> Self {
        Self::measured_from(value, "agent-usage-capture")
    }

    fn measured_from(value: u64, source: &str) -> Self {
        Self {
            value: Some(value),
            source: Some(source.to_string()),
            status: None,
        }
    }

    fn unavailable(status: &str) -> Self {
        Self { value: None, source: None, status: Some(status.to_string()) }
    }
}

fn unknown_token_dimension() -> AccountingTokenDimension {
    AccountingTokenDimension::unavailable("unknown")
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct AccountingPricing {
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    currency: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    amount_micro: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    priced_amount_micro: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    price_book_id: Option<String>,
}

/// The wall-clock milliseconds between a record's two endpoints.
///
/// `None` when either timestamp is missing or unparseable, and `None` when the
/// two do not form a forward interval — an `ended_at` before its `started_at`
/// has no non-negative elapsed time, and a duration is not worth guessing.
/// Every surface that reports an invocation's elapsed time derives it here when
/// the record carries no `duration_ms` of its own, so the two readers of this
/// archive cannot drift apart again.
/// §FS-rhei-cost-accounting.3.4.1
fn invocation_elapsed_ms(record: &AccountingInvocationRecord) -> Option<u64> {
    let started = parse_rfc3339_utc(&record.started_at)?;
    let ended = parse_rfc3339_utc(&record.ended_at)?;
    u64::try_from(ended.duration_since(started).ok()?.as_millis()).ok()
}

/// One invocation as a reading publishes it: the record as stored, and the
/// elapsed time between its endpoints where the record itself carries none.
///
/// The derivation happens here, on the way out, and never on the record that
/// was parsed — a reading recomputes what it reports and never rewrites what
/// it read (§FS-rhei-cost-accounting.5.1), and nothing on disk is touched. A
/// record that carries its own `duration_ms` is published with that number
/// untouched: it was measured in milliseconds while the agent ran, while the
/// endpoints are RFC 3339 to the second, so recomputing over it would report a
/// 31 ms invocation as zero. Reporting nothing is left for the cases where
/// nothing is known — an endpoint missing or unparseable, or endpoints that do
/// not form a forward interval.
/// §FS-rhei-cost-accounting.3.4.1
fn published_invocation_json(record: &AccountingInvocationRecord) -> serde_json::Value {
    let mut published = serde_json::json!(record);
    let derived = if record.duration_ms.is_none() { invocation_elapsed_ms(record) } else { None };
    if let (Some(object), Some(elapsed)) = (published.as_object_mut(), derived) {
        object.insert("duration_ms".to_string(), serde_json::json!(elapsed));
    }
    published
}

#[cfg(test)]
fn write_invocation_record(
    accounting_root: &Path,
    record: &AccountingInvocationRecord,
) -> MietteResult<PathBuf> {
    write_invocation_record_value(accounting_root, record, serde_json::json!(record))
}

/// Add selected-profile provenance to new records without making the field
/// mandatory when old v1 records are decoded. §FS-rhei-cost-accounting.3
fn write_invocation_record_with_profile(
    accounting_root: &Path,
    record: &AccountingInvocationRecord,
    model_profile: Option<&str>,
) -> MietteResult<PathBuf> {
    let mut value = serde_json::json!(record);
    if let (Some(object), Some(model_profile)) = (value.as_object_mut(), model_profile) {
        object.insert("model_profile".to_string(), serde_json::json!(model_profile));
    }
    write_invocation_record_value(accounting_root, record, value)
}

fn write_invocation_record_value(
    accounting_root: &Path,
    record: &AccountingInvocationRecord,
    value: serde_json::Value,
) -> MietteResult<PathBuf> {
    // §FS-rhei-cost-accounting.2: File names use path-safe file ids.
    let dir = accounting_root.join("invocations");
    fs::create_dir_all(&dir)
        .map_err(|err| file_io_report(&dir, "failed to create accounting invocation directory", err))?;
    let file_id = invocation_file_id(record);
    let path = dir.join(format!("{file_id}.json"));
    write_json_atomic(&path, &value)?;
    Ok(path)
}
