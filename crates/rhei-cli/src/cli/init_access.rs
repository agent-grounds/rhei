// Init protects host effects and destination creation together. §FS-rhei-init.2

/// Discover existing owners without creating directories or parsing manifests.
/// File discovery also finds an execution root containing a nested host. §FS-rhei-recover.4
fn init_root_access(
    host: &Path,
    project: &Path,
) -> std::io::Result<Vec<rhei_core::root_access::RootAccessGuard>> {
    let mut roots = Vec::new();
    for path in [host, project] {
        let absolute = std::path::absolute(path)?;
        let mut existing = absolute.as_path();
        loop {
            match fs::metadata(existing) {
                Ok(_) => break,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    existing = existing.parent().ok_or(error)?;
                }
                Err(error) => return Err(error),
            }
        }
        roots.extend(rhei_core::root_access::input_roots(existing)?);
        roots.extend(rhei_core::root_access::input_roots(&existing.join("index.panta.md"))?);
    }
    rhei_core::root_access::shared_roots(roots)
}

#[cfg(test)]
thread_local! {
    static INIT_AFTER_MANIFEST: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
}
