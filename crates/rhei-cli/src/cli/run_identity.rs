// One collision-checked identity, selected while all execution roots are locked.
// §FS-rhei-run.2.7

/// One platform-normalized key for locked roots and retained registry workspaces.
/// Preserve the lock-root fallback, including the empty relative parent. §FS-rhei-run.2.7
fn run_root_key(root: &Path) -> PathBuf {
    let root = if root.as_os_str().is_empty() { Path::new(".") } else { root };
    rhei_core::platform::canonical_path(root).unwrap_or_else(|_| root.to_path_buf())
}

/// One run's identity, shared by every publication and execution mode.
/// §FS-rhei-run.2.7
struct RunIdentity {
    id: String,
    started: Instant,
    started_wall: std::time::SystemTime,
    headless: bool,
    /// Shared across execution modes and project roots. §FS-rhei-budgets.7.2
    budget: BudgetRun,
    /// The build, and the checkout HEAD once the run holds its locks.
    /// §FS-rhei-run-headless.2
    provenance: rhei_tui::RunProvenance,
}

impl RunIdentity {
    /// The initial candidate and start, also used by artifact-free dry runs.
    /// §FS-rhei-run.2.7 §FS-rhei-budgets.7.2
    fn new() -> Self {
        let started_wall = std::time::SystemTime::now();
        #[cfg(test)]
        let started_wall = RUN_IDENTITY_TEST_WALL.with(|wall| wall.get().unwrap_or(started_wall));
        Self {
            id: short_run_id(started_wall),
            started: Instant::now(),
            started_wall,
            headless: is_headless_child(),
            budget: begin_budget_run(),
            provenance: rhei_tui::RunProvenance {
                rhei_build: rhei_build_record(),
                checkout_head_at_start: None,
            },
        }
    }

    /// Check history under the caller's held run locks before naming any artifact.
    /// Linear probing visits the complete 24-bit space even with a frozen clock.
    /// §FS-rhei-run.2.7
    fn allocate(roots: &BTreeSet<PathBuf>) -> MietteResult<Self> {
        let mut identity = Self::new();
        let history = RunIdentityHistory::read(roots)?;
        let initial = u32::from_str_radix(&identity.id, 16).expect("six hex candidate");
        for offset in 0..=0xff_ffff {
            let candidate = format!("{:06x}", initial.wrapping_add(offset) & 0xff_ffff);
            if !history.ids.contains(&candidate) {
                identity.id = candidate;
                return Ok(identity);
            }
        }
        Err(miette!(
            help = "retain the existing history and use a different execution workspace",
            "cannot allocate a run id: all six-digit hexadecimal ids are already reserved"
        ))
    }
}

/// Reservations and independent run starts; invocation start times are not run starts.
/// §FS-rhei-run.2.7 §FS-rhei-cost-accounting.11
#[derive(Default)]
struct RunIdentityHistory {
    ids: BTreeSet<String>,
    starts: BTreeMap<String, BTreeSet<std::time::SystemTime>>,
}

impl RunIdentityHistory {
    /// Read all records, including tasks outside the current cost selection.
    /// §FS-rhei-run.2.7
    fn read(roots: &BTreeSet<PathBuf>) -> MietteResult<Self> {
        let mut history = Self::read_run_evidence(roots)?;
        for root in roots {
            let (records, errors, _) = read_accounting_root(&root.join(ACCOUNTING_DIR));
            if let Some(error) = errors.first() {
                return Err(miette!(
                    help = "repair the unreadable invocation record before starting another run",
                    "cannot check reserved run ids: {error}"
                ));
            }
            for (_, record) in records {
                if let Some(id) = record.run_id.as_deref() {
                    history.ids.insert(id.to_string());
                }
                if let Some((id, _, _)) = invocation_attempt_parts(&record.invocation_id) {
                    history.ids.insert(id.to_string());
                }
            }
        }
        Ok(history)
    }

    /// Reports and descriptors reserve identities independently of accounting.
    /// Only their run starts can establish historical reuse. §FS-rhei-run.2.7 §FS-rhei-cost-accounting.11
    fn read_run_evidence(roots: &BTreeSet<PathBuf>) -> MietteResult<Self> {
        let mut history = Self::default();
        for root in roots {
            let runtime = root.join("runtime");
            for path in run_history_report_paths(&runtime.join("run-reports"))? {
                if let Some(id) = path
                    .file_stem()
                    .and_then(OsStr::to_str)
                    .and_then(|stem| stem.rsplit_once('-').map(|(_, id)| id))
                    .filter(|id| is_short_run_id(id))
                {
                    history.ids.insert(id.to_string());
                }
                history.read_report(&path)?;
            }
            history.read_report(&runtime.join("run-report.md"))?;
            // A directory or malformed descriptor contains no readable identity;
            // publication remains best effort. §FS-rhei-run.2.7
            if let Some(descriptor) = read_descriptor(&run_descriptor_path(root)) {
                history.descriptor(&descriptor);
            }
        }
        // Read retained external descriptors without sweeping or touching unrelated roots.
        // Registry availability remains best effort. §FS-rhei-run.2.7
        if let Some(entries) = run_registry_dir().and_then(|dir| fs::read_dir(dir).ok()) {
            let root_keys: BTreeSet<_> = roots.iter().map(|root| run_root_key(root)).collect();
            for entry in entries.flatten() {
                if let Some(descriptor) = read_descriptor(&entry.path()) {
                    if root_keys.contains(&run_root_key(&descriptor.workspace)) {
                        history.descriptor(&descriptor);
                    }
                }
            }
        }
        Ok(history)
    }

    /// Retain a descriptor's id even when its start cannot be parsed.
    /// §FS-rhei-run.2.7 §FS-rhei-cost-accounting.11
    fn descriptor(&mut self, descriptor: &RunDescriptor) {
        self.reserve(&descriptor.id, Some(&descriptor.started_at));
    }

    /// The existing report header names both the identity and its start.
    /// §FS-rhei-run.2.7 §FS-rhei-cost-accounting.11
    fn read_report(&mut self, path: &Path) -> MietteResult<()> {
        let body = match fs::read_to_string(path) {
            Ok(body) => body,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(file_io_report(path, "cannot read reserved run identity", err)),
        };
        if let Some(value) = body.lines().find_map(|line| line.strip_prefix("Run: ")) {
            match value.rsplit_once(" / ") {
                Some((started, id)) => self.reserve(id, Some(started)),
                None => self.reserve(value, None),
            }
        }
        Ok(())
    }

    /// Equal timestamps in duplicate artifacts remain evidence of just one run.
    /// §FS-rhei-run.2.7 §FS-rhei-cost-accounting.11
    fn reserve(&mut self, id: &str, started: Option<&str>) {
        if !is_short_run_id(id) {
            return;
        }
        self.ids.insert(id.to_string());
        if let Some(started) = started.and_then(rhei_tui::parse_rfc3339) {
            self.starts.entry(id.to_string()).or_default().insert(started);
        }
    }
}

/// The public six-digit lowercase hexadecimal shape. §FS-rhei-run.2.7
fn is_short_run_id(id: &str) -> bool {
    id.len() == 6 && id.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Missing history is empty; an unreadable directory cannot establish absence.
/// §FS-rhei-run.2.7
fn run_history_report_paths(dir: &Path) -> MietteResult<Vec<PathBuf>> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(file_io_report(dir, "cannot inspect reserved run ids", err)),
    };
    let mut paths = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|err| file_io_report(dir, "cannot inspect reserved run ids", err))?
            .path();
        if path.extension().and_then(OsStr::to_str) == Some("md") {
            paths.push(path);
        }
    }
    Ok(paths)
}

/// A newly admitted root must not reinterpret an already published run id.
/// Refuse rather than renaming the running invocation. §FS-rhei-run.2.7
fn validate_admitted_run_identity(
    roots: &BTreeSet<PathBuf>,
    identity: &RunIdentity,
) -> MietteResult<()> {
    if RunIdentityHistory::read(roots)?.ids.contains(&identity.id) {
        return Err(miette!(
            help = "retry the run so one unused id can be selected across all current roots",
            "new execution root already reserves this run id '{}'",
            identity.id
        ));
    }
    Ok(())
}
