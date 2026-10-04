//! A plan below a Panta project's directory that the project does not discover
//! as a member is a bare rhei, and it charges an account of its own.
//!
//! Today rhei walks up from the plan to the nearest `index.panta.md` and
//! charges that project. A reproducer that isolates its `XDG_STATE_HOME` but
//! makes its scratch directory inside the project then holds no witness for the
//! project's account, adopts the project's live journal and appends to it, and
//! the project's next charge, on its real state directory, is refused as
//! untrustworthy (`agent-grounds/rhei#435`). The nested plan was never a member:
//! the loader reads it alone, so its account is at its own execution root
//! ([§FS-rhei-budgets.5.1](../../docs/functional-spec/rhei-budgets.spec.md#51-where-it-lives)).
//!
//! The two member cases pass before the fix and must keep passing after it: a
//! single-file member and a Directory Workspace member still charge the
//! project. They are the guard against a fix that gives every rhei an account.

// §FS-rhei-budgets.5.1 §FS-rhei-budgets.1 §FS-rhei-panta.6

use std::fs;
use std::path::{Path, PathBuf};

use super::budget_support::*;
use super::*;

/// The reproducer's machine: no agents, so a hand edge needs none and a run
/// spawns none.
const MACHINE: &str = r#"name: budget-ping-pong
version: 1
states:
  work: { initial: true, description: Work }
  review: { description: Review }
  cancelled: { description: Stop, final: true }
transitions:
  - { from: work, to: review, description: Round done }
  - { from: review, to: work, description: Another round }
  - { from: work, to: cancelled, description: Stop }
  - { from: review, to: cancelled, description: Stop }
"#;

const SETTINGS: &str = r#"{ "defaults": { "agent_timeout": "30s", "transition_limit": 100 } }"#;

/// Task 1 is final, so `rhei run` has nothing to admit there; task 2 is the
/// one a hand edge moves.
const NEIGHBOUR: &str = r#"# Rhei: Neighbour

## Tasks

### Task 1: Work
**State:** cancelled

### Task 2: Work
**State:** work
"#;

const WORKSPACE_INDEX: &str = "# Rhei: Team\n";

const WORKSPACE_TASK: &str = r#"### Task 1: Work
**State:** work
"#;

/// One test root holding two machines (`home`, the real one, and `isolated`, a
/// reproducer's), the machine file, and the project `project/` with its member
/// `plan.rhei.md`, already initialized and charged twice on `home`.
struct World {
    dir: TestDir,
    machine: PathBuf,
}

impl World {
    fn new(prefix: &str) -> Self {
        let dir = unique_temp_dir(prefix);
        let machine = write_fixture_file(&dir, "states.yaml", MACHINE);
        let world = World { dir, machine };
        for home in ["home", "isolated"] {
            let settings = world.home(home).join(".config/rhei");
            fs::create_dir_all(&settings).expect("create machine settings directory");
            fs::write(settings.join("settings.json"), SETTINGS).expect("write machine settings");
        }
        let project = world.project();
        fs::create_dir_all(&project).expect("create the project");
        write_fixture_file(&project, "index.panta.md", "# Panta: project\n");
        write_fixture_file(&project, "plan.rhei.md", PLAN);
        let plan = project.join("plan.rhei.md");
        let init = world.rhei("home", &project, |cmd| {
            cmd.args(["budget", "init"]).arg(&plan);
            cmd.args(["--invocations", "10", "--reason", "establish"]);
        });
        assert_success(&init);
        assert_success(&world.edge("home", &plan, "1", "work", "review"));
        assert_success(&world.edge("home", &plan, "1", "review", "work"));
        world
    }

    fn home(&self, name: &str) -> PathBuf {
        self.dir.join(format!(".{name}"))
    }

    fn project(&self) -> PathBuf {
        self.dir.join("project")
    }

    /// `rhei <args>` from `cwd` on the named machine. The parent reservation is
    /// removed because this suite may itself run inside a `rhei run`.
    // §FS-rhei-budgets.7.1
    fn rhei(&self, home: &str, cwd: &Path, args: impl FnOnce(&mut Command)) -> CliRun {
        let mut cmd = rhei_command(self.home(home));
        cmd.env_remove("RHEI_BUDGET_PARENT_RESERVATION");
        cmd.env_remove("RHEI_BUDGET_PARENT_ACCOUNT");
        cmd.current_dir(cwd).arg("--state-machine").arg(&self.machine);
        args(&mut cmd);
        CliRun::from(&cmd.output().expect("rhei should run"))
    }

    fn edge(&self, home: &str, plan: &Path, task: &str, from: &str, to: &str) -> CliRun {
        let cwd = plan.parent().expect("a plan has a directory");
        self.rhei(home, cwd, |cmd| {
            cmd.arg("transition").arg(plan);
            cmd.args(["--task", task, "--from", from, "--to", to, "--no-callbacks"]);
        })
    }

    /// The project's one journal, byte for byte.
    fn journal(&self) -> Vec<u8> {
        let accounts = accounts(&self.project());
        assert_eq!(accounts.len(), 1, "the project holds exactly one account: {accounts:?}");
        fs::read(accounts[0].join("journal.jsonl")).expect("the project's journal is readable")
    }
}

/// The live account directories a root holds, none when it holds no account.
fn accounts(root: &Path) -> Vec<PathBuf> {
    match fs::read_dir(root.join(".agent-grounds/rhei/budgets")) {
        Ok(entries) => entries
            .map(|entry| entry.expect("account entry").path())
            .filter(|path| path.is_dir() && path.file_name() != Some("retired".as_ref()))
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn receipts(journal: &[u8]) -> usize {
    journal.split(|byte| *byte == b'\n').filter(|line| !line.is_empty()).count()
}

fn said(result: &CliRun) -> String {
    format!("{}{}", result.stdout, result.stderr)
}

/// The issue: a bare plan under `project/runtime/`, run and moved under an
/// isolated state directory, leaves the project's journal alone, holds an
/// account at its own execution root, and the project's next charge on the real
/// machine is applied.
// §FS-rhei-budgets.5.1
#[test]
fn a_plan_below_a_project_that_is_no_member_charges_its_own_account() {
    let world = World::new("budget-nested-plan");
    let journal = world.journal();
    let neighbour = world.project().join("runtime/repro/work/neighbour");
    fs::create_dir_all(&neighbour).expect("create the nested plan's directory");
    let nested = write_fixture_file(&neighbour, "plan.rhei.md", NEIGHBOUR);

    // The reproducer's run: with no agent it moves task 2 around once and halts
    // non-zero. Its exit is not the question; what it charged, and where, is.
    let run = world.rhei("isolated", &neighbour, |cmd| {
        cmd.arg("run").arg(&nested).args(["--no-tui", "--no-callbacks"]);
    });
    let moved = world.edge("isolated", &nested, "2", "work", "review");
    let after = world.journal();
    let next = world.edge("home", &world.project().join("plan.rhei.md"), "1", "work", "review");

    assert!(
        after == journal,
        "the nested plan appended to the project's journal ({} -> {} receipts), \
         and the project's next charge then {}:\n{}\n-- the nested run:\n{}",
        receipts(&journal),
        receipts(&after),
        if next.status.success() { "was applied" } else { "was refused" },
        said(&next),
        said(&run)
    );
    assert_success(&moved);
    let own = accounts(&neighbour);
    assert_eq!(own.len(), 1, "the nested plan holds an account at its own execution root: {own:?}");
    assert!(
        next.status.success(),
        "the project's next charge on the real machine is applied:\n{}",
        said(&next)
    );
}

/// A second single-file member charges the project's account, the one the
/// first member established, and holds none of its own.
// §FS-rhei-budgets.5.1
#[test]
fn a_single_file_member_charges_the_project_account() {
    let world = World::new("budget-nested-member");
    let journal = world.journal();
    let member = write_fixture_file(&world.project(), "other.rhei.md", PLAN);

    assert_success(&world.edge("home", &member, "1", "work", "review"));

    let after = world.journal();
    assert!(
        receipts(&after) > receipts(&journal) && after.starts_with(&journal),
        "the member's edge extends the project's journal ({} -> {} receipts)",
        receipts(&journal),
        receipts(&after)
    );
}

/// A Directory Workspace the project discovers is a member too: its edge
/// extends the project's journal, and its own directory holds no account.
// §FS-rhei-budgets.5.1
#[test]
fn a_directory_workspace_member_charges_the_project_account() {
    let world = World::new("budget-nested-workspace");
    let journal = world.journal();
    let team = world.project().join("team");
    fs::create_dir_all(team.join("tasks")).expect("create the workspace member");
    let index = write_fixture_file(&team, "index.rhei.md", WORKSPACE_INDEX);
    write_fixture_file(&team.join("tasks"), "01-work.md", WORKSPACE_TASK);

    assert_success(&world.edge("home", &index, "1", "work", "review"));

    let after = world.journal();
    assert!(
        receipts(&after) > receipts(&journal) && after.starts_with(&journal),
        "the workspace member's edge extends the project's journal ({} -> {} receipts)",
        receipts(&journal),
        receipts(&after)
    );
    assert!(accounts(&team).is_empty(), "a member holds no account of its own");
}
