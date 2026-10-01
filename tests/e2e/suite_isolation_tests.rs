//! The suite's own hygiene: where a spawned `rhei` writes is what the test
//! chose, not what the test happened to stand in. §REQ-test-isolation
//!
//! Both of a spawn's state locations are pinned because the run registry they
//! name is machine-wide — `$XDG_STATE_HOME/rhei/runs/<id>.json`, per
//! §FS-rhei-run-headless.2 — so what these tests watch is that the pinning put
//! the state under the home the test chose and nowhere near the directory the
//! binary stood in.

use std::collections::BTreeSet;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use super::templates_tests::run_raw;
use super::*;

/// The names directly under `dir`, so two readings taken around a spawn can be
/// compared for whatever appeared between them.
fn entry_names(dir: &Path) -> BTreeSet<String> {
    std::fs::read_dir(dir)
        .expect("directory should be readable")
        .map(|entry| entry.expect("directory entry").file_name().to_string_lossy().into_owned())
        .collect()
}

/// Which of `names` `git status` would report at `dir`. An ignored entry is not
/// what §REQ-test-isolation.1 forbids: a test that must write inside the
/// repository writes beneath the ignored `scratchpad/`, and two of them do.
///
/// Each name is offered both bare and with a trailing slash, because a pattern
/// written `scratchpad/` matches a directory only. Bare, it matches just while
/// the directory is on disk — and these names are read from a tree other tests
/// are still creating and removing their own directories in, so a name asked
/// about a moment too late would come back unignored. The slashed form matches
/// whether the directory is there or not, and neither form matches a name no
/// pattern covers, so offering both cannot excuse an entry that the repository
/// does not ignore.
///
/// Where git cannot answer, every name counts. An absent git makes this
/// assertion stricter rather than silently weaker, and `check-ignore` exits 1
/// when nothing matched, so the exit code is not consulted.
fn reported_by_git(dir: &Path, names: &BTreeSet<String>) -> Vec<String> {
    let all = || names.iter().cloned().collect::<Vec<_>>();
    if names.is_empty() {
        return Vec::new();
    }
    let Ok(mut child) = git_command()
        .arg("-C")
        .arg(dir)
        .args(["check-ignore", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return all();
    };
    let mut stdin = child.stdin.take().expect("git stdin should be piped");
    let listing =
        names.iter().map(|name| format!("{name}\n{name}/")).collect::<Vec<_>>().join("\n");
    let written = stdin.write_all(listing.as_bytes()).and_then(|()| stdin.flush());
    drop(stdin);
    let Ok(output) = child.wait_with_output() else {
        return all();
    };
    if written.is_err() {
        return all();
    }
    let ignored = String::from_utf8_lossy(&output.stdout).into_owned();
    let ignored: BTreeSet<&str> =
        ignored.lines().map(|line| line.trim().trim_end_matches('/')).collect();
    names.iter().filter(|name| !ignored.contains(name.as_str())).cloned().collect()
}

/// §REQ-test-isolation.2: the working directory a test hands the binary comes
/// back exactly as it went in, because the state home is a separate decision.
///
/// This is the pin. It fails whatever order the suite runs in and whatever is
/// already on disk, because the directory it watches is its own and empty.
#[test]
fn spawning_rhei_leaves_its_working_directory_untouched() {
    let cwd = unique_temp_dir("suite-isolation-working-dir");
    let before = entry_names(&cwd);

    let result = run_raw(&["--version"], &cwd);
    assert_success(&result);

    let added: Vec<String> = entry_names(&cwd).difference(&before).cloned().collect();
    assert!(
        added.is_empty(),
        "spawning `rhei` put {added:?} into the working directory it was handed, {}. \
         Where a spawned process writes is chosen, never derived from where it stands \
         (§REQ-test-isolation.2): a helper that joins the state home onto its cwd \
         argument writes into whatever directory the caller only wanted to stand in.",
        cwd.display()
    );
}

/// §REQ-test-isolation.1: the checkout is not a test's directory. The command it
/// spawns is an instantiation (§FS-rhei-templates), chosen because that is the
/// one the defect was found under.
///
/// This is the shape that left an untracked `.home/` at the checkout root after
/// every full gate run — a test that wanted the repository as its working
/// directory and said nothing about state. It is the acceptance test rather than
/// the pin: it compares against what was at the root when it started, so in a
/// suite that still has the defect another test may create the entry first and
/// leave this one nothing to report. Run alone it fails outright, and
/// [`spawning_rhei_leaves_its_working_directory_untouched`] is what holds the
/// regression out regardless of order.
#[test]
fn spawning_rhei_from_the_checkout_adds_nothing_git_would_report() {
    let root = repo_root();
    let output = unique_temp_dir("suite-isolation-checkout");
    let before = entry_names(&root);

    let result = run_raw(
        &[
            "instantiate",
            "hourly-human-intervention",
            "--output",
            output.join("hourly").to_str().expect("output path should be unicode"),
        ],
        &root,
    );
    assert_success(&result);

    let appeared: BTreeSet<String> = entry_names(&root).difference(&before).cloned().collect();
    let reported = reported_by_git(&root, &appeared);
    assert!(
        reported.is_empty(),
        "a test spawned from the checkout put {reported:?} at its root, {}, where \
         `git status` reports it as a change nobody made and this repository's own \
         pre-commit and pre-push hooks read it as one (§REQ-test-isolation.1).",
        root.display()
    );
}

/// The decoy repository's three tells, read with a `git` pointed at it on
/// purpose: its `HEAD`, how many entries its index holds, and the local
/// configuration a fixture would leave in it.
#[derive(Debug, PartialEq, Eq)]
struct DecoyState {
    head: String,
    index_entries: usize,
    config: Vec<String>,
}

/// A `git` aimed at `repo` by flag rather than by environment.
///
/// `--git-dir` and `--work-tree` beat an inherited `GIT_DIR`, so this reads and
/// builds the decoy the same way whether or not the removal under test exists
/// yet — and `GIT_INDEX_FILE` is pinned because no flag covers it.
/// §REQ-test-isolation.3
fn decoy_git(repo: &Path) -> Command {
    let git_dir = repo.join(".git");
    let mut cmd = git_command();
    cmd.arg("--git-dir").arg(&git_dir).arg("--work-tree").arg(repo);
    cmd.env("GIT_INDEX_FILE", git_dir.join("index"));
    cmd
}

fn decoy_git_stdout(repo: &Path, args: &[&str]) -> String {
    let output = decoy_git(repo).args(args).output().expect("git should run on the decoy");
    assert!(
        output.status.success(),
        "git {args:?} on the decoy at {} failed: {}",
        repo.display(),
        stderr(&output)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// A repository no test names, with five tracked files and one commit — the
/// stand-in for the checkout a hook is committing to.
fn build_decoy_repository(repo: &Path) {
    std::fs::create_dir_all(repo).expect("create the decoy directory");
    decoy_git_stdout(repo, &["init", "-q"]);
    for index in 1..=5 {
        std::fs::write(repo.join(format!("f{index}")), format!("{index}\n"))
            .expect("write a decoy file");
    }
    decoy_git_stdout(repo, &["add", "-A"]);
    decoy_git_stdout(
        repo,
        &[
            "-c",
            "user.name=decoy",
            "-c",
            "user.email=decoy@example.invalid",
            "commit",
            "-qm",
            "decoy",
        ],
    );
}

fn decoy_state(repo: &Path) -> DecoyState {
    DecoyState {
        head: decoy_git_stdout(repo, &["rev-parse", "HEAD"]).trim().to_string(),
        index_entries: decoy_git_stdout(repo, &["ls-files"]).lines().count(),
        config: decoy_git_stdout(repo, &["config", "--local", "--list"])
            .lines()
            .filter(|line| line.starts_with("user.") || line.starts_with("core.bare"))
            .map(str::to_string)
            .collect(),
    }
}

/// §REQ-test-isolation.3: every `GIT_*` the child would otherwise see is taken
/// off, and only what the test sets afterwards is deliberate.
///
/// The variables are staged on the child rather than on this process, because
/// `set_var` would race every other test in this binary. Setting them first and
/// letting the removal run afterwards is the same seam in the same order.
///
/// Both of the rule's two sources are watched, because the staged one is not
/// what produces the defect. A variable this process inherited is removed by
/// spelling it on the command, which `get_envs` reports as `(key, None)` and the
/// staged assertion therefore cannot see — so the inherited half is read off a
/// command with nothing staged, where every removal present is an inherited one.
/// That half is vacuous under a plain `cargo test`, whose environment carries no
/// `GIT_*`; under a hook, the one configuration §REQ-test-isolation.3 is written
/// for, it is the whole of the rule.
#[test]
fn a_suite_spawn_carries_no_git_variable_it_was_not_given() {
    let decoy = unique_temp_dir("suite-isolation-git-env");
    let staged: Vec<(&str, std::ffi::OsString)> = repository_env_as_a_hook_leaves_it(&decoy)
        .into_iter()
        .chain(repository_env_names_no_report_mentioned(&decoy))
        .collect();
    let names: Vec<&str> = staged.iter().map(|(key, _)| *key).collect();

    let mut cmd = git_command_with_staged_env(&staged);
    // Set on the command the helper returned, so it is the deliberate one. Four
    // fixtures set this to bound git's ancestor discovery to their own directory.
    cmd.env("GIT_CEILING_DIRECTORIES", decoy.as_ref() as &Path);

    let carried: Vec<String> = cmd
        .get_envs()
        .filter(|(key, value)| value.is_some() && key.to_string_lossy().starts_with("GIT_"))
        .map(|(key, _)| key.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        carried,
        vec!["GIT_CEILING_DIRECTORIES".to_string()],
        "a spawn the suite built carries {carried:?} into the child. Which repository a \
         spawned command acts on is chosen, never inherited (§REQ-test-isolation.3): only \
         a variable set after the removal is deliberate, and these were staged as a hook \
         leaves them — {names:?}."
    );

    // The inherited half, which is the one a hook produces: nothing is staged
    // here, so every removal the command carries is one this process was handed.
    // §REQ-test-isolation.3
    let bare = git_command();
    let taken_off: BTreeSet<String> = bare
        .get_envs()
        .filter(|(_, value)| value.is_none())
        .map(|(key, _)| key.to_string_lossy().into_owned())
        .collect();
    let left_on: Vec<String> = std::env::vars_os()
        .map(|(key, _)| key.to_string_lossy().into_owned())
        .filter(|key| key.starts_with("GIT_"))
        .filter(|key| !taken_off.contains(key))
        .collect();
    assert!(
        left_on.is_empty(),
        "this harness process was handed {left_on:?} and a `git` the suite builds does not \
         take them off, so the child acts on whatever repository they name \
         (§REQ-test-isolation.3) — which under a hook is the repository being committed to. \
         An inherited variable has to be spelled on the command as a removal, because \
         `env_remove` is the only way to say \"not this one\" about a variable the parent \
         would otherwise pass down."
    );
}

/// §REQ-test-isolation.3: the repository `GIT_DIR` names is not written to.
///
/// The failing fixtures are the symptom; the writes are the defect. This runs
/// the sequence `init_git_repo` runs in the integration harness, with git's
/// repository variables staged on the child as a hook leaves them, and then
/// reads the decoy back: a repository no test named, which must have the `HEAD`,
/// the index and the configuration it started with.
#[test]
fn a_git_fixture_leaves_the_repository_git_dir_names_untouched() {
    let root = unique_temp_dir("suite-isolation-git-decoy");
    let decoy = root.join("decoy");
    let fixture = root.join("fixture");
    build_decoy_repository(&decoy);
    let before = decoy_state(&decoy);

    std::fs::create_dir_all(&fixture).expect("create the fixture repository directory");
    let at = fixture.to_str().expect("fixture path should be unicode");
    let mut refused: Vec<String> = Vec::new();
    for args in [
        vec!["-C", at, "init", "-q"],
        vec!["-C", at, "config", "user.email", "rhei@example.test"],
        vec!["-C", at, "config", "user.name", "Rhei Test"],
    ] {
        run_fixture_git(&decoy, &args, &mut refused);
    }
    std::fs::write(fixture.join("README.md"), "repo\n").expect("write the fixture readme");
    for args in [vec!["-C", at, "add", "README.md"], vec!["-C", at, "commit", "-qm", "initial"]] {
        run_fixture_git(&decoy, &args, &mut refused);
    }

    let after = decoy_state(&decoy);
    assert_eq!(
        before,
        after,
        "a fixture's git wrote into {}, a repository no test named — the one a hook-run \
         suite is handed as `GIT_DIR`, which is the repository being committed to \
         (§REQ-test-isolation.3). `-C {at}` named a work tree and settled nothing about \
         which repository was written. What the fixture's own git reported: {refused:?}",
        decoy.display()
    );
    assert!(
        refused.is_empty(),
        "a fixture's git setup did not succeed, and a fixture whose git setup does not \
         succeed fails the test that owns it rather than contending for another \
         repository's index (§REQ-test-isolation.3): {refused:?}"
    );
}

/// One fixture `git`, with the repository variables staged as a hook leaves
/// them. A refusal is collected rather than asserted, so that the decoy is read
/// back even when the first call is the one that fails.
fn run_fixture_git(decoy: &Path, args: &[&str], refused: &mut Vec<String>) {
    let output = git_command_as_if_inherited(decoy)
        .args(args)
        .output()
        .expect("the fixture's git should run");
    if !output.status.success() {
        refused.push(format!("git {args:?}: {}", stderr(&output).trim().replace('\n', " / ")));
    }
}

/// §REQ-test-isolation.3: removing the way is what holds it — no file in the
/// suite names `git` as a program except the one helper that applies the
/// removal.
///
/// This is the difference between a fix and a fix that holds. A call site
/// corrected in place leaves the next caller the same mistake to make, and the
/// next caller is a file nobody will think to check.
#[test]
fn only_the_shared_helper_names_git_as_a_program() {
    let tests = repo_root().join("tests");
    let helper = tests.join("support").join("git_env.rs");
    // Spelled rather than written, so that this file does not count itself.
    let needle = format!("Command::new({:?})", "git");
    let mut offenders: Vec<String> = Vec::new();
    for path in rust_sources_under(&tests) {
        if path == helper {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("a test source should be readable");
        let count = text.matches(needle.as_str()).count();
        if count > 0 {
            let shown = path.strip_prefix(repo_root()).unwrap_or(&path).display().to_string();
            offenders.push(format!("{shown} ({count})"));
        }
    }
    offenders.sort();
    assert!(
        offenders.is_empty(),
        "{} file(s) under tests/ spawn `git` without going through \
         `tests/support/git_env.rs`, so each one inherits whatever repository `GIT_DIR` \
         names (§REQ-test-isolation.3): {offenders:?}. Build the command with \
         `git_command()` instead, and set only what the fixture deliberately wants.",
        offenders.len()
    );
}

/// Every `.rs` file under `dir`, so the guard above reads the suite rather than
/// a list of files someone kept up to date.
fn rust_sources_under(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&next) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                found.push(path);
            }
        }
    }
    found
}
