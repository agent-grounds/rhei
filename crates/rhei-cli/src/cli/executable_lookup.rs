// Where a program a subprocess would be started with resolves, read without
// starting it: the evidence version probe and the MCP spawn preflight share it.

/// Where `program` resolves for a process started in `cwd` whose `PATH` is
/// `path`: itself, taken from `cwd`, when it names a path, else the first
/// executable match on `path`. `None` when nothing executable is there.
/// §FS-rhei-agents.6.2 §FS-rhei-evidence.7
fn resolve_executable(program: &str, cwd: &Path, path: Option<&OsStr>) -> Option<PathBuf> {
    if names_a_path(program) {
        let candidate = cwd.join(program);
        return is_executable_file(&candidate).then_some(candidate);
    }
    std::env::split_paths(path?).find_map(|dir| {
        program_names(program)
            .into_iter()
            .map(|name| dir.join(name))
            .find(|candidate| is_executable_file(candidate))
    })
}

/// Whether `program` is a path rather than a name to look up on `PATH`.
fn names_a_path(program: &str) -> bool {
    let named = Path::new(program);
    named.components().count() > 1 || named.is_absolute()
}

/// The file names a `PATH` lookup of `program` tries: on Windows also each
/// `PATHEXT` extension, when the name carries none of its own.
fn program_names(program: &str) -> Vec<String> {
    let mut names = vec![program.to_string()];
    if cfg!(windows) && Path::new(program).extension().is_none() {
        let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
        names.extend(
            pathext.split(';').filter(|ext| !ext.is_empty()).map(|ext| format!("{program}{ext}")),
        );
    }
    names
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    fs::metadata(path).is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}
