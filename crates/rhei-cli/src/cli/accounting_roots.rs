// Which accounting roots one reading covers, and what reading them produces.
//
// A standalone workspace has a single root, which is both the run root and its
// rhei's. A Panta project has several — one per participating rhei execution
// root, plus the run root — and single-file rheis share the project directory,
// so two of them can be the same directory. Everything here exists because that
// union has to be read once, counted once, and said out loud.

// §AR-source-file-size.3
// §FS-rhei-panta.6.5 §FS-rhei-cost-accounting.2 §AR-rhei-panta.5

/// An accounting root is this directory under an execution root.
/// §FS-rhei-cost-accounting.2
const ACCOUNTING_DIR: &str = "runtime/accounting";

/// One accounting root a reading will cover.
/// §FS-rhei-panta.6.5
#[derive(Clone, Debug)]
struct AccountingRoot {
    /// `runtime/accounting` under the execution root, spelled as the command
    /// resolved it: this is the text a reading prints and a payload carries.
    path: PathBuf,
    /// The in-scope rheis whose execution root this is, in enumeration order.
    /// Empty for a run root no rhei shares.
    rheis: Vec<String>,
    /// A rhei outside the scope roots here too, so which of this root's records
    /// are in scope is decided by their `task_id` rather than by the root
    /// itself. §FS-rhei-panta.6.5
    shared: bool,
}

/// One accounting root after it was read: what it is, and what it contributed.
/// §FS-rhei-panta.6.5 §FS-rhei-cost-accounting.8.4
#[derive(Clone, Debug)]
struct AccountingRootReading {
    path: PathBuf,
    rheis: Vec<String>,
    /// The books reachable beside this root's records. A record is priced by
    /// the book of the root it was read from, never by another root's.
    /// §FS-rhei-cost-accounting.5.2
    books: ReachablePriceBooks,
    /// How many records it contributed, after the shared-root filter and after
    /// deduplication.
    invocation_count: u64,
}

/// A root's identity, so two spellings of one directory are one root.
///
/// Comparing the spellings would count a shared root twice, which is exactly
/// what §REQ-cross-platform.5 forbids. A directory that does not exist stands
/// for itself: two absent roots are still two roots, and an absent one holds
/// nothing either way.
// §REQ-cross-platform.5
fn canonical_root_key(root: &Path) -> PathBuf {
    fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf())
}

/// Record one execution root in enumeration order, folding a repeat into the
/// entry that already stands for it.
fn enumerate_root(
    order: &mut Vec<(PathBuf, Vec<String>)>,
    seen: &mut HashMap<PathBuf, usize>,
    root: &Path,
    rhei: Option<&str>,
) {
    let key = canonical_root_key(root);
    let index = match seen.get(&key) {
        Some(index) => *index,
        None => {
            order.push((root.to_path_buf(), Vec::new()));
            seen.insert(key, order.len() - 1);
            order.len() - 1
        }
    };
    if let Some(id) = rhei {
        order[index].1.push(id.to_string());
    }
}

/// The accounting roots one reading covers, in enumeration order: the run root
/// first when the reading is project-wide, then each in-scope rhei's execution
/// root, deduplicated by canonicalized path.
///
/// A narrowed reading covers the named rheis' roots alone — the run root is not
/// one of them unless a named rhei happens to root there.
// §FS-rhei-panta.6.5 §AR-rhei-panta.5
fn accounting_roots(
    loaded: &LoadedPlan,
    run_root: &Path,
    scope: &RheiScope,
) -> Vec<AccountingRoot> {
    // How many rheis root where, over the whole project rather than over the
    // scope: whether a root is shared is a fact about the project, and it is
    // what decides whether the records in it have to be filtered.
    let mut owners: HashMap<PathBuf, usize> = HashMap::new();
    for id in &loaded.rhei_ids {
        if let Some(root) = loaded.rhei_roots.get(id) {
            *owners.entry(canonical_root_key(root)).or_default() += 1;
        }
    }

    let mut order: Vec<(PathBuf, Vec<String>)> = Vec::new();
    let mut seen: HashMap<PathBuf, usize> = HashMap::new();
    if scope.is_none() {
        enumerate_root(&mut order, &mut seen, run_root, None);
    }
    for id in &loaded.rhei_ids {
        if scope.as_ref().is_some_and(|scope| !scope.contains(id)) {
            continue;
        }
        if let Some(root) = loaded.rhei_roots.get(id) {
            enumerate_root(&mut order, &mut seen, root, Some(id));
        }
    }

    order
        .into_iter()
        .map(|(root, rheis)| {
            let shared =
                owners.get(&canonical_root_key(&root)).copied().unwrap_or(0) > rheis.len();
            AccountingRoot { path: root.join(ACCOUNTING_DIR), rheis, shared }
        })
        .collect()
}

/// One record as it was read, with the root it came from.
#[derive(Clone, Debug)]
struct InspectedRecord {
    path: PathBuf,
    record: AccountingInvocationRecord,
    /// Index into [`CostInspection::roots`], so the record keeps the price
    /// books of the root it was read from. §FS-rhei-cost-accounting.5.2
    root: usize,
}

/// A stored record together with the price books of the accounting root it was
/// read from. A reading over several roots prices each record by its own root's
/// book, never by another root's, because that is what a record's book already
/// means. §FS-rhei-cost-accounting.5.2
#[derive(Clone, Copy)]
struct ScopedRecord<'a> {
    record: &'a AccountingInvocationRecord,
    books: &'a ReachablePriceBooks,
}

/// Everything one reading of the accounting artifacts found, over however many
/// roots its scope selected. §FS-rhei-panta.6.5
#[derive(Clone, Debug)]
struct CostInspection {
    summary: Option<rhei_tui::AccountingRunSummary>,
    invocations: Vec<InspectedRecord>,
    /// Every accounting root covered, in enumeration order, whether or not it
    /// held anything. §FS-rhei-cost-accounting.8
    roots: Vec<AccountingRootReading>,
    errors: Vec<String>,
    /// The subset of `errors` caused by contradictory attempt identity. Run
    /// preflight names these independently from price-book validation.
    // §FS-rhei-cost-accounting.11
    identity_conflicts: Vec<String>,
    /// A root existed and could not be read, so the reading saw less than it
    /// was asked for and no aggregate over it reports `complete`.
    // §FS-rhei-cost-accounting.6.2
    unreadable_root: bool,
}

impl CostInspection {
    /// Every record read, each carrying its own root's price books.
    fn scoped(&self) -> impl Iterator<Item = ScopedRecord<'_>> {
        self.invocations
            .iter()
            .map(|held| ScopedRecord { record: &held.record, books: &self.roots[held.root].books })
    }

    /// The books of the root one held record was read from.
    /// §FS-rhei-cost-accounting.5.2
    fn books_of(&self, held: &InspectedRecord) -> &ReachablePriceBooks {
        &self.roots[held.root].books
    }

    /// An inspection built from records alone, for the tests that render a
    /// reading without a directory behind it.
    #[cfg(test)]
    fn from_records(
        summary: Option<rhei_tui::AccountingRunSummary>,
        records: Vec<AccountingInvocationRecord>,
    ) -> Self {
        Self {
            summary,
            invocations: records
                .into_iter()
                .enumerate()
                .map(|(index, record)| InspectedRecord {
                    path: PathBuf::from(format!("{index}.json")),
                    record,
                    root: 0,
                })
                .collect(),
            roots: vec![AccountingRootReading {
                path: PathBuf::from(ACCOUNTING_DIR),
                rheis: Vec::new(),
                books: ReachablePriceBooks::builtin_only(),
                invocation_count: 0,
            }],
            errors: Vec::new(),
            identity_conflicts: Vec::new(),
            unreadable_root: false,
        }
    }
}

/// The records one accounting root holds, in `started_at` order, with whatever
/// would not read.
///
/// A missing or empty `invocations/` contributes nothing and is not an error: a
/// member that has never been run is an ordinary member (§FS-rhei-panta.6.5). A
/// root that exists and cannot be read is the third return value, because with
/// several roots it would otherwise silently shrink a total that still claimed
/// to be whole. §FS-rhei-cost-accounting.6.2
fn read_accounting_root(
    accounting_root: &Path,
) -> (Vec<(PathBuf, AccountingInvocationRecord)>, Vec<String>, bool) {
    let mut invocations = Vec::new();
    let mut errors = Vec::new();
    let dir = accounting_root.join("invocations");
    // §FS-rhei-cost-accounting.2: Invocation records are authoritative.
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return (invocations, errors, false)
        }
        Err(err) => {
            errors.push(format!("{}: {err}", dir.display()));
            return (invocations, errors, true);
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(OsStr::to_str) != Some("json") {
            continue;
        }
        match fs::read_to_string(&path).map_err(|err| err.to_string()).and_then(|text| {
            serde_json::from_str::<AccountingInvocationRecord>(&text).map_err(|err| err.to_string())
        }) {
            Ok(record) => invocations.push((path, record)),
            Err(err) => errors.push(format!("{}: {err}", path.display())),
        }
    }
    invocations.sort_by(|(_, a), (_, b)| {
        a.started_at.cmp(&b.started_at).then_with(|| a.invocation_id.cmp(&b.invocation_id))
    });
    (invocations, errors, false)
}

/// Whether a repeated `invocation_id` is one record read twice or two different
/// records sharing an id. §FS-rhei-panta.6.5
fn same_stored_record(
    left: &AccountingInvocationRecord,
    right: &AccountingInvocationRecord,
) -> bool {
    matches!(
        (serde_json::to_value(left), serde_json::to_value(right)),
        (Ok(left), Ok(right)) if left == right
    )
}

/// Whether this id already carries the run/move/attempt suffix written by
/// current Rhei. A repeated current id always means the same attempt; only a
/// legacy visit-level id is eligible for inference.
// §FS-rhei-cost-accounting.3.7
fn is_attempt_scoped_invocation_id(id: &str) -> bool {
    let Some((before_attempt, attempt)) = id.rsplit_once("::attempt-") else { return false };
    let Some((before_move, moves)) = before_attempt.rsplit_once("::move-") else { return false };
    let Some((_, run_id)) = before_move.rsplit_once("::run-") else { return false };
    !run_id.is_empty() && moves.parse::<u64>().is_ok() && attempt.parse::<u64>().is_ok()
}

/// Stable facts carried both beside and inside a legacy visit-level id. A
/// disagreement means the records cannot describe retries of one visit.
// §FS-rhei-cost-accounting.3.7
fn same_legacy_base_facts(
    left: &AccountingInvocationRecord,
    right: &AccountingInvocationRecord,
) -> bool {
    left.task_id == right.task_id
        && left.state == right.state
        && left.visit == right.visit
        && left.target_slug == right.target_slug
}

/// A stored interval that can distinguish one legacy process from another.
// §FS-rhei-cost-accounting.3.7
fn valid_invocation_interval(
    record: &AccountingInvocationRecord,
) -> Option<(std::time::SystemTime, std::time::SystemTime)> {
    let started = rhei_tui::parse_rfc3339(&record.started_at)?;
    let ended = rhei_tui::parse_rfc3339(&record.ended_at)?;
    (ended >= started).then_some((started, ended))
}

/// Independent run evidence or non-overlapping forward intervals distinguish
/// two legacy attempts. Missing run attribution is deliberately not failure:
/// interval evidence can still retain that history.
// §FS-rhei-cost-accounting.3.5 §FS-rhei-cost-accounting.3.7
fn distinct_legacy_attempts(
    left: &AccountingInvocationRecord,
    right: &AccountingInvocationRecord,
) -> bool {
    if !same_legacy_base_facts(left, right) {
        return false;
    }
    if matches!((&left.run_id, &right.run_id), (Some(left), Some(right)) if left != right) {
        return true;
    }
    match (valid_invocation_interval(left), valid_invocation_interval(right)) {
        (Some((left_start, left_end)), Some((right_start, right_end))) => {
            left_end <= right_start || right_end <= left_start
        }
        _ => false,
    }
}

/// Read every root of one reading as a single set.
///
/// The root decides which records are in scope, except where one root is shared
/// by an out-of-scope rhei — there the record's `task_id` decides, because
/// deciding by root would report every rhei sharing it. Attempt-scoped ids are
/// keys directly; legacy visit ids use run and interval evidence. Exact copies
/// are dropped, while ambiguous or contradictory records are reported.
/// §FS-rhei-panta.6.5 §FS-rhei-cost-accounting.3.7
fn read_cost_inspection_over(roots: &[AccountingRoot], scope: &RheiScope) -> CostInspection {
    // §FS-rhei-recover.4: direct accounting readers share the same root boundary.
    let execution_roots = roots.iter().filter_map(|root| root.path.parent()?.parent())
        .filter(|root| root.exists()).map(Path::to_path_buf).collect::<Vec<_>>();
    let _guards = match rhei_core::root_access::shared_roots(execution_roots) {
        Ok(guards) => guards,
        Err(err) => return CostInspection { summary: None, invocations: Vec::new(), roots: Vec::new(),
            errors: vec![err.to_string()], identity_conflicts: Vec::new(), unreadable_root: true },
    };
    let mut readings: Vec<AccountingRootReading> = Vec::new();
    let mut invocations: Vec<InspectedRecord> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let mut unreadable_root = false;
    let mut seen: HashMap<String, Vec<usize>> = HashMap::new();
    let mut identity_conflicts: Vec<String> = Vec::new();

    for root in roots {
        let index = readings.len();
        let (records, mut root_errors, unreadable) = read_accounting_root(&root.path);
        errors.append(&mut root_errors);
        unreadable_root |= unreadable;
        let mut invocation_count = 0u64;
        for (path, record) in records {
            if root.shared && !task_in_rhei_scope(scope, &record.task_id) {
                continue;
            }
            if let Some(prior) = seen.get(&record.invocation_id) {
                if prior
                    .iter()
                    .any(|index| same_stored_record(&invocations[*index].record, &record))
                {
                    continue;
                }
                let conflict_with = if is_attempt_scoped_invocation_id(&record.invocation_id) {
                    prior.first()
                } else {
                    prior.iter().find(|index| {
                        !distinct_legacy_attempts(&invocations[**index].record, &record)
                    })
                };
                if let Some(first) = conflict_with {
                    let error = format!(
                        "{}: a different record is already read as invocation '{}' from {}",
                        path.display(),
                        record.invocation_id,
                        invocations[*first].path.display()
                    );
                    errors.push(error.clone());
                    identity_conflicts.push(error);
                    continue;
                }
            }
            seen.entry(record.invocation_id.clone()).or_default().push(invocations.len());
            invocations.push(InspectedRecord { path, record, root: index });
            invocation_count += 1;
        }
        readings.push(AccountingRootReading {
            path: root.path.clone(),
            rheis: root.rheis.clone(),
            // §FS-rhei-cost-accounting.5.2: the book beside the records is what
            // a reading can reach, so it is read from the same root they are.
            books: ReachablePriceBooks::beside(&root.path),
            invocation_count,
        });
    }

    // Each root is read in `started_at` order, but appending root by root
    // groups the union by root, and a reading is one set rather than several.
    // §FS-rhei-summary.2.2
    invocations.sort_by(|a, b| {
        a.record
            .started_at
            .cmp(&b.record.started_at)
            .then_with(|| a.record.invocation_id.cmp(&b.record.invocation_id))
    });

    let inspection = CostInspection {
        summary: None,
        invocations,
        roots: readings,
        errors,
        identity_conflicts,
        unreadable_root,
    };
    let summary = summarize_records(inspection.scoped())
        .map(|summary| demote_if(summary, inspection.unreadable_root));
    CostInspection { summary, ..inspection }
}

/// One accounting root, read on its own: what a writer's own runtime root
/// holds, with no project above it to widen to.
fn read_cost_inspection(accounting_root: &Path) -> CostInspection {
    let root =
        AccountingRoot { path: accounting_root.to_path_buf(), rheis: Vec::new(), shared: false };
    read_cost_inspection_over(&[root], &None)
}

/// Where an empty reading looked, printed under the line that carries the
/// answer.
///
/// One root is named inline; several are counted, then listed one per line and
/// indented two spaces, in enumeration order. An empty reading over a project
/// holding thousands of records reads as "this run cost nothing" unless it says
/// which directories it looked in. §FS-rhei-cost-accounting.8
fn roots_tail(target: &Path, roots: &[AccountingRootReading]) -> String {
    match roots {
        [] => String::new(),
        [only] => format!("searched {}\n", only.path.display()),
        many => {
            let mut out = format!(
                "searched {} accounting roots under {}:\n",
                many.len(),
                target.display()
            );
            for root in many {
                out.push_str(&format!("  {}\n", root.path.display()));
            }
            out
        }
    }
}

/// The `roots` array of a cost payload: one entry per root the reading covered,
/// in enumeration order, present whether or not the reading found anything.
// §FS-rhei-cost-accounting.8.4
fn roots_json(roots: &[AccountingRootReading]) -> Vec<serde_json::Value> {
    roots
        .iter()
        .map(|root| {
            serde_json::json!({
                "path": root.path.display().to_string(),
                "rheis": root.rheis,
                "invocation_count": root.invocation_count,
            })
        })
        .collect()
}
