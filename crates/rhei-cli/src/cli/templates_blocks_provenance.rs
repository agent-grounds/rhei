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
        let inventory = composition_inventory(dir)?;
        let content = format!("sha256:{}", inventory.digest());
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

        let revision = match git_source_revision(dir, &content, &inventory) {
            Ok(Some(revision)) => revision,
            discovered => {
                let unavailable = discovered.is_err() || broken_git_marker(dir);
                SourceRevision {
                    block_path: None,
                    commit: None,
                    content: Some(content),
                    kind: if unavailable { "unavailable" } else { "local" }.into(),
                    replay: "not-guaranteed".into(),
                    release: None,
                    status: if unavailable { "unavailable" } else { "unversioned" }.into(),
                    template: None,
                }
            }
        };
        Ok(SourceIdentity { locator, revision })
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

    // Metadata discovery failure is nonfatal; source reads above remain fallible. §FS-rhei-library.4.1
    fn git_source_revision(
        dir: &Path, content: &str, inventory: &CompositionInventory,
    ) -> Result<Option<SourceRevision>, ()> {
        let root = git_repository_root(dir)?;
        let Some(root) = root else { return Ok(None) };
        let root = fs::canonicalize(root.trim()).map_err(|_| ())?;
        let commit = git_output(&root, &["rev-parse", "--verify", "HEAD^{commit}"])?;
        let commit = commit.ok_or(())?;
        let block_path = dir
            .strip_prefix(&root)
            .ok()
            .map(slash_path)
            .filter(|path| !path.is_empty());
        let pathspec = block_path.as_deref().unwrap_or(".");
        let status = git_output(&root, &["status", "--porcelain=v1", "--untracked-files=all", "--", pathspec])?
            .ok_or(())?;
        let mut dirty = false;
        let mut untracked = false;
        for line in status.lines() {
            if line.starts_with("??") {
                untracked = true;
            } else if !line.trim().is_empty() {
                dirty = true;
            }
        }
        // Compare consumed bytes to HEAD, including hidden symlink dependencies.
        // Index membership alone cannot establish immutable replay. §FS-rhei-library.4.1
        for (path, bytes) in &inventory.dependencies {
            let Ok(relative) = path.strip_prefix(&root) else {
                untracked = true;
                continue;
            };
            let object = format!("{}:{}", commit.trim(), slash_path(relative));
            let recorded = git_bytes(&root, &["cat-file", "blob", &object])?;
            match recorded {
                None => untracked = true,
                Some(recorded) if &recorded != bytes => dirty = true,
                Some(_) => {}
            }
        }
        let status = match (dirty, untracked) {
            (false, false) => "clean",
            (true, false) => "dirty",
            (false, true) => "untracked",
            (true, true) => "dirty-untracked",
        };
        let escaping = inventory.escaping;
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

    fn git_bytes(dir: &Path, args: &[&str]) -> Result<Option<Vec<u8>>, ()> {
        let output = std::process::Command::new("git")
            .arg("-C").arg(dir).args(args).output().map_err(|_| ())?;
        Ok(output.status.success().then_some(output.stdout))
    }

    fn git_repository_root(dir: &Path) -> Result<Option<String>, ()> {
        let output = std::process::Command::new("git")
            .env("LC_ALL", "C")
            .arg("-C").arg(dir).args(["rev-parse", "--show-toplevel"])
            .output().map_err(|_| ())?;
        if output.status.success() {
            Ok(Some(String::from_utf8_lossy(&output.stdout).into_owned()))
        } else if String::from_utf8_lossy(&output.stderr).contains("not a git repository") {
            Ok(None)
        } else {
            Err(())
        }
    }

    fn git_output(dir: &Path, args: &[&str]) -> Result<Option<String>, ()> {
        git_bytes(dir, args).map(|output| output.map(|bytes| String::from_utf8_lossy(&bytes).into_owned()))
    }

    fn broken_git_marker(dir: &Path) -> bool {
        // The fallback must obey the same discovery boundary as Git. §FS-rhei-library.4.1
        let ceilings = std::env::var_os("GIT_CEILING_DIRECTORIES")
            .map(|value| std::env::split_paths(&value)
                .filter_map(|path| fs::canonicalize(path).ok()).collect::<Vec<_>>())
            .unwrap_or_default();
        for ancestor in dir.ancestors() {
            if ancestor != dir && ceilings.iter().any(|ceiling| ceiling == ancestor) {
                break;
            }
            if ancestor.join(".git").exists() {
                return true;
            }
        }
        false
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
