// Which spawn record answers for an invocation the plan names differently from
// the one that did the visit's work: a target edited in place between two runs,
// after the work finished and before the ticket moved.
//
// Its own part because the completion condition asks one invocation at a time,
// and this question can only be answered about the whole state at once: a
// record belongs to no current invocation only if *every* current invocation
// disowns it, and a pairing is safe only if it is the only one possible.

// §AR-source-file-size.3 §FS-rhei-agents.3.2 §FS-rhei-agents.8.4

/// The names an invocation reads as its own spawn record, in the order they are
/// tried: the one its identity and number spell and, for an uncounted re-entry,
/// the unsuffixed name a runtime from before entry numbers wrote, which it reads
/// only at the current `moves`.
///
/// The own-record check reads through this list, and the orphan test excludes
/// every name on every current invocation's list, not only the one that
/// answered. A fallback the own-record lookup trusts goes on this list, so a
/// record it would read as an invocation's own is never taken for an orphan.
// §FS-rhei-agents.8.1 §FS-rhei-agents.8.4
fn own_spawn_record_names(
    runtime_dir: &Path,
    task_id: &str,
    state_name: &str,
    identity: Option<&str>,
    number: LogNumber,
) -> Vec<PathBuf> {
    let suffix = log_suffix(identity, number.value());
    let mut names = vec![spawn_record_path(runtime_dir, task_id, state_name, suffix.as_deref())];
    if matches!(number, LogNumber::Entry(entry) if entry > 1) {
        names.push(spawn_record_path(runtime_dir, task_id, state_name, identity));
    }
    names
}

/// The first record found under an invocation's own names: its own name as it
/// stands, a fallback name only at the current `moves`, as `current_visit_record`
/// reads it. §FS-rhei-agents.8.4
fn own_spawn_record(
    names: &[PathBuf],
    task_id: &str,
    state_name: &str,
    moves: u64,
) -> Option<SpawnRecord> {
    let (own, fallbacks) = names.split_first()?;
    read_spawn_record(own).or_else(|| {
        fallbacks.iter().find_map(|path| {
            read_spawn_record(path).filter(|record| {
                record.task == task_id && record.state == state_name && record.moves == moves
            })
        })
    })
}

/// What one visit's records say about the state as a whole, before any one
/// invocation is asked about.
// §FS-rhei-agents.3.2 §FS-rhei-agents.8.4
#[derive(Debug, Default, PartialEq)]
struct VisitPairing {
    /// Whether any record of this task and state, of any identity and however it
    /// ended, exists at the current move count: the state already ran in this
    /// visit, so its records decide and the files on disk do not.
    ran_this_visit: bool,
    /// The one recordless invocation, by the first of its own names, and the
    /// one orphaned record that answers for it.
    paired: Option<(PathBuf, PathBuf)>,
    /// The orphans an ambiguous edit leaves unpaired. Empty unless there is at
    /// least one orphan and one recordless invocation, and not one of each.
    ambiguous: Vec<PathBuf>,
}

/// Pair at most one orphaned record with at most one invocation that has no
/// successful current-visit record of its own.
///
/// `own_names[i]` is invocation `i`'s list of own names and `own_proven[i]`
/// whether the record under them proves its successful work this visit;
/// `records` are this task's and state's records, matched by field. A removed
/// fan-out member leaves an orphan nothing pairs with, an added one leaves a
/// recordless invocation no orphan answers for, and both pass silently: only
/// a choice between several candidates is ambiguous.
// §FS-rhei-agents.3.2 §FS-rhei-agents.8.4
fn pair_orphaned_record(
    own_names: &[Vec<PathBuf>],
    own_proven: &[bool],
    records: &[(PathBuf, SpawnRecord)],
    task_id: &str,
    state_name: &str,
    moves: u64,
) -> VisitPairing {
    let ran_this_visit = records
        .iter()
        .any(|(_, record)| record.task == task_id && record.state == state_name && record.moves == moves);
    let orphans = records
        .iter()
        .filter(|(path, record)| {
            record.proves_successful_work(task_id, state_name, moves)
                && !own_names.iter().flatten().any(|own| own == path)
        })
        .map(|(path, _)| path.clone())
        .collect::<Vec<_>>();
    let recordless = own_names
        .iter()
        .zip(own_proven)
        .filter(|(_, proven)| !**proven)
        .filter_map(|(names, _)| names.first().cloned())
        .collect::<Vec<_>>();
    match (orphans.as_slice(), recordless.as_slice()) {
        ([orphan], [invocation]) => VisitPairing {
            ran_this_visit,
            paired: Some((invocation.clone(), orphan.clone())),
            ambiguous: Vec::new(),
        },
        ([], _) | (_, []) => VisitPairing { ran_this_visit, ..VisitPairing::default() },
        _ => VisitPairing { ran_this_visit, paired: None, ambiguous: orphans },
    }
}

impl InvocationCompletion<'_> {
    /// An invocation's own names, or `None` when the ledger that numbers them
    /// cannot be read. §FS-rhei-agents.8.1
    fn own_names(&self, resolved: &ResolvedAgent) -> Option<Vec<PathBuf>> {
        let task_id = self.task.id.to_string();
        let number = resolve_log_number(
            self.machine,
            self.state_name,
            self.visit_count,
            self.artifact_root,
            self.runtime_dir,
            &task_id,
        )
        .ok()?;
        let identity = resolved_agent_log_identity(resolved);
        Some(own_spawn_record_names(
            self.runtime_dir,
            &task_id,
            self.state_name,
            identity.as_deref(),
            number,
        ))
    }

    /// The first of an invocation's own names, which the pairing knows it by.
    fn first_own_name(&self, resolved: &ResolvedAgent) -> Option<PathBuf> {
        self.own_names(resolved).and_then(|names| names.into_iter().next())
    }

    /// The visit's pairing, read once per completion and shared by every
    /// invocation asked about. §FS-rhei-agents.8.4
    fn visit_pairing(&self) -> &VisitPairing {
        self.pairing.get_or_init(|| {
            let task_id = self.task.id.to_string();
            let own_names = self
                .invocations
                .iter()
                .map(|resolved| self.own_names(resolved).unwrap_or_default())
                .collect::<Vec<_>>();
            let own_proven = own_names
                .iter()
                .map(|names| {
                    own_spawn_record(names, &task_id, self.state_name, self.moves).is_some_and(|record| {
                        record.proves_successful_work(&task_id, self.state_name, self.moves)
                    })
                })
                .collect::<Vec<_>>();
            let records = spawn_records_for_state(self.runtime_dir, &task_id, self.state_name);
            pair_orphaned_record(
                &own_names,
                &own_proven,
                &records,
                &task_id,
                self.state_name,
                self.moves,
            )
        })
    }

    /// Whether the visit's one orphaned record answers for `resolved`.
    /// §FS-rhei-agents.3.2
    fn paired_with(&self, own_names: &[PathBuf]) -> bool {
        self.visit_pairing()
            .paired
            .as_ref()
            .is_some_and(|(invocation, _)| own_names.first() == Some(invocation))
    }

    /// Say what the pairing decided, at the moment the pass decides to skip or
    /// spawn: one note for an invocation an orphan answers for and that is not
    /// spawned after all, one warning for an ambiguous edit. Each is said once
    /// per run, however many passes reach the same decision.
    // §FS-rhei-agents.3.2 §FS-rhei-agents.8.4
    fn announce_visit_pairing(
        &self,
        sink: &Arc<dyn rhei_tui::EventSink>,
        spawned: &[ResolvedAgent],
    ) {
        let task_id = self.task.id.to_string();
        let state = self.state_name;
        let pairing = self.visit_pairing();
        if let Some((invocation, record)) = &pairing.paired {
            let paired = self
                .invocations
                .iter()
                .find(|resolved| self.first_own_name(resolved).as_ref() == Some(invocation));
            let still_spawned =
                spawned.iter().any(|resolved| self.first_own_name(resolved).as_ref() == Some(invocation));
            if let Some(resolved) = paired.filter(|_| !still_spawned) {
                let text = format!(
                    "note: task {task_id} state '{state}': reusing this visit's finished spawn \
                     ({}) for {}; `rhei reset` redoes it",
                    record.display(),
                    invocation_target_label(resolved)
                );
                self.announce_once(sink, rhei_tui::MessageLevel::Info, text);
            }
        }
        if !pairing.ambiguous.is_empty() {
            let orphans =
                pairing.ambiguous.iter().map(|path| path.display().to_string()).collect::<Vec<_>>();
            let text = format!(
                "warning: task {task_id} state '{state}': this visit's finished spawns ({}) \
                 belong to no current invocation, and more than one could stand for the \
                 invocations without their own, so none is reused and each is spawned",
                orphans.join(", ")
            );
            self.announce_once(sink, rhei_tui::MessageLevel::Warn, text);
        }
    }

    fn announce_once(
        &self,
        sink: &Arc<dyn rhei_tui::EventSink>,
        level: rhei_tui::MessageLevel,
        text: String,
    ) {
        static SAID: std::sync::OnceLock<Mutex<HashSet<String>>> = std::sync::OnceLock::new();
        let key = format!("{}\0{}\0{}\0{text}", self.task.id, self.state_name, self.moves);
        let Ok(mut said) = SAID.get_or_init(|| Mutex::new(HashSet::new())).lock() else {
            return;
        };
        if said.insert(key) {
            emit_run_message(sink, level, text);
        }
    }
}

/// The target an operator wrote for this invocation, as the note names it: the
/// selector, else the model, else the agent. §FS-rhei-agents.3.2
fn invocation_target_label(resolved: &ResolvedAgent) -> String {
    resolved
        .target
        .as_ref()
        .map(ExecutionTarget::selector)
        .or_else(|| resolved.model.clone())
        .unwrap_or_else(|| resolved.agent.id().to_string())
}
