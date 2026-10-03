// The supervisors a run is holding for an empty visit, and the rule that gives
// one of them another turn: something advanced after it was held.
//
// Its own part because this stall is the one whose release is not the run's
// generic "some earlier pass made progress". The pass driver, the sequential
// completion and the worker pool all hold a supervisor here; the pass driver
// alone releases.

// §AR-source-file-size.3 §FS-rhei-run.3.6

/// How far every transition ledger the run can write to had grown, root by root.
///
/// Every applied transition appends one line to its execution root's
/// `runtime/state-transitions.log`, whoever applied it, while a poll's
/// self-loop only reschedules its attempt in plan metadata and a deadline sleep
/// writes nothing. So a ledger that has not grown since a hold is a world in
/// which nothing advanced (§FS-rhei-run.3.6 items 2 and 3).
// §FS-rhei-run.3.6
type LedgerMark = Vec<(PathBuf, u64)>;

/// Where the run's transition ledgers stand now: the run's own root and every
/// root the loaded plan gives a ticket. §FS-rhei-run.3.6
fn transition_ledger_mark(
    workspace_root: &Path,
    task_roots: &HashMap<String, PathBuf>,
) -> LedgerMark {
    let mut roots: BTreeSet<&Path> = task_roots.values().map(PathBuf::as_path).collect();
    roots.insert(workspace_root);
    roots
        .into_iter()
        .map(|root| {
            let ledger = root.join("runtime").join("state-transitions.log");
            let len = fs::metadata(&ledger).map(|meta| meta.len()).unwrap_or(0);
            (root.to_path_buf(), len)
        })
        .collect()
}

/// Supervisors held for an empty visit (§FS-rhei-supervision.3.6), each with
/// the ledger mark taken when it was held.
///
/// A held ticket also sits in the run's `stalled_tasks`, so every scheduler
/// skips it as it skips any stall. Only the release differs: the generic one
/// fires on any progress since the last release, which for a supervisor
/// always includes the move that woke it.
// §FS-rhei-run.3.6
#[derive(Default)]
struct EmptyVisitHolds {
    held_at: HashMap<String, LedgerMark>,
}

impl EmptyVisitHolds {
    /// Hold `task_id` as of now. The mark is taken after the visit's exit was
    /// judged, so the move that woke the supervisor, and anything else the
    /// visit already saw, is behind it. §FS-rhei-run.3.6 item 1
    fn hold(
        &mut self,
        task_id: &str,
        workspace_root: &Path,
        task_roots: &HashMap<String, PathBuf>,
    ) {
        self.held_at
            .insert(task_id.to_string(), transition_ledger_mark(workspace_root, task_roots));
    }

    /// Release what the run has earned the right to try again, and say whether
    /// anything was released.
    ///
    /// Every other stall goes, as the generic rule says. A held supervisor goes
    /// only when some ledger grew since its hold, and the hold is forgotten
    /// with it, so the advances between two releases buy one re-spawn: the
    /// next empty visit is held afresh. §FS-rhei-run.3.6
    fn release(
        &mut self,
        stalled_tasks: &mut HashSet<String>,
        workspace_root: &Path,
        task_roots: &HashMap<String, PathBuf>,
    ) -> bool {
        let before = stalled_tasks.len();
        if !self.held_at.is_empty() {
            let now = transition_ledger_mark(workspace_root, task_roots);
            self.held_at.retain(|id, mark| stalled_tasks.contains(id) && *mark == now);
        }
        stalled_tasks.retain(|id| self.held_at.contains_key(id));
        stalled_tasks.len() < before
    }

    /// A deadline sleep releases every other stall but no held supervisor: the
    /// sleep advanced nothing, and the attempt it made due releases one only by
    /// advancing something. §FS-rhei-run.3.6 item 3
    fn keep_held(&self, stalled_tasks: &mut HashSet<String>) {
        stalled_tasks.retain(|id| self.held_at.contains_key(id));
    }
}
