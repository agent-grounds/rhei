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
        self.last.is_empty() && self.halts.is_empty()
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
    /// spent without saying how much of it was estimated. What was unpriced is
    /// named in the same parenthesis, in one line, because a table cell cannot
    /// hold the halt's continuation lines.
    /// §FS-rhei-run-report.3.1 §FS-rhei-budgets.6.2 §FS-rhei-budgets.8.1
    fn estimated(bound: &rhei_tui::BoundReport) -> String {
        bound
            .marks
            .as_ref()
            .and_then(|marks| {
                marks.one_line(rhei_core::budget::built_in::SPEND_RESERVE, bound.currency.as_deref())
            })
            .map(|row| format!(" ({row})"))
            .unwrap_or_default()
    }

    /// The ceiling rows and the policy row of §FS-rhei-budgets.2.3, once per
    /// report, where the machine delegated a count ceiling; travel first, as
    /// every other view lists the keys. §FS-rhei-run-report.3.1
    fn delegation_rows(&self) -> Vec<String> {
        let mut ceilings: Vec<&rhei_tui::BoundCeiling> =
            self.last.values().filter_map(|bound| bound.ceiling.as_ref()).collect();
        ceilings.sort_by_key(|ceiling| ceiling.key != "transition_limit");
        let Some(first) = ceilings.first() else { return Vec::new() };
        let named = |path: &Option<String>, role: &str| {
            path.clone().unwrap_or_else(|| format!("the {role} settings file"))
        };
        let keys: Vec<&str> = ceilings.iter().map(|ceiling| ceiling.key.as_str()).collect();
        let policy = format!(
            "count ceiling policy: delegated {} by {}=false in {}",
            keys.join(", "),
            rhei_core::budget::CLAMP_PROJECTS,
            named(&first.delegated_by, "machine")
        );
        ceilings
            .iter()
            .map(|ceiling| {
                format!(
                    "{} ceiling: {} (project, {})",
                    ceiling.key,
                    ceiling.value,
                    named(&ceiling.path, "project")
                )
            })
            .chain(std::iter::once(policy))
            .collect()
    }

    fn render_markdown(&self) -> String {
        if self.is_empty() {
            return String::new();
        }
        let mut out = String::from("## Bounds\n\n");
        // Outstanding gets a column of its own so every row adds up to its
        // bound, a reservation still held at the end included. §FS-rhei-run-report.3.1
        out.push_str(
            "| Dimension | Bound | Mode | Consumed at start | Consumed at end | \
             Outstanding at end | Remaining |\n\
             | --- | --- | --- | ---: | ---: | ---: | ---: |\n",
        );
        for (dimension, bound) in &self.last {
            let started = self.first.get(dimension).map(|b| b.consumed).unwrap_or(bound.consumed);
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {} |\n",
                md_cell(dimension),
                md_cell(&Self::provenance(bound)),
                md_cell(bound.window.as_deref().unwrap_or(&bound.mode)),
                md_cell(&Self::amount(bound, started)),
                md_cell(&format!(
                    "{}{}",
                    Self::amount(bound, bound.consumed),
                    Self::estimated(bound)
                )),
                md_cell(&Self::amount(bound, bound.outstanding)),
                md_cell(&Self::amount(bound, bound.remaining)),
            ));
        }
        out.push('\n');
        let delegation = self.delegation_rows();
        for row in &delegation {
            out.push_str(&format!("- {row}\n"));
        }
        if !delegation.is_empty() {
            out.push('\n');
        }
        // The same line is one stop, however many passes refused it. §FS-rhei-run-report.3.1
        let mut listed = std::collections::HashSet::new();
        for (task, bound, renews_at, remedy) in &self.halts {
            let line = format!(
                "- `{}` was stopped by its {} bound of {}; {}\n",
                task,
                bound.dimension,
                Self::provenance(bound),
                match renews_at {
                    Some(at) => format!("it renews at {at}"),
                    None => remedy.clone(),
                }
            );
            if listed.insert(line.clone()) {
                out.push_str(&line);
            }
        }
        if !self.halts.is_empty() {
            out.push('\n');
        }
        out
    }
}

/// The bound halts still true of their tickets, for `## Attention`. Apart from
/// [`BoundsSection::halts`], which keeps every distinct halt of the run as history.
///
/// A halt is taken back by what can be seen after it: the ticket's next slot, and
/// at the overlay a plan state other than the one refused, or a reading of the plan
/// that does not yield to a halt. Only the latest refusal of a ticket is kept, and
/// so compared. §FS-rhei-run-report.3.1
#[derive(Default)]
struct StandingBoundHalts(HashMap<String, StandingBoundHalt>);

/// One ticket's latest refusal: its Attention reading and the state it was refused
/// in, `None` from a record written before `budget_halt` named one.
/// §FS-rhei-run-report.3.1 §FS-rhei-run-json.2.1
struct StandingBoundHalt {
    refused_in: Option<String>,
    cause: HaltCause,
}

impl StandingBoundHalts {
    /// A window renews by itself, so its row has a deadline and nothing to do.
    /// Otherwise the next action is the record's remedy verbatim, never a second
    /// resolution of it. §FS-rhei-run-report.3.1 §FS-rhei-budgets.8
    fn record(
        &mut self,
        task: &str,
        refused_in: Option<String>,
        bound: &rhei_tui::BoundReport,
        renews_at: Option<&str>,
        remedy: &str,
    ) {
        let blocker = format!("{} bound of {}", bound.dimension, BoundsSection::provenance(bound));
        let (blocker, next_action) = match renews_at {
            Some(at) => (format!("{blocker}, renews {at}"), "renews automatically".to_string()),
            None => (blocker, remedy.to_string()),
        };
        let cause = HaltCause::BoundHalted { blocker, next_action };
        self.0.insert(task.to_string(), StandingBoundHalt { refused_in, cause });
    }

    fn take_back(&mut self, task: &str) {
        self.0.remove(task);
    }

    /// A refused start outranks what the classifier inferred from the ticket not
    /// moving, while it is in the state refused: once it has moved the refusal says
    /// nothing about where it is now, so the classifier's reading stands. A record
    /// with no state proves no move. A terminal ticket has no cause to replace.
    /// §FS-rhei-run-report.3.1
    fn overlay(
        &self,
        rhei: &rhei_core::ast::Rhei,
        machines: &rhei_validator::MachineSet,
        halt_causes: &mut HashMap<String, HaltCause>,
    ) {
        let moved_from = |id: &str, refused_in: &str| {
            find_task_by_id(&rhei.tasks, &parse_task_id(id)).is_some_and(|task| {
                normalized_state_name(task.state.as_str(), machines.for_task(&task.id)) != refused_in
            })
        };
        for (task, halt) in &self.0 {
            if halt.refused_in.as_deref().is_some_and(|refused_in| moved_from(task, refused_in)) {
                continue;
            }
            if let Some(cause) = halt_causes.get_mut(task).filter(|c| c.yields_to_bound_halt()) {
                *cause = halt.cause.clone();
            }
        }
    }
}
