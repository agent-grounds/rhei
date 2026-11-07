//! The intake's unresolved-Prior fixture, in each mirrored host shape.
//! §FS-rhei-library.7.5

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::instantiate_project_validation_support::snapshot;
use super::*;

#[derive(Clone, Copy)]
pub enum Host {
    Single,
    Member,
    Directory,
}

pub struct Scenario {
    _root: TestDir,
    pub scope: PathBuf,
    pub target: PathBuf,
    pub machine: PathBuf,
    pub template: PathBuf,
    pub cwd: PathBuf,
    home: PathBuf,
    scratch: PathBuf,
    into: &'static str,
}

impl Scenario {
    pub fn new(host: Host) -> Self {
        let root = unique_temp_dir("into-validation-refusal");
        let home = root.join("home");
        let scratch = root.join("scratch");
        let template = home.join(".agent-grounds/rhei/templates/leak");
        let fixture = fixture_path("into-validation-refusal");
        copy_dir_recursive(&fixture.join("template"), &template);
        fs::create_dir(&scratch).unwrap();
        let (scope, into) = match host {
            Host::Single => (root.join("plan"), "plan"),
            Host::Member => (root.join("proj"), "member"),
            Host::Directory => (root.join("dirhost"), "dirhost"),
        };
        fs::create_dir(&scope).unwrap();
        let machine = scope.join("states.yaml");
        fs::copy(fixture.join("host.states.yaml"), &machine).unwrap();
        let target = match host {
            Host::Single | Host::Member => {
                let target = scope.join(format!("{into}.rhei.md"));
                fs::copy(fixture.join("host.plan.rhei.md"), &target).unwrap();
                if matches!(host, Host::Member) {
                    write_fixture_file(&scope, "index.panta.md", "# Panta: Scratch\n");
                }
                target
            }
            Host::Directory => {
                fs::create_dir(scope.join("tasks")).unwrap();
                write_fixture_file(&scope, "index.rhei.md", "# Rhei: Host\n");
                write_fixture_file(
                    &scope,
                    "tasks/001-mine.md",
                    "### Task 1: Mine\n**State:** pending\n",
                );
                scope.clone()
            }
        };
        // Match the report: single/member from inside, directory from its parent.
        let cwd = if matches!(host, Host::Directory) { root.to_path_buf() } else { scope.clone() };
        // Both host and template carry an artifact; refusal must neither alter
        // existing artifacts nor publish the template's bundled copy.
        fs::create_dir(scope.join("runtime")).unwrap();
        write_fixture_file(&scope, "runtime/kept.txt", "existing artifact\n");
        fs::create_dir(template.join("scripts")).unwrap();
        write_fixture_file(&template, "scripts/incoming.txt", "unpublished bundle\n");
        Self { _root: root, scope, target, machine, template, cwd, home, scratch, into }
    }

    pub fn run(&self, args: &[&str]) -> CliRun {
        let mut cmd = rhei_command(&self.home);
        // An outer orchestration's reservation/state must not reach a nested CLI.
        for (name, _) in std::env::vars_os() {
            if name.to_string_lossy().starts_with("RHEI_") {
                cmd.env_remove(name);
            }
        }
        let output = cmd
            .current_dir(&self.cwd)
            .env("XDG_CONFIG_HOME", self.home.join(".config"))
            .env("TMPDIR", &self.scratch)
            .env("TMP", &self.scratch)
            .env("TEMP", &self.scratch)
            .env("NO_COLOR", "1")
            .args(args)
            .output()
            .expect("run isolated rhei");
        CliRun::from(&output)
    }

    pub fn validate(&self) -> CliRun {
        self.run(&["validate", self.target.to_str().unwrap()])
    }

    pub fn place(&self, dry_run: bool) -> CliRun {
        let mut args = vec!["instantiate", "leak", "--into", self.into];
        if dry_run {
            args.push("--dry-run");
        }
        self.run(&args)
    }

    pub fn place_in_project(&self, dry_run: bool) -> CliRun {
        let mut args = vec!["instantiate", "leak", "--into", self.scope.to_str().unwrap()];
        if dry_run {
            args.push("--dry-run");
        }
        self.run(&args)
    }

    /// Match this scenario's mirror root, including diagnostics shortened from
    /// the invocation directory. An original ancestor named test-scratch is
    /// unrelated to this root. §FS-rhei-library.7.5
    pub fn exposes_mirror(&self, report: &str) -> bool {
        let mut relative = PathBuf::new();
        for _ in self.cwd.strip_prefix(&*self._root).unwrap().components() {
            relative.push("..");
        }
        relative.push("scratch");
        let canonical = fs::canonicalize(&self.scratch).unwrap();
        let report = report.replace('\\', "/");
        [&self.scratch, &canonical, &relative].iter().any(|path| {
            let spelling = format!("{}/", path.to_string_lossy().replace('\\', "/"));
            report.match_indices(&spelling).any(|(at, _)| {
                at == 0
                    || report[..at]
                        .chars()
                        .next_back()
                        .is_some_and(|c| c.is_whitespace() || ['\'', '"', '`', '('].contains(&c))
            })
        })
    }

    pub fn authored_bytes(&self) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
        snapshot(&self.scope)
            .into_iter()
            // Permanent sibling locks are normal command infrastructure.
            .filter(|(path, _)| path.extension().is_none_or(|ext| ext != "lock"))
            .collect()
    }

    pub fn assert_scratch_discarded(&self) {
        assert!(fs::read_dir(&self.scratch).unwrap().next().is_none());
    }

    pub fn names_original(&self, report: &str, path: &Path) -> bool {
        let relative = path.strip_prefix(&self.cwd).unwrap_or(path);
        // Compare separator-blind: Windows reports spell paths with `\`.
        let report = report.replace('\\', "/");
        [path, relative].iter().any(|path| {
            if path.as_os_str().is_empty() {
                return report.split_whitespace().any(|word| word.trim_matches(['\'', '`']) == ".");
            }
            let spelling = path.to_string_lossy().replace('\\', "/");
            report.contains(&spelling)
        })
    }
}
