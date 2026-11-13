// Which ticket a runtime file matched by prefix belongs to: the longest ticket
// id in the project whose prefix it matches.
//
// Its own part because it is the one question a narrowed reset asks of every
// ticket in the project, not only of those in its scope: `x.build`'s prefix is
// only safe to sweep once `x.build-docs`'s is known.

// §AR-source-file-size.3 §FS-rhei-reset.2.1

/// Every prefix a project ticket's runtime files are matched by, per directory.
/// §FS-rhei-reset.2.1
#[derive(Default)]
struct PrefixOwners {
    by_dir: BTreeMap<PathBuf, BTreeSet<String>>,
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
            for base in bases {
                let runtime = base.join("runtime");
                let ids: &[&str] = if local_id == task_id { &[task_id] } else { &[task_id, local_id] };
                for id in ids {
                    for target in scoped_runtime_targets(&runtime, id, machine) {
                        if let ScopedTarget::Prefixed { dir, prefix } = target {
                            owners.by_dir.entry(dir).or_default().insert(prefix);
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
}
