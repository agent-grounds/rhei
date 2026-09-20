// Shared group identity is allocated before work and carried into the central
// entry; an observed move count cannot manufacture an earned edge.
// §AR-neural-admission.6 §FS-rhei-budgets.7

#[derive(Clone)]
struct BudgetEdgeBinding {
    task: String,
    transition: String,
    owner: String,
}
thread_local! {
    static BUDGET_EDGE_BINDING: std::cell::RefCell<Option<BudgetEdgeBinding>> = const { std::cell::RefCell::new(None) };
}
struct BudgetEdgeGuard(Option<BudgetEdgeBinding>);
impl BudgetEdgeGuard {
    fn enter(task: &str, lease: &BudgetLease) -> Self {
        let binding = BudgetEdgeBinding {
            task: task.into(),
            transition: lease.transition_receipt_id.clone(),
            owner: lease.travel_owner.clone(),
        };
        Self(BUDGET_EDGE_BINDING.with(|slot| slot.replace(Some(binding))))
    }
}
impl Drop for BudgetEdgeGuard {
    fn drop(&mut self) {
        BUDGET_EDGE_BINDING.with(|slot| slot.replace(self.0.take()));
    }
}
fn budget_edge_suffix(task: &str) -> String {
    BUDGET_EDGE_BINDING.with(|slot| {
        slot.borrow()
            .as_ref()
            .filter(|b| b.task == task)
            .map(|b| format!(" {} {}", b.transition, b.owner))
            .unwrap_or_default()
    })
}

fn budget_central_edge(
    root: &Path,
    task: &str,
    transition: &str,
    owner: &str,
) -> MietteResult<Option<(String, String)>> {
    let ledger = LockedTransitionLedger::lock(root)?;
    let text = match fs::read_to_string(ledger.path()) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(file_io_report(ledger.path(), "cannot read earned edge receipt", error))
        }
    };
    let mut found = None;
    for line in text.lines() {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.get(2) != Some(&transition) {
            continue;
        }
        if fields.len() != 4 || fields[0] != task || fields[3] != owner || found.is_some() {
            return Err(miette!(help = "the transition ledger was edited by hand; restore it from version control before running again", "untrustworthy central earned-edge identity: {transition}"));
        }
        let (from, to) =
            fields[1].split_once('@').ok_or_else(|| miette!(help = "the transition ledger was edited by hand; restore it from version control before running again", "malformed earned edge"))?;
        found = Some((from.into(), to.into()));
    }
    Ok(found)
}

/// Recover a transition-side receipt after a crash before its budget append.
/// Missing central/outcome evidence retains exposure; it never reruns work.
/// §AR-neural-admission.6
fn budget_recover_central_edges(
    project: &BudgetProject,
    loaded: &LoadedPlan,
    journal: &mut rhei_core::budget::Journal,
) -> MietteResult<()> {
    let snapshot = journal.snapshot().map_err(budget_error)?;
    for (owner, reservation) in &snapshot.reservations {
        if reservation["travel_reservation_id"] != *owner {
            continue;
        }
        let Some(transition) = reservation["transition_receipt_id"].as_str() else { continue };
        let Some(ticket) = reservation["ticket_identity"].as_str() else { continue };
        let Some(binding) = journal.identities().get(ticket) else { continue };
        let Some(task) = binding["display_id"].as_str() else { continue };
        let root = loaded.task_root(task, &project.root);
        if let Some((from, to)) = budget_central_edge(&root, task, transition, owner)? {
            journal
                .record_transition(
                    owner,
                    transition,
                    &from,
                    &to,
                    &budget_audit("recover central bounded transition receipt")?,
                )
                .map_err(budget_error)?;
        }
    }
    Ok(())
}
