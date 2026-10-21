//! Missing account directories must not hide witnessed missing history.
//! §FS-rhei-budgets.5.4 §FS-rhei-budgets.10 §FS-rhei-remove.2

use super::budget_path_reuse_support::said;
use super::budget_support::home_for;
use super::*;

const PLAN: &str =
    "# Rhei: Project\n\n## Tasks\n\n### Task 1: Hold the account\n**State:** pending\n";
const ADJUST: &[&str] =
    &["budget", "adjust", "plan.rhei.md", "--invocations", "11", "--reason", "review"];
const REMOVE: &[&str] = &["remove", "plan.rhei.md", "--task", "1", "--dry-run"];

struct Fixture {
    _dir: TestDir,
    project: PathBuf,
    home: PathBuf,
    account: PathBuf,
    witness: PathBuf,
    history: Vec<u8>,
    manifest: Vec<u8>,
}

impl Fixture {
    fn new() -> Self {
        let dir = unique_temp_dir("budget-missing-directory-504");
        let project = dir.join("project");
        fs::create_dir_all(&project).expect("project directory");
        let project =
            rhei_core::platform::canonical_path(&project).expect("canonical project path");
        write_fixture_file(&project, "index.panta.md", "# Panta: Missing account\n");
        write_fixture_file(&project, "plan.rhei.md", PLAN);
        let home = home_for(&dir);
        let init = run(
            &project,
            &home,
            &["budget", "init", "plan.rhei.md", "--invocations", "10", "--reason", "establish"],
        );
        assert_success(&init);
        let accounts: Vec<_> = fs::read_dir(project.join(".agent-grounds/rhei/budgets"))
            .expect("budget init creates accounts")
            .map(|entry| entry.expect("account entry").path())
            .filter(|path| path.is_dir())
            .collect();
        assert_eq!(accounts.len(), 1, "one established account");
        let account = accounts[0].clone();
        let witness = rhei_core::platform::canonical_path(&home.join("state"))
            .expect("state directory")
            .join("rhei/budget-authority")
            .join(account.file_name().expect("account uuid"))
            .join("history.jsonl");
        let history = fs::read(&witness).expect("committed witness");
        assert_eq!(history, fs::read(account.join("journal.jsonl")).expect("journal"));
        assert_eq!(
            String::from_utf8_lossy(&history).lines().count(),
            2,
            "init commits two receipts"
        );
        assert!(account.join("journal.jsonl.lock").is_file(), "init leaves the incidental lock");
        let manifest = fs::read(project.join("index.panta.md")).expect("manifest");
        Self { _dir: dir, project, home, account, witness, history, manifest }
    }

    fn run(&self, args: &[&str]) -> CliRun {
        run(&self.project, &self.home, args)
    }

    fn assert_unchanged(&self, directory_missing: bool) {
        assert_eq!(
            self.account.exists(),
            !directory_missing,
            "account directory must not be recreated"
        );
        assert!(!self.account.join("journal.jsonl").exists(), "no journal may be recreated");
        assert_eq!(fs::read(&self.witness).expect("witness survives"), self.history);
        assert_eq!(
            fs::read(self.project.join("plan.rhei.md")).expect("plan survives"),
            PLAN.as_bytes()
        );
        assert_eq!(
            fs::read(self.project.join("index.panta.md")).expect("manifest survives"),
            self.manifest
        );
    }

    fn assert_show_contrast(&self, directory_missing: bool) {
        let show = self.run(&["budget", "show", "plan.rhei.md"]);
        let report = said(&show);
        assert_eq!(show.status.code(), Some(1), "damaged show exits 1:\n{report}");
        for expected in [
            "damaged [journal_absent]",
            "this project has no budget journal",
            &self.account.join("journal.jsonl").display().to_string(),
            &self.witness.display().to_string(),
            "2 receipts",
            "rhei budget forget",
        ] {
            assert!(report.contains(expected), "show must contain {expected:?}:\n{report}");
        }
        self.assert_unchanged(directory_missing);
    }

    fn assert_refusal(&self, args: &[&str], directory_missing: bool) {
        let result = self.run(args);
        let refusal = said(&result);
        assert_eq!(result.status.code(), Some(1), "refusal retains exit 1:\n{refusal}");
        self.assert_unchanged(directory_missing);
        if args == REMOVE {
            assert!(
                refusal.contains("plan.1 cannot be removed")
                    && refusal.contains("unreadable: budget account:"),
                "unreadable evidence still blocks removal:\n{refusal}"
            );
        }
        assert!(
            refusal.contains("this project has no budget journal"),
            "missing history must be diagnosed even without its account directory; got:\n{refusal}"
        );
        for expected in
            ["committed history at", &self.witness.display().to_string(), "records 2 receipts"]
        {
            assert!(refusal.contains(expected), "refusal must contain {expected:?}:\n{refusal}");
        }
        assert!(
            !refusal.contains("journal.jsonl.lock"),
            "incidental lock is not the cause:\n{refusal}"
        );
        assert!(
            !refusal.contains("cannot reach the budget path"),
            "missing history is the cause:\n{refusal}"
        );
    }
}

fn run(project: &Path, home: &Path, args: &[&str]) -> CliRun {
    let mut command = rhei_command(home);
    // §FS-rhei-budgets.7.1: the enclosing runner's reservation is not this fixture's.
    for (name, _) in std::env::vars().filter(|(name, _)| name.starts_with("RHEI_")) {
        command.env_remove(name);
    }
    command
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("GIT_CEILING_DIRECTORIES", project.parent().expect("fixture root"))
        .current_dir(project)
        .args(args);
    CliRun::from(&command.output().expect("rhei command runs"))
}

fn check_missing_directory(args: &[&str]) {
    let control = Fixture::new();
    fs::remove_file(control.account.join("journal.jsonl"))
        .expect("remove only the control journal");
    control.assert_show_contrast(false);
    control.assert_refusal(args, false);

    let case = Fixture::new();
    fs::remove_dir_all(&case.account).expect("remove whole account directory, keep witness");
    case.assert_show_contrast(true);
    case.assert_refusal(args, true);
}

/// §FS-rhei-budgets.5.4: absence of the incidental lock must not mask journal_absent.
#[test]
#[ignore = "red until #504: adjust must diagnose missing journal with its witness; implement removes this"]
fn missing_budget_directory_adjust_reports_witnessed_missing_journal() {
    check_missing_directory(ADJUST);
}

/// §FS-rhei-budgets.5.4 §FS-rhei-remove.2: unreadable history still refuses removal.
#[test]
#[ignore = "red until #504: remove must diagnose missing journal with its witness; implement removes this"]
fn missing_budget_directory_remove_reports_witnessed_missing_journal() {
    check_missing_directory(REMOVE);
}
