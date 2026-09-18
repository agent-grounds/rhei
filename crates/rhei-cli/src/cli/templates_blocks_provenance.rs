    // Source identity is established once at resolution and carried through
    // the typed compiler; flattened output never has to reconstruct it.
    // §FS-rhei-library.4.1 §AR-rhei-library.2
    fn composition_source_identity(
        requested: &str,
        resolved: &ResolvedTemplate,
        dir: &Path,
    ) -> MietteResult<SourceIdentity> {
        let portable = !Path::new(requested).is_absolute() && !requested.starts_with('~');
        let tier = composition_source_tier(requested, resolved, dir)?;
        let content = format!("sha256:{}", composition_tree_digest(dir)?);
        let locator = SourceLocator {
            portable,
            requested: requested.replace('\\', "/"),
            resolved: portable.then(|| requested.replace('\\', "/")),
            tier: tier.into(),
        };

        if resolved._extracted.is_some() {
            return Ok(SourceIdentity {
                locator,
                revision: SourceRevision {
                    block_path: None,
                    commit: None,
                    content: Some(content),
                    kind: "built-in".into(),
                    replay: "exact".into(),
                    release: Some(env!("CARGO_PKG_VERSION").into()),
                    status: "shipped".into(),
                    template: Some(requested.into()),
                },
            });
        }

        match git_source_revision(dir, &content)? {
            Some(revision) => Ok(SourceIdentity { locator, revision }),
            None if broken_git_marker(dir) => Ok(SourceIdentity {
                locator,
                revision: SourceRevision {
                    block_path: None,
                    commit: None,
                    content: Some(content),
                    kind: "unavailable".into(),
                    replay: "not-guaranteed".into(),
                    release: None,
                    status: "unavailable".into(),
                    template: None,
                },
            }),
            None => Ok(SourceIdentity {
                locator,
                revision: SourceRevision {
                    block_path: None,
                    commit: None,
                    content: Some(content),
                    kind: "local".into(),
                    replay: "not-guaranteed".into(),
                    release: None,
                    status: "unversioned".into(),
                    template: None,
                },
            }),
        }
    }

    fn composition_source_tier(
        requested: &str,
        resolved: &ResolvedTemplate,
        dir: &Path,
    ) -> MietteResult<&'static str> {
        if resolved._extracted.is_some() {
            return Ok(TemplateSource::Builtin.as_str());
        }
        if template_reference_is_path(requested) {
            return Ok("path");
        }
        for (source, root) in template_search_roots(TemplateSourceFilter::All)? {
            if source != TemplateSource::Builtin
                && fs::canonicalize(root.path().join(requested)).ok().as_deref() == Some(dir)
            {
                return Ok(source.as_str());
            }
        }
        Ok("path")
    }

    fn git_source_revision(dir: &Path, content: &str) -> MietteResult<Option<SourceRevision>> {
        let root = git_output(dir, &["rev-parse", "--show-toplevel"])?;
        let Some(root) = root else { return Ok(None) };
        let root = PathBuf::from(root.trim());
        let commit = git_output(&root, &["rev-parse", "HEAD"])?;
        let Some(commit) = commit else { return Ok(None) };
        let block_path = dir
            .strip_prefix(&root)
            .ok()
            .map(slash_path)
            .filter(|path| !path.is_empty());
        let pathspec = block_path.as_deref().unwrap_or(".");
        let status = git_output(&root, &["status", "--porcelain=v1", "--untracked-files=all", "--", pathspec])?
            .unwrap_or_default();
        let mut dirty = false;
        let mut untracked = false;
        for line in status.lines() {
            if line.starts_with("??") {
                untracked = true;
            } else if !line.trim().is_empty() {
                dirty = true;
            }
        }
        if !composition_entries_are_tracked(&root, dir)? {
            untracked = true;
        }
        let status = match (dirty, untracked) {
            (false, false) => "clean",
            (true, false) => "dirty",
            (false, true) => "untracked",
            (true, true) => "dirty-untracked",
        };
        let escaping = tree_has_escaping_symlink(dir)?;
        Ok(Some(SourceRevision {
            block_path,
            commit: Some(commit.trim().into()),
            content: Some(content.into()),
            kind: "git".into(),
            replay: if status == "clean" && !escaping { "exact" } else { "not-guaranteed" }.into(),
            release: None,
            status: status.into(),
            template: None,
        }))
    }

    fn git_output(dir: &Path, args: &[&str]) -> MietteResult<Option<String>> {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .map_err(|err| miette!(help = "install Git or use a local source without Git metadata", "inspect composition source revision: {err}"))?;
        Ok(output.status.success().then(|| String::from_utf8_lossy(&output.stdout).into_owned()))
    }

    fn composition_entries_are_tracked(repository: &Path, source: &Path) -> MietteResult<bool> {
        let mut entries = Vec::new();
        collect_composition_entries(source, source, &mut entries)?;
        for (relative, _, _) in entries {
            let path = source.join(relative);
            let path = path.strip_prefix(repository).unwrap_or(&path);
            let output = std::process::Command::new("git")
                .arg("-C")
                .arg(repository)
                .args(["ls-files", "--error-unmatch", "--"])
                .arg(path)
                .output()
                .map_err(|err| miette!("inspect tracked composition source: {err}"))?;
            if !output.status.success() {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn broken_git_marker(dir: &Path) -> bool {
        dir.ancestors().any(|ancestor| ancestor.join(".git").exists())
    }

    fn composition_tree_digest(root: &Path) -> MietteResult<String> {
        use sha2::{Digest, Sha256};
        let mut entries = Vec::<(String, &'static str, Vec<u8>)>::new();
        collect_composition_entries(root, root, &mut entries)?;
        entries.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
        let mut digest = Sha256::new();
        for (path, kind, bytes) in entries {
            for field in [path.as_bytes(), kind.as_bytes(), bytes.as_slice()] {
                digest.update((field.len() as u64).to_be_bytes());
                digest.update(field);
            }
        }
        Ok(format!("{:x}", digest.finalize()))
    }

    fn collect_composition_entries(
        root: &Path,
        dir: &Path,
        entries: &mut Vec<(String, &'static str, Vec<u8>)>,
    ) -> MietteResult<()> {
        for entry in fs::read_dir(dir).map_err(|err| file_io_report(dir, "read source inventory", err))? {
            let path = entry
                .map_err(|err| miette!("read source inventory entry: {err}"))?
                .path();
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with('.'))
            {
                continue;
            }
            let metadata = fs::symlink_metadata(&path)
                .map_err(|err| file_io_report(&path, "inspect source inventory", err))?;
            if metadata.file_type().is_symlink() {
                let target = fs::read_link(&path)
                    .map_err(|err| file_io_report(&path, "read source symlink", err))?;
                entries.push((slash_path(path.strip_prefix(root).unwrap()), "symlink", target.as_os_str().to_string_lossy().as_bytes().to_vec()));
            } else if metadata.is_dir() {
                collect_composition_entries(root, &path, entries)?;
            } else if metadata.is_file() {
                let bytes = fs::read(&path).map_err(|err| file_io_report(&path, "read source bytes", err))?;
                entries.push((slash_path(path.strip_prefix(root).unwrap()), "file", bytes));
            }
        }
        Ok(())
    }

    fn tree_has_escaping_symlink(root: &Path) -> MietteResult<bool> {
        fn visit(root: &Path, dir: &Path) -> std::io::Result<bool> {
            for entry in fs::read_dir(dir)? {
                let path = entry?.path();
                if path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with('.'))
                {
                    continue;
                }
                let metadata = fs::symlink_metadata(&path)?;
                if metadata.file_type().is_symlink() {
                    let target = fs::read_link(&path)?;
                    let target = if target.is_absolute() {
                        target
                    } else {
                        path.parent().unwrap_or(root).join(target)
                    };
                    if !lexically_normal(&target).starts_with(lexically_normal(root)) {
                        return Ok(true);
                    }
                } else if metadata.is_dir() && visit(root, &path)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        visit(root, root).map_err(|err| file_io_report(root, "inspect source symlinks", err))
    }

    fn lexically_normal(path: &Path) -> PathBuf {
        let mut result = PathBuf::new();
        for component in path.components() {
            match component {
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    result.pop();
                }
                other => result.push(other.as_os_str()),
            }
        }
        result
    }

    fn slash_path(path: &Path) -> String {
        path.components()
            .filter_map(|component| match component {
                std::path::Component::Normal(value) => Some(value.to_string_lossy()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/")
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
            assert_eq!(
                composition_tree_digest(first.path()).unwrap(),
                composition_tree_digest(second.path()).unwrap()
            );
        }

        #[cfg(unix)]
        #[test]
        fn escaping_symlink_downgrades_exact_replay() {
            use std::os::unix::fs::symlink;
            let dir = tempfile::tempdir().unwrap();
            let source = dir.path().join("source");
            fs::create_dir(&source).unwrap();
            symlink("../outside", source.join("escape")).unwrap();
            assert!(tree_has_escaping_symlink(&source).unwrap());
        }
    }
