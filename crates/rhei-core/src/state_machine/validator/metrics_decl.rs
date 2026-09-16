// The `metrics:` declaration of a states file: which measurement trajectories
// the engine records for this machine, and where their values come from.
//
// Its own part because the declaration is pure schema and static validation;
// recording and presentation live with the engine (`cli/metrics_ledger.rs`),
// which only ever reads a machine that already passed the checks here.

// §AR-source-file-size.3 §FS-rhei-metrics.1

/// The placeholder every per-iteration path template must carry.
// §FS-rhei-metrics.1
pub const METRIC_ITERATION_PLACEHOLDER: &str = "{iteration}";

/// Direction a metric is meant to move; orients delta presentation only.
// §FS-rhei-metrics.1
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MetricGoal {
    /// Larger values are progress.
    Increase,
    /// Smaller values are progress.
    Decrease,
}

/// Value shape of a metric; string metrics render without deltas.
// §FS-rhei-metrics.1
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MetricKind {
    /// Numeric value with deltas between iterations.
    #[default]
    Number,
    /// Categorical value; rendered verbatim, no deltas.
    String,
}

/// One declared metric from the top-level `metrics:` mapping.
// §FS-rhei-metrics.1
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MetricDef {
    /// Display name; the metric key when absent.
    #[serde(default)]
    pub label: Option<String>,
    /// States whose successful runs materialize the metric. Always a list.
    pub measured_by: Vec<String>,
    /// States whose sessions are the intended movers; presentation only.
    #[serde(default)]
    pub drivers: Vec<String>,
    /// Workspace-relative per-iteration boundary artifact path template.
    /// Any file format: only its existence marks a successful measurement.
    pub artifact: String,
    /// JSON document holding the value when it differs from `artifact`.
    #[serde(default)]
    pub value_artifact: Option<String>,
    /// JSON Pointer into the value document, `{iteration}`-templated.
    #[serde(default)]
    pub pointer: Option<String>,
    /// Command whose stdout is the value; the non-JSON escape hatch.
    #[serde(default)]
    pub program: Option<String>,
    /// Template of `{<json-pointer>}` segments rendered beside the value.
    #[serde(default)]
    pub detail: Option<String>,
    /// Suffix rendered after the value, e.g. `%`.
    #[serde(default)]
    pub unit: Option<String>,
    /// Orients delta arrows and regression emphasis.
    #[serde(default)]
    pub goal: Option<MetricGoal>,
    /// Value shape; defaults to `number`.
    #[serde(default)]
    pub kind: MetricKind,
}

impl StateMachine {
    /// Reject metric declarations that reference unknown states or leave the
    /// value source ambiguous. A machine that declares no metrics passes
    /// untouched: declared metrics change nothing about scheduling.
    // §FS-rhei-metrics.1 §FS-rhei-metrics.5
    fn validate_metrics_configuration(&self) -> Result<(), StateMachineLoadError> {
        for (key, metric) in &self.metrics {
            if metric.measured_by.is_empty() {
                return Err(StateMachineLoadError::Invalid(format!(
                    "metric '{key}' declares an empty `measured_by` list; name at \
                     least one state whose successful runs materialize the metric"
                )));
            }
            for state in metric.measured_by.iter().chain(metric.drivers.iter()) {
                if !self.states.contains_key(state) {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "metric '{key}' references state '{state}', which this \
                         machine does not declare"
                    )));
                }
            }
            match (&metric.pointer, &metric.program) {
                (Some(_), Some(_)) => {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "metric '{key}' declares both `pointer` and `program`; \
                         a metric has exactly one value source"
                    )));
                }
                (None, None) => {
                    return Err(StateMachineLoadError::Invalid(format!(
                        "metric '{key}' declares neither `pointer` nor `program`; \
                         a metric has exactly one value source"
                    )));
                }
                _ => {}
            }
            if !metric.artifact.contains(METRIC_ITERATION_PLACEHOLDER) {
                return Err(StateMachineLoadError::Invalid(format!(
                    "metric '{key}' artifact '{}' has no `{METRIC_ITERATION_PLACEHOLDER}` \
                     placeholder; the boundary artifact must be per-iteration so a \
                     successful measurement is distinguishable from the last one",
                    metric.artifact
                )));
            }
        }
        Ok(())
    }
}
