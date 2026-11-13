/// The directory a plan's relative links resolve against, and whether that
/// directory is a Directory Workspace root they must stay inside.
///
/// A Single-File Plan resolves links against the plan file's directory and may
/// reach above it; a Directory Workspace resolves them against its root and may
/// not leave it. The kind is carried, never guessed from the path, because a
/// directory alone does not say which of the two it is. §FS-rhei-plan-language.3.6
#[derive(Debug, Clone, Copy)]
pub struct LinkBase<'a> {
    /// The directory a relative link target is joined to.
    pub dir: &'a Path,
    /// Whether `dir` is a Directory Workspace root that links must stay inside.
    pub contains: bool,
}

impl<'a> LinkBase<'a> {
    /// A Directory Workspace root: links resolve here and must not leave it.
    pub fn workspace_root(dir: &'a Path) -> Self {
        Self { dir, contains: true }
    }

    /// A Single-File Plan's directory: links resolve here and may reach above it.
    pub fn plan_directory(dir: &'a Path) -> Self {
        Self { dir, contains: false }
    }
}

/// Link bases for a merged Panta graph: each ticket and content section
/// resolves against the root of the rhei that owns it, and is held to
/// containment exactly when that root is a Directory Workspace's.
/// §AR-rhei-panta.5 §FS-rhei-plan-language.3.6
#[derive(Debug, Clone, Copy)]
pub struct LinkBases<'a> {
    /// The base for a ticket or section with no recorded root of its own.
    pub default: LinkBase<'a>,
    /// Owning rhei root for each project-qualified ticket id.
    pub tasks: &'a HashMap<String, PathBuf>,
    /// Owning rhei root for each merged content section, in section order.
    pub sections: &'a [PathBuf],
    /// The roots among those that belong to a Directory Workspace rhei.
    pub workspace_roots: &'a HashSet<PathBuf>,
}

impl<'a> LinkBases<'a> {
    fn base_for(&self, root: Option<&'a PathBuf>) -> LinkBase<'a> {
        match root {
            Some(dir) => LinkBase { dir, contains: self.workspace_roots.contains(dir) },
            None => self.default,
        }
    }

    fn for_task(&self, task_id: &str) -> LinkBase<'a> {
        self.base_for(self.tasks.get(task_id))
    }

    fn for_section(&self, index: usize) -> LinkBase<'a> {
        self.base_for(self.sections.get(index))
    }
}

/// `path` with `.` dropped and each `..` folded into the segment before it,
/// without touching the file system: the target need not exist. A `..` above
/// the start of a relative path is kept; one above the root stays at the root.
/// §FS-rhei-plan-language.3.6
fn normalize_lexically(path: &Path) -> PathBuf {
    let mut out: Vec<Component<'_>> = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match out.last() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                Some(Component::RootDir | Component::Prefix(_)) => {}
                _ => out.push(component),
            },
            other => out.push(other),
        }
    }
    out.iter().collect()
}

/// A link base made absolute, so a base given as `.`, or as the empty parent of
/// a bare plan file name, still names a directory: in a refusal, and as the root
/// a link is held inside. `..` is kept, because a refusal names the path that was
/// actually tried. §FS-rhei-plan-language.3.6
fn absolute_link_base(dir: &Path) -> PathBuf {
    let dir = if dir.as_os_str().is_empty() { Path::new(".") } else { dir };
    std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf())
}

/// The workspace root, absolute and normalized, when `target` joined to it and
/// normalized lands outside it. `None` when the link stays inside, including
/// one that leaves and comes back. §FS-rhei-plan-language.3.6
fn link_leaves_workspace_root(root: &Path, target: &Path) -> Option<PathBuf> {
    // Absolute first: a relative root such as `.` normalizes to nothing, and
    // every path starts with nothing.
    let root = normalize_lexically(&absolute_link_base(root));
    let landed = normalize_lexically(&root.join(target));
    (!landed.starts_with(&root)).then_some(root)
}
