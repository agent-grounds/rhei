// Whose a runtime path a narrowed reset's target matches is: the target's ticket,
// unless a ticket the reset keeps could own it; and the names rhei writes itself,
// which are matched by prefix.
//
// Its own part because it is the one question a narrowed reset asks of every
// ticket in the project, not only of those in its scope: `x.build`'s files are
// only safe to sweep once it is known whether `x.build-docs` is kept.

// §AR-source-file-size.3 §FS-rhei-reset.2.1

/// A name rhei writes itself in one directory, `<before><id>-…`: a log, a spawn
/// record or a report (`task-<id>-`), a snapshot session or a capture (`<id>-`).
/// §FS-rhei-reset.2.1
struct NamedPrefix {
    dir: PathBuf,
    /// What rhei writes before the id.
    before: &'static str,
    /// The ticket id, as the name spells it.
    id: String,
    /// Whether the name spells an id as a capture does. §FS-rhei-cost-accounting.2
    capture: bool,
}

impl NamedPrefix {
    fn new(dir: PathBuf, before: &'static str, id: &str) -> Self {
        Self { dir, before, id: id.to_string(), capture: false }
    }

    fn capture(dir: PathBuf, id: &str) -> Self {
        Self { dir, before: "", id: safe_accounting_file_segment(id), capture: true }
    }

    /// The text every one of the ticket's names begins with: rhei always writes `-`
    /// after the id, so `auth.1` never matches `auth.10`.
    fn prefix(&self) -> String {
        format!("{}{}-", self.before, self.id)
    }

    /// The entries of `dir` its prefix matches; none where `dir` is not a directory.
    fn entries(&self) -> std::io::Result<Vec<PathBuf>> {
        matching_entries(&self.dir, &self.prefix())
    }

    /// Whether `path` is this ticket's: an entry of `dir` its prefix matches, where
    /// no id of `kept` could stand where the ticket's id stands. §FS-rhei-reset.2.1
    /// §FS-rhei-remove.4.2
    fn owns<'k>(&self, path: &Path, kept: impl IntoIterator<Item = &'k str>) -> bool {
        let name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default();
        path.parent() == Some(self.dir.as_path())
            && name.starts_with(&self.prefix())
            && !kept.into_iter().any(|other| self.could_stand(name, other))
    }

    /// Whether `other`, another ticket's id, could stand where this ticket's id
    /// stands in `name`, compared as the name spells ids. §FS-rhei-reset.2.1
    fn could_stand(&self, name: &str, other: &str) -> bool {
        let spelled: std::borrow::Cow<'_, str> =
            if self.capture { safe_accounting_file_segment(other).into() } else { other.into() };
        if spelled.len() <= self.id.len() || !spelled.contains(self.id.as_str()) {
            return false;
        }
        let (before, after) = ([self.before.to_string()], ["-".to_string(), String::new()]);
        id_could_stand(name, &self.id, &before, &after, &spelled)
    }
}

impl ScopedTarget {
    /// Whether `other` could stand where the target's id stands in `path`, a path it
    /// matches. An exact path names one ticket's file. §FS-rhei-reset.2.1
    fn could_stand(&self, path: &Path, other: &str) -> bool {
        match self {
            ScopedTarget::Exact(_) => false,
            ScopedTarget::Prefixed(named) => {
                path.file_name().and_then(|name| name.to_str()).is_some_and(|name| named.could_stand(name, other))
            }
            ScopedTarget::Pattern(pattern) => pattern.could_stand(path, other),
        }
    }
}

/// Every id of every project ticket, split by whether the reset keeps the ticket or
/// reaches it, which is what a path a target matches is judged against.
/// §FS-rhei-reset.2.1
#[derive(Default)]
struct RuntimeOwners {
    /// Every id, qualified and rhei-local, of every project ticket the reset leaves
    /// out: a path one of them could stand in stays. §FS-rhei-reset.2.1
    kept: BTreeSet<String>,
    /// Every id, qualified and rhei-local, of every ticket it reaches, with the
    /// tickets that id names. §FS-rhei-reset.2.5.3
    reached: BTreeMap<String, BTreeSet<String>>,
}

impl RuntimeOwners {
    /// Index every ticket of the project, by its qualified id and its rhei-local one,
    /// on the side of `scope` it falls.
    fn index(loaded: &LoadedPlan, scope: ResetScope<'_>) -> Self {
        let mut owners = Self::default();
        for task_id in loaded.task_roots.keys() {
            let local_id = rhei_local_id_str(task_id);
            let ids: &[&str] = if local_id == task_id { &[task_id] } else { &[task_id, local_id] };
            for id in ids {
                if scope.contains(task_id) {
                    owners.reached.entry(id.to_string()).or_default().insert(task_id.clone());
                } else {
                    owners.kept.insert(id.to_string());
                }
            }
        }
        owners
    }

    /// The paths `target` matches now that its ticket owns, which are what a narrowed
    /// reset removes for it; the directory a listing failed in, with the error, where
    /// one did. §FS-rhei-reset.2.1
    fn entries(&self, target: &ScopedTarget) -> Result<Vec<PathBuf>, (PathBuf, std::io::Error)> {
        let kept = || self.kept.iter().map(String::as_str);
        match target {
            ScopedTarget::Exact(path) => Ok(if path.exists() { vec![path.clone()] } else { Vec::new() }),
            ScopedTarget::Prefixed(named) => {
                let mut paths = named.entries().map_err(|err| (named.dir.clone(), err))?;
                paths.retain(|path| named.owns(path, kept()));
                Ok(paths)
            }
            ScopedTarget::Pattern(pattern) => {
                let mut paths = pattern.entries()?;
                paths.retain(|path| pattern.owns(path, kept()));
                Ok(paths)
            }
        }
    }

    /// Whether `target` takes anything on disk now.
    fn takes_any(&self, target: &ScopedTarget) -> bool {
        self.entries(target).is_ok_and(|paths| !paths.is_empty())
    }

    /// The tickets the reset reaches whose longer id could stand where `target`'s id
    /// stands in `path`, a path it takes: they lose that file too. §FS-rhei-reset.2.5.3
    fn standing<'o>(&'o self, target: &'o ScopedTarget, path: &'o Path) -> impl Iterator<Item = &'o String> {
        self.reached.iter().filter(|(id, _)| target.could_stand(path, id)).flat_map(|(_, tickets)| tickets)
    }
}
