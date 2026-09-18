// External registry evidence remains usable when a root is uncheckable. §FS-rhei-run-headless.3

/// Only inspection failures become unknown; pending markers and other guard errors refuse.
/// Established markers use ErrorKind::Other even when their contents cannot be read. §FS-rhei-recover.4
fn registry_inspection<T>(result: std::io::Result<T>) -> std::io::Result<Result<T, String>> {
    match result {
        Ok(value) => Ok(Ok(value)),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::NotFound
            ) =>
        {
            Ok(Err(format!("workspace access could not be checked: {error}")))
        }
        Err(error) => Err(error),
    }
}

/// Discover first, lock once in canonical order, then classify only guarded roots.
/// Unknown roots retain external descriptors without any dependent root read. §FS-rhei-recover.4
fn registry_root_access(
    descriptors: &[(PathBuf, RunDescriptor)],
    guards: &mut Vec<rhei_core::root_access::RootAccessGuard>,
) -> std::io::Result<Vec<Result<(), String>>> {
    use rhei_core::root_access::{check_pending, input_roots, RootAccessGuard};
    let mut groups = Vec::new();
    let mut roots = BTreeMap::new();
    for (_, run) in descriptors {
        let discovery = match fs::metadata(&run.workspace) {
            // A confirmed missing workspace still follows the existing liveness/pruning contract.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(error),
            Ok(_) => check_pending(&run.workspace).and_then(|()| input_roots(&run.workspace)),
        };
        let group = registry_inspection(discovery)?;
        if let Ok(group) = &group {
            for root in group {
                roots.insert(root.clone(), Ok(()));
            }
        }
        groups.push(group);
    }
    for (root, access) in &mut roots {
        *access = registry_inspection(check_pending(root))?;
    }
    for (root, access) in &mut roots {
        if access.is_ok() {
            match registry_inspection(RootAccessGuard::shared(root))? {
                Ok(guard) => guards.push(guard),
                Err(reason) => *access = Err(reason),
            }
        }
    }
    for (root, access) in &mut roots {
        if access.is_ok() {
            *access = registry_inspection(check_pending(root))?;
        }
    }
    Ok(groups
        .into_iter()
        .map(|group| {
            group.and_then(|group| group.into_iter().try_for_each(|root| roots[&root].clone()))
        })
        .collect())
}
