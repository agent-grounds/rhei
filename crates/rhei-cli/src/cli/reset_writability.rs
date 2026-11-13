// Whether the filesystem would let a reset finish: asked before any preview, so the
// dry run and the reset refuse alike, and how a reset stopped later says so.
//
// Its own part because the question covers the whole write sequence, and each step
// of that sequence lives with its writer. It asks through the same enumerations
// those writers walk, so the check and the writes cannot disagree on what is written.

// §AR-source-file-size.3 §FS-rhei-reset.2.3 §FS-rhei-reset.4

/// What a reset the filesystem would stop says it cannot do. One phrase, so the
/// renderer can keep its lines whole. §FS-rhei-reset.4
const RESET_CANNOT_WRITE: &str = "it cannot write everywhere it needs to";

/// What the help adds where a refusal line names a lock file: the file is the matter,
/// and the directory it sits in may be fine. §FS-rhei-reset.4
const RESET_LOCK_FILE_HELP: &str = "A lock file named here has to open for writing itself.";

/// What a reset stopped after something landed adds to the error it has. §FS-rhei-reset.4
const RESET_STOPPED_PART_WAY: &str = "The reset stopped part way, so the plan is partly reset: \
     fix the cause and run the reset again, which finishes what this one left.";

/// The directories a reset has asked, and the ones that said no with what they said,
/// beside the lock files that did. Keyed by canonical path, so one is asked and named
/// once, in one spelling. §FS-rhei-reset.2.3 §FS-rhei-reset.4
#[derive(Default)]
struct ResetDirectoryCheck {
    asked: BTreeSet<PathBuf>,
    failures: BTreeMap<PathBuf, String>,
    lock_file_failed: bool,
}

impl ResetDirectoryCheck {
    /// Ask `dir` by doing what a write does, creating a temp file and dropping it.
    fn write_in(&mut self, dir: &Path) {
        let dir = rhei_core::platform::canonical_path(dir).unwrap_or_else(|_| dir.to_path_buf());
        if !self.asked.insert(dir.clone()) {
            return;
        }
        if let Err(err) = tempfile::NamedTempFile::new_in(&dir) {
            // Its text ends in the random name it tried; the two runs must print one text.
            let text = err.to_string();
            let os_error = text.split(" at path ").next().unwrap_or(&text).to_string();
            self.failures.entry(dir).or_insert(os_error);
        }
    }

    /// `dir` could not be listed, under the key `write_in` would give it.
    fn unlistable(&mut self, dir: &Path, err: std::io::Error) {
        let dir = rhei_core::platform::canonical_path(dir).unwrap_or_else(|_| dir.to_path_buf());
        self.failures.entry(dir).or_insert(err.to_string());
    }

    /// The entry at `path` is made or removed, which asks the directory holding it.
    fn entry(&mut self, path: &Path) {
        self.write_in(path.parent().filter(|dir| !dir.as_os_str().is_empty()).unwrap_or(Path::new(".")));
    }

    /// `path` is removed, with everything under it. A symlink is one entry, not a tree.
    fn removal(&mut self, path: &Path) {
        self.entry(path);
        if fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir()) {
            self.tree(path);
        }
    }

    /// Every directory of a tree has to be listable, and writable where it holds an entry.
    fn tree(&mut self, dir: &Path) {
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(err) => {
                self.unlistable(dir, err);
                return;
            }
        };
        let mut holds_an_entry = false;
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    self.unlistable(dir, err);
                    continue;
                }
            };
            holds_an_entry = true;
            // The entry's own type, so a symlink is not followed out of the tree.
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                self.tree(&entry.path());
            }
        }
        if holds_an_entry {
            self.write_in(dir);
        }
    }

    /// The lock sidecars `ResetWriterLocks::acquire` opens. An absent one is an entry the
    /// reset would make. One already there is opened as the lock opens it, which takes no
    /// lock and truncates nothing, and a lock held on another handle stays held.
    fn sidecars(
        &mut self,
        loaded: &LoadedPlan,
        input: &Path,
        scope: ResetScope<'_>,
    ) -> MietteResult<()> {
        let (metadata_paths, task_paths) = reset_plan_lock_paths(loaded, input, scope)?;
        let mut sidecars = Vec::new();
        for path in metadata_paths.iter().chain(&task_paths) {
            sidecars.push(plan_lock_path(path)?);
        }
        for root in reset_ledger_roots(loaded, input, scope)? {
            sidecars.push(root.join(TRANSITION_LEDGER.lock));
        }
        for sidecar in &sidecars {
            if !sidecar.exists() {
                self.entry(sidecar);
            } else if let Err(err) = open_lock_sidecar(sidecar) {
                self.failures.entry(sidecar.clone()).or_insert(err.to_string());
                self.lock_file_failed = true;
            }
        }
        Ok(())
    }

    /// What a narrowed reset would remove, in the order it removes it. §FS-rhei-reset.2.1
    fn scoped_removals(&mut self, removals: &ScopedRemovals) -> MietteResult<()> {
        for target in &removals.targets {
            match target {
                ScopedTarget::Exact(path) if path.exists() => self.removal(path),
                ScopedTarget::Exact(_) => {}
                ScopedTarget::Prefixed { dir, prefix } => match removals.owners.entries(dir, prefix) {
                    // Only what the removal removes: it skips a dangling link.
                    Ok(paths) => paths.iter().filter(|path| path.exists()).for_each(|path| self.removal(path)),
                    Err(err) => self.unlistable(dir, err),
                },
                ScopedTarget::Pattern(pattern) => match removals.owners.pattern_entries(pattern) {
                    Ok(paths) => paths.iter().filter(|path| path.exists()).for_each(|path| self.removal(path)),
                    Err((dir, err)) => self.unlistable(&dir, err),
                },
            }
        }
        for (root, ids) in &removals.ledgers {
            let ledger = canonical_reset_root(root)?.join("runtime").join(TRANSITION_LEDGER.file);
            // An unreadable ledger is `collect_reset_decision`'s to refuse, before this.
            let Ok(raw) = read_transition_ledger(&ledger) else { continue };
            if raw.lines().any(|line| !ledger_line_is_kept(line, ids)) {
                self.entry(&ledger);
            }
        }
        Ok(())
    }

    /// The refusal as §FS-rhei-reset.4 prints it, the same with or without `--dry-run`.
    /// A path keeps the form its target was typed in.
    fn refusal(&self, input: &Path) -> Option<Report> {
        if self.failures.is_empty() {
            return None;
        }
        let mut lines = vec![format!("Refusing to reset: {RESET_CANNOT_WRITE}, so nothing was changed.")];
        for (dir, error) in &self.failures {
            let shown = if input.is_absolute() { dir.display().to_string() } else { display_path(dir) };
            lines.push(format!("  {shown}: {error}"));
        }
        let help = if self.lock_file_failed {
            format!("{} {RESET_LOCK_FILE_HELP}", temp_write_help())
        } else {
            temp_write_help().to_string()
        };
        Some(miette!(help = help, "{}", lines.join("\n")))
    }
}

/// Whether a lock sidecar the reset opens cannot be opened. The real reset asks this
/// before it takes its locks, because the failure would stop it there, without the text
/// the dry run prints. §FS-rhei-reset.2.3
fn reset_sidecars_are_blocked(
    loaded: &LoadedPlan,
    input: &Path,
    scope: ResetScope<'_>,
) -> MietteResult<bool> {
    let mut check = ResetDirectoryCheck::default();
    check.sidecars(loaded, input, scope)?;
    Ok(!check.failures.is_empty())
}

/// The refusal of a reset the filesystem would stop, or `None` when every directory it
/// has to write in or remove from says yes. A directory it has nothing to do in is not
/// asked. §FS-rhei-reset.2.3
fn reset_filesystem_refusal(
    loaded: &LoadedPlan,
    input: &Path,
    scope: ResetScope<'_>,
    machines: &rhei_validator::MachineSet,
    authored: &AuthoredStates,
) -> MietteResult<Option<Report>> {
    let mut check = ResetDirectoryCheck::default();
    check.sidecars(loaded, input, scope)?;

    let no_moves = BTreeMap::new();
    for (file, _sample_task_id) in reset_target_files(loaded, input, scope) {
        let marked = read_plan_source(&file, "failed to read plan file")?;
        // Scanned after the mark, as the locked rewrite will. §REQ-cross-platform.9
        let (_, raw) = split_byte_order_mark(&marked);
        let moves = authored.by_file.get(&file).unwrap_or(&no_moves);
        let under = reset_strips_under(scope, moves);
        if reset_file_text(raw, moves, loaded.document_role(&file), under)? != raw {
            check.entry(&file);
        }
    }
    for (file, edit) in reset_metadata_targets(loaded, input, scope, authored) {
        let marked = read_plan_source(&file, "failed to read metadata document")?;
        let (_, raw) = split_byte_order_mark(&marked);
        if reset_metadata_text(&file, raw, &edit)? != raw {
            check.entry(&file);
        }
    }

    if scope.is_narrowed() {
        check.scoped_removals(&scoped_removals(loaded, input, scope, machines))?;
    } else {
        for tree in reset_runtime_preview(loaded, input, scope) {
            check.removal(&tree);
        }
    }
    Ok(check.refusal(input))
}

/// What of a reset's writes has landed, so a failure can say what is true of the plan.
/// §FS-rhei-reset.4
#[derive(Default)]
struct ResetProgress {
    /// A plan file written, a ledger pruned, or a tree removed.
    landed: bool,
    /// Every file removed, for the reset event too. §FS-rhei-evidence.10.1
    removed_paths: Vec<PathBuf>,
}

impl ResetProgress {
    /// The error `err`, adding that the plan is partly reset once something has landed.
    /// A first write that fails has landed nothing, so it stays as it is. §FS-rhei-reset.4
    fn stopped(&self, err: Report) -> Report {
        if !self.landed && self.removed_paths.is_empty() {
            return err;
        }
        let help = err.help().map_or_else(|| temp_write_help().to_string(), |help| help.to_string());
        miette!(help = help, "{err}\n{RESET_STOPPED_PART_WAY}")
    }
}

/// Write the plan files the decision changes, then remove what it removes, recording in
/// `progress` what has landed. Returns whether any runtime output was removed.
/// §FS-rhei-reset.2 §FS-rhei-reset.2.3
fn apply_reset(
    loaded: &LoadedPlan,
    input: &Path,
    decision: &ResetDecision,
    machines: &rhei_validator::MachineSet,
    reset_locks: &mut ResetWriterLocks,
    progress: &mut ResetProgress,
) -> MietteResult<bool> {
    // Each plan file's tasks return to the states *that file* authored them
    // in; a file whose tasks never moved still has its runtime lines
    // (assignee, result links) stripped. §FS-rhei-reset.2.2
    let no_moves: BTreeMap<String, String> = BTreeMap::new();
    let scope = decision.reset_scope();
    for (file, _sample_task_id) in reset_target_files(loaded, input, scope) {
        let file_authored = decision.authored.by_file.get(&file).unwrap_or(&no_moves);
        let role = loaded.document_role(&file);
        progress.landed |= reset_plan_file_states(&file, role, file_authored, scope, reset_locks.plan(&file)?)?;
    }
    // Clear selected live tickets in their owning documents, including the
    // basin manifest and tickets that never moved, and hold the supervisors a
    // ticket scope leaves out. §FS-rhei-reset.2 §FS-rhei-reset.2.5.3
    for (path, edit) in reset_metadata_targets(loaded, input, scope, &decision.authored) {
        progress.landed |= clear_runtime_metadata_in_file(&path, &edit, reset_locks.plan(&path)?)?;
    }

    // §FS-rhei-panta.6.4: a narrowed reset removes per-ticket artifacts, never
    // whole `runtime/` trees — sibling rheis share one execution root.
    if scope.is_narrowed() {
        return remove_scoped_runtime_artifacts(
            loaded,
            input,
            scope,
            machines,
            reset_locks,
            progress,
        );
    }

    let mut removed_runtime = false;
    // Delete exactly the runtime directories named in the confirmed preview.
    // A cooperating writer cannot create another one until these locks leave.
    // §FS-rhei-reset.1.2 §FS-rhei-reset.4
    for runtime_dir in &decision.runtime_targets {
        if runtime_dir.exists() {
            remove_runtime_path(runtime_dir, &mut progress.removed_paths)?;
            progress.landed = true;
            removed_runtime = true;
        }
    }
    Ok(removed_runtime)
}
