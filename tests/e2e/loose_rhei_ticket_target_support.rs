//! Two loose plans side by side with no `index.panta.md`, and the `rhei` that
//! runs from inside their directory. §FS-rhei-panta.6.7 §FS-rhei-complete.2.1

use std::fs;
use std::path::{Path, PathBuf};

use super::new_tests::flattened_output;
use super::*;

pub const ITALY: &str = "# Rhei: Italy\n\n## Tasks\n\n\
### Task 1: Book the trains\n**State:** pending\n**Assignee:** me\n\n\
### Task 2: Pack\n**State:** draft\n";

pub const ROME: &str = "# Rhei: Rome\n\n## Tasks\n\n### Task 1: Book the hotel\n**State:** draft\n";

/// The refusal a directory of several loose rheis gives today, flattened. The
/// ticket commands meet it until a qualified id picks its rhei. §FS-rhei-panta.6
pub const NO_SINGLE_PLAN: &str = "so there is no single plan to pick";

/// `<root>/panta/` holding `italy.rhei.md` and `rome.rhei.md`, the machine
/// kept one level up so it is no candidate of the walk.
pub struct Loose {
    pub _root: TestDir,
    pub dir: PathBuf,
    pub home: PathBuf,
    pub machine: PathBuf,
}

impl Loose {
    pub fn new(prefix: &str) -> Self {
        let root = unique_temp_dir(prefix);
        let dir = root.join("panta");
        fs::create_dir_all(&dir).expect("the loose directory should be created");
        fs::write(dir.join("italy.rhei.md"), ITALY).expect("italy");
        fs::write(dir.join("rome.rhei.md"), ROME).expect("rome");
        let machine = write_fixture_file(&root, "states.yaml", STATE_MACHINE);
        let home = root.join(".home");
        Loose { dir, home, machine, _root: root }
    }

    /// Write one more entry beside the two plans.
    pub fn with(self, name: &str, contents: &str) -> Self {
        let path = self.dir.join(name);
        fs::create_dir_all(path.parent().expect("an entry has a parent")).expect("parent");
        fs::write(path, contents).expect("the extra entry should be written");
        self
    }

    /// `rhei <args>` from inside the loose directory.
    pub fn rhei(&self, args: &[&str]) -> CliRun {
        self.rhei_in(&self.dir, args, &[])
    }

    /// `rhei <args>` from `cwd`, with extra environment.
    pub fn rhei_in(&self, cwd: &Path, args: &[&str], env: &[(&str, &str)]) -> CliRun {
        let mut cmd = rhei_command(&self.home);
        cmd.current_dir(cwd).arg("--state-machine").arg(&self.machine).args(args);
        for (key, value) in env {
            cmd.env(key, value);
        }
        CliRun::from(&cmd.output().expect("rhei command should run"))
    }

    pub fn read(&self, name: &str) -> String {
        fs::read_to_string(self.dir.join(name)).expect("the plan should be readable")
    }

    /// The pick acted on italy and nothing else: rome is byte-identical.
    pub fn assert_rome_untouched(&self) {
        assert_eq!(self.read("rome.rhei.md"), ROME, "rome.rhei.md must not change");
    }
}

/// The command succeeded rather than meeting the several-loose-rheis refusal.
pub fn assert_picked(result: &CliRun, what: &str) {
    assert!(
        result.status.success(),
        "{what} should pick italy.rhei.md by its id; got:\n{}",
        flattened_output(result)
    );
}

/// The command refused, and not because the walk failed to pick a plan.
pub fn assert_reached_italy(result: &CliRun, needle: &str) {
    let said = flattened_output(result);
    assert!(
        !said.contains(NO_SINGLE_PLAN),
        "the qualified id should have picked italy.rhei.md; got:\n{said}"
    );
    assert!(said.contains(needle), "expected {needle:?} from italy's ticket; got:\n{said}");
}
