// Rhei's project-local and user-global home, `.agent-grounds/rhei/`, and the
// deprecated `.agents/rhei/` it replaced. §FS-rhei-templates.1.1

// One module owns both names, the preference between them, the per-level
// ancestor walk and the deprecation warning, so the rule reads identically at
// every call site and there is one place to delete when the deprecation ends.

/// Rhei's home for its own project-local material. §FS-rhei-templates.1.1
const RHEI_HOME_DIR: &str = ".agent-grounds";

/// The home it replaced. Still read, and warned about. §FS-rhei-templates.1.1
const DEPRECATED_RHEI_HOME_DIR: &str = ".agents";

/// One candidate location for rhei material, carrying whether reading it is
/// deprecated and, if so, where the material belongs instead.
/// §FS-rhei-templates.1
#[derive(Clone, Debug)]
struct RheiHomePath {
    path: PathBuf,
    /// The `.agent-grounds` path to move the material to, when `path` is under
    /// the deprecated home.
    move_to: Option<PathBuf>,
}

impl RheiHomePath {
    /// A path that is neither home, so nothing about it is deprecated — the
    /// built-in tier's placeholder is the only such root.
    fn plain(path: impl Into<PathBuf>) -> Self {
        RheiHomePath { path: path.into(), move_to: None }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn into_path(self) -> PathBuf {
        self.path
    }

    /// Say, once per process, that this path is deprecated and where its
    /// contents belong. A no-op for the current home. §FS-rhei-templates.1.3
    fn warn_if_deprecated(&self) {
        if let Some(move_to) = self.move_to.as_ref() {
            warn_deprecated_rhei_home(&self.path, move_to);
        }
    }

    /// This path, when it is the deprecated one — for the callers that owe it a
    /// message of their own rather than the generic move-warning.
    /// §FS-rhei-templates.1.1
    fn deprecated_path(&self) -> Option<&Path> {
        self.move_to.as_ref().map(|_| self.path.as_path())
    }
}

/// Both names for `<base>/<home>/rhei/<leaf>`, the current home first. Every
/// tier searches both, and within a tier the `.agent-grounds` name wins.
/// §FS-rhei-templates.1.1
fn rhei_home_paths(base: &Path, leaf: &str) -> [RheiHomePath; 2] {
    let current = base.join(RHEI_HOME_DIR).join("rhei").join(leaf);
    let deprecated = base.join(DEPRECATED_RHEI_HOME_DIR).join("rhei").join(leaf);
    [
        RheiHomePath { path: current.clone(), move_to: None },
        RheiHomePath { path: deprecated, move_to: Some(current) },
    ]
}

/// Where rhei *writes* project-local material. Never the deprecated home, so
/// rhei's own output is never a path rhei then warns about.
/// §FS-rhei-templates.1.1
fn rhei_home_write_path(base: &Path, leaf: &str) -> PathBuf {
    base.join(RHEI_HOME_DIR).join("rhei").join(leaf)
}

/// The directories that exist under `base`, current home first. Both take part
/// in the search at a tier. §FS-rhei-templates.1.1
fn existing_rhei_home_dirs(base: &Path, leaf: &str) -> Vec<RheiHomePath> {
    rhei_home_paths(base, leaf).into_iter().filter(|candidate| candidate.path.is_dir()).collect()
}

/// The file rhei reads under `base`: first match wins, and the two are never
/// merged. §FS-rhei-agents.1.1
fn resolve_rhei_home_file(base: &Path, leaf: &str) -> Option<RheiHomePath> {
    rhei_home_paths(base, leaf).into_iter().find(|candidate| candidate.path.is_file())
}

/// Every genuine project ancestor holding either template-home name, nearest
/// first. Each contributing level includes both names in current-then-deprecated
/// order. The configured user roots are excluded without stopping the walk; a
/// walk with no other level uses `fallback` for empty-search diagnostics.
/// Filesystem, user-home, Git, and Panta boundaries do not stop the walk.
/// §FS-rhei-templates.1.2
fn ancestor_template_roots(
    start: &Path,
    fallback: &Path,
    user_home: Option<&Path>,
) -> Vec<RheiHomePath> {
    ancestor_template_roots_from_levels(start.ancestors(), fallback, user_home)
}

/// The production ancestor-root selection over caller-supplied levels. Keeping
/// the filesystem walk at the caller gives fallback coverage deterministic
/// inputs without reproducing the selection algorithm. §FS-rhei-templates.1.2
fn ancestor_template_roots_from_levels<'a>(
    levels: impl IntoIterator<Item = &'a Path>,
    fallback: &Path,
    user_home: Option<&Path>,
) -> Vec<RheiHomePath> {
    let user_roots = user_home.map(|home| rhei_home_paths(home, "templates"));
    let user_root_reals: Option<[PathBuf; 2]> =
        user_roots.as_ref().map(|roots| [real_path(roots[0].path()), real_path(roots[1].path())]);
    // `cwd`'s ancestors resolve symlinks (`getcwd`'s physical path) while
    // `$HOME` is read literally, so a symlinked temp root (macOS's
    // `/var` -> `/private/var`) breaks a lexical identity check. §FS-rhei-templates.1.2
    let is_user_root = |candidate: &RheiHomePath| {
        user_root_reals
            .as_ref()
            .is_some_and(|reals| reals.iter().any(|root| *root == real_path(candidate.path())))
    };
    let roots = levels
        .into_iter()
        .filter(|level| !existing_rhei_home_dirs(level, "templates").is_empty())
        .flat_map(|level| rhei_home_paths(level, "templates"))
        .filter(|candidate| !is_user_root(candidate))
        .collect::<Vec<_>>();
    if roots.is_empty() {
        rhei_home_paths(fallback, "templates")
            .into_iter()
            .filter(|candidate| !is_user_root(candidate))
            .collect()
    } else {
        roots
    }
}

/// `path`, symlink-resolved when that succeeds. Comparing this form is what
/// lets the exact-user-root check survive a symlinked temp root; callers keep
/// using the original, unresolved path everywhere else. §FS-rhei-templates.1.2
fn real_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Whether this process still owes a warning about `read`, taking the debt when
/// it does. Once per distinct deprecated path: templates and settings are
/// resolved repeatedly inside one `rhei run`, so a per-lookup warning would
/// bury the run's own output. §FS-rhei-templates.1.3
fn claim_deprecated_rhei_home_warning(read: &Path) -> bool {
    if serving_shell_completion() {
        return false;
    }
    static WARNED: std::sync::OnceLock<Mutex<HashSet<PathBuf>>> = std::sync::OnceLock::new();
    let warned = WARNED.get_or_init(|| Mutex::new(HashSet::new()));
    let Ok(mut warned) = warned.lock() else {
        return false;
    };
    warned.insert(read.to_path_buf())
}

/// Whether this run is answering a Tab press, recorded rather than read from
/// the environment: `clap_complete` removes `COMPLETE` before it runs the
/// completer, so by the time a candidate list is being built nothing is left to
/// read. Every completion is a fresh process, so the once-per-process guard
/// cannot keep the warning off the candidate list; this is what does.
/// §FS-rhei-templates.1.3
static SERVING_SHELL_COMPLETION: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Record that this process is serving a completion request, before
/// `clap_complete` takes over. §FS-rhei-templates.1.3
fn mark_serving_shell_completion() {
    SERVING_SHELL_COMPLETION.store(true, std::sync::atomic::Ordering::Relaxed);
}

fn serving_shell_completion() -> bool {
    SERVING_SHELL_COMPLETION.load(std::sync::atomic::Ordering::Relaxed)
}

/// On stderr, because stdout would corrupt `--json`. The verb is **move**: a
/// reader who copies instead ends up editing a file that the copy at the
/// current name now shadows. §FS-rhei-templates.1.3
fn warn_deprecated_rhei_home(read: &Path, move_to: &Path) {
    if !claim_deprecated_rhei_home_warning(read) {
        return;
    }
    eprintln!(
        "warning: read rhei material from {}, which is deprecated. Move it to {} — \
         `.agents/` holds agent instructions and an agent runtime may mount it read-only, \
         so rhei's own files cannot stay there. Move rather than copy: a copy left behind \
         is shadowed and nothing reads it.",
        read.display(),
        move_to.display()
    );
}

#[cfg(test)]
mod template_root_order_tests {
    use super::*;

    /// Test-only spelling of the approved root order. The end-to-end tests pin
    /// that production discovery consumes this whole sequence.
    /// §FS-rhei-templates.1.2
    fn ancestor_template_roots(start: &Path) -> Vec<RheiHomePath> {
        start
            .ancestors()
            .flat_map(|level| rhei_home_paths(level, "templates"))
            .collect()
    }

    #[test]
    fn template_ancestor_discovery_orders_roots_through_the_filesystem_root() {
        let temp = tempfile::tempdir().expect("create root-order fixture");
        let marker = temp.path().join("workspace");
        let start = marker.join("panta/member");
        std::fs::create_dir_all(marker.join(".git")).expect("create Git marker");
        std::fs::create_dir_all(&start).expect("create nested start");
        std::fs::write(marker.join("index.panta.md"), "# Panta: Marker\n")
            .expect("create Panta marker");

        let roots = ancestor_template_roots(&start);
        let levels = start.ancestors().collect::<Vec<_>>();
        assert_eq!(roots.len(), levels.len() * 2);
        for (level, pair) in levels.iter().zip(roots.chunks_exact(2)) {
            let expected = rhei_home_paths(level, "templates");
            assert_eq!(pair[0].path(), expected[0].path());
            assert_eq!(pair[1].path(), expected[1].path());
        }

        let marker_current = rhei_home_paths(&marker, "templates")[0].path().to_path_buf();
        assert!(
            roots.iter().any(|root| root.path() == marker_current),
            "Git and Panta markers must not remove their level or any parent"
        );
        let filesystem_root = levels.last().expect("an absolute temp path has a root");
        let final_pair = rhei_home_paths(filesystem_root, "templates");
        assert_eq!(roots[roots.len() - 2].path(), final_pair[0].path());
        assert_eq!(roots[roots.len() - 1].path(), final_pair[1].path());
    }

    #[test]
    fn template_ancestor_fallback_excludes_user_roots_and_orders_the_project_pair() {
        let temp = tempfile::tempdir().expect("create fallback fixture");
        let home = temp.path().join("home");
        let fallback = temp.path().join("project");
        for root in rhei_home_paths(&home, "templates") {
            std::fs::create_dir_all(root.path()).expect("create user template root");
        }

        let roots = ancestor_template_roots_from_levels(
            [home.as_path()],
            &fallback,
            Some(home.as_path()),
        );
        let expected = rhei_home_paths(&fallback, "templates");
        assert_eq!(roots.len(), 2);
        assert_eq!(roots[0].path(), expected[0].path());
        assert_eq!(roots[1].path(), expected[1].path());
        assert!(roots[0].deprecated_path().is_none());
        assert_eq!(roots[1].deprecated_path(), Some(expected[1].path()));

        let excluded = ancestor_template_roots_from_levels(
            std::iter::empty(),
            &home,
            Some(home.as_path()),
        );
        assert!(excluded.is_empty(), "fallback must not reinsert either exact user root");
    }

    /// A symlinked ancestor level must still match the exact user root reached
    /// through its real path — the mismatch a lexical comparison would miss on
    /// a platform whose temp root is itself a symlink (macOS's `/var`). §FS-rhei-templates.1.2
    #[test]
    #[cfg(unix)]
    fn template_ancestor_fallback_excludes_a_user_root_reached_through_a_symlinked_level() {
        let temp = tempfile::tempdir().expect("create symlink fixture");
        let real_home = temp.path().join("real-home");
        for root in rhei_home_paths(&real_home, "templates") {
            std::fs::create_dir_all(root.path()).expect("create user template root");
        }
        let linked_home = temp.path().join("linked-home");
        std::os::unix::fs::symlink(&real_home, &linked_home).expect("create home symlink");

        let excluded = ancestor_template_roots_from_levels(
            [linked_home.as_path()],
            &real_home,
            Some(real_home.as_path()),
        );
        assert!(
            excluded.is_empty(),
            "the level reached through a symlink must still resolve to the exact user root: {excluded:?}"
        );
    }
}
