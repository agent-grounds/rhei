// The lock a create holds, and the write it makes under it.
//
// Its own part because serializing a create is a concurrency concern that knows
// nothing about ids, markdown, or state machines: it only knows which file
// stands for the scope being written to.

// §FS-rhei-new.4

/// An exclusive lock held for the length of one create.
///
/// Held from before the first plan load through the write, both validation
/// passes, and any rollback. Anything narrower still loses tickets: the sibling
/// numbering is read from the same file the write then modifies, and a rollback
/// restores a snapshot taken before it. `Drop` releases it on every exit path,
/// and the OS releases it if the process dies — a lock nobody holds cannot go
/// stale, so there is nothing to reap.
///
/// The lock is the same [`LockedPlanFile`] every other rewriting command takes,
/// so a create reads and replaces the file it locked the way they do. That is
/// not a tidiness: on Windows the lock is a mandatory byte range, and a create
/// with a lock of its own devising is refused its own read and its own rename.
// §FS-rhei-new.4
struct NewCreateLock {
    locked: LockedPlanFile,
}

impl NewCreateLock {
    /// The path this lock was taken on, so a second lock can tell whether it
    /// would be locking the same file twice from the same process.
    fn path(&self) -> &Path {
        &self.locked.path
    }
}

impl Drop for NewCreateLock {
    fn drop(&mut self) {
        self.locked.release();
    }
}

/// Take the create lock for `target`, blocking until it is free.
///
/// Blocking rather than failing, like every other plan-file lock rhei takes: a
/// create that gave up on a busy project would hand the caller back exactly the
/// race the lock exists to remove.
// §FS-rhei-new.4
fn lock_new_create(target: &Path) -> MietteResult<NewCreateLock> {
    lock_plan_path(&new_create_lock_path(target))
}

/// Take the destination plan file's own lock, on top of the scope lock.
///
/// The scope lock makes sibling numbering safe, but no other command takes it:
/// `rhei complete`, `rhei transition`, `rhei reset`, and `rhei run` all lock the
/// *plan file* they rewrite, so a create holding only the scope lock serializes
/// against other creates and against nothing else — and a create that reads,
/// splices, and writes a whole file while a completion rewrites a `**State:**`
/// line in it silently drops the completion.
///
/// Always taken second, so the two locks are only ever acquired scope-first and
/// no cycle is possible. `None` in the two cases where taking it would be wrong
/// rather than merely unnecessary:
///
/// - the destination *is* the scope file (a lone plan), and `fs2` does not
///   define what a second exclusive lock on the same file from the same process
///   does;
/// - the destination does not exist yet, so it holds no ticket any other
///   command could be rewriting, and the scope lock already serializes the
///   creates that might race to make it.
// §FS-rhei-new.4
fn lock_new_destination(
    scope: &NewCreateLock,
    destination: &Path,
) -> MietteResult<Option<NewCreateLock>> {
    if !destination.is_file() || same_path(scope.path(), destination) {
        return Ok(None);
    }
    lock_plan_path(destination).map(Some)
}

/// Take an exclusive lock on one existing file.
// §FS-rhei-new.4
fn lock_plan_path(path: &Path) -> MietteResult<NewCreateLock> {
    LockedPlanFile::open(path).map(|locked| NewCreateLock { locked })
}

/// The two locks a create holds while it decides, writes, and verifies: the
/// scope lock always, and the destination's own lock when the destination is a
/// different file. Together they answer the only question the write has to ask
/// of them — *do I hold a lock on this path* — which on Windows decides
/// whether a read and a rename go through the handle or the path.
// §FS-rhei-new.4
struct NewCreateLocks<'a> {
    scope: &'a NewCreateLock,
    destination: Option<&'a NewCreateLock>,
}

impl NewCreateLocks<'_> {
    /// The lock this create holds on `path`, when it holds one.
    fn covering(&self, path: &Path) -> Option<&LockedPlanFile> {
        if let Some(destination) = self.destination {
            if same_path(destination.path(), path) {
                return Some(&destination.locked);
            }
        }
        same_path(self.scope.path(), path).then_some(&self.scope.locked)
    }

    /// Read `path` the way this create's own locks allow.
    ///
    /// A plain `fs::read` is the whole Windows failure: the lock there is a
    /// mandatory byte range, so a create is refused its own read of the file it
    /// just locked. The witnessed content then comes back empty where it had
    /// content, the comparison reads that as somebody else rewriting the plan,
    /// and the create gives up with a diagnostic naming a race that never
    /// happened.
    // §FS-rhei-new.4
    fn read(&self, path: &Path) -> Option<String> {
        match self.covering(path) {
            Some(locked) => locked.read_to_string("failed to read plan file").ok(),
            None => fs::read_to_string(path).ok(),
        }
    }
}

/// The file that stands for the whole create scope: the project manifest for a
/// project, the index for a bare workspace, the plan file itself for a lone
/// plan. Each already exists wherever a create is legal, so its persistent
/// sibling sidecar has one stable identity before any destination is chosen.
// §FS-rhei-new.4
fn new_create_lock_path(target: &Path) -> PathBuf {
    if let Some(project_dir) = workspace::panta_project_dir(target) {
        return project_dir.join(workspace::PANTA_INDEX_FILE);
    }
    if let Some(workspace_dir) = workspace::workspace_dir(target) {
        return workspace_dir.join("index.rhei.md");
    }
    target.to_path_buf()
}

/// Write a plan file, replacing an existing one through a temp file in its own
/// directory so an interrupted create cannot leave a truncated plan where a
/// whole one was.
///
/// A file that does not exist yet is written directly: there is no previous
/// content a partial write could destroy, a failed create removes it whole
/// anyway, and a plain write gives it the mode a plain create would.
///
/// The lock the create holds on `path`, when it holds one, is handed to
/// [`persist_locked`]: on Windows a rename over a file this process has locked
/// is refused, and the lone-plan case — where the destination *is* the scope
/// file — is refused its own write without it.
// §FS-rhei-new.4 §FS-rhei-new.5.1
fn write_plan_file_atomically(
    path: &Path,
    contents: &str,
    locked: Option<&LockedPlanFile>,
) -> MietteResult<()> {
    let Ok(existing) = fs::metadata(path) else {
        return fs::write(path, contents)
            .map_err(|err| file_io_report(path, "failed to write", err));
    };
    let parent = path.parent().unwrap_or(Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent).map_err(|err| {
        miette!(help = temp_write_help(), "failed to create temp file next to {}: {err}",
            display_path(path))
    })?;
    tmp.write_all(contents.as_bytes())
        .map_err(|err| miette!(help = temp_write_help(), "failed to write temp file: {err}"))?;
    // The replacement is the same file to its author, so it keeps the file's
    // own permissions rather than a temp file's private ones.
    let _ = tmp.as_file().set_permissions(existing.permissions());
    persist_locked(tmp, path, locked).map_err(|err| {
        miette!(help = temp_write_help(), "failed to persist {}: {err}", display_path(path))
    })?;
    Ok(())
}
