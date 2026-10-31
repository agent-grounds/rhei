//! A fake Codex that writes only where a managed workspace would let it.
//!
//! It records the arguments it was given and refuses every write outside its
//! `--add-dir` roots. Under each root it keeps `.git`, `.agents` and `.codex`
//! read-only unless that exact path is itself a root, as Codex does, and under
//! any sandbox but `workspace-write` it writes nothing at all: that is the
//! read-only fallback a managed workspace gives `danger-full-access`. Neither
//! the working directory nor the temporary directory is writable unless
//! granted, so every root Rhei owes has to be on the command line.
//! §FS-rhei-agents.2.2 §FS-rhei-templates.4.1

use std::fs;
use std::path::{Path, PathBuf};

use super::*;

/// One state, launched through `cdx[workspace]`, owing one artifact.
pub(super) const ROOTS_MACHINE: &str = r#"name: codex-roots
version: 1
states:
  working:
    initial: true
    target: cdx[workspace]:openai:fixture
    attempts: 1
    agent_timeout: 60s
    outputs:
      - name: finding
        path: runtime/exports/{task_id}/working.md
  completed:
    final: true
transitions:
  - from: working
    to: completed
"#;

pub(super) const ONE_TASK: &str = "### Task 1: Write through the roots\n**State:** working\n";

/// The shim's body. `FIXTURE`, `LOG`, `STATE` and `EXTRA` are JSON literals.
/// Commits require fixture ownership as well as writable grants.
/// §FS-rhei-agents.2.2.1 §REQ-test-isolation.4
const SHIM: &str = r#"
import json
import subprocess

PROTECTED = ('.git', '.agents', '.codex')
argv = sys.argv[1:]
head = argv[:argv.index('--')] if '--' in argv else list(argv)
roots = [head[i + 1] for i, arg in enumerate(head[:-1]) if arg == '--add-dir']
sandbox = next((head[i + 1] for i, arg in enumerate(head[:-1]) if arg == '--sandbox'), None)


def norm(path):
    return os.path.normcase(os.path.realpath(str(path)))


def under(path, root):
    path, root = norm(path), norm(root)
    return path == root or path.startswith(root.rstrip(os.sep) + os.sep)


def writable(path):
    if sandbox != 'workspace-write':
        return False
    for root in roots:
        if under(path, root) and not any(under(path, os.path.join(root, n)) for n in PROTECTED):
            return True
    return False


refused = []


def guarded_write(path, text):
    if writable(path):
        write(path, text)
    else:
        refused.append(str(path))


prompt = agent_prompt()
found = re.search(r'^## State: (\S+)', prompt, re.MULTILINE)
state = found.group(1) if found else 'unknown'
task = env('RHEI_TASK_ID')
root = pathlib.Path(env('RHEI_ROOT', '.'))
guarded_write(root / 'runtime' / 'exports' / task / (state + '.md'), 'finding\n')
for name in ('root-guards', 'budget-authority'):
    guarded_write(pathlib.Path(STATE) / 'rhei' / name / 'nested-probe', 'probe\n')
for extra in EXTRA:
    guarded_write(pathlib.Path(extra) / 'probe', 'probe\n')


def git(*args):
    return subprocess.run(['git'] + list(args), capture_output=True, text=True)


committed = False
dirs = git('rev-parse', '--path-format=absolute', '--show-toplevel', '--git-dir', '--git-common-dir')
if dirs.returncode == 0:
    checkout, git_dir, common_dir = dirs.stdout.splitlines()
    # Grants do not establish ownership. §FS-rhei-agents.2.2.1 §REQ-test-isolation.4
    blocked = [d for d in (checkout, git_dir, common_dir) if not under(d, FIXTURE)]
    blocked.extend(d for d in (git_dir, common_dir) if not writable(os.path.join(d, 'objects')))
    refused.extend(blocked)
    if not blocked:
        committed = git(
            '-c', 'user.name=Rhei Test', '-c', 'user.email=rhei@example.test',
            '-c', 'commit.gpgsign=false', 'commit', '--no-verify', '--allow-empty', '-q',
            '-m', 'roots probe ' + task,
        ).returncode == 0
result_path = env('RHEI_RESULT_PATH')
if result_path:
    guarded_write(result_path, 'Wrote through the roots.\n')
append(LOG, json.dumps({
    'argv': argv, 'cwd': os.getcwd(), 'roots': roots, 'sandbox': sandbox,
    'refused': refused, 'committed': committed, 'state': state,
}) + '\n')
"#;

/// Write the shim into `dir`, logging to `log`, writing nested state under
/// `state_home`, and probing each of `extra` as a granted directory.
/// Bind ownership to canonical `dir`, independently of launch arguments.
/// §FS-rhei-agents.2.2.1 §REQ-test-isolation.4
pub(super) fn write_roots_shim(
    dir: &Path,
    log: &Path,
    state_home: &Path,
    extra: &[&Path],
) -> PathBuf {
    let literal = |path: &Path| serde_json::to_string(&path.display().to_string()).unwrap();
    let fixture = rhei_core::platform::canonical_path(dir).expect("canonical fixture boundary");
    let extra: Vec<String> = extra.iter().map(|path| path.display().to_string()).collect();
    let body = format!(
        "FIXTURE = {}\nLOG = {}\nSTATE = {}\nEXTRA = {}\n{SHIM}",
        literal(&fixture),
        literal(log),
        literal(state_home),
        serde_json::to_string(&extra).unwrap()
    );
    write_python_agent(dir, "fake_codex.py", &body)
}

/// Every line the shim logged, in order.
pub(super) fn shim_records(log: &Path) -> Vec<serde_json::Value> {
    fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).expect("shim log line is JSON"))
        .collect()
}

/// A `family: codex` profile running the shim; `extra` fields are laid over it.
pub(super) fn cdx_profile(shim: &Path, extra: serde_json::Value) -> serde_json::Value {
    let mut profile = serde_json::json!({
        "family": "codex",
        "command": serde_json::from_str::<serde_json::Value>(&fixture_command(shim)).unwrap(),
        "timeout": "60s",
    });
    if let (Some(profile), Some(extra)) = (profile.as_object_mut(), extra.as_object()) {
        profile.extend(extra.clone());
    }
    profile
}

pub(super) fn write_settings_at(root: &Path, agents: serde_json::Value) {
    let dir = root.join(".agent-grounds/rhei");
    fs::create_dir_all(&dir).expect("create settings dir");
    fs::write(dir.join("settings.json"), serde_json::json!({ "agents": agents }).to_string())
        .expect("write settings");
}

/// A plan directory at `root` holding one task file.
pub(super) fn write_plan_at(root: &Path, title: &str, task: &str) {
    fs::create_dir_all(root.join("tasks")).expect("create plan");
    fs::write(root.join("index.rhei.md"), format!("# Rhei: {title}\n")).expect("write index");
    fs::write(root.join("tasks/01.md"), task).expect("write task");
}

/// A git repository at `dir` with one commit, or `None` without git.
pub(super) fn git_repository(dir: &Path) -> Option<PathBuf> {
    fs::create_dir_all(dir).expect("create repository dir");
    if !git_init_or_absent(dir) {
        return None;
    }
    git_ok(dir, &["commit", "--allow-empty", "-q", "-m", "initial"]);
    Some(dir.to_path_buf())
}

/// Run git in `dir` with a fixed identity, and require it to succeed.
pub(super) fn git_ok(dir: &Path, args: &[&str]) -> String {
    let output = git_command()
        .current_dir(dir)
        .args(["-c", "user.name=Rhei Test", "-c", "user.email=rhei@example.test"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .expect("git should start");
    assert!(
        output.status.success(),
        "git {args:?} in {} failed: {}",
        dir.display(),
        stderr(&output)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// `rhei --state-machine <machine> run <plan>` from `cwd`, plain and sequential.
pub(super) fn run_plan(home: &Path, cwd: &Path, machine: &Path, plan: &Path) -> CliRun {
    let output = rhei_command(home)
        .current_dir(cwd)
        .arg("--state-machine")
        .arg(machine)
        .arg("run")
        .arg(plan)
        .args(["--no-tui", "--no-dashboard"])
        .output()
        .expect("rhei run should start");
    CliRun::from(&output)
}

/// The same directory, judged by identity where both exist.
pub(super) fn same_dir(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// What every launch through the roots must show: the granted roots before the
/// separator, absolute and distinct, covering `expected`; `workspace-write`
/// without full access; and every write the shim tried, done. §FS-rhei-agents.2.2
pub(super) fn assert_wrote_through_roots(
    record: &serde_json::Value,
    expected: &[&Path],
    run: &CliRun,
) {
    let argv: Vec<String> = serde_json::from_value(record["argv"].clone()).unwrap();
    let roots: Vec<String> = serde_json::from_value(record["roots"].clone()).unwrap();
    let context = format!(
        "argv={argv:?}\nrecord={record:#}\nstdout:\n{}\nstderr:\n{}",
        run.stdout, run.stderr
    );
    assert!(!argv.iter().any(|arg| arg.contains("danger-full-access")), "{context}");
    assert_eq!(record["sandbox"], "workspace-write", "{context}");
    if let Some(separator) = argv.iter().position(|arg| arg == "--") {
        assert!(
            !argv[separator..].iter().any(|arg| arg == "--add-dir"),
            "a root after --: {context}"
        );
    }
    for root in &roots {
        assert!(Path::new(root).is_absolute(), "root {root:?} is not absolute: {context}");
    }
    for (index, root) in roots.iter().enumerate() {
        let duplicate =
            roots[..index].iter().any(|earlier| same_dir(Path::new(earlier), Path::new(root)));
        assert!(!duplicate, "root {root:?} is passed twice: {context}");
    }
    for want in expected {
        assert!(
            roots.iter().any(|root| same_dir(Path::new(root), want)),
            "no --add-dir names {}: {context}",
            want.display()
        );
    }
    assert_eq!(record["refused"], serde_json::json!([]), "writes were refused: {context}");
    assert_eq!(record["committed"], true, "the commit did not land: {context}");
}
