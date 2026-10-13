// Shared grant/charge transactions. §FS-rhei-agents.8.4 §FS-rhei-retry.2

impl SpawnPlan {
    /// Record this spawn, now that it has ended.
    ///
    /// Called from the two places a subprocess is waited on, after the footer
    /// its log gets, and from nowhere a spawn can fail to start — the record's
    /// whole value is that its presence proves a worker ran.
    // §FS-rhei-agents.8.4
    fn record_spawn(&self, ended: SpawnEnding<'_>) {
        if let Some(parent) = self.record.parent() {
            if fs::create_dir_all(parent).is_err() {
                return;
            }
        }
        let attempt_charged = !matches!(ended.ending, "interrupted" | "provider_limited");
        let mut record = SpawnRecord {
            task: ended.task_id.to_string(),
            state: ended.state_name.to_string(),
            moves: self.moves,
            attempt: self.attempt,
            // An interrupted invocation is not an attempt the ticket spent: the
            // run ended it and the next one re-executes it. It keeps its
            // attempt log all the same. §FS-rhei-run.3.2 §FS-rhei-agents.3.2.3
            charged: self.charged + u64::from(attempt_charged),
            grants: Vec::new(),
            attempt_charged,
            kind: ended.kind.to_string(),
            worker: ended.worker.to_string(),
            log: self.log.clone(),
            started: ended.started.to_string(),
            ended: ended.ended.to_string(),
            duration: ended.duration.to_string(),
            code: ended.code,
            ending: ended.ending.to_string(),
            reverted: None,
        };
        // §FS-rhei-agents.8.4: re-read under the grant's lock, never overwrite its audit.
        let _ = update_spawn_record_with_policy(&self.record, true, |stored| {
            if let Some(stored) = stored.filter(|stored| stored.moves == self.moves) {
                record.charged = stored.charged + u64::from(attempt_charged);
                record.grants = stored.grants;
            } else if let Some(previous) = &self.previous {
                // Legacy unsuffixed records may supply this entry's previous attempt.
                record.grants = previous.grants.clone();
            }
            Ok(Some(record))
        });
    }
}

/// The facts a finished spawn records about itself. §FS-rhei-agents.8.4
struct SpawnEnding<'a> {
    task_id: &'a str,
    state_name: &'a str,
    kind: &'a str,
    worker: &'a str,
    started: &'a str,
    ended: &'a str,
    duration: &'a str,
    code: Option<i32>,
    ending: &'a str,
}

/// One operator refill of the current visit. §FS-rhei-agents.8.4
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct AttemptGrant {
    at: String,
    os_user: String,
    attempts: u64,
}

impl SpawnRecord {
    /// Grants never cross the record's move-count visit boundary. §FS-rhei-agents.3.2.3
    fn allowance(&self, configured: u64) -> u64 {
        self.grants.iter().fold(configured, |total, grant| total.saturating_add(grant.attempts))
    }
}

impl SpawnPlan {
    /// Only the previous record of this visit can grant its next attempt. §FS-rhei-agents.3.2.3
    fn effective_allowance(&self, configured: u64) -> u64 {
        self.previous.as_ref().map_or(configured, |record| record.allowance(configured))
    }

    /// Polling keeps its independent bound. §FS-rhei-retry.2
    fn effective_budget(&self, budget: AttemptBudget) -> AttemptBudget {
        match budget {
            AttemptBudget::Visit(configured) => {
                AttemptBudget::Visit(self.effective_allowance(configured))
            }
            poll => poll,
        }
    }
}

/// Keep stable sidecars outside the record directory: its entries are JSON records,
/// and replacing a record cannot replace its lock identity. §FS-rhei-agents.8.4
fn lock_spawn_record(path: &Path) -> MietteResult<LockedPlanFile> {
    let parent = path.parent().ok_or_else(|| diagnostic!("spawn record has no directory"))?;
    let root = if parent.file_name() == Some(OsStr::new("spawns")) {
        parent.parent().and_then(Path::parent).unwrap_or(parent)
    } else {
        parent
    };
    let locks = root.join(".rhei").join("spawn-locks");
    fs::create_dir_all(&locks)
        .map_err(|err| file_io_report(&locks, "cannot create spawn locks", err))?;
    let name = path.file_name().ok_or_else(|| diagnostic!("spawn record has no filename"))?;
    LockedPlanFile::open(&locks.join(name))
}

/// Re-read, modify, stage and rename while holding the same stable lock for
/// every record mutation. Readers see only complete snapshots. §FS-rhei-agents.8.4
fn update_spawn_record(
    path: &Path,
    update: impl FnOnce(Option<SpawnRecord>) -> MietteResult<Option<SpawnRecord>>,
) -> MietteResult<Option<SpawnRecord>> {
    update_spawn_record_with_policy(path, false, update)
}

/// Only a completed charge can replace malformed prior JSON; a grant cannot
/// infer exhaustion from it. Valid records always merge under the lock. §FS-rhei-agents.8.4
fn update_spawn_record_with_policy(
    path: &Path,
    replace_malformed: bool,
    update: impl FnOnce(Option<SpawnRecord>) -> MietteResult<Option<SpawnRecord>>,
) -> MietteResult<Option<SpawnRecord>> {
    let lock = lock_spawn_record(path)?;
    let stored = match fs::read_to_string(path) {
        Ok(raw) => match serde_json::from_str(&raw) {
            Ok(record) => Some(record),
            Err(_) if replace_malformed => None,
            Err(err) => {
                return Err(diagnostic!("cannot parse spawn record {}: {err}", path.display()))
            }
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => return Err(file_io_report(path, "cannot read spawn record", err)),
    };
    let updated = update(stored)?;
    if let Some(record) = &updated {
        let body = serde_json::to_string_pretty(record)
            .map_err(|err| diagnostic!("cannot serialize spawn record: {err}"))?;
        write_spawn_record_atomic(path, &body, &lock)?;
    }
    Ok(updated)
}

/// Stage the whole record beside its destination and rename while the shared
/// charge/grant lock remains held. §FS-rhei-agents.8.4
fn write_spawn_record_atomic(path: &Path, body: &str, _lock: &LockedPlanFile) -> MietteResult<()> {
    let parent = path.parent().ok_or_else(|| diagnostic!("spawn record has no directory"))?;
    let mut staged = tempfile::NamedTempFile::new_in(parent)
        .map_err(|err| file_io_report(path, "cannot stage spawn record", err))?;
    staged
        .write_all(body.as_bytes())
        .map_err(|err| file_io_report(path, "cannot write staged spawn record", err))?;
    #[cfg(test)]
    spawn_record_before_replace(path, staged.path());
    persist_locked(staged, path)
        .map_err(|err| file_io_report(path, "cannot replace spawn record", err.error))
}

// A per-thread barrier observes complete snapshots at replacement. §FS-rhei-agents.8.4
#[cfg(test)]
thread_local! {
    static SPAWN_RECORD_REPLACE_OBSERVER: std::cell::RefCell<Option<SpawnRecordReplaceObserver>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
type SpawnRecordReplaceObserver = Box<dyn FnOnce(&Path, &Path)>;

#[cfg(test)]
fn spawn_record_before_replace(path: &Path, staged: &Path) {
    let observer = SPAWN_RECORD_REPLACE_OBSERVER.with(|slot| slot.borrow_mut().take());
    if let Some(observer) = observer {
        observer(path, staged);
    }
}
