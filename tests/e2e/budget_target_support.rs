//! The world the `rhei budget` target cases share: a home and a machine that sit
//! beside the targets, and a way to run a line rhei printed as it was printed.
//!
//! The comparison is with `rhei list` itself rather than with a sentence held
//! here, so these cases cannot drift from the refusal they are about.

// §FS-rhei-budgets.10.1
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use super::budget_support::*;
use super::*;

/// The text of a plan that parses, for a file refused for its name alone.
pub(super) const PLAN_TEXT: &str = "# Rhei: P\n\n## Tasks\n\n### Task 1: A\n**State:** pending\n";

/// What a file in a workspace's `tasks/` holds, which is no plan by itself.
pub(super) const TASK_FILE_TEXT: &str = "### Task 1: A\n**State:** pending\n";

pub(super) struct World {
    pub(super) dir: TestDir,
}

pub(super) fn world(prefix: &str) -> World {
    let dir = unique_temp_dir(prefix);
    let agent = write_python_agent(&dir, "mock-agent.py", FINISHING_AGENT);
    write_machine_settings(&dir, &agent_defaults(&agent, ""));
    World { dir }
}

impl World {
    /// `relative` under this world, written with `text`, its directories made.
    pub(super) fn write(&self, relative: &str, text: &str) -> PathBuf {
        let path = self.dir.join(relative);
        fs::create_dir_all(path.parent().expect("a parent")).expect("create the directory");
        fs::write(&path, text).expect("write the fixture");
        path
    }

    /// A directory holding one plan and no manifest: what the issue names.
    pub(super) fn plain(&self, relative: &str) {
        self.write(&format!("{relative}/plan.rhei.md"), PLAN_TEXT);
    }

    /// A Directory Workspace holding one ticket.
    pub(super) fn workspace(&self, relative: &str) {
        self.write(&format!("{relative}/index.rhei.md"), "# Rhei: W\n");
        self.write(&format!("{relative}/tasks/01-a.md"), TASK_FILE_TEXT);
    }

    /// A Panta Project with the named member rheis.
    pub(super) fn project(&self, relative: &str, members: &[&str]) {
        self.write(&format!("{relative}/index.panta.md"), "# Panta: X\n");
        for member in members {
            self.write(&format!("{relative}/{member}.rhei.md"), PLAN_TEXT);
        }
    }

    /// Every `rhei` these cases spawn.
    pub(super) fn rhei(&self) -> Command {
        let mut cmd = rhei_command(home_for(&self.dir));
        cmd.current_dir(&*self.dir);
        cmd
    }

    pub(super) fn run(&self, args: &[&str]) -> Output {
        self.rhei().args(args).output().expect("rhei should run")
    }

    pub(super) fn run_in(&self, cwd: &Path, args: &[&str]) -> Output {
        self.rhei().current_dir(cwd).args(args).output().expect("rhei should run")
    }

    /// `rhei budget init` of `target`, which has to work for the case to mean anything.
    pub(super) fn init(&self, target: &str) {
        let args = ["budget", "init", target, "--invocations", "5", "--reason", "seed"];
        let out = self.run(&args);
        assert!(out.status.success(), "`{args:?}` seeds the account:\n{}", raw_stderr(&out));
    }

    /// Every file under `relative` and under this machine's state directory,
    /// byte for byte: an account, a witness, an index or a lock a refusal wrote
    /// shows up here wherever it landed.
    pub(super) fn bytes_under(&self, relative: &str) -> BTreeMap<PathBuf, Vec<u8>> {
        let mut found = BTreeMap::new();
        collect_bytes(&self.dir.join(relative), &mut found);
        collect_bytes(&home_for(&self.dir).join("state"), &mut found);
        found
    }

    /// The `project_id` `rhei budget show <target>` reports, which is the account.
    pub(super) fn project_id(&self, target: &str) -> String {
        let out = self.run(&["budget", "show", target, "--format", "json"]);
        assert!(out.status.success(), "`show {target}` reads the account:\n{}", raw_stderr(&out));
        let report: serde_json::Value =
            serde_json::from_slice(&out.stdout).expect("`show --format json` prints JSON");
        report["project_id"].as_str().expect("a project_id").to_owned()
    }

    /// Require that `target` is refused by all four verbs from `cwd`, in the first
    /// line `refusal` holds, with nothing under `watched` or the state directory
    /// changed by any of them.
    #[track_caller]
    pub(super) fn assert_every_verb_refuses(
        &self,
        cwd: &Path,
        target: &str,
        refusal: &str,
        watched: &str,
    ) {
        let before = self.bytes_under(watched);
        for verb in every_verb(target) {
            let out = self.run_in(cwd, &verb);
            let said = raw_stderr(&out);
            assert_eq!(out.status.code(), Some(1), "`{verb:?}` should refuse:\n{said}");
            assert_eq!(first_line(&out), refusal, "`{verb:?}` should say what list says:\n{said}");
            assert!(
                self.bytes_under(watched) == before,
                "`{verb:?}` should change nothing under {watched} or the state directory"
            );
        }
    }

    /// What `rhei list` says of `target`, which it has to refuse.
    #[track_caller]
    pub(super) fn list_refusal(&self, cwd: &Path, target: &str) -> String {
        let out = self.run_in(cwd, &["list", target]);
        assert_eq!(
            out.status.code(),
            Some(1),
            "the premise is that `rhei list {target}` refuses:\n{}",
            raw_stderr(&out)
        );
        first_line(&out)
    }

    /// What `rhei list` says of a file refused for its name, read with text that
    /// parses in it, because on any other text `rhei list` reports the parse first.
    /// The file's own text is back in place afterwards. §FS-rhei-budgets.10.1
    #[track_caller]
    pub(super) fn list_refusal_for_name(&self, target: &str) -> String {
        let path = self.dir.join(target);
        let own = fs::read(&path).expect("read the file");
        fs::write(&path, PLAN_TEXT).expect("give the file a plan's text");
        let refusal = self.list_refusal(&self.dir, target);
        fs::write(&path, own).expect("put the file's text back");
        refusal
    }

    /// Run a line rhei printed, in the platform's own shell, from this world's
    /// directory, finding the binary under test as `rhei`.
    pub(super) fn paste(&self, line: &str) -> CliRun {
        paste_with(&self.rhei(), line, &self.dir)
    }

    /// `rhei list` of the target `printed` names, spelled as `printed` spells it.
    /// A directory target still runs today, so running a printed command alone would
    /// pin nothing: what the fix owes is that its target is one the plan verbs take.
    #[track_caller]
    pub(super) fn assert_printed_target_is_listable(&self, printed: &str) {
        assert_printed_target_is_listable_with(&self.rhei(), printed, &self.dir);
    }
}

/// [`World::paste`] with the environment of `template`, whichever world built it.
pub(super) fn paste_with(template: &Command, line: &str, cwd: &Path) -> CliRun {
    let binary = rhei_binary();
    let mut dirs = vec![binary.parent().expect("the binary's directory").to_path_buf()];
    dirs.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
    let mut cmd = rhei_core::platform::system_shell_command(line);
    for (name, value) in template.get_envs() {
        match value {
            Some(value) => cmd.env(name, value),
            None => cmd.env_remove(name),
        };
    }
    cmd.env("PATH", std::env::join_paths(dirs).expect("join PATH")).current_dir(cwd);
    CliRun::from(&cmd.output().expect("the printed line should start"))
}

/// [`World::assert_printed_target_is_listable`] for any world's environment.
#[track_caller]
pub(super) fn assert_printed_target_is_listable_with(
    template: &Command,
    printed: &str,
    cwd: &Path,
) {
    let line = format!("rhei list {}", printed_target(printed));
    let listed = paste_with(template, &line, cwd);
    assert!(
        listed.status.success(),
        "rhei printed `{printed}`, whose target the plan verbs refuse: `{line}` said:\n{}",
        listed.stderr
    );
}

/// The four verbs on `target`.
pub(super) fn every_verb(target: &str) -> [Vec<&str>; 4] {
    [
        vec!["budget", "init", target, "--invocations", "5", "--reason", "probe"],
        vec!["budget", "show", target],
        vec!["budget", "adjust", target, "--invocations", "3", "--reason", "probe"],
        vec!["budget", "forget", target, "--reason", "probe"],
    ]
}

/// The first thing a refusal says, past the fixture agent's warning. Compared whole,
/// so a refusal with the right words on another path is not the same refusal.
pub(super) fn first_line(out: &Output) -> String {
    let said = raw_stderr(out);
    said.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.contains("declares no family"))
        .unwrap_or_default()
        .to_owned()
}

/// The command starting `start` on the physical line that holds it, as printed: a
/// command cut by a wrap comes back cut, and fails where it is run.
#[track_caller]
pub(super) fn printed_command(said: &str, start: &str) -> String {
    let line = said
        .lines()
        .find(|line| line.contains(start))
        .unwrap_or_else(|| panic!("nothing printed `{start}`; got:\n{said}"));
    let rest = &line[line.find(start).expect("the line holds it")..];
    let end = rest.find("<TEXT>").map_or(rest.len(), |at| at + "<TEXT>".len());
    rest[..end].trim_end().trim_end_matches('`').to_owned()
}

/// The target word of `rhei budget <verb> <target> [flags]`, its quoting kept.
fn printed_target(printed: &str) -> &str {
    let after_verb = printed.splitn(4, ' ').nth(3).expect("a target follows the verb");
    after_verb.split(" --").next().expect("split yields one")
}

/// `printed` with only the values a reader chooses filled in.
pub(super) fn filled_in(printed: &str) -> String {
    printed.replace("<N>", "6").replace("<TEXT>", "reader")
}

/// Everything a command wrote, wherever it put it, as written.
pub(super) fn said_raw(out: &Output) -> String {
    format!("{}{}", stdout(out), raw_stderr(out))
}

fn collect_bytes(path: &Path, found: &mut BTreeMap<PathBuf, Vec<u8>>) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries {
        let entry = entry.expect("directory entry");
        if entry.file_type().expect("entry type").is_dir() {
            collect_bytes(&entry.path(), found);
        } else {
            found.insert(entry.path(), fs::read(entry.path()).expect("read the file"));
        }
    }
    found.entry(path.to_path_buf()).or_default();
}
