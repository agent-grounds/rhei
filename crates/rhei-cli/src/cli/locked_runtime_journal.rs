// The discipline every project-level, append-only runtime file is written
// under: an exclusive `fs2` hold on a stable sidecar that lives *outside* the
// replaceable runtime tree, the data file opened only after the lock is taken,
// and a partial append truncated back to the length recorded at open.
//
// Its own part because two files are now written this way — the transition
// ledger and the project note store — and it is the discipline that is shared,
// not what a record says. What each file holds stays with the writer next door.

// §AR-source-file-size.3 §FS-rhei-complete.3.1 §FS-rhei-note.3.3

/// Which project-level runtime file a hold is over.
///
/// The sidecar is named after the data file with the separator flattened, so a
/// reader who sees `runtime.notes.md.lock` beside `runtime/` can tell what it
/// guards without knowing this type. §FS-rhei-note.3.3
#[derive(Clone, Copy)]
struct RuntimeJournalKind {
    /// The data file's name under `runtime/`.
    file: &'static str,
    /// The sidecar's name at the execution root, outside `runtime/`.
    lock: &'static str,
    /// How a diagnostic names the file.
    noun: &'static str,
    /// Help for every diagnostic about it.
    help: fn() -> &'static str,
    /// Whether the lock observer the ledger's own tests install watches this
    /// hold. One thread-local observer serves one file; a note appended while a
    /// ledger case is armed must not answer for the ledger's lock.
    #[cfg_attr(not(test), allow(dead_code))]
    observed: bool,
}

/// The central ledger every applied transition appends to. §FS-rhei-complete.3.1
const TRANSITION_LEDGER: RuntimeJournalKind = RuntimeJournalKind {
    file: "state-transitions.log",
    lock: "runtime.state-transitions.log.lock",
    noun: "state transition log",
    help: transition_log_help,
    observed: true,
};

/// The project note store `rhei note` appends to. §FS-rhei-note.3.1
const NOTE_STORE_JOURNAL: RuntimeJournalKind = RuntimeJournalKind {
    file: "notes.md",
    lock: "runtime.notes.md.lock",
    noun: "project note store",
    help: note_store_help,
    observed: false,
};

/// One exclusive hold on a project-level runtime file's stable external
/// sidecar.
///
/// Every writer and reset path over such a file uses this type. The sidecar
/// lives outside the replaceable runtime tree, and the data file is opened only
/// after the lock is acquired, so a hold taken before a reset deletes
/// `runtime/` still excludes every appender afterwards.
/// §AR-agent-orchestrator-workflow.3.3.1 §FS-rhei-next.3.1 §FS-rhei-viz.4
struct LockedRuntimeJournal {
    _lock: fs::File,
    file: Option<fs::File>,
    path: PathBuf,
    original_len: u64,
    created_by_open: bool,
    kind: RuntimeJournalKind,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LedgerLockEvent {
    Contended,
    Acquired,
}

#[cfg(test)]
thread_local! {
    static LEDGER_LOCK_OBSERVER: std::cell::RefCell<Option<mpsc::Sender<LedgerLockEvent>>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn set_ledger_lock_observer(observer: mpsc::Sender<LedgerLockEvent>) {
    LEDGER_LOCK_OBSERVER.with(|installed| *installed.borrow_mut() = Some(observer));
}

#[cfg(test)]
fn notify_ledger_lock_observer(event: LedgerLockEvent) {
    LEDGER_LOCK_OBSERVER.with(|installed| {
        let observer = installed.borrow().clone();
        if let Some(observer) = observer {
            let _ = observer.send(event);
        }
        if event == LedgerLockEvent::Acquired {
            installed.borrow_mut().take();
        }
    });
}

fn lock_runtime_journal(
    file: &fs::File,
    path: &Path,
    kind: RuntimeJournalKind,
) -> MietteResult<()> {
    #[cfg(test)]
    if kind.observed && LEDGER_LOCK_OBSERVER.with(|installed| installed.borrow().is_some()) {
        match file.try_lock_exclusive() {
            Ok(()) => {
                notify_ledger_lock_observer(LedgerLockEvent::Acquired);
                return Ok(());
            }
            Err(err) if lock_is_contended(&err) => {
                notify_ledger_lock_observer(LedgerLockEvent::Contended);
            }
            Err(err) => {
                return Err(file_io_report(
                    path,
                    &format!("failed to lock {}", kind.noun),
                    err,
                ));
            }
        }
    }

    file.lock_exclusive()
        .map_err(|err| file_io_report(path, &format!("failed to lock {}", kind.noun), err))?;
    #[cfg(test)]
    if kind.observed {
        notify_ledger_lock_observer(LedgerLockEvent::Acquired);
    }
    Ok(())
}

impl LockedRuntimeJournal {
    /// Acquire the stable synchronization identity without creating or opening
    /// the replaceable runtime tree. Reset retains this form through cleanup;
    /// appenders open the current data pathname only after this succeeds.
    fn lock(workspace_root: &Path, kind: RuntimeJournalKind) -> MietteResult<Self> {
        let workspace_root = rhei_core::platform::canonical_path(workspace_root).map_err(|err| {
            file_io_report(workspace_root, &format!("failed to resolve {} root", kind.noun), err)
        })?;
        let data_file = workspace_root.join("runtime").join(kind.file);
        let lock_path = workspace_root.join(kind.lock);
        let lock = fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|err| miette!(
                help = (kind.help)(),
                "failed to open {} lock: {err}", kind.noun
            ))?;
        lock_runtime_journal(&lock, &lock_path, kind)?;

        Ok(Self {
            _lock: lock,
            file: None,
            path: data_file,
            original_len: 0,
            created_by_open: false,
            kind,
        })
    }

    fn open(workspace_root: &Path, kind: RuntimeJournalKind) -> MietteResult<Self> {
        let mut locked = Self::lock(workspace_root, kind)?;
        let runtime_dir = locked.path.parent().expect("the journal has a runtime parent");
        fs::create_dir_all(runtime_dir)
            .map_err(|err| miette!(
                help = runtime_dir_help(),
                "failed to create runtime directory: {err}"
            ))?;

        // Plain `write(true)`, not `append(true)`: on Windows an append-mode
        // handle is granted `FILE_APPEND_DATA` but not `FILE_WRITE_DATA`, so
        // `restore`'s `set_len` truncation back to `original_len` fails with
        // access denied. The exclusive `_lock` already serializes every
        // writer, so this handle can safely seek to end itself instead of
        // relying on the kernel's O_APPEND positioning.
        let (mut file, created_by_open) = match fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&locked.path)
        {
            Ok(file) => (file, true),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                let file = fs::OpenOptions::new()
                    .write(true)
                    .open(&locked.path)
                    .map_err(|err| miette!(
                        help = (kind.help)(),
                        "failed to open {}: {err}", kind.noun
                    ))?;
                (file, false)
            }
            Err(err) => {
                return Err(miette!(
                    help = (kind.help)(),
                    "failed to open {}: {err}", kind.noun
                ));
            }
        };

        let original_len = file
            .metadata()
            .map_err(|err| {
                file_io_report(&locked.path, &format!("failed to inspect {}", kind.noun), err)
            })?
            .len();
        file.seek(std::io::SeekFrom::End(0)).map_err(|err| {
            file_io_report(&locked.path, &format!("failed to seek {}", kind.noun), err)
        })?;
        locked.file = Some(file);
        locked.original_len = original_len;
        locked.created_by_open = created_by_open;
        Ok(locked)
    }

    fn path(&self) -> &Path {
        &self.path
    }

    /// The open append handle, for a writer that composes its own record bytes.
    fn handle(&mut self) -> &mut fs::File {
        self.file.as_mut().expect("the journal file is held until drop")
    }

    /// Append one whole line and flush it, under the hold taken at open.
    fn append_line(&mut self, line: &str) -> MietteResult<()> {
        let noun = self.kind.noun;
        let help = self.kind.help;
        let path = self.path.clone();
        let file = self.handle();
        writeln!(file, "{line}").map_err(|err| {
            miette!(help = help(), "failed to write to the {noun}: {err}")
        })?;
        file.flush()
            .map_err(|err| file_io_report(&path, &format!("failed to flush {}", noun), err))
    }

    /// Close the append handle, flushing it, so the data pathname may be
    /// replaced: Windows will not rename over a destination this process still
    /// holds open.
    fn release_handle(&mut self) -> MietteResult<()> {
        if let Some(mut file) = self.file.take() {
            file.flush().map_err(|err| {
                file_io_report(&self.path, &format!("failed to flush {}", self.kind.noun), err)
            })?;
        }
        Ok(())
    }

    /// Truncate this writer's partial append back to the length recorded at
    /// open, and remove the file again when this hold is what created it.
    fn restore(&mut self) -> MietteResult<()> {
        let noun = self.kind.noun;
        let path = self.path.clone();
        let original_len = self.original_len;
        let file = self.handle();
        file.set_len(original_len)
            .map_err(|err| file_io_report(&path, &format!("failed to restore {noun}"), err))?;
        file.seek(std::io::SeekFrom::End(0))
            .map_err(|err| file_io_report(&path, &format!("failed to seek restored {noun}"), err))?;
        file.flush()
            .map_err(|err| file_io_report(&path, &format!("failed to flush restored {noun}"), err))?;
        if self.created_by_open {
            self.file.take();
            fs::remove_file(&path)
                .map_err(|err| file_io_report(&path, &format!("failed to remove {noun}"), err))?;
        }
        Ok(())
    }
}
