// The directories a worker is granted to write, passed through its profile's
// `writable_roots` flag and computed afresh at every launch, because settings
// may be hoisted to a Panta project and the checkout may change between states.

// §FS-rhei-agents.2.2

/// What one launch's writable roots are computed from.
#[derive(Debug, Clone, Copy)]
struct LaunchRoots<'a> {
    /// The plan's rhei root: the member's own, inside a Panta project.
    rhei_root: &'a Path,
    /// The run's root: the Panta project around a member, else the rhei root.
    project_root: &'a Path,
    /// The checkout root the agent runs in, as `resolve_agent_checkout_root`
    /// resolved it. §FS-rhei-agents.4
    checkout_root: &'a Path,
}

/// The settings error for a profile whose `writable_roots.path_flag` is empty,
/// named by its full key so the author finds it. §FS-rhei-agents.1.1.2
fn empty_writable_roots_flag(id: &str, profile: &CustomAgentProfile) -> Option<String> {
    let adapter = profile.writable_roots.as_ref()?;
    adapter.path_flag.trim().is_empty().then(|| {
        format!(
            "agent '{id}' has an empty 'agents.{id}.writable_roots.path_flag'; \
             the writable-root flag must be non-empty"
        )
    })
}

/// The writable roots of one launch, in the order §FS-rhei-agents.2.2 lists
/// them: the rhei root and the project around it, the checkout, its git
/// directories, and `rhei/` under each state base — absolute and one per
/// directory.
fn launch_writable_roots(roots: LaunchRoots<'_>) -> Vec<PathBuf> {
    ordered_writable_roots(roots, checkout_git_dirs(roots.checkout_root), nested_rhei_state_dirs())
}

/// The roots of [`launch_writable_roots`] once git and the state resolvers
/// have answered: the order is the contract, so it is decided here alone.
/// §FS-rhei-agents.2.2
fn ordered_writable_roots(
    roots: LaunchRoots<'_>,
    git_dirs: Vec<PathBuf>,
    state_dirs: Vec<PathBuf>,
) -> Vec<PathBuf> {
    let candidates = [roots.rhei_root, roots.project_root, roots.checkout_root]
        .into_iter()
        .map(Path::to_path_buf)
        .chain(git_dirs)
        .chain(state_dirs);
    distinct_absolute_roots(candidates)
}

/// Make each candidate absolute and drop one that is the same directory as an
/// earlier root, keeping the earlier spelling. Nothing else is rewritten: no
/// `~` or `$HOME` expansion, and spaces, quotes, non-ASCII characters and
/// platform separators stay as resolved. §FS-rhei-agents.2.2
fn distinct_absolute_roots(candidates: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    for candidate in candidates {
        let Ok(root) = std::path::absolute(&candidate) else { continue };
        if !roots.iter().any(|earlier| roots_are_one_directory(earlier, &root)) {
            roots.push(root);
        }
    }
    roots
}

/// The checkout's git directory and git common directory, as git reports them,
/// so a commit in a linked worktree can write its metadata. Outside a
/// repository there are none. §FS-rhei-agents.2.2
fn checkout_git_dirs(checkout_root: &Path) -> Vec<PathBuf> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(checkout_root)
        .args(["rev-parse", "--path-format=absolute", "--git-dir", "--git-common-dir"])
        .stdin(std::process::Stdio::null())
        .output();
    match output {
        Ok(output) if output.status.success() => git_dirs_from_rev_parse(&output.stdout),
        _ => Vec::new(),
    }
}

/// One directory per line of `git rev-parse` output, spelled as git spelled it:
/// on Unix the bytes are not decoded first, so a name that is not UTF-8 reaches
/// [`writable_root_args`] as itself. §FS-rhei-agents.2.2
fn git_dirs_from_rev_parse(stdout: &[u8]) -> Vec<PathBuf> {
    stdout
        .split(|byte| *byte == b'\n')
        .map(<[u8]>::trim_ascii)
        .filter(|line| !line.is_empty())
        .map(path_from_git_bytes)
        .collect()
}

/// A Unix path is its bytes.
#[cfg(unix)]
fn path_from_git_bytes(line: &[u8]) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;
    PathBuf::from(std::ffi::OsStr::from_bytes(line))
}

/// Git for Windows writes paths as UTF-8, which decodes without loss.
#[cfg(not(unix))]
fn path_from_git_bytes(line: &[u8]) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(line).into_owned())
}

/// `rhei/` under each state base the root-guard and budget-authority resolvers
/// use for this run — normally one, `$XDG_STATE_HOME/rhei` — where a worker's
/// nested `rhei` writes. A base neither resolver can locate grants nothing.
/// §FS-rhei-agents.2.2 §FS-rhei-recover.4 §FS-rhei-budgets.5.3
fn nested_rhei_state_dirs() -> Vec<PathBuf> {
    [
        rhei_core::root_access::guard_state_base().ok(),
        rhei_core::budget::authority_state_base().ok(),
    ]
    .into_iter()
    .flatten()
    .map(|base| base.join("rhei"))
    .collect()
}

/// One `<path_flag> <root>` pair per root, each root a single argument, and
/// the roots left out because they have no Unicode spelling: the profile's
/// arguments are text, and a lossy spelling would grant a directory that does
/// not exist. §FS-rhei-agents.2.2
fn writable_root_args(path_flag: &str, roots: &[PathBuf]) -> (Vec<String>, Vec<PathBuf>) {
    let mut args = Vec::new();
    let mut left_out = Vec::new();
    for root in roots {
        match root.to_str() {
            Some(spelling) => args.extend([path_flag.to_string(), spelling.to_string()]),
            None => left_out.push(root.clone()),
        }
    }
    (args, left_out)
}

/// The console warning for a root [`writable_root_args`] left out, naming it
/// with its bytes escaped so the directory can be found. §FS-rhei-agents.2.2
fn left_out_root_warning(root: &Path) -> String {
    format!("  warning: writable root {root:?} has no Unicode spelling, so it is not granted")
}
