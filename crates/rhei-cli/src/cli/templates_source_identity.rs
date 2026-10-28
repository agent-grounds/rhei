    // Where a laid template came from and whether its source was committed:
    // computed once per instantiation, written into the fence line and the
    // `Source:` line, and never a reason to refuse. §FS-rhei-templates.6.1.5

    /// The state of one source file, ordered by precedence: the worst wins.
    /// §FS-rhei-templates.6.1.5
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    enum SourceState {
        Clean,
        Unversioned,
        Unknown,
        Dirty,
    }

    impl SourceState {
        fn word(self) -> &'static str {
            match self {
                SourceState::Clean => "clean",
                SourceState::Unversioned => "unversioned",
                SourceState::Unknown => "unknown",
                SourceState::Dirty => "dirty",
            }
        }
    }

    /// One file read, the root it belongs to, and its state. A root is a
    /// repository's top level, or the directory of a file outside any.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct SourceFile {
        root: PathBuf,
        state: SourceState,
    }

    /// What one instantiation records about its source. §FS-rhei-templates.6.1.5
    #[derive(Debug, Clone)]
    struct SourceIdentity {
        builtin: bool,
        /// `<repo>@<40-hex>` or `<40-hex>`, absent when no commit is known.
        git: Option<String>,
        template_files: Vec<SourceFile>,
        input_files: Vec<SourceFile>,
    }

    impl SourceIdentity {
        /// Ask git about a template's own files and the input files it read.
        /// A built-in template has no files to ask about. §FS-rhei-templates.6.1.5
        fn of(template_dir: &Path, builtin: bool, inputs: &[PathBuf]) -> SourceIdentity {
            let template_files = if builtin {
                Vec::new()
            } else {
                let mut files = Vec::new();
                for dir in own_template_dirs(template_dir) {
                    collect_template_files(&dir, &mut files);
                }
                source_file_states(&files)
            };
            let git = (!builtin).then(|| template_commit(template_dir)).flatten();
            SourceIdentity { builtin, git, template_files, input_files: source_file_states(inputs) }
        }

        /// The fence's state word: the worst of the template's own files, or
        /// `builtin`. Inputs never enter it. §FS-rhei-templates.6.1.5
        fn state_word(&self) -> &'static str {
            if self.builtin {
                return "builtin";
            }
            self.template_files
                .iter()
                .map(|file| file.state)
                .max()
                .unwrap_or(SourceState::Clean)
                .word()
        }

        /// `git:<repo>@<40-hex> <state>`, or just the state word.
        /// §FS-rhei-library.7.1
        fn fence_fields(&self) -> String {
            match &self.git {
                Some(git) => format!("git:{git} {}", self.state_word()),
                None => self.state_word().to_string(),
            }
        }

        /// `Source: template <group> · inputs <group>`. §FS-rhei-templates.6.1.3
        fn source_line(&self) -> String {
            let template =
                if self.builtin { "builtin".to_string() } else { source_group(&self.template_files) };
            let inputs = if self.input_files.is_empty() {
                "none".to_string()
            } else {
                source_group(&self.input_files)
            };
            format!("Source: template {template} · inputs {inputs}")
        }
    }

    /// The `laid` fence a flat instantiation appends to the machine it wrote.
    /// §FS-rhei-library.7.1
    fn laid_fence(
        manifest: &TemplateManifest,
        digest: &str,
        source: &SourceIdentity,
        values: &BTreeMap<String, serde_json::Value>,
    ) -> String {
        let inputs = serde_json::Value::Object(values.clone().into_iter().collect());
        format!(
            "# --- laid {} {} src:sha256:{digest} {} inputs:sha256:{} ---\n",
            manifest.name,
            manifest.version_string(),
            source.fence_fields(),
            sha256_hex(canonical_json_text(&inputs).as_bytes())
        )
    }

    /// Append the `laid` fence to `machine`, when the lay wrote one.
    /// §FS-rhei-library.7.1
    fn append_laid_fence(machine: &Path, fence: &str) -> MietteResult<()> {
        if !machine.is_file() {
            return Ok(());
        }
        let mut text = read_text(machine)?;
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(fence);
        fs::write(machine, text).map_err(|err| file_io_report(machine, "failed to write", err))
    }

    /// The files of `--values` and `--set-file` an instantiation read.
    /// §FS-rhei-templates.6.1.5
    fn input_file_paths(values_files: &[PathBuf], set_files: &[String]) -> Vec<PathBuf> {
        let mut paths = values_files.to_vec();
        paths.extend(
            set_files.iter().filter_map(|entry| entry.split_once('=')).map(|(_, path)| path.into()),
        );
        paths
    }

    /// A template's directory and every `includes:` part's, resolved through the
    /// same discovery the lay used. A built-in part has no files to ask about.
    fn own_template_dirs(template_dir: &Path) -> Vec<PathBuf> {
        let mut dirs = vec![template_dir.to_path_buf()];
        let mut index = 0;
        while index < dirs.len() {
            let dir = dirs[index].clone();
            index += 1;
            let Ok(manifest) = load_template_manifest(&dir) else { continue };
            for entry in &manifest.includes {
                let Ok(resolved) = resolve_include(&dir, entry.template()) else { continue };
                if !resolved.is_builtin() && !dirs.contains(&resolved.path) {
                    dirs.push(resolved.path.clone());
                }
            }
        }
        dirs
    }

    /// Every file under `dir`, as the source digest walks it.
    fn collect_template_files(dir: &Path, out: &mut Vec<PathBuf>) {
        if dir.is_file() {
            out.push(dir.to_path_buf());
            return;
        }
        let Ok(entries) = fs::read_dir(dir) else { return };
        let mut entries: Vec<PathBuf> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
        entries.sort();
        for entry in entries {
            collect_template_files(&entry, out);
        }
    }

    /// The state of each file, asked of the repository holding it once the
    /// file's symlinks are resolved, with one `git status` per repository.
    /// §FS-rhei-templates.6.1.5
    fn source_file_states(files: &[PathBuf]) -> Vec<SourceFile> {
        let mut tops: BTreeMap<PathBuf, GitAnswer> = BTreeMap::new();
        let mut by_repo: BTreeMap<PathBuf, Vec<(usize, PathBuf)>> = BTreeMap::new();
        let mut out: Vec<Option<SourceFile>> = vec![None; files.len()];
        for (index, file) in files.iter().enumerate() {
            let resolved = rhei_core::platform::canonical_path(file).unwrap_or_else(|_| file.clone());
            let dir = resolved.parent().map(Path::to_path_buf).unwrap_or_default();
            let top = tops.entry(dir.clone()).or_insert_with(|| repository_top(&dir)).clone();
            match top {
                GitAnswer::Out(top) => {
                    by_repo.entry(PathBuf::from(top)).or_default().push((index, resolved));
                }
                GitAnswer::NotARepository => {
                    out[index] = Some(SourceFile { root: dir, state: SourceState::Unversioned });
                }
                GitAnswer::Failed => {
                    out[index] = Some(SourceFile { root: dir, state: SourceState::Unknown });
                }
            }
        }
        for (top, members) in by_repo {
            let relative: Vec<String> = members
                .iter()
                .map(|(_, path)| {
                    let rel = path.strip_prefix(&top).unwrap_or(path);
                    rel.components()
                        .map(|part| part.as_os_str().to_string_lossy())
                        .collect::<Vec<_>>()
                        .join("/")
                })
                .collect();
            let mut args = vec![
                "status",
                "--porcelain",
                "-z",
                "--ignored=matching",
                "--untracked-files=all",
                "--",
            ];
            args.extend(relative.iter().map(String::as_str));
            let listed: Option<BTreeSet<String>> = git_answer(&top, &args).out().map(|text| {
                text.split('\0')
                    .flat_map(|entry| [entry.get(3..).unwrap_or(""), entry])
                    .filter(|path| !path.is_empty())
                    .map(str::to_string)
                    .collect()
            });
            for ((index, _), rel) in members.iter().zip(&relative) {
                let state = match &listed {
                    None => SourceState::Unknown,
                    Some(listed) if listed.contains(rel) => SourceState::Dirty,
                    Some(_) => SourceState::Clean,
                };
                out[*index] = Some(SourceFile { root: top.clone(), state });
            }
        }
        out.into_iter().flatten().collect()
    }

    /// `<repo>@<40-hex>` or `<40-hex>` for the repository holding the resolved
    /// template directory. §FS-rhei-templates.6.1.5
    fn template_commit(template_dir: &Path) -> Option<String> {
        let resolved = rhei_core::platform::canonical_path(template_dir)
            .unwrap_or_else(|_| template_dir.to_path_buf());
        let head = repository_head(&resolved)?;
        Some(match head.repo {
            Some(repo) => format!("{repo}@{}", head.commit),
            None => head.commit,
        })
    }

    /// One group of the `Source:` line: `clean`, or one entry per root holding
    /// a file that is not clean, `<root> <state> (<n> file[s])`, the root's
    /// state the worst of its files. §FS-rhei-templates.6.1.3
    fn source_group(files: &[SourceFile]) -> String {
        let mut roots: BTreeMap<String, (SourceState, usize)> = BTreeMap::new();
        for file in files.iter().filter(|file| file.state != SourceState::Clean) {
            let root = display_path(&file.root).display().to_string();
            let entry = roots.entry(root).or_insert((file.state, 0));
            entry.0 = entry.0.max(file.state);
            entry.1 += 1;
        }
        if roots.is_empty() {
            return "clean".to_string();
        }
        roots
            .into_iter()
            .map(|(root, (state, count))| {
                let noun = if count == 1 { "file" } else { "files" };
                format!("{root} {} ({count} {noun})", state.word())
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
