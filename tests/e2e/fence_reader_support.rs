//! CLI fixtures for the shared fence contract. §FS-rhei-plan-language.2.1

use super::*;

pub const MACHINE: &str = "name: fence-test\nversion: 1\nstates:\n  open:\n    initial: true\n  done:\n    final: true\ntransitions:\n  - { from: open, to: done }\n";
pub const CONTROL: &str = "```markdown\nplain quotation\n```\n";
pub const TRANSCRIPT: &str = "````markdown\n```console\n$ rhei validate plan.rhei.md\n````\n";
pub const INFO_STRING: &str = "```x\n# Rhei: q\n```y\n# Rhei: q2\n```\n";
pub const TILDE: &str = "~~~\n```\n~~~\n";

pub fn command(dir: &Path) -> std::process::Command {
    let mut cmd = rhei_command(dir.join(".home"));
    cmd.env("XDG_CONFIG_HOME", dir.join(".home/config"));
    cmd.current_dir(dir);
    cmd
}

pub fn run(dir: &Path, args: &[&str]) -> CliRun {
    CliRun::from(&command(dir).args(args).output().expect("rhei runs"))
}

pub fn fixture(prefix: &str, workspace: bool, tasks: &str) -> (TestDir, PathBuf) {
    let dir = unique_temp_dir(prefix);
    write_fixture_file(&dir, "states.yaml", MACHINE);
    let plan = if workspace {
        std::fs::create_dir_all(dir.join("tasks")).expect("tasks directory");
        write_fixture_file(&dir, "index.rhei.md", "# Rhei: Fence contract\n");
        write_fixture_file(&dir, "tasks/pair.md", tasks)
    } else {
        write_fixture_file(
            &dir,
            "plan.rhei.md",
            &format!("# Rhei: Fence contract\n\n## Tasks\n\n{tasks}"),
        )
    };
    (dir, plan)
}

pub fn target(workspace: bool) -> &'static str {
    if workspace {
        "index.rhei.md"
    } else {
        "plan.rhei.md"
    }
}
