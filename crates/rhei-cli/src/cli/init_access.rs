// Init protects host effects and destination creation together. §FS-rhei-init.2

/// Discover existing owners without creating directories or parsing manifests.
/// File discovery also finds an execution root containing a nested host. §FS-rhei-recover.4
/// Each failed filesystem operation retains its own path and cause. §FS-rhei-errors.6
fn init_root_access(
    host: &Path,
    project: &Path,
) -> MietteResult<Vec<rhei_core::root_access::RootAccessGuard>> {
    let mut roots = Vec::new();
    for path in [host, project] {
        let absolute = std::path::absolute(path).map_err(|err| miette!(
            help = cwd_help(), "cannot resolve the init path {}: {err}", path.display()))?;
        let mut existing = absolute.as_path();
        loop {
            match fs::metadata(existing) {
                Ok(_) => break,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    existing = existing.parent().ok_or_else(|| file_io_report(existing, "cannot find an existing init owner", error))?;
                }
                Err(error) => return Err(file_io_report(existing, "cannot inspect init owner", error)),
            }
        }
        roots.extend(rhei_core::root_access::input_roots(existing).map_err(|err| root_access_report(existing, err))?);
        roots.extend(rhei_core::root_access::input_roots(&existing.join("index.panta.md")).map_err(|err| root_access_report(existing, err))?);
    }
    rhei_core::root_access::shared_roots(roots).map_err(|err| root_access_report(host, err))
}

#[cfg(test)]
thread_local! {
    static INIT_AFTER_MANIFEST: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
}
