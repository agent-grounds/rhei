//! A project to narrow a reset below a rhei in: the plans, the commands that
//! finish their tickets, and the bytes a ticket the scope leaves out must keep.

// §FS-rhei-reset.2.5

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use super::*;

/// The issue's plan: `paper.3` and `paper.4` both depend on `paper.2`, and
/// `paper.1` reads nothing the others produce.
pub(super) const PAPER: &str = "# Rhei: Paper review

## Tasks

### Task 1: venue
**State:** pending

Look up the venue's review criteria.

### Task 2: ingest
**State:** pending

Read the paper PDF.

### Task 3: related-work
**State:** pending
**Prior:** Task 2

Survey the related work the paper cites.

### Task 4: triage
**State:** pending
**Prior:** Task 2

Sort the paper's claims for review.
";

/// The message every test today fails with: the flag under test does not exist.
pub(super) const UNKNOWN_FLAG: &str = "unexpected argument '--task'";

/// A directory holding one project, and the home every `rhei` in it runs under.
pub(super) struct ScopeWorld {
    /// Keeps the tree alive for as long as the test holds the world.
    _dir: TestDir,
    pub(super) root: PathBuf,
    home: PathBuf,
}

impl ScopeWorld {
    pub(super) fn new(name: &str) -> Self {
        let dir = unique_temp_dir(name);
        let root = dir.join("project");
        fs::create_dir_all(&root).expect("create the project");
        let home = dir.join(".home");
        ScopeWorld { _dir: dir, root, home }
    }

    /// The issue's single-file plan with every ticket finished.
    pub(super) fn paper(name: &str) -> Self {
        let world = Self::new(name);
        world.write("paper.rhei.md", PAPER);
        world.finish("paper.rhei.md", &["paper.1", "paper.2", "paper.3", "paper.4"]);
        world
    }

    pub(super) fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    pub(super) fn write(&self, relative: &str, contents: &str) {
        let path = self.path(relative);
        fs::create_dir_all(path.parent().expect("a parent")).expect("create the parent");
        fs::write(path, contents).expect("write the fixture file");
    }

    pub(super) fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.path(relative))
            .unwrap_or_else(|err| panic!("read {relative}: {err}"))
    }

    pub(super) fn exists(&self, relative: &str) -> bool {
        self.path(relative).exists()
    }

    /// `rhei <args>`, from the project directory.
    pub(super) fn command(&self, args: &[&str]) -> Command {
        let mut command = rhei_command(&self.home);
        command.current_dir(&self.root).args(args);
        command
    }

    pub(super) fn rhei(&self, args: &[&str]) -> CliRun {
        CliRun::from(&self.command(args).output().expect("rhei should start"))
    }

    /// `rhei reset <target> <args>`.
    pub(super) fn reset(&self, target: &str, args: &[&str]) -> CliRun {
        let mut all = vec!["reset", target];
        all.extend_from_slice(args);
        self.rhei(&all)
    }

    /// A reset spawned in the background, for a test that acts while it waits.
    pub(super) fn spawn_reset(&self, target: &str, args: &[&str]) -> Child {
        let mut command = self.command(&["reset", target]);
        command.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
        command.spawn().expect("rhei reset should start")
    }

    /// A fixture step that has to succeed.
    #[track_caller]
    pub(super) fn step(&self, args: &[&str]) {
        let run = self.rhei(args);
        assert!(
            run.status.success(),
            "fixture step `rhei {args:?}` failed:\n{}{}",
            run.stdout,
            run.stderr
        );
    }

    /// Finish each ticket in turn with a result naming it.
    #[track_caller]
    pub(super) fn finish(&self, target: &str, tickets: &[&str]) {
        for ticket in tickets {
            let result = format!("result of {ticket}");
            self.step(&["complete", target, "--task", ticket, "--result", &result]);
        }
    }

    /// `rhei list` reads a ticket's state as `[<state>]` at the end of its line.
    pub(super) fn state_of(&self, target: &str, ticket: &str) -> String {
        let listing = self.rhei(&["list", target]);
        let prefix = format!("Task {ticket}: ");
        let line = listing
            .stdout
            .lines()
            .map(str::trim_start)
            .find(|line| line.starts_with(&prefix))
            .unwrap_or_else(|| panic!("{ticket} is not listed:\n{}", listing.stdout));
        let (_, state) = line.split_once(" [").expect("a state in brackets");
        state.split(']').next().expect("a closing bracket").to_string()
    }

    /// Every file under `relative`, by path, with its bytes: what a reset that
    /// keeps something must leave byte for byte.
    pub(super) fn snapshot(&self, relative: &str) -> BTreeMap<PathBuf, Vec<u8>> {
        let mut files = BTreeMap::new();
        collect(&self.path(relative), &mut files);
        files
    }
}

fn collect(path: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
    if path.is_dir() {
        for entry in fs::read_dir(path).expect("read the directory") {
            collect(&entry.expect("a directory entry").path(), files);
        }
    } else if path.is_file() {
        files.insert(path.to_path_buf(), fs::read(path).expect("read the file"));
    }
}

/// The text of the task `heading` opens, up to the next task heading at any depth.
pub(super) fn task_block<'a>(plan: &'a str, heading: &str) -> &'a str {
    let start = plan.find(heading).unwrap_or_else(|| panic!("no {heading} in:\n{plan}"));
    let rest = &plan[start + heading.len()..];
    let end = rest.find("\n#").map_or(rest.len(), |at| at + 1);
    &plan[start..start + heading.len() + end]
}

/// The ledger rows naming `ticket`, in order.
pub(super) fn ledger_rows(world: &ScopeWorld, ticket: &str) -> Vec<String> {
    let ledger =
        fs::read_to_string(world.path("runtime/state-transitions.log")).unwrap_or_default();
    let prefix = format!("{ticket} ");
    ledger.lines().filter(|line| line.starts_with(&prefix)).map(str::to_string).collect()
}

/// The run either printed `expected` on a line of its stdout, or failed.
#[track_caller]
pub(super) fn assert_line(run: &CliRun, expected: &str) {
    assert!(
        run.status.success() && run.stdout.lines().any(|line| line == expected),
        "expected the line\n  {expected}\nexit {:?}, stdout:\n{}\nstderr:\n{}",
        run.status.code(),
        run.stdout,
        run.stderr
    );
}

/// A refusal that must change nothing: non-zero, and naming `expected`.
#[track_caller]
pub(super) fn assert_refused(run: &CliRun, expected: &str) {
    let said = format!("{}{}", run.stdout, run.stderr);
    assert!(
        !run.status.success() && said.contains(expected) && !said.contains(UNKNOWN_FLAG),
        "expected a refusal naming `{expected}`, exit {:?}:\n{said}",
        run.status.code()
    );
}
