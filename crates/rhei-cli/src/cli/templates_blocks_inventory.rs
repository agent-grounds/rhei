    // Effective bytes and immutable-revision dependencies share one read inventory.
    // Paths in the digest remain source-relative, even for escaping links. §FS-rhei-library.4.1
    #[derive(Default)]
    struct CompositionInventory {
        entries: Vec<(String, &'static str, Vec<u8>)>,
        dependencies: BTreeMap<PathBuf, Vec<u8>>,
        escaping: bool,
    }

    impl CompositionInventory {
        fn digest(&self) -> String {
            use sha2::{Digest, Sha256};
            let mut entries = self.entries.iter().collect::<Vec<_>>();
            entries.sort_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));
            let mut digest = Sha256::new();
            for (path, kind, bytes) in entries {
                for field in [path.as_bytes(), kind.as_bytes(), bytes.as_slice()] {
                    digest.update((field.len() as u64).to_be_bytes());
                    digest.update(field);
                }
            }
            format!("{:x}", digest.finalize())
        }
    }

    fn composition_inventory(root: &Path) -> MietteResult<CompositionInventory> {
        let mut inventory = CompositionInventory::default();
        collect_composition_entries(root, root, &mut inventory)?;
        Ok(inventory)
    }

    fn collect_composition_entries(
        root: &Path,
        dir: &Path,
        inventory: &mut CompositionInventory,
    ) -> MietteResult<()> {
        for entry in fs::read_dir(dir).map_err(|err| file_io_report(dir, "read source inventory", err))? {
            let path = entry.map_err(|err| miette!(help = "check that the source block directory is readable", "read source inventory entry: {err}"))?.path();
            if path.file_name().and_then(|name| name.to_str()).is_some_and(|name| name.starts_with('.')) {
                continue;
            }
            let metadata = fs::symlink_metadata(&path)
                .map_err(|err| file_io_report(&path, "inspect source inventory", err))?;
            if metadata.is_dir() {
                collect_composition_entries(root, &path, inventory)?;
                continue;
            }
            let relative = slash_path(path.strip_prefix(root).unwrap());
            let effective = if metadata.file_type().is_symlink() {
                let target = fs::read_link(&path)
                    .map_err(|err| file_io_report(&path, "read source symlink", err))?;
                inventory.entries.push((relative.clone(), "symlink", target.to_string_lossy().as_bytes().to_vec()));
                resolve_composition_symlink(root, &path, inventory)?
            } else {
                path.clone()
            };
            let bytes = fs::read(&effective)
                .map_err(|err| file_io_report(&path, "read source bytes", err))?;
            inventory.entries.push((relative, "file", bytes.clone()));
            inventory.dependencies.insert(effective, bytes);
        }
        Ok(())
    }

    // Inspect intermediate links as well as the final target; canonicalizing only
    // the last path would miss mutable hidden links in the chain. §FS-rhei-library.4.1
    fn resolve_composition_symlink(
        root: &Path,
        path: &Path,
        inventory: &mut CompositionInventory,
    ) -> MietteResult<PathBuf> {
        let mut pending = path.to_path_buf();
        for _ in 0..40 {
            let mut resolved = PathBuf::new();
            let mut components = pending.components();
            let mut redirected = false;
            while let Some(component) = components.next() {
                match component {
                    std::path::Component::CurDir => continue,
                    std::path::Component::ParentDir => { resolved.pop(); continue; }
                    std::path::Component::Prefix(_) | std::path::Component::RootDir => {
                        resolved.push(component.as_os_str());
                        continue;
                    }
                    _ => resolved.push(component.as_os_str()),
                }
                let metadata = fs::symlink_metadata(&resolved)
                    .map_err(|err| file_io_report(path, "resolve source symlink", err))?;
                if metadata.file_type().is_symlink() {
                    let target = fs::read_link(&resolved)
                        .map_err(|err| file_io_report(path, "read source symlink", err))?;
                    inventory.escaping |= !resolved.starts_with(root);
                    inventory.dependencies.insert(resolved.clone(), target.to_string_lossy().as_bytes().to_vec());
                    pending = resolved.parent().unwrap_or(root).join(target).join(components.as_path());
                    redirected = true;
                    break;
                }
            }
            if !redirected {
                inventory.escaping |= !resolved.starts_with(root);
                return Ok(resolved);
            }
        }
        Err(miette!(help = "remove the symlink cycle from the source block", "too many source symlinks resolving {}", path.display()))
    }

    #[cfg(test)]
    mod provenance_source_tests {
        use super::*;

        #[test]
        fn source_digest_ignores_creation_order_and_hidden_host_metadata() {
            let first = tempfile::tempdir().unwrap();
            let second = tempfile::tempdir().unwrap();
            fs::write(first.path().join("b"), "two").unwrap();
            fs::write(first.path().join("a"), "one").unwrap();
            fs::write(second.path().join("a"), "one").unwrap();
            fs::write(second.path().join("b"), "two").unwrap();
            fs::create_dir(first.path().join(".git")).unwrap();
            fs::write(first.path().join(".git/config"), "host-only").unwrap();
            assert_eq!(composition_inventory(first.path()).unwrap().digest(), composition_inventory(second.path()).unwrap().digest());
        }

        #[cfg(unix)]
        #[test]
        fn escaping_symlink_downgrades_exact_replay() {
            let dir = tempfile::tempdir().unwrap();
            let source = dir.path().join("source");
            fs::create_dir(&source).unwrap();
            fs::write(dir.path().join("outside"), "consumed bytes").unwrap();
            std::os::unix::fs::symlink("../outside", source.join("escape")).unwrap();
            assert!(composition_inventory(&source).unwrap().escaping);
        }
    }
