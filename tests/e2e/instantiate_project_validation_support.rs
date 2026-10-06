//! The intake fixture for §FS-rhei-templates.6.1.2: a valid workspace template
//! and an unrelated existing sibling with a chapter after `## Tasks`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::*;

pub struct Scenario {
    _root: TestDir,
    pub project: PathBuf,
    pub template: PathBuf,
    pub cwd: PathBuf,
    home: PathBuf,
    scratch: PathBuf,
}

impl Scenario {
    pub fn new() -> Self {
        let root = unique_temp_dir("instantiate-project-validation");
        let project = root.join("project");
        let template = root.join("mini");
        let cwd = root.join("launcher");
        let home = root.join("home");
        let scratch = root.join("scratch");
        for dir in [&project, &cwd, &home, &scratch] {
            fs::create_dir_all(dir).expect("create isolated fixture directories");
        }
        copy_dir_recursive(&fixture_path("instantiate-project-validation/mini"), &template);
        write_fixture_file(&project, "index.panta.md", "# Panta: proj\n");
        fs::create_dir_all(project.join(".agent-grounds/rhei")).expect("create settings home");
        write_fixture_file(
            &project,
            ".agent-grounds/rhei/settings.json",
            "{\"defaults\":{\"agent_timeout\":\"42m\"}}\n",
        );
        Self { _root: root, project, template, cwd, home, scratch }
    }

    pub fn output(&self) -> PathBuf {
        self.project.join("new")
    }

    pub fn run(&self, flags: &[&str]) -> CliRun {
        let output = rhei_command(&self.home)
            .current_dir(&self.cwd)
            .env("TMPDIR", &self.scratch)
            .env("TMP", &self.scratch)
            .env("TEMP", &self.scratch)
            .arg("instantiate")
            .arg(&self.template)
            .args(flags)
            .output()
            .expect("run instantiate");
        assert!(
            fs::read_dir(&self.scratch).unwrap().next().is_none(),
            "instantiation must discard scratch even on refusal with --keep-on-error"
        );
        CliRun::from(&output)
    }

    pub fn output_run(&self, flags: &[&str]) -> CliRun {
        let output = self.output();
        let mut args = vec!["--output", output.to_str().expect("fixture output path")];
        args.extend_from_slice(flags);
        self.run(&args)
    }

    pub fn add_broken_sibling(&self) {
        fs::copy(
            fixture_path("instantiate-project-validation/broken-sibling.rhei.md"),
            self.project.join("broken-sibling.rhei.md"),
        )
        .expect("copy the intake's broken sibling");
    }

    pub fn add_incoming_settings(&self) {
        write_fixture_file(
            &self.template,
            "settings.json",
            "{\"defaults\":{\"agent_timeout\":\"15m\",\"transition_limit\":50}}\n",
        );
    }

    pub fn clean_control(&self) {
        let before = snapshot(&self.project);
        let control = self.output_run(&[]);
        assert_success(&control);
        assert!(control.stdout.contains("Added to the Panta project"), "{}", control.stdout);
        assert!(self.output().join("index.rhei.md").is_file());
        fs::remove_dir_all(self.output()).expect("remove the clean control member");
        assert_eq!(snapshot(&self.project), before, "clean control preserves existing bytes");
    }
}

/// Include directories as well as bytes: creating an empty settings home or
/// leaving hidden staging is a mutation too. Homes and scratch are outside it.
pub fn snapshot(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    fn walk(root: &Path, dir: &Path, entries: &mut BTreeMap<PathBuf, Option<Vec<u8>>>) {
        for entry in fs::read_dir(dir).expect("read project snapshot") {
            let entry = entry.expect("snapshot entry");
            let path = entry.path();
            let is_dir = entry.file_type().expect("snapshot file type").is_dir();
            let bytes = if is_dir { None } else { Some(fs::read(&path).expect("snapshot bytes")) };
            entries.insert(path.strip_prefix(root).expect("relative path").to_path_buf(), bytes);
            if is_dir {
                walk(root, &path, entries);
            }
        }
    }
    let mut entries = BTreeMap::new();
    walk(root, root, &mut entries);
    entries
}

pub fn assert_refused_for_sibling(run: &CliRun) {
    assert_eq!(run.status.code(), Some(1), "expected refusal:\n{}\n{}", run.stdout, run.stderr);
    assert!(run.stderr.contains("broken-sibling.rhei.md"), "{}", run.stderr);
    assert!(run.stderr.contains("Tasks section must be the final"), "{}", run.stderr);
    assert!(run.stderr.contains("## Appendix"), "parser context is preserved:\n{}", run.stderr);
}

pub fn assert_sibling_remedy(scenario: &Scenario, run: &CliRun) {
    assert_refused_for_sibling(run);
    assert!(
        run.stderr.to_lowercase().contains("existing sibling"),
        "refusal must distinguish the existing sibling from the instantiated output:\n{}",
        run.stderr
    );
    let sibling = scenario.project.join("broken-sibling.rhei.md");
    let relative = Path::new("..").join("project").join("broken-sibling.rhei.md");
    assert!(
        [sibling, relative].iter().any(|path| {
            run.stderr.contains(&format!("rhei validate {}", shell_quote(&path.to_string_lossy())))
        }),
        "remedy must identify the inspectable sibling from the invocation directory:\n{}",
        run.stderr
    );
    assert!(!run.stderr.contains("rhei validate <plan>"), "{}", run.stderr);
}
