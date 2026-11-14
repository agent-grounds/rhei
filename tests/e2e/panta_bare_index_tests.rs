//! A project's manifest named by its bare file name, `index.panta.md`, names the
//! same project as `./index.panta.md` and `.`, so a command reads the same
//! member rheis whichever spelling it was given.
//!
//! The shape is the one agent-grounds/rhei#718 reported: a project whose only
//! member links a file that does not exist, validated from inside the project.

use std::fs;
use std::path::Path;

use super::*;

/// A pending member task whose link names a file the project does not hold.
const BROKEN_MEMBER: &str =
    "# Rhei: Single\n\n## Tasks\n\n### Task 1: Load\n**State:** pending\n\nRead [y](missing.md).\n";

/// The same member with nothing for validation to refuse.
const HEALTHY_MEMBER: &str =
    "# Rhei: Single\n\n## Tasks\n\n### Task 1: Load\n**State:** pending\n\nRead the notes.\n";

/// A project `proj/` holding `index.panta.md` and one member, `single.rhei.md`.
fn project(prefix: &str, member: &str) -> TestDir {
    let dir = unique_temp_dir(prefix);
    let proj = dir.join("proj");
    fs::create_dir_all(&proj).expect("create project dir");
    fs::write(proj.join("index.panta.md"), "# Panta: Proj\n").expect("write manifest");
    fs::write(proj.join("single.rhei.md"), member).expect("write member");
    dir
}

/// Run `rhei <args>` with the project directory as the current directory.
fn run_inside(dir: &Path, args: &[&str]) -> CliRun {
    let out = rhei_command(dir.join(".home"))
        .current_dir(dir.join("proj"))
        .args(args)
        .output()
        .expect("rhei should run");
    CliRun::from(&out)
}

fn output(result: &CliRun) -> String {
    format!("{}\n{}", result.stdout, result.stderr)
}

/// `rhei validate index.panta.md` refuses the broken member exactly as
/// `./index.panta.md` and `.` do, instead of validating an empty project.
// §FS-rhei-panta.6 §FS-rhei-panta.6.4
#[test]
#[ignore = "red until #718: bare index.panta.md validates no member and exits 0; implement removes this"]
fn validate_bare_index_panta_refuses_the_broken_member_like_dot_slash() {
    let dir = project("panta-bare-validate", BROKEN_MEMBER);
    for spelling in ["./index.panta.md", ".", "index.panta.md"] {
        let result = run_inside(&dir, &["validate", spelling]);
        let out = output(&result);
        assert!(
            !result.status.success(),
            "`rhei validate {spelling}` must refuse the project's broken member:\n{out}"
        );
        assert!(
            out.contains("missing.md") && out.contains("single.1"),
            "`rhei validate {spelling}` must name the member's missing link:\n{out}"
        );
        assert!(
            !out.contains("holds no rheis"),
            "`rhei validate {spelling}` must not call the project empty:\n{out}"
        );
    }
}

/// `rhei list index.panta.md` lists the member's ticket, as `./index.panta.md`
/// does, instead of saying the project has no tickets.
// §FS-rhei-panta.6 §FS-rhei-panta.6.4
#[test]
#[ignore = "red until #718: bare index.panta.md lists an empty project; implement removes this"]
fn list_bare_index_panta_lists_the_member_ticket_like_dot_slash() {
    let dir = project("panta-bare-list", HEALTHY_MEMBER);
    for spelling in ["./index.panta.md", ".", "index.panta.md"] {
        let result = run_inside(&dir, &["list", spelling]);
        let out = output(&result);
        assert_success(&result);
        assert!(
            result.stdout.contains("single.1"),
            "`rhei list {spelling}` must list the member's ticket:\n{out}"
        );
        assert!(
            !out.contains("no tickets yet"),
            "`rhei list {spelling}` must not call the project empty:\n{out}"
        );
    }
}
