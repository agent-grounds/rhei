struct RunGitConsistencyGuard {
    repo_root: Option<PathBuf>,
    head_before: Option<String>,
    tracked_paths: Vec<PathBuf>,
}

impl RunGitConsistencyGuard {
    fn capture(workspace_root: &Path, input: &Path, enabled: bool) -> Self {
        if !enabled {
            return Self { repo_root: None, head_before: None, tracked_paths: Vec::new() };
        }

        let Some(repo_root) = git_toplevel(workspace_root) else {
            return Self { repo_root: None, head_before: None, tracked_paths: Vec::new() };
        };
        let Ok(head_before) = git_head(&repo_root) else {
            return Self { repo_root: None, head_before: None, tracked_paths: Vec::new() };
        };
        let tracked_paths = vec![
            git_status_pathspec(&repo_root, input),
            git_status_pathspec(&repo_root, &workspace_root.join("runtime").join("results")),
        ];

        Self { repo_root: Some(repo_root), head_before: Some(head_before), tracked_paths }
    }

    /// The commit the guard captured, with the repository's reduced remote:
    /// one capture serves both the guard and the run's provenance.
    /// §FS-rhei-run-headless.2
    fn checkout_head_at_start(&self) -> Option<rhei_tui::CheckoutHead> {
        let (repo_root, head) = (self.repo_root.as_deref()?, self.head_before.as_deref()?);
        let head = checkout_head_at(repo_root)
            .filter(|found| found.commit == head)
            .unwrap_or(rhei_tui::CheckoutHead { repo: None, commit: head.to_string() });
        build_identity::is_commit(&head.commit).then_some(head)
    }

    fn verify_after_success(&self) -> MietteResult<()> {
        // §FS-rhei-run.3.1 §AR-agent-orchestrator-workflow.3.4: detect stale HEAD after run-owned writes.
        let (Some(repo_root), Some(head_before)) = (&self.repo_root, &self.head_before) else {
            return Ok(());
        };
        let Ok(head_after) = git_head(repo_root) else {
            return Ok(());
        };
        if head_after == *head_before {
            return Ok(());
        }

        let dirty = git_dirty_tracked_paths(repo_root, &self.tracked_paths)?;
        if dirty.is_empty() {
            return Ok(());
        }

        Err(miette!(
            help = git_worktree_help(),
            "rhei run observed HEAD move from {} to {}, but tracked Rhei-owned plan/result paths remain uncommitted:\n  {}\n\nThe worktree reflects the latest orchestrator transitions, but HEAD does not. Commit or revert these paths before treating HEAD as durable plan state.",
            short_git_head(head_before),
            short_git_head(&head_after),
            dirty.join("\n  ")
        ))
    }
}

fn git_head(repo_root: &Path) -> Result<String, String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("rev-parse")
        .arg("HEAD")
        .output()
        .map_err(|err| err.to_string())?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("git rev-parse HEAD exited with {}", output.status)
        } else {
            stderr
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn git_dirty_tracked_paths(repo_root: &Path, paths: &[PathBuf]) -> MietteResult<Vec<String>> {
    let mut command = std::process::Command::new("git");
    command
        .arg("-C")
        .arg(repo_root)
        .arg("status")
        .arg("--porcelain=v1")
        .arg("--untracked-files=no")
        .arg("--");
    for path in paths {
        command.arg(path);
    }

    let output = command
        .output()
        .map_err(|err| miette!(
            help = git_worktree_help(),
            "failed to inspect git worktree status: {err}"
        ))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(miette!(
            help = git_worktree_help(),
            "failed to inspect git worktree status: {}",
            if stderr.is_empty() {
                output.status.to_string()
            } else {
                stderr
            }
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .filter_map(|line| line.get(3..).or(Some(line)))
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect())
}

fn short_git_head(head: &str) -> &str {
    head.get(..12).unwrap_or(head)
}

fn git_status_pathspec(repo_root: &Path, path: &Path) -> PathBuf {
    // §AR-agent-orchestrator-workflow.3.4: nested relative invocations must target the real Rhei path.
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().map(|cwd| cwd.join(path)).unwrap_or_else(|_| path.to_path_buf())
    };
    let normalized = rhei_core::platform::canonical_path(&absolute).unwrap_or(absolute);
    normalized.strip_prefix(repo_root).map(Path::to_path_buf).unwrap_or(normalized)
}
