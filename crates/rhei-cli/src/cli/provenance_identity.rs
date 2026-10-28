// What a run and a lay say about where they came from: the build, a
// repository's commit and remote, and canonical JSON for the digests and the
// checkpoint document.

/// The commit this binary was built from, as build.rs resolved it.
/// §FS-rhei-version.2
fn rhei_build_identity() -> build_identity::BuildIdentity {
    build_identity::BuildIdentity::from_env_values(
        env!("RHEI_BUILD_COMMIT"),
        env!("RHEI_BUILD_DIRTY"),
    )
}

/// The `rhei_build` object of run.json, `run_started` and the invocation
/// records. §FS-rhei-run-headless.2
fn rhei_build_record() -> rhei_tui::RheiBuild {
    let identity = rhei_build_identity();
    rhei_tui::RheiBuild {
        version: env!("CARGO_PKG_VERSION").to_string(),
        commit: identity.commit,
        dirty: identity.dirty,
    }
}

/// What one git command answered: its trimmed stdout, that the directory is
/// in no repository, or that git could not answer at all.
#[derive(Debug, Clone, PartialEq, Eq)]
enum GitAnswer {
    Out(String),
    NotARepository,
    Failed,
}

impl GitAnswer {
    fn out(self) -> Option<String> {
        match self {
            GitAnswer::Out(text) => Some(text),
            _ => None,
        }
    }
}

/// `git --no-optional-locks -C <dir> <args>`, so asking never writes the index.
/// §FS-rhei-templates.6.1.5
fn git_answer(dir: &Path, args: &[&str]) -> GitAnswer {
    let output = std::process::Command::new("git")
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdin(std::process::Stdio::null())
        .output();
    let Ok(output) = output else {
        return GitAnswer::Failed;
    };
    if output.status.success() {
        return GitAnswer::Out(String::from_utf8_lossy(&output.stdout).trim_end().to_string());
    }
    if String::from_utf8_lossy(&output.stderr).contains("not a git repository") {
        GitAnswer::NotARepository
    } else {
        GitAnswer::Failed
    }
}

/// The resolved top level of the repository holding `dir`.
fn repository_top(dir: &Path) -> GitAnswer {
    match git_answer(dir, &["rev-parse", "--show-toplevel"]) {
        GitAnswer::Out(top) if !top.is_empty() => {
            let top = PathBuf::from(top);
            GitAnswer::Out(
                rhei_core::platform::canonical_path(&top)
                    .unwrap_or(top)
                    .to_string_lossy()
                    .into_owned(),
            )
        }
        GitAnswer::Out(_) => GitAnswer::Failed,
        other => other,
    }
}

/// HEAD of the repository holding `dir`, with its remote reduced: the
/// `upstream` remote, else `origin`. `None` when no commit is known.
/// §FS-rhei-templates.6.1.5 §FS-rhei-run-headless.2
fn repository_head(dir: &Path) -> Option<rhei_tui::CheckoutHead> {
    let commit = git_answer(dir, &["rev-parse", "HEAD"]).out()?;
    if !build_identity::is_commit(&commit) {
        return None;
    }
    let repo = ["upstream", "origin"].iter().find_map(|remote| {
        let key = format!("remote.{remote}.url");
        git_answer(dir, &["config", "--get", &key]).out().map(|url| reduce_remote(&url))
    });
    Some(rhei_tui::CheckoutHead { repo: repo.flatten(), commit })
}

/// A remote URL reduced to what may be published: `owner/repo`, its host kept
/// in front when it is not `github.com`, with the scheme, any credentials, a
/// port, a `.git` suffix and an ssh user stripped. A local path or a `file://`
/// URL reduces to nothing. §FS-rhei-summary.4
fn reduce_remote(url: &str) -> Option<String> {
    let url = url.trim();
    let (host, path) = if let Some((scheme, rest)) = url.split_once("://") {
        if scheme.eq_ignore_ascii_case("file") {
            return None;
        }
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        (authority.rsplit('@').next().unwrap_or(authority), path)
    } else {
        // scp-like `user@host:owner/repo`; a Windows drive or a path is local.
        let (head, path) = url.split_once(':')?;
        if head.len() <= 1 || head.contains('/') || head.contains('\\') {
            return None;
        }
        (head.rsplit('@').next().unwrap_or(head), path)
    };
    let host = host.split(':').next().unwrap_or(host).to_ascii_lowercase();
    let path = path.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path).trim_matches('/');
    if host.is_empty() || path.is_empty() {
        return None;
    }
    Some(if host == "github.com" { path.to_string() } else { format!("{host}/{path}") })
}

/// The checkout HEAD a run starts on, or `None` when the workspace is in no
/// repository or git cannot answer. §FS-rhei-run-headless.2
fn checkout_head_at(repo_root: &Path) -> Option<rhei_tui::CheckoutHead> {
    repository_head(repo_root)
}

/// JSON with object keys in sorted order and no insignificant whitespace, the
/// one encoding the inputs digest and the checkpoint document use.
/// §FS-rhei-templates.6.1.5 §FS-rhei-summary.6.2
fn canonical_json_text(value: &serde_json::Value) -> String {
    serde_json::to_string(&canonical_json(value.clone())).unwrap_or_default()
}

#[cfg(test)]
mod provenance_identity_tests {
    use super::*;

    #[test]
    fn remotes_reduce_to_owner_and_repo_and_paths_to_nothing() {
        // §FS-rhei-summary.4
        let cases = [
            ("https://github.com/example-org/templates.git", Some("example-org/templates")),
            ("https://user:token@github.com/o/r", Some("o/r")),
            ("git@github.com:o/r.git", Some("o/r")),
            ("ssh://git@gitlab.example.org:2222/o/r.git", Some("gitlab.example.org/o/r")),
            ("https://GitHub.com/o/r/", Some("o/r")),
            ("file:///home/u/remote.git", None),
            ("/home/u/remote.git", None),
            ("../remote.git", None),
            ("C:\\repos\\remote.git", None),
            ("C:/repos/remote.git", None),
        ];
        for (url, reduced) in cases {
            assert_eq!(reduce_remote(url).as_deref(), reduced, "{url}");
        }
    }

    #[test]
    fn canonical_json_sorts_nested_keys_and_drops_whitespace() {
        let value = serde_json::json!({"b": {"z": 1, "a": [true, null]}, "a": "x"});
        assert_eq!(canonical_json_text(&value), r#"{"a":"x","b":{"a":[true,null],"z":1}}"#);
    }
}
