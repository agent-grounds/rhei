use crate::rhei_tui::event::{EventSink, MessageLevel, RunEvent, UsageReport, UsageSummary};

/// Frontend sink for non-TTY output, and so what a headless run's
/// `runtime/run.log` is written through.
///
/// The engine still drives most human-readable stdout via direct `println!`
/// calls (so that byte-for-byte output is preserved). This sink therefore
/// reacts only to the few events that have no other path to a reader:
/// `Message` events, terminal errors re-emitted through the channel, run links,
/// and accounting.
///
/// It appends lines rather than keying a view, so it is the one line-oriented
/// frontend: it prints one accounting line per invocation, on the `Final`
/// report only, and holds no per-invocation state to decide that with.
/// §FS-rhei-cost-accounting.7.1 §FS-rhei-run-tui.1.3
pub struct StdoutSink;

impl StdoutSink {
    pub fn new() -> Self {
        Self
    }
}

impl Default for StdoutSink {
    fn default() -> Self {
        Self::new()
    }
}

impl EventSink for StdoutSink {
    fn emit(&self, event: RunEvent) {
        if let RunEvent::Budget { event } = event {
            println!("{}", event.message());
        } else if let RunEvent::Message { level, text } = event {
            match level {
                MessageLevel::Info => println!("{text}"),
                MessageLevel::Warn | MessageLevel::Error => eprintln!("{text}"),
            }
        } else if let RunEvent::RunLink { label, url } = event {
            println!("{label}: {url}");
        } else if let RunEvent::UsageReported { task, report, usage, .. } = event {
            if let Some(line) = accounting_line(&task, report, &usage) {
                println!("{line}");
            }
        }
    }
}

/// The accounting line a usage report owes a line-oriented frontend, or `None`
/// when it owes none.
///
/// A streamed report owes nothing: it is a running total, and a line already
/// written cannot be revised into the next one, so printing them all is what
/// made `runtime/run.log` count one invocation several times. A final report
/// with no priced cost owes nothing either — an unpriced or extraction-failed
/// invocation stays silent here exactly as its accounting record is silent
/// about what it cost. §FS-rhei-cost-accounting.7.1
fn accounting_line(task: &str, report: UsageReport, usage: &UsageSummary) -> Option<String> {
    if report != UsageReport::Final {
        return None;
    }
    let cost = usage.cost_micro.or(usage.priced_cost_micro)?;
    Some(format!(
        "accounting: task {task} {} {}",
        usage.agent,
        format_cost_micro(cost, usage.currency.as_deref())
    ))
}

fn format_cost_micro(value: u64, currency: Option<&str>) -> String {
    let units = value / 1_000_000;
    let cents = (value % 1_000_000) / 10_000;
    match currency {
        Some("USD") | None => format!("${units}.{cents:02}"),
        Some(currency) => format!("{units}.{cents:02} {currency}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rhei_tui::event::{DimensionSummary, PricingStatus, UsageCoverage, UsageStatus};

    fn usage(cost_micro: Option<u64>) -> UsageSummary {
        UsageSummary {
            invocation_id: "plan.1::work::codex::visit-1".to_string(),
            state: "work".to_string(),
            agent: "codex".to_string(),
            provider: Some("openai".to_string()),
            model: Some("gpt-5.6-luna".to_string()),
            total: DimensionSummary::default(),
            input_total: DimensionSummary::default(),
            input_cached_read: DimensionSummary::default(),
            input_cache_write: DimensionSummary::default(),
            output_total: DimensionSummary::default(),
            output_cached_read: DimensionSummary::default(),
            output_cache_write: DimensionSummary::default(),
            cost_micro,
            priced_cost_micro: None,
            currency: Some("USD".to_string()),
            coverage: UsageCoverage::Complete,
            status: UsageStatus::Measured,
            pricing_status: PricingStatus::Priced,
        }
    }

    /// A running total is not a line: the figure is still rising, and the sink
    /// cannot take a line back once it has printed one.
    // §FS-rhei-cost-accounting.7.1
    #[test]
    fn a_streamed_report_writes_no_line() {
        assert_eq!(accounting_line("plan.1", UsageReport::Streamed, &usage(Some(9_625_000))), None);
    }

    /// One line, and it carries the final report's own figure.
    // §FS-rhei-cost-accounting.7.1
    #[test]
    fn a_final_report_with_a_cost_writes_one_line_carrying_it() {
        assert_eq!(
            accounting_line("plan.1", UsageReport::Final, &usage(Some(11_550_000))).as_deref(),
            Some("accounting: task plan.1 codex $11.55")
        );
    }

    /// Clause 5: an invocation with no priced cost writes nothing, final report
    /// or not — the same silence its accounting record keeps.
    // §FS-rhei-cost-accounting.7.1
    #[test]
    fn a_final_report_without_a_cost_writes_no_line() {
        assert_eq!(accounting_line("plan.1", UsageReport::Final, &usage(None)), None);
    }

    /// The priced figure stands in when no charged cost was measured, which is
    /// the one case a line is written from `priced_cost_micro`.
    // §FS-rhei-cost-accounting.7.1
    #[test]
    fn a_final_report_falls_back_to_the_priced_cost() {
        let mut usage = usage(None);
        usage.priced_cost_micro = Some(11_550_000);
        assert_eq!(
            accounting_line("plan.1", UsageReport::Final, &usage).as_deref(),
            Some("accounting: task plan.1 codex $11.55")
        );
    }
}
