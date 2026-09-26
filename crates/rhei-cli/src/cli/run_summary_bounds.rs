// The run report's `## Bounds` section: where each dimension stood when the
// run began and where it stood when it ended, and the halts in between.
//
// Its own part because the section answers a different question from every
// other block of the report — *what was this bounded by, and how close did it
// come* — and because it is the one block whose numbers are not all counts:
// the spend dimension's are money, and the rendering that decides between the
// two is the reason the model and its renderer belong together.

// §AR-source-file-size.3 §FS-rhei-run-report.3.1 §FS-rhei-budgets.9

/// Each dimension at the start and at the end of the run, and the halts.
///
/// Both ends, because the record answers "what was this bounded by" and "what
/// did it spend": one number cannot do both, and a reader who has only the
/// second cannot tell a run that finished from one that nearly did not.
/// §FS-rhei-run-report.3.1 §FS-rhei-budgets.9
#[derive(Clone, Default)]
struct BoundsSection {
    first: BTreeMap<String, rhei_tui::BoundReport>,
    last: BTreeMap<String, rhei_tui::BoundReport>,
    halts: Vec<(String, rhei_tui::BoundReport, Option<String>, String)>,
}

impl BoundsSection {
    fn is_empty(&self) -> bool {
        self.last.is_empty()
    }

    /// `80 (built_in)`, or the clamped form naming requester and limiter.
    /// A money dimension's amounts are written in the account's currency.
    /// §FS-rhei-budgets.2.3 §FS-rhei-run-report.3.1
    fn provenance(bound: &rhei_tui::BoundReport) -> String {
        match &bound.limiting_source {
            Some(limiter) => format!(
                "{} (requested by the {}, limited by {limiter} settings)",
                Self::amount(bound, bound.effective),
                bound.value_source
            ),
            None => format!("{} ({})", Self::amount(bound, bound.effective), bound.value_source),
        }
    }

    /// One number of this dimension, as money where the dimension is money.
    /// §FS-rhei-run-report.3.1
    fn amount(bound: &rhei_tui::BoundReport, value: u64) -> String {
        match bound.marks {
            Some(_) => rhei_core::money::format_micro(value, bound.currency.as_deref()),
            None => value.to_string(),
        }
    }

    /// How much of a dimension was charged at the worst case rather than
    /// measured. A report that showed only the total would say a day was
    /// spent without saying how much of it was estimated.
    /// §FS-rhei-run-report.3.1 §FS-rhei-budgets.6.2
    fn estimated(bound: &rhei_tui::BoundReport) -> String {
        bound
            .marks
            .and_then(|marks| {
                marks.row(rhei_core::budget::built_in::SPEND_RESERVE, bound.currency.as_deref())
            })
            .map(|row| format!(" ({row})"))
            .unwrap_or_default()
    }

    fn render_markdown(&self) -> String {
        if self.is_empty() {
            return String::new();
        }
        let mut out = String::from("## Bounds\n\n");
        out.push_str(
            "| Dimension | Bound | Mode | Consumed at start | Consumed at end | Remaining |\n\
             | --- | --- | --- | ---: | ---: | ---: |\n",
        );
        for (dimension, bound) in &self.last {
            let started = self.first.get(dimension).map(|b| b.consumed).unwrap_or(bound.consumed);
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} |\n",
                md_cell(dimension),
                md_cell(&Self::provenance(bound)),
                md_cell(bound.window.as_deref().unwrap_or(&bound.mode)),
                md_cell(&Self::amount(bound, started)),
                md_cell(&format!(
                    "{}{}",
                    Self::amount(bound, bound.consumed),
                    Self::estimated(bound)
                )),
                md_cell(&Self::amount(bound, bound.remaining)),
            ));
        }
        out.push('\n');
        for (task, bound, renews_at, remedy) in &self.halts {
            out.push_str(&format!(
                "- `{}` was stopped by its {} bound of {}; {}\n",
                task,
                bound.dimension,
                Self::provenance(bound),
                match renews_at {
                    Some(at) => format!("it renews at {at}"),
                    None => remedy.clone(),
                }
            ));
        }
        if !self.halts.is_empty() {
            out.push('\n');
        }
        out
    }
}
