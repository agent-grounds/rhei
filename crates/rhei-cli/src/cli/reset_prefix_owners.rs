// Which ticket a runtime file matched by prefix, or by a component pattern,
// belongs to: the longest ticket id in the project whose prefix it matches, or
// that a component pattern's path is owned by.
//
// Its own part because it is the one question a narrowed reset asks of every
// ticket in the project, not only of those in its scope: `x.build`'s prefix is
// only safe to sweep once `x.build-docs`'s is known.

// §AR-source-file-size.3 §FS-rhei-reset.2.1

/// Every prefix a project ticket's runtime files are matched by, per directory, and
/// every project ticket id.
/// §FS-rhei-reset.2.1
#[derive(Default)]
struct PrefixOwners {
    by_dir: BTreeMap<PathBuf, BTreeSet<String>>,
    /// Every project ticket's qualified id and its rhei-local one, which a path a
    /// component pattern matches is asked about. §FS-rhei-reset.2.1
    ids: BTreeSet<String>,
}

impl PrefixOwners {
    /// Index every ticket of the project, by its qualified id and its rhei-local
    /// one, at each root a narrowed reset sweeps for it: its own and the project's.
    fn index(loaded: &LoadedPlan, input: &Path, machines: &rhei_validator::MachineSet) -> Self {
        let project_root = execution_workspace_root(input);
        let mut owners = Self::default();
        for (task_id, root) in &loaded.task_roots {
            let machine = machines.for_task_str(task_id);
            let local_id = rhei_local_id_str(task_id);
            let bases: &[&PathBuf] =
                if *root == project_root { &[root] } else { &[root, &project_root] };
            let ids: &[&str] = if local_id == task_id { &[task_id] } else { &[task_id, local_id] };
            owners.ids.extend(ids.iter().map(|id| id.to_string()));
            for base in bases {
                let runtime = base.join("runtime");
                for id in ids {
                    for target in scoped_runtime_targets(&runtime, id, machine) {
                        match target {
                            ScopedTarget::Prefixed { dir, prefix } => {
                                owners.by_dir.entry(dir).or_default().insert(prefix);
                            }
                            ScopedTarget::Pattern(_) | ScopedTarget::Exact(_) => {}
                        }
                    }
                }
            }
        }
        owners
    }

    /// Whether `name`, an entry of `dir` that `prefix` matches, belongs to a longer
    /// id: a longer prefix in the same directory matches it too.
    fn owned_by_longer(&self, dir: &Path, prefix: &str, name: &str) -> bool {
        self.by_dir.get(dir).is_some_and(|prefixes| {
            prefixes.iter().any(|other| other.len() > prefix.len() && name.starts_with(other.as_str()))
        })
    }

    /// The entries of `dir` that `prefix` matches and no longer id owns, which are
    /// what a narrowed reset removes for it. §FS-rhei-reset.2.1
    fn entries(&self, dir: &Path, prefix: &str) -> std::io::Result<Vec<PathBuf>> {
        let mut paths = matching_entries(dir, prefix)?;
        paths.retain(|path| {
            let name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default();
            !self.owned_by_longer(dir, prefix, name)
        });
        Ok(paths)
    }

    /// The paths `pattern` matches that its ticket owns, where no longer project id
    /// could stand at `{task_id}`, which are what a narrowed reset removes for it;
    /// the directory a listing failed in, with the error, where one did.
    /// §FS-rhei-reset.2.1
    fn pattern_entries(
        &self,
        pattern: &ComponentPattern,
    ) -> Result<Vec<PathBuf>, (PathBuf, std::io::Error)> {
        let mut paths = pattern.entries()?;
        paths.retain(|path| pattern.owns(path, self.ids.iter().map(String::as_str)));
        Ok(paths)
    }
}
