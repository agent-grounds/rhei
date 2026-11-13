// What a `--task` reset says about its reach: the rheis its tickets are in, the
// rhei it leaves, the supervisor it holds, the tickets it keeps and the records
// it cannot attribute.
//
// Its own part because none of it is printed for a reset of whole rheis, whose
// output stays as it was.

// §AR-source-file-size.3 §FS-rhei-reset.4

/// How many kept tickets the preview names before it counts the rest.
const KEPT_TICKETS_NAMED: usize = 5;

/// The lines a `--task` reset prints first: its rheis in a project of more than
/// one, and the tickets it reaches past the rheis its ids resolved in.
/// §FS-rhei-reset.4 §FS-rhei-reset.2.5.4
fn report_ticket_scope(loaded: &LoadedPlan, tickets: &TicketScope) {
    if loaded.rhei_ids.len() > 1 {
        let rheis = tickets.rheis();
        println!(
            "Scope: `rhei reset` narrowed to {} ticket(s) that --task reaches, in {} {}: {}",
            tickets.reached.len(),
            rheis.len(),
            if rheis.len() == 1 { "rhei" } else { "rheis" },
            rheis.into_iter().collect::<Vec<_>>().join(", ")
        );
    }
    let beyond = tickets.beyond_resolution();
    let Some(bound) = tickets.resolved_in.as_ref().filter(|_| !beyond.is_empty()) else {
        return;
    };
    let reached: BTreeSet<&str> = beyond.iter().map(|id| owning_rhei(id)).collect();
    println!(
        "The tickets --task reaches leave {} {}, where their ids were resolved: {} ticket(s) in {}.",
        if bound.len() == 1 { "rhei" } else { "rheis" },
        bound.iter().map(String::as_str).collect::<Vec<_>>().join(", "),
        beyond.len(),
        reached.into_iter().collect::<Vec<_>>().join(", ")
    );
}

/// What a ticket scope leaves where it stands, and says so after its moves: the
/// supervisors it holds, the tickets it keeps, and the roots where it kept records
/// keyed by bare local ids. Empty for a reset of whole rheis. §FS-rhei-reset.4
#[derive(Default)]
struct TicketScopeLeft {
    held: Vec<HeldSupervisor>,
    /// Every ticket outside the scope, in plan order.
    kept: Vec<String>,
    bare_kept: BTreeSet<PathBuf>,
}

impl TicketScopeLeft {
    fn collect(
        loaded: &LoadedPlan,
        input: &Path,
        scope: ResetScope<'_>,
        machines: &rhei_validator::MachineSet,
        authored: &AuthoredStates,
    ) -> Self {
        let Some(tickets) = scope.tickets() else { return Self::default() };
        let mut all = Vec::new();
        collect_plan_tasks(&loaded.rhei.tasks, &mut all);
        Self {
            held: held_supervisors(loaded, input, scope, authored),
            kept: all
                .iter()
                .map(|task| task.id.to_string())
                .filter(|id| !tickets.contains(id))
                .collect(),
            bare_kept: scoped_removals(loaded, input, scope, machines).bare_kept,
        }
    }

    /// Print it, with `hold` as the verb for a held supervisor ("Would hold" in the
    /// preview, "Held" in the summary). §FS-rhei-reset.4
    fn report(&self, hold: &str) {
        report_held_supervisors(&self.held, hold);
        if let Some(line) = kept_tickets_line(&self.kept) {
            println!("{line}");
        }
        for root in &self.bare_kept {
            println!(
                "Kept records keyed by bare local ids at {}: only a reset of every ticket there \
                 sweeps them.",
                display_path(root)
            );
        }
    }
}

/// The line naming the tickets a ticket scope keeps, up to five ids and counted past
/// that; `None` when it keeps none. §FS-rhei-reset.4
fn kept_tickets_line(kept: &[String]) -> Option<String> {
    if kept.is_empty() {
        return None;
    }
    let named = kept.iter().take(KEPT_TICKETS_NAMED).map(String::as_str).collect::<Vec<_>>();
    let more = match kept.len().saturating_sub(KEPT_TICKETS_NAMED) {
        0 => String::new(),
        rest => format!(" and {rest} more"),
    };
    Some(format!(
        "Kept {} task(s) as they stand, with their results and logs: {}{more}",
        kept.len(),
        named.join(", ")
    ))
}
