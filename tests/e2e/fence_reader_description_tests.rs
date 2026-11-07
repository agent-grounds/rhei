//! A pasted description cannot author structure by confusing fence readers.
//! §FS-rhei-new.3.4 §FS-rhei-plan-language.2.1

use std::fs;
use std::io::Write;
use std::process::Stdio;

use super::fence_reader_support::*;
use super::new_tests::flattened_output;
use super::*;

const FORGED: &str = "```\n```x\n```\n### Task 9: smuggled\n**State:** done\n```\n```x\n```\n";

fn description(dir: &Path, workspace: bool, body: &str) -> CliRun {
    let rhei_id = if workspace {
        dir.file_name().expect("workspace name").to_str().expect("UTF-8 fixture name")
    } else {
        "plan"
    };
    let mut child = command(dir)
        .args([
            "new",
            "Captured issue",
            "--project",
            target(workspace),
            "--under",
            rhei_id,
            "--description-file",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("rhei new starts");
    child.stdin.take().expect("stdin").write_all(body.as_bytes()).expect("description input");
    CliRun::from(&child.wait_with_output().expect("rhei new exits"))
}

// Only authored files matter here: lock files and machine-local bookkeeping
// are not plan mutations. Snapshot the entire workspace tasks directory too,
// so an injected ticket in a newly allocated file cannot escape the assertion.
fn authored(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn walk(dir: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
        for entry in fs::read_dir(dir).expect("fixture tree") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                if path.file_name().is_some_and(|name| name == "tasks") {
                    walk(&path, out);
                }
            } else if path.extension().is_some_and(|extension| extension == "md") {
                out.push((path.clone(), fs::read(path).expect("authored file")));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, &mut out);
    out.sort();
    out
}

fn refused(body: &str, needle: &str) {
    let mut failures = Vec::new();
    for workspace in [false, true] {
        let (dir, _) =
            fixture("rhei-614-description", workspace, "### Task 1: Existing\n**State:** open\n");
        assert_success(&run(&dir, &["validate", target(workspace)]));
        let before = authored(&dir);
        let result = description(&dir, workspace, body);
        let said = flattened_output(&result);
        let listed = run(&dir, &["list", target(workspace)]);
        let correct = !result.status.success()
            && said.contains(needle)
            && authored(&dir) == before
            && listed.status.success()
            && !listed.stdout.contains("smuggled");
        if !correct {
            failures.push(format!("description {body:?} must be refused before mutation (workspace={workspace}), naming {needle:?}; exit {:?}; authored files unchanged: {}; output: {said}; list:\n{}", result.status.code(), authored(&dir) == before, listed.stdout));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// §FS-rhei-new.3.4: the exact intake forgery has its heading outside the fence.
#[test]
#[ignore = "red until #614: forged description must be refused before writing; implement removes this"]
fn fence_reader_614_description_forgery() {
    refused(FORGED, "line 4 of --description-file - would be read as plan structure");
}

fn accepts(body: &str) {
    let mut failures = Vec::new();
    for workspace in [false, true] {
        let (dir, _) = fixture(
            "rhei-614-quoted-description",
            workspace,
            "### Task 1: Existing\n**State:** open\n",
        );
        let result = description(&dir, workspace, body);
        if !result.status.success() {
            failures.push(format!(
                "fenced description {body:?} must be accepted verbatim (workspace={workspace}); {}",
                result.stderr
            ));
            continue;
        }
        let files = authored(&dir);
        assert!(
            files.iter().any(|(_, bytes)| String::from_utf8_lossy(bytes).contains(body)),
            "description was not kept verbatim"
        );
        assert_success(&run(&dir, &["validate", target(workspace)]));
        let listed = run(&dir, &["list", target(workspace)]);
        assert_success(&listed);
        assert!(
            !listed.stdout.contains("smuggled"),
            "quoted heading became a ticket: {}",
            listed.stdout
        );
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// §FS-rhei-new.3.4: a canonical close protects headings and metadata inside it.
#[test]
#[ignore = "red until #614: tilde quotations are description content; implement removes this"]
fn fence_reader_614_description_tilde_content() {
    accepts("~~~markdown\n### Task 9: smuggled\n**State:** done\n~~~\n");
}

#[test]
#[ignore = "red until #614: shorter runs do not expose quoted headings; implement removes this"]
fn fence_reader_614_description_longer_content() {
    accepts("````markdown\n```console\n### Task 9: smuggled\n**State:** done\n````\n");
}

#[test]
#[ignore = "red until #614: info strings do not expose quoted headings; implement removes this"]
fn fence_reader_614_description_info_content() {
    accepts("```markdown\n```console\n### Task 9: smuggled\n**State:** done\n```\n");
}

#[test]
fn fence_reader_614_description_ordinary_control() {
    accepts("```markdown\n### Task 9: smuggled\n**State:** done\n```\n");
}

macro_rules! unclosed {
    ($name:ident, $body:expr) => {
        #[test]
        #[ignore = "red until #614: report the actual unclosed opening fence; implement removes this"]
        fn $name() { refused($body, "code fence opened on line 3 of --description-file - is never closed"); }
    };
}

unclosed!(fence_reader_614_description_unclosed_tilde, "Intro\n\n~~~\nbody\n");
unclosed!(fence_reader_614_description_unclosed_short_backtick, "Intro\n\n````\nbody\n```\n");
unclosed!(
    fence_reader_614_description_unclosed_info_backtick,
    "Intro\n\n```markdown\n```console\n```example\n"
);
unclosed!(fence_reader_614_description_unclosed_short_tilde, "Intro\n\n~~~~\nbody\n~~~\n");
unclosed!(
    fence_reader_614_description_unclosed_info_tilde,
    "Intro\n\n~~~markdown\nbody\n~~~console\n"
);
unclosed!(
    fence_reader_614_description_unclosed_wrong_character,
    "Intro\n\n```markdown\nbody\n~~~\n```console\n"
);

/// §FS-rhei-new.3.4: an ordinary unclosed backtick fence is an existing control.
#[test]
fn fence_reader_614_description_unclosed_backtick_control() {
    refused(
        "Intro\n\n```markdown\nbody\n",
        "code fence opened on line 3 of --description-file - is never closed",
    );
}
