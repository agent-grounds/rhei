//! The usage an invocation reported and the run's rollup of it, in the shape
//! every surface shares: the run summary, the stream, and the report.
//! §FS-rhei-cost-accounting.6 §FS-rhei-run-json.2.1

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DimensionStatus {
    Measured,
    Partial,
    Unsupported,
    Omitted,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DimensionSummary {
    pub value: Option<u64>,
    pub status: DimensionStatus,
    pub missing_count: u64,
    pub measured_count: u64,
}

impl Default for DimensionSummary {
    fn default() -> Self {
        Self { value: None, status: DimensionStatus::Unknown, missing_count: 0, measured_count: 0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UsageCoverage {
    Complete,
    Partial,
    Unpriced,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UsageStatus {
    Measured,
    UnsupportedAgent,
    ExtractorUnavailable,
    ExtractorFailed,
    NoUsageEmitted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PricingStatus {
    Priced,
    PartialPrice,
    Unpriced,
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct UsageSummary {
    pub invocation_id: String,
    pub state: String,
    pub agent: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub total: DimensionSummary,
    pub input_total: DimensionSummary,
    pub input_cached_read: DimensionSummary,
    pub input_cache_write: DimensionSummary,
    pub output_total: DimensionSummary,
    pub output_cached_read: DimensionSummary,
    pub output_cache_write: DimensionSummary,
    pub cost_micro: Option<u64>,
    pub priced_cost_micro: Option<u64>,
    pub currency: Option<String>,
    pub coverage: UsageCoverage,
    pub status: UsageStatus,
    pub pricing_status: PricingStatus,
}

/// Which of an invocation's usage reports an event carries.
///
/// One invocation emits several: one `Streamed` running total per turn a
/// streaming extractor measures, then one `Final` after its durable record is
/// written. A frontend that keys a view by invocation id upserts both; one that
/// appends lines prints the `Final` only, because a written line cannot be
/// revised into the next running total. §FS-rhei-cost-accounting.7.1
///
/// An enum rather than a `bool` because `final` is a Rust keyword and because
/// the JSON record contract names both values (§FS-rhei-run-json.2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UsageReport {
    /// A running total for an invocation still in progress, re-summed from its
    /// whole capture stream, so it reads low until the last turn is in.
    Streamed,
    /// The one report that follows the invocation's durable record and carries
    /// what that record says it cost.
    Final,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AccountingRunSummary {
    pub total: DimensionSummary,
    pub input_total: DimensionSummary,
    pub input_cached_read: DimensionSummary,
    pub input_cache_write: DimensionSummary,
    pub output_total: DimensionSummary,
    pub output_cached_read: DimensionSummary,
    pub output_cache_write: DimensionSummary,
    pub cost_micro: Option<u64>,
    pub priced_cost_micro: Option<u64>,
    pub currency: Option<String>,
    pub coverage: UsageCoverage,
    pub pricing_status: PricingStatus,
    pub invocation_count: u64,
    pub measured_invocation_count: u64,
    pub missing_invocation_count: u64,
    /// Agent spawns in the selection that no accounting record answers for.
    /// Counted beside `invocation_count`, which keeps counting records, and
    /// defaulted so a summary serialized before the field existed still reads.
    /// §FS-rhei-cost-accounting.6.2.1
    #[serde(default)]
    pub unrecorded_agent_invocation_count: u64,
}

/// Summarize invocation usage records into the shared run/task accounting shape.
/// §FS-rhei-cost-accounting.6: rollups summarize invocation records consistently.
pub fn summarize_usage_summaries<'a>(
    usages: impl IntoIterator<Item = &'a UsageSummary>,
) -> Option<AccountingRunSummary> {
    let usages: Vec<&UsageSummary> = usages.into_iter().collect();
    if usages.is_empty() {
        return None;
    }

    let measured_invocation_count =
        usages.iter().filter(|usage| usage.status == UsageStatus::Measured).count() as u64;
    let missing_invocation_count = usages.len() as u64 - measured_invocation_count;
    let priced_cost_micro = sum_options(usages.iter().map(|usage| usage.priced_cost_micro));
    let cost_micro = if usages.iter().all(|usage| usage.cost_micro.is_some()) {
        Some(usages.iter().filter_map(|usage| usage.cost_micro).sum())
    } else {
        None
    };
    let currency = usages.iter().find_map(|usage| usage.currency.clone());
    let pricing_status = summarize_pricing_status(&usages);
    let coverage = summarize_coverage(&usages, cost_micro, priced_cost_micro);

    Some(AccountingRunSummary {
        total: summarize_dimension(usages.iter().map(|usage| &usage.total)),
        input_total: summarize_dimension(usages.iter().map(|usage| &usage.input_total)),
        input_cached_read: summarize_dimension(usages.iter().map(|usage| &usage.input_cached_read)),
        input_cache_write: summarize_dimension(usages.iter().map(|usage| &usage.input_cache_write)),
        output_total: summarize_dimension(usages.iter().map(|usage| &usage.output_total)),
        output_cached_read: summarize_dimension(
            usages.iter().map(|usage| &usage.output_cached_read),
        ),
        output_cache_write: summarize_dimension(
            usages.iter().map(|usage| &usage.output_cache_write),
        ),
        cost_micro,
        priced_cost_micro,
        currency,
        coverage,
        pricing_status,
        invocation_count: usages.len() as u64,
        measured_invocation_count,
        missing_invocation_count,
        // Records alone cannot see a spawn that left none; the reading that
        // holds the spawn records adds them. §FS-rhei-cost-accounting.6.2.1
        unrecorded_agent_invocation_count: 0,
    })
}

fn summarize_dimension<'a>(
    dimensions: impl IntoIterator<Item = &'a DimensionSummary>,
) -> DimensionSummary {
    let mut value = 0u64;
    let mut saw_value = false;
    let mut missing_count = 0u64;
    let mut measured_count = 0u64;
    let mut unavailable_status = None;

    for dimension in dimensions {
        if let Some(v) = dimension.value {
            value = value.saturating_add(v);
            saw_value = true;
        }
        measured_count = measured_count.saturating_add(dimension.measured_count);
        missing_count = missing_count.saturating_add(dimension.missing_count);
        if dimension.status != DimensionStatus::Measured {
            unavailable_status = Some(dimension.status);
        }
    }

    let status = if saw_value && missing_count == 0 {
        DimensionStatus::Measured
    } else if saw_value {
        DimensionStatus::Partial
    } else {
        unavailable_status.unwrap_or(DimensionStatus::Unknown)
    };

    DimensionSummary { value: saw_value.then_some(value), status, missing_count, measured_count }
}

fn sum_options(values: impl IntoIterator<Item = Option<u64>>) -> Option<u64> {
    let mut total = 0u64;
    let mut saw = false;
    for value in values.into_iter().flatten() {
        total = total.saturating_add(value);
        saw = true;
    }
    saw.then_some(total)
}

fn summarize_pricing_status(usages: &[&UsageSummary]) -> PricingStatus {
    let mut saw_priced = false;
    let mut saw_partial = false;
    let mut saw_unpriced = false;
    let mut saw_applicable = false;
    for usage in usages {
        match usage.pricing_status {
            PricingStatus::Priced => {
                saw_priced = true;
                saw_applicable = true;
            }
            PricingStatus::PartialPrice => {
                saw_partial = true;
                saw_applicable = true;
            }
            PricingStatus::Unpriced => {
                saw_unpriced = true;
                saw_applicable = true;
            }
            PricingStatus::NotApplicable => {}
        }
    }
    if !saw_applicable {
        PricingStatus::NotApplicable
    } else if saw_partial || (saw_priced && saw_unpriced) {
        PricingStatus::PartialPrice
    } else if saw_priced {
        PricingStatus::Priced
    } else {
        PricingStatus::Unpriced
    }
}

fn summarize_coverage(
    usages: &[&UsageSummary],
    cost_micro: Option<u64>,
    priced_cost_micro: Option<u64>,
) -> UsageCoverage {
    if usages.iter().all(|usage| usage.coverage == UsageCoverage::None) {
        return UsageCoverage::None;
    }
    if usages.iter().any(|usage| {
        matches!(usage.coverage, UsageCoverage::Partial) || usage.status != UsageStatus::Measured
    }) {
        return UsageCoverage::Partial;
    }
    if cost_micro.is_some() {
        UsageCoverage::Complete
    } else if priced_cost_micro.is_some() {
        UsageCoverage::Partial
    } else if usages.iter().any(|usage| usage.coverage == UsageCoverage::Unpriced) {
        UsageCoverage::Unpriced
    } else {
        UsageCoverage::None
    }
}
