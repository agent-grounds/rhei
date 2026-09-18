// The machine-wide registry of runs: `$XDG_STATE_HOME/rhei/runs/<id>.json`.
//
// Entries exist to map a bare id to a workspace; the workspace's own
// descriptor is the authoritative copy. An entry outlives its run, because the
// question `rhei attach <id>` answers after a run ends — "how did it go?" — is
// the one most worth answering.

// §FS-rhei-run-headless.2 §FS-rhei-run-headless.3 §FS-rhei-run-headless.6

/// How many ended runs the registry keeps. Retention is what makes a run id
/// resolvable after the fact; a cap is what keeps that from becoming an
/// unbounded directory on a machine that runs a plan every minute.
// §FS-rhei-run-headless.2
const RETAINED_ENDED_RUNS: usize = 100;

/// The machine-wide registry directory of runs.
/// `$XDG_STATE_HOME/rhei/runs`, falling back to `~/.local/state/rhei/runs`.
// §FS-rhei-run-headless.2
pub(crate) fn run_registry_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))?;
    Some(base.join("rhei").join("runs"))
}

pub(crate) fn run_registry_path(id: &str) -> Option<PathBuf> {
    Some(run_registry_dir()?.join(format!("{id}.json")))
}

/// A registry entry the sweep could not decide about — an unreadable workspace,
/// a lock it could not probe, an entry a newer `rhei` wrote in a shape this
/// build does not understand.
// §FS-rhei-run-headless.3
pub(crate) struct UndecidedRun {
    pub(crate) path: PathBuf,
    pub(crate) descriptor: Option<RunDescriptor>,
    pub(crate) reason: String,
}

impl UndecidedRun {
    pub(crate) fn summary_line(&self) -> String {
        match &self.descriptor {
            Some(descriptor) => descriptor.summary_line(),
            None => format!("(unreadable entry)  {}", self.path.display()),
        }
    }
}

/// What one pass over the registry found.
#[derive(Default)]
pub(crate) struct RegistrySweep {
    pub(crate) live: Vec<RunDescriptor>,
    /// Runs that are over but whose workspace still names them.
    pub(crate) ended: Vec<RunDescriptor>,
    pub(crate) undecided: Vec<UndecidedRun>,
    /// Retain sorted root access through listing or reference resolution. §FS-rhei-panta.6.6
    root_guards: Vec<rhei_core::root_access::RootAccessGuard>,
    access_error: Option<String>,
}

impl RegistrySweep {
    /// A pending root is a refusal, never an empty or partial listing. §FS-rhei-recover.4
    fn ensure_access(&self) -> MietteResult<()> {
        match &self.access_error {
            Some(error) => Err(diagnostic!("{error}")),
            None => Ok(()),
        }
    }

    /// Every entry this pass could not call finished: the live runs first, then
    /// the ones whose liveness it could not decide.
    ///
    /// An undecided entry is not an ended entry, and the commands that act on a
    /// run — `attach`, `stop` — must be able to reach it. An entry too damaged
    /// to parse has no descriptor and so cannot be named here at all.
    // §FS-rhei-run-headless.3
    pub(crate) fn not_known_to_have_ended(&self) -> Vec<&RunDescriptor> {
        self.live
            .iter()
            .chain(self.undecided.iter().filter_map(|entry| entry.descriptor.as_ref()))
            .collect()
    }
}

/// Read every registry entry and classify it, pruning only what is provably
/// gone.
///
/// Pruning here rather than in a separate sweep is deliberate: the listing is
/// the only thing that reliably runs. But it is *destructive*, and an entry is
/// removed only when the workspace no longer names it. Anything unreadable is
/// kept and reported, because an unreadable file says nothing about the
/// process it describes.
// §FS-rhei-run-headless.3 §FS-rhei-run-headless.6
pub(crate) fn sweep_run_registry() -> RegistrySweep {
    classify_run_registry(Pruning::Prune)
}

/// The same classification with nothing removed.
///
/// Shell completion reads the registry on a tab keypress, and a keypress must
/// not unlink a file: the operator did not ask for anything to happen yet, and
/// `rhei runs` — which the operator *did* ask for — prunes the same entries a
/// moment later anyway.
// §FS-rhei-run-headless.3
pub(crate) fn read_run_registry() -> RegistrySweep {
    classify_run_registry(Pruning::Keep)
}

/// Whether a classification pass is allowed to remove what it finds.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pruning {
    Prune,
    Keep,
}

/// Discover registry identities before reading any execution-root data. §FS-rhei-panta.6.6
fn classify_run_registry(pruning: Pruning) -> RegistrySweep {
    let mut sweep = RegistrySweep::default();
    let Some(dir) = run_registry_dir() else {
        return sweep;
    };
    let Ok(entries) = fs::read_dir(&dir) else {
        return sweep;
    };
    let mut descriptors = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let descriptor = match read_descriptor_result(&path) {
            DescriptorRead::Loaded(descriptor) => *descriptor,
            // Raced with another sweep's prune; nothing to report.
            DescriptorRead::Missing => continue,
            // An entry a newer `rhei` wrote, or one this process could not
            // read. Deleting it would let an older binary destroy a newer
            // one's registry.
            DescriptorRead::Unreadable(why) => {
                sweep.undecided.push(UndecidedRun {
                    path,
                    descriptor: None,
                    reason: format!("the entry itself could not be read: {why}"),
                });
                continue;
            }
        };
        descriptors.push((path, descriptor));
    }
    classify_registry_roots(descriptors, sweep, pruning)
}

/// Multi-root registry reads retain one sorted guard set, including during pruning. §FS-rhei-recover.4
fn classify_registry_roots(
    descriptors: Vec<(PathBuf, RunDescriptor)>,
    mut sweep: RegistrySweep,
    pruning: Pruning,
) -> RegistrySweep {
    let access = match registry_root_access(&descriptors, &mut sweep.root_guards) {
        Ok(access) => access,
        Err(error) => {
            sweep.undecided.clear();
            sweep.access_error = Some(error.to_string());
            return sweep;
        }
    };
    for ((path, descriptor), access) in descriptors.into_iter().zip(access) {
        // Registry-only unknowns never inspect the workspace or enter pruning. §FS-rhei-run-headless.3
        if let Err(reason) = access {
            sweep.undecided.push(UndecidedRun { path, descriptor: Some(descriptor), reason });
            continue;
        }
        match descriptor.liveness() {
            Liveness::Live => sweep.live.push(descriptor),
            Liveness::Ended => sweep.ended.push(descriptor),
            Liveness::Gone => {
                if pruning == Pruning::Prune {
                    let _ = fs::remove_file(&path);
                }
            }
            Liveness::Unknown(reason) => {
                sweep.undecided.push(UndecidedRun {
                    path,
                    descriptor: Some(descriptor),
                    reason,
                });
            }
        }
    }
    newest_first(&mut sweep.live);
    newest_first(&mut sweep.ended);
    cap_ended_entries(&mut sweep.ended, pruning);
    sweep
}

fn newest_first(runs: &mut [RunDescriptor]) {
    runs.sort_by(|a, b| b.started_at.cmp(&a.started_at).then_with(|| a.id.cmp(&b.id)));
}

/// Drop the oldest ended entries past the retention cap. Sorted newest first
/// already, so the tail is exactly what goes.
fn cap_ended_entries(ended: &mut Vec<RunDescriptor>, pruning: Pruning) {
    if ended.len() <= RETAINED_ENDED_RUNS {
        return;
    }
    for descriptor in ended.drain(RETAINED_ENDED_RUNS..) {
        let Some(path) = run_registry_path(&descriptor.id) else {
            continue;
        };
        if pruning == Pruning::Keep {
            continue;
        }
        // Re-read before removing: an entry file whose name and contents
        // disagree must not take a different run's pointer down with it.
        if read_descriptor(&path)
            .is_some_and(|entry| entry.id == descriptor.id && entry.pid == descriptor.pid)
        {
            let _ = fs::remove_file(path);
        }
    }
}

/// Resolve a run reference: an exact id, a unique id prefix, a path, or — with
/// no reference at all — the enclosing workspace's run.
///
/// Two tiers, and the split is *not* live-versus-ended: it is **not known to
/// have ended** versus ended. An entry whose liveness could not be decided is
/// resolvable, because the run it names may well be working right now — and a
/// reference that stops resolving the moment a lock file becomes unreadable
/// takes `attach` and `stop` away exactly when they are needed. Within the
/// first tier a decidedly live run still wins an exact tie.
///
/// Ended entries come last because they accumulate: a prefix that resolves
/// today would otherwise start reporting "matches 4 runs" tomorrow, for runs
/// the operator has forgotten.
// §FS-rhei-run-headless.3
pub(crate) fn resolve_run(reference: Option<&str>) -> MietteResult<RunDescriptor> {
    let Some(reference) = reference else {
        return descriptor_for_path(Path::new("."));
    };

    let sweep = sweep_run_registry();
    sweep.ensure_access()?;
    let current = sweep.not_known_to_have_ended();
    if let Some(exact) = current.iter().find(|run| run.id == reference) {
        return Ok((*exact).clone());
    }

    // A path is checked before a prefix so a directory named like an id — or an
    // id-shaped plan stem — resolves to the thing the operator can see.
    let as_path = Path::new(reference);
    if as_path.exists() {
        return descriptor_for_path(as_path);
    }

    match prefix_matches(&current, reference).as_slice() {
        [] => {}
        [only] => return Ok((*only).clone()),
        ambiguous => return Err(ambiguous_reference(reference, ambiguous, "runs")),
    }

    let ended = sweep.ended.iter().collect::<Vec<_>>();
    if let Some(exact) = ended.iter().find(|run| run.id == reference) {
        return Ok((*exact).clone());
    }
    match prefix_matches(&ended, reference).as_slice() {
        [] => {}
        [only] => return Ok((*only).clone()),
        ambiguous => return Err(ambiguous_reference(reference, ambiguous, "runs that have ended")),
    }

    // Not "no *live* run": an ended run resolves too, so pointing only at the
    // live listing points away from the answer. §FS-rhei-run-headless.2
    Err(miette!(
        help = "`rhei runs` lists what is live; a run that has ended resolves by its own id \
                until it falls out of the 100 the registry keeps",
        "no run matches '{reference}'"
    ))
}

fn prefix_matches<'a>(runs: &[&'a RunDescriptor], reference: &str) -> Vec<&'a RunDescriptor> {
    runs.iter().copied().filter(|run| run.id.starts_with(reference)).collect()
}

/// How many candidates an ambiguous reference lists before it summarizes.
/// A hundred retained entries share prefixes freely, and 203 lines of
/// miette-wrapped output is not an answer to "which one did you mean?".
// §FS-rhei-run-headless.3
const LISTED_AMBIGUOUS_MATCHES: usize = 10;

fn ambiguous_reference(
    reference: &str,
    matches: &[&RunDescriptor],
    what: &str,
) -> miette::Report {
    let mut listed = matches
        .iter()
        .take(LISTED_AMBIGUOUS_MATCHES)
        .map(|run| run.summary_line())
        .collect::<Vec<_>>();
    if let Some(rest) = matches.len().checked_sub(LISTED_AMBIGUOUS_MATCHES).filter(|n| *n > 0) {
        listed.push(format!("... and {rest} more"));
    }
    miette!(
        help = "name one of these runs in full",
        "'{reference}' matches {} {what}:\n  {}",
        matches.len(),
        listed.join("\n  ")
    )
}

/// The descriptor a plan path or workspace directory points at.
fn descriptor_for_path(path: &Path) -> MietteResult<RunDescriptor> {
    let workspace = execution_workspace_root(&normalize_workspace_input(path));
    // Retain exclusion while resolving the authoritative descriptor. §FS-rhei-recover.4
    let _root_guards = rhei_core::root_access::for_input(&workspace).map_err(|err| diagnostic!("{err}"))?;
    let descriptor_path = run_descriptor_path(&workspace);
    read_descriptor(&descriptor_path).ok_or_else(|| {
        miette!(
            help = format!(
                "start one with `rhei run --headless {}`, or list live runs with `rhei runs`",
                shell_quote(&path.display().to_string())
            ),
            "no run has been recorded for {} (looked for {})",
            workspace.display(),
            descriptor_path.display()
        )
    })
}
