//! The commit this rhei was built from, resolved once at build time.
//!
//! §FS-rhei-version.2: the build script and the unit tests run the same
//! [`resolve`], so the four sources are tried in one order everywhere: the
//! environment, the published crate's `.cargo_vcs_info.json`, git, `unknown`.
//! This file is included by `build.rs` through `#[path]` and is a module of the
//! library, so it uses nothing but `std`.

/// What the build knows about its source: a commit and whether the sources
/// differed from it, either of which may be unknown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildIdentity {
    pub commit: Option<String>,
    pub dirty: Option<bool>,
}

impl BuildIdentity {
    pub const UNKNOWN: BuildIdentity = BuildIdentity { commit: None, dirty: None };

    /// The `rhei version` value: `<40-hex>`, `<40-hex>-dirty` or `unknown`.
    pub fn version_line(&self) -> String {
        match (&self.commit, self.dirty) {
            (Some(commit), Some(true)) => format!("{commit}-dirty"),
            (Some(commit), _) => commit.clone(),
            (None, _) => "unknown".to_string(),
        }
    }

    /// The `rhei --version` parenthetical, without its parentheses.
    pub fn parenthetical(&self) -> String {
        match (&self.commit, self.dirty) {
            (Some(commit), Some(true)) => format!("{}, dirty", &commit[..7]),
            (Some(commit), _) => commit[..7].to_string(),
            (None, _) => "build unknown".to_string(),
        }
    }

    /// The `RHEI_BUILD_COMMIT` / `RHEI_BUILD_DIRTY` pair the build script sets,
    /// always both, so `env!` can never fail a build.
    pub fn env_values(&self) -> (String, String) {
        let commit = self.commit.clone().unwrap_or_else(|| "unknown".to_string());
        let dirty = match self.dirty {
            Some(true) => "true",
            Some(false) => "false",
            None => "unknown",
        };
        (commit, dirty.to_string())
    }

    /// Read back what [`Self::env_values`] wrote.
    pub fn from_env_values(commit: &str, dirty: &str) -> BuildIdentity {
        let commit = is_commit(commit).then(|| commit.to_string());
        let dirty = match dirty {
            "true" if commit.is_some() => Some(true),
            "false" if commit.is_some() => Some(false),
            _ => None,
        };
        BuildIdentity { commit, dirty }
    }
}

/// A full, lowercase, 40-digit commit id.
pub fn is_commit(text: &str) -> bool {
    text.len() == 40 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Resolve the build's identity from the first source that answers.
///
/// `env_commit`/`env_dirty` are `RHEI_BUILD_COMMIT`/`RHEI_BUILD_DIRTY`;
/// `vcs_info` is the text of `.cargo_vcs_info.json` beside the manifest, if
/// any; `git` runs `git --no-optional-locks <args>` in the manifest's directory
/// and answers its trimmed stdout, or `None` when git is missing or fails.
pub fn resolve(
    env_commit: Option<&str>,
    env_dirty: Option<&str>,
    vcs_info: Option<&str>,
    git: &mut dyn FnMut(&[&str]) -> Option<String>,
) -> BuildIdentity {
    if let Some(commit) = env_commit.map(str::trim).filter(|commit| is_commit(commit)) {
        let dirty = match env_dirty.map(str::trim) {
            Some("true") => Some(true),
            Some("false") => Some(false),
            _ => None,
        };
        return BuildIdentity { commit: Some(commit.to_string()), dirty };
    }
    if let Some(identity) = vcs_info.and_then(parse_vcs_info) {
        return identity;
    }
    from_git(git).unwrap_or(BuildIdentity::UNKNOWN)
}

/// `git.sha1` and `git.dirty` of a `.cargo_vcs_info.json`. `cargo package`
/// writes `dirty` only when it is true. Parsed by hand: the build script has
/// no dependencies.
fn parse_vcs_info(text: &str) -> Option<BuildIdentity> {
    let after = &text[text.find("\"sha1\"")? + "\"sha1\"".len()..];
    let after = after.trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
    let commit = &after[..after.find('"')?];
    if !is_commit(commit) {
        return None;
    }
    let dirty = text.find("\"dirty\"").is_some_and(|at| {
        let rest = text[at + "\"dirty\"".len()..].trim_start();
        rest.strip_prefix(':').is_some_and(|rest| rest.trim_start().starts_with("true"))
    });
    Some(BuildIdentity { commit: Some(commit.to_string()), dirty: Some(dirty) })
}

/// HEAD and the dirty check, consulted only when the repository git finds
/// tracks rhei-cli's own `Cargo.toml`, so a crate unpacked inside another
/// repository never borrows its HEAD.
fn from_git(git: &mut dyn FnMut(&[&str]) -> Option<String>) -> Option<BuildIdentity> {
    git(&["ls-files", "--error-unmatch", "--", "Cargo.toml"])?;
    let commit = git(&["rev-parse", "HEAD"]).filter(|commit| is_commit(commit))?;
    let status = git(&[
        "status",
        "--porcelain",
        "--",
        ":(top)crates",
        ":(top)Cargo.toml",
        ":(top)Cargo.lock",
    ]);
    let dirty = status.map(|status| !status.trim().is_empty());
    Some(BuildIdentity { commit: Some(commit), dirty })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA: &str = "d0446a7aa9c3e1f0b2d4c6a8e0f1a2b3c4d5e6f7";
    const OTHER: &str = "0123456789abcdef0123456789abcdef01234567";

    fn no_git(_: &[&str]) -> Option<String> {
        None
    }

    #[test]
    fn the_environment_pair_wins_over_every_other_source() {
        let vcs = format!(r#"{{"git": {{"sha1": "{OTHER}"}}}}"#);
        let mut git = |_: &[&str]| -> Option<String> { panic!("git must not be asked") };
        let identity = resolve(Some(SHA), Some("true"), Some(&vcs), &mut git);
        assert_eq!(identity, BuildIdentity { commit: Some(SHA.into()), dirty: Some(true) });
        assert_eq!(identity.version_line(), format!("{SHA}-dirty"));
        assert_eq!(identity.parenthetical(), "d0446a7, dirty");
    }

    #[test]
    fn a_published_crate_reads_its_vcs_info_dirty_or_not() {
        let clean = format!("{{\n  \"git\": {{\n    \"sha1\": \"{SHA}\"\n  }},\n  \"path_in_vcs\": \"crates/rhei-cli\"\n}}");
        let identity = resolve(None, None, Some(&clean), &mut no_git);
        assert_eq!(identity, BuildIdentity { commit: Some(SHA.into()), dirty: Some(false) });
        assert_eq!(identity.parenthetical(), "d0446a7");

        let dirty = format!(r#"{{"git": {{"sha1": "{SHA}", "dirty": true}}}}"#);
        let identity = resolve(None, None, Some(&dirty), &mut no_git);
        assert_eq!(identity.version_line(), format!("{SHA}-dirty"));
    }

    #[test]
    fn with_no_git_the_build_is_unknown() {
        let identity = resolve(None, None, None, &mut no_git);
        assert_eq!(identity, BuildIdentity::UNKNOWN);
        assert_eq!(identity.version_line(), "unknown");
        assert_eq!(identity.parenthetical(), "build unknown");
        assert_eq!(identity.env_values(), ("unknown".into(), "unknown".into()));
    }

    #[test]
    fn a_manifest_the_found_repository_does_not_track_is_unknown() {
        let mut git = |args: &[&str]| -> Option<String> {
            match args[0] {
                "ls-files" => None,
                "rev-parse" => Some(OTHER.into()),
                _ => Some(String::new()),
            }
        };
        assert_eq!(resolve(None, None, None, &mut git), BuildIdentity::UNKNOWN);
    }

    #[test]
    fn a_tracked_checkout_reads_head_and_the_dirty_check() {
        let mut git = |args: &[&str]| -> Option<String> {
            match args[0] {
                "ls-files" => Some("Cargo.toml".into()),
                "rev-parse" => Some(SHA.into()),
                "status" => Some("?? crates/rhei-cli/src/new.rs".into()),
                _ => None,
            }
        };
        let identity = resolve(None, None, None, &mut git);
        assert_eq!(identity, BuildIdentity { commit: Some(SHA.into()), dirty: Some(true) });
    }

    #[test]
    fn env_values_round_trip() {
        let identity = BuildIdentity { commit: Some(SHA.into()), dirty: Some(false) };
        let (commit, dirty) = identity.env_values();
        assert_eq!(BuildIdentity::from_env_values(&commit, &dirty), identity);
        assert_eq!(BuildIdentity::from_env_values("unknown", "unknown"), BuildIdentity::UNKNOWN);
    }
}
