//! A command a help offers as the next action stays whole on one physical
//! line, so the line a reader pastes is the command and does what the help
//! promised. §FS-rhei-errors.2.1
//!
//! Port of the agent-grounds/rhei#583 reproducer: at the default 80 columns,
//! and at 120 columns where the prose ahead of the command fills the line, the
//! missing-directory help printed `Create it with: mkdir -p` and put the
//! directory on the next line, so the pasted line failed with `missing
//! operand`. Its siblings lost their operands the same way, and pasting them
//! listed or inspected the working directory instead. Every assertion reads
//! the raw physical lines; none rejoins them.

use std::fs;
use std::process::Output;

use super::offered_command_paste_support::*;
use super::*;

/// A relative directory long enough that `mkdir -p <it>` does not fit an
/// 80-column line, while fitting a 120-column one on a line of its own.
const LONG_DIR: &str =
    "a-directory-that-does-not-exist/and-is-long-enough/to-pass-the-wrap-column-of-the-renderer";

/// A fixture: an isolated home, and an empty working directory beside it.
struct Fixture {
    dir: TestDir,
}

impl Fixture {
    fn new(prefix: &str) -> Self {
        let dir = unique_temp_dir(prefix);
        fs::create_dir_all(dir.join("work")).expect("create the working directory");
        Fixture { dir }
    }

    fn home(&self) -> std::path::PathBuf {
        self.dir.join(".home")
    }

    fn work(&self) -> std::path::PathBuf {
        self.dir.join("work")
    }

    /// `rhei <args>` from the working directory, captured: stderr is no
    /// terminal, so the renderer uses its 80-column default.
    fn rhei(&self, args: &[&str]) -> Output {
        rhei_command(self.home())
            .current_dir(self.work())
            .env("NO_COLOR", "1")
            .args(args)
            .output()
            .expect("rhei should run")
    }
}

fn refused(out: &Output) -> String {
    let rendered = raw_stderr(out);
    assert_eq!(out.status.code(), Some(1), "the refusal exits 1:\n{rendered}");
    rendered
}

const CREATE_IT: &str = "Create it with: ";
const CHECK_SPELLING: &str = "Check the spelling: ";
#[cfg(unix)] // Only the unix-only unreadable-path tests offer it.
const INSPECT_IT: &str = "Inspect it with: ";
const REMOVE_OR_PICK: &str = "Remove it or pick another: ";
const LOOK_WITH: &str = "Look with: ";
const REMOVE_THAT_ONE: &str = "remove that one: ";

/// At the default 80 columns the missing-directory help's `mkdir -p` keeps its
/// directory on its line, and pasting that line creates it.
// §FS-rhei-errors.2.1 §FS-rhei-errors.6
#[test]
fn a_missing_directory_is_offered_mkdir_whole_at_eighty_columns() {
    let fixture = Fixture::new("offered-mkdir-80");
    let target = format!("{LONG_DIR}/x.rhei.md");
    let rendered = refused(&fixture.rhei(&["validate", &target]));

    let pasted = offered_whole(&rendered, CREATE_IT, None, 80);

    #[cfg(unix)]
    {
        assert_eq!(pasted, format!("mkdir -p {LONG_DIR}"), "the offer is the directory's mkdir");
        paste_ok(&fixture.work(), &pasted);
        assert!(fixture.work().join(LONG_DIR).is_dir(), "the pasted line created the directory");
    }
    #[cfg(not(unix))]
    let _ = pasted;
}

/// At 120 columns the command alone would fit, but the prose ahead of it fills
/// the line: the command still stays whole on the line that offers it.
// §FS-rhei-errors.2.1
#[cfg(unix)]
#[test]
fn a_missing_directory_is_offered_mkdir_whole_when_the_prose_fills_a_wide_line() {
    let fixture = Fixture::new("offered-mkdir-120");
    let command = format!("mkdir -p {LONG_DIR}");
    assert!(command.len() + 8 < 120, "fixture: the command alone fits a 120-column line");
    let target = format!("{LONG_DIR}/x.rhei.md");
    let (status, rendered) =
        rendered_on_terminal(&fixture.home(), &fixture.work(), 120, &["validate", &target]);
    assert_eq!(status.code(), Some(1), "the refusal exits 1:\n{rendered}");

    let pasted = offered_whole(&rendered, CREATE_IT, None, 120);

    assert_eq!(pasted, command);
    paste_ok(&fixture.work(), &pasted);
    assert!(fixture.work().join(LONG_DIR).is_dir(), "the pasted line created the directory");
}

/// Control: a short directory's `mkdir -p` was never split, and pastes.
// §FS-rhei-errors.2.1
#[test]
fn a_short_missing_directory_is_offered_mkdir_whole() {
    let fixture = Fixture::new("offered-mkdir-short");
    let rendered = refused(&fixture.rhei(&["validate", "nodir/x.rhei.md"]));

    let pasted = offered_whole(&rendered, CREATE_IT, None, 0);

    #[cfg(unix)]
    {
        assert_eq!(pasted, "mkdir -p nodir");
        paste_ok(&fixture.work(), &pasted);
        assert!(fixture.work().join("nodir").is_dir(), "the pasted line created the directory");
    }
    #[cfg(not(unix))]
    let _ = pasted;
}

/// Control: a terminal wide enough for the whole help never split it.
// §FS-rhei-errors.2.1
#[cfg(unix)]
#[test]
fn a_terminal_wide_enough_keeps_the_mkdir_offer_whole() {
    let fixture = Fixture::new("offered-mkdir-200");
    let target = format!("{LONG_DIR}/x.rhei.md");
    let (status, rendered) =
        rendered_on_terminal(&fixture.home(), &fixture.work(), 200, &["validate", &target]);
    assert_eq!(status.code(), Some(1), "the refusal exits 1:\n{rendered}");

    let pasted = offered_whole(&rendered, CREATE_IT, None, 0);

    assert_eq!(pasted, format!("mkdir -p {LONG_DIR}"));
    paste_ok(&fixture.work(), &pasted);
    assert!(fixture.work().join(LONG_DIR).is_dir(), "the pasted line created the directory");
}

/// A misspelled name in an existing long directory is offered `ls` of that
/// directory on one line, and the pasted line lists it, not the working one.
// §FS-rhei-errors.2.1 §FS-rhei-errors.6
#[test]
fn a_misspelled_name_is_offered_ls_of_its_directory_whole() {
    let fixture = Fixture::new("offered-ls");
    fs::create_dir_all(fixture.work().join(LONG_DIR)).expect("the existing directory");
    write_fixture_file(&fixture.work().join(LONG_DIR), "expected-marker", "");
    write_fixture_file(&fixture.work(), "wrong-directory-marker", "");
    let rendered = refused(&fixture.rhei(&["validate", &format!("{LONG_DIR}/x.rhei.md")]));

    let pasted = offered_whole(&rendered, CHECK_SPELLING, None, 80);

    #[cfg(unix)]
    {
        assert_eq!(pasted, format!("ls {LONG_DIR}"));
        let listed = paste_ok(&fixture.work(), &pasted);
        assert!(listed.contains("expected-marker"), "the offered directory is listed:\n{listed}");
        assert!(!listed.contains("wrong-directory-marker"), "not the working one:\n{listed}");
    }
    #[cfg(not(unix))]
    let _ = pasted;
}

/// A path the user cannot read is offered `ls -ld` of itself on one line, and
/// the pasted line inspects that path, not `.`.
// §FS-rhei-errors.2.1 §FS-rhei-errors.6
#[cfg(unix)]
#[test]
fn an_unreadable_path_is_offered_ls_ld_of_itself_whole() {
    let fixture = Fixture::new("offered-ls-ld-denied");
    fs::create_dir_all(fixture.work().join(LONG_DIR)).expect("the existing directory");
    let locked = format!("{LONG_DIR}/locked.yaml");
    write_fixture_file(&fixture.work().join(LONG_DIR), "locked.yaml", "states: {}\n");
    if !lock(&fixture.work().join(&locked)) {
        return;
    }
    let out = fixture.rhei(&["states", "--state-machine", &locked]);
    unlock(&fixture.work().join(&locked));
    let rendered = refused(&out);

    let pasted = offered_whole(&rendered, INSPECT_IT, None, 80);

    assert_eq!(pasted, format!("ls -ld {locked}"));
    let inspected = paste_ok(&fixture.work(), &pasted);
    assert!(inspected.contains("locked.yaml"), "the offered path is inspected:\n{inspected}");
}

/// A path already in the way is offered `ls -ld` of itself on one line, and
/// the pasted line inspects that path, not `.`.
// §FS-rhei-errors.2.1 §FS-rhei-errors.6
#[test]
fn an_existing_path_in_the_way_is_offered_ls_ld_of_itself_whole() {
    let fixture = Fixture::new("offered-ls-ld-exists");
    fs::create_dir_all(fixture.work().join(LONG_DIR)).expect("the existing directory");
    write_fixture_file(&fixture.work().join(LONG_DIR), "panta", "a file where panta/ goes\n");
    let rendered = refused(&fixture.rhei(&["init", LONG_DIR, "--no-agents"]));

    let pasted = offered_whole(&rendered, REMOVE_OR_PICK, None, 80);

    #[cfg(unix)]
    {
        assert_eq!(pasted, format!("ls -ld {LONG_DIR}/panta"));
        let inspected = paste_ok(&fixture.work(), &pasted);
        assert!(inspected.contains("/panta"), "the offered path is inspected:\n{inspected}");
    }
    #[cfg(not(unix))]
    let _ = pasted;
}

/// A project with one rhei, `auth`, in the fixture's working directory.
fn with_rhei(fixture: &Fixture) {
    let init = fixture.rhei(&["init", "--no-agents"]);
    assert!(init.status.success(), "rhei init:\n{}", raw_stderr(&init));
    let new = fixture.rhei(&["new", "Authentication", "--id", "auth"]);
    assert!(new.status.success(), "rhei new:\n{}", raw_stderr(&new));
}

/// A `--description-file` missing from an existing long directory is offered
/// `ls` of that directory on one line, and the pasted line lists it.
// §FS-rhei-errors.2.1 §FS-rhei-new.1.1
#[test]
fn a_missing_description_file_is_offered_ls_of_its_directory_whole() {
    let fixture = Fixture::new("offered-description-ls");
    with_rhei(&fixture);
    fs::create_dir_all(fixture.work().join(LONG_DIR)).expect("the existing directory");
    write_fixture_file(&fixture.work().join(LONG_DIR), "expected-marker", "");
    let missing = format!("{LONG_DIR}/issue.md");
    let rendered =
        refused(&fixture.rhei(&["new", "X", "--under", "auth", "--description-file", &missing]));

    let pasted = offered_whole(&rendered, LOOK_WITH, None, 80);

    #[cfg(unix)]
    {
        assert_eq!(pasted, format!("ls {LONG_DIR}"));
        let listed = paste_ok(&fixture.work(), &pasted);
        assert!(listed.contains("expected-marker"), "the offered directory is listed:\n{listed}");
    }
    #[cfg(not(unix))]
    let _ = pasted;
}

/// An unreadable `--description-file` is offered `ls -l` of itself on one
/// line, and the pasted line inspects that file.
// §FS-rhei-errors.2.1 §FS-rhei-new.1.1
#[cfg(unix)]
#[test]
fn an_unreadable_description_file_is_offered_ls_l_of_itself_whole() {
    let fixture = Fixture::new("offered-description-ls-l");
    with_rhei(&fixture);
    fs::create_dir_all(fixture.work().join(LONG_DIR)).expect("the existing directory");
    write_fixture_file(&fixture.work().join(LONG_DIR), "issue.md", "a description\n");
    let locked = format!("{LONG_DIR}/issue.md");
    if !lock(&fixture.work().join(&locked)) {
        return;
    }
    let out = fixture.rhei(&["new", "X", "--under", "auth", "--description-file", &locked]);
    unlock(&fixture.work().join(&locked));
    let rendered = refused(&out);

    let pasted = offered_whole(&rendered, INSPECT_IT, None, 80);

    assert_eq!(pasted, format!("ls -l {locked}"));
    let inspected = paste_ok(&fixture.work(), &pasted);
    assert!(inspected.contains("issue.md"), "the offered file is inspected:\n{inspected}");
}

/// An explicit `--output` that already exists is offered `rm -rf` of it on one
/// line, and the pasted line removes it.
// §FS-rhei-errors.2.1 §FS-rhei-templates.6.2
#[test]
fn an_existing_output_is_offered_rm_rf_of_itself_whole() {
    let fixture = Fixture::new("offered-rm-rf");
    let output = fixture.work().join(LONG_DIR);
    fs::create_dir_all(&output).expect("the existing output");
    write_fixture_file(&output, "sentinel.txt", "in the way\n");
    let template = repo_root().join("crates/rhei-cli/templates/fix");
    let rendered = refused(&fixture.rhei(&[
        "instantiate",
        template.to_str().expect("a UTF-8 template path"),
        "--output",
        output.to_str().expect("a UTF-8 output path"),
    ]));

    let pasted = offered_whole(&rendered, REMOVE_THAT_ONE, None, 80);

    #[cfg(unix)]
    {
        assert!(pasted.starts_with("rm -rf "), "the offer is the rm -rf: {pasted}");
        paste_ok(&fixture.work(), &pasted);
        assert!(!output.exists(), "the pasted line removed the output");
    }
    #[cfg(not(unix))]
    let _ = pasted;
}

/// A rebind refused for linked bundle paths offers the one `rm` that removes
/// every link, mid-sentence; it stays whole, and the pasted command removes
/// every link and nothing they point to. Unix only: a symbolic link is made
/// without privileges only there. §REQ-cross-platform.2
// §FS-rhei-errors.2.1 §FS-rhei-library.2.2
#[cfg(unix)]
#[test]
fn linked_bundle_paths_are_offered_one_rm_whole() {
    use super::into_project_lay_support::*;
    use std::os::unix::fs::symlink;

    const PROJECT: &str = "reports-named-long-enough-that-each-link-path-crosses-the-wrap-column";
    let dir = unique_temp_dir("offered-link-rm");
    write_project_template(&dir, "plain", &laid_machine(&[]), "");
    let project = write_project(&dir, PROJECT, PLAIN_MANIFEST, Some(&laid_machine(&[])));
    let outside = dir.join("outside");
    fs::create_dir_all(outside.join("prompt_templates")).expect("the link targets");
    write_fixture_file(&outside.join("prompt_templates"), "brief.md", "Brief {subject}.\n");
    let script = write_fixture_file(&outside, "collect.sh", "#!/bin/sh\nexit 0\n");
    fs::create_dir_all(project.join("scripts")).expect("the project's scripts");
    symlink("../outside/prompt_templates", project.join("prompt_templates")).expect("link");
    symlink("../../outside/collect.sh", project.join("scripts/collect.sh")).expect("link");

    let out = rhei_command(dir.join(".home"))
        .current_dir(&dir)
        .env("NO_COLOR", "1")
        .args(["instantiate", "plain", "--into", PROJECT])
        .output()
        .expect("rhei should run");
    let rendered = refused(&out);

    let pasted = offered_whole(&rendered, "remove the links (`", Some("`"), 80);

    assert!(pasted.starts_with("rm "), "the offer is the rm: {pasted}");
    paste_ok(&dir, &pasted);
    for link in ["prompt_templates", "scripts/collect.sh"] {
        assert!(fs::symlink_metadata(project.join(link)).is_err(), "the link {link} is gone");
    }
    assert!(script.is_file(), "what a link pointed to is still there");
    assert!(outside.join("prompt_templates/brief.md").is_file(), "and so is the directory");
}
