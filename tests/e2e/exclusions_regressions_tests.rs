//! Regression coverage for excluded instruction sources and lossless adapter paths.
//! §FS-rhei-plan-language.3.13 §FS-rhei-agents.1.1.2 §FS-rhei-next.3.1

use super::*;
use std::fs;
use std::path::Path;

fn adapter_settings(root: &Path, agent: &Path, deny_read: bool) {
    let dir = root.join(".agent-grounds/rhei");
    fs::create_dir_all(&dir).unwrap();
    let adapter = if deny_read { r#", "deny_read": {"path_flag": "--deny-read"}"# } else { "" };
    fs::write(dir.join("settings.json"), format!(
        r#"{{"agents": {{"mock": {{"command": {}, "stdin_prompt": true, "timeout": "10s"{adapter}}}}}}}"#,
        fixture_command(agent))).unwrap();
}

const MACHINE: &str = "name: exclusions-regression\nversion: 1\nstates:\n  review:\n    initial: true\n    agent: mock\n  completed: { final: true }\ntransitions: [{ from: review, to: completed }]\n";

/// The selected template is required in both worker surfaces; neither an exact
/// exclusion nor its directory/alias may leak its marker. Unselected templates
/// can still be excluded. §FS-rhei-plan-language.3.13 §FS-rhei-memory.4.1
#[test]
fn exclusions_reject_selected_template_for_run_and_next_with_allowed_control() {
    for exclusion in ["prompt_templates/review.md", "prompt_templates/", "unused.md"] {
        let plan = format!("# Rhei: Template boundary\n\n## Tasks\n\n### Task 1: Review\n**State:** review\n**Excludes:** checkout={exclusion}\n");
        let (dir, plan_path, machine) = setup_single_file("exclusions-template", &plan);
        assert!(std::process::Command::new("git")
            .args(["init", "-q"])
            .arg(&*dir)
            .status()
            .unwrap()
            .success());
        fs::write(
            &machine,
            MACHINE.replace("    agent: mock", "    agent: mock\n    prompt_template: review"),
        )
        .unwrap();
        fs::create_dir_all(dir.join("prompt_templates")).unwrap();
        fs::write(dir.join("prompt_templates/review.md"), "PROMPT-TEMPLATE-SECRET-R1-01").unwrap();
        let agent = write_python_agent(
            &dir,
            "capture.py",
            r#"write(pathlib.Path(env('RHEI_ROOT')) / 'runtime' / 'captured.md', agent_prompt())
result('Template control completed.\n')
"#,
        );
        adapter_settings(&dir, &agent, false);
        let blocked = exclusion != "unused.md";
        for command in ["validate", "next", "run"] {
            let args: &[&str] = match command {
                "next" => &["--peek"],
                "run" => &["--no-tui", "--no-callbacks"],
                _ => &[],
            };
            let output = run_cli(command, &plan_path, &machine, args);
            if blocked {
                assert!(!output.status.success(), "{command} accepted {exclusion}");
                assert!(
                    output.stderr.contains("required prompt-template source"),
                    "{}",
                    output.stderr
                );
                assert!(!output.stdout.contains("PROMPT-TEMPLATE-SECRET-R1-01"));
                assert!(!dir.join("runtime/captured.md").exists());
            } else {
                assert_success(&output);
                if command == "next" {
                    assert!(output.stdout.contains("PROMPT-TEMPLATE-SECRET-R1-01"));
                }
                if command == "run" {
                    assert!(fs::read_to_string(dir.join("runtime/captured.md"))
                        .unwrap()
                        .contains("PROMPT-TEMPLATE-SECRET-R1-01"));
                }
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn exclusions_reject_selected_template_through_canonical_directory_alias() {
    let plan = "# Rhei: Alias\n\n## Tasks\n\n### Task 1: Review\n**State:** review\n**Excludes:** artifact=alias/\n";
    let (dir, plan_path, machine) = setup_single_file("exclusions-template-alias", plan);
    fs::write(
        &machine,
        MACHINE.replace("    agent: mock", "    agent: mock\n    prompt_template: review"),
    )
    .unwrap();
    fs::create_dir_all(dir.join("prompt_templates")).unwrap();
    fs::write(dir.join("prompt_templates/review.md"), "PROMPT-TEMPLATE-ALIAS-R1-01").unwrap();
    std::os::unix::fs::symlink(dir.join("prompt_templates"), dir.join("alias")).unwrap();
    let agent = write_python_agent(&dir, "unused.py", "result('Unexpected spawn.\\n')");
    adapter_settings(&dir, &agent, false);
    for command in ["validate", "next", "run"] {
        let args: &[&str] = match command {
            "next" => &["--peek"],
            "run" => &["--no-tui", "--no-callbacks"],
            _ => &[],
        };
        let output = run_cli(command, &plan_path, &machine, args);
        assert!(!output.status.success());
        assert!(output.stderr.contains("required prompt-template source"), "{}", output.stderr);
        assert!(!output.stdout.contains("PROMPT-TEMPLATE-ALIAS-R1-01"));
    }
}

/// A controlled Python wrapper installs a read audit hook in its child. The
/// child creates the previously absent directory and attempts real file reads;
/// exact denial permits descendants, recursive denial refuses them. This proves
/// the adapter protocol, not an OS sandbox for arbitrary executables.
/// §FS-rhei-agents.1.1.2 §FS-rhei-plan-language.3.13
#[test]
fn exclusions_adapter_distinguishes_missing_file_and_recursive_directory() {
    for recursive in [false, true] {
        let suffix = if recursive { "/" } else { "" };
        let alias_parent = if cfg!(unix) { "alias/" } else { "" };
        let plan = format!("# Rhei: Future directory\n\n## Tasks\n\n### Task 1: Review\n**State:** review\n**Excludes:** checkout={alias_parent}private{suffix}\n");
        let (dir, plan_path, machine) = setup_single_file("exclusions-future-directory", &plan);
        assert!(std::process::Command::new("git")
            .args(["init", "-q"])
            .arg(&*dir)
            .status()
            .unwrap()
            .success());
        fs::write(&machine, MACHINE).unwrap();
        #[cfg(unix)]
        {
            fs::create_dir_all(dir.join("real")).unwrap();
            std::os::unix::fs::symlink(dir.join("real"), dir.join("alias")).unwrap();
        }
        let agent = write_python_agent(
            &dir,
            "deny-wrapper.py",
            r#"import json
import subprocess
args = sys.argv[1:]
separator = args.index('--')
assert '--deny-read' not in args[separator:]
denied = []
for at, arg in enumerate(args[:separator]):
    if arg == '--deny-read':
        raw = args[at + 1]
        target = pathlib.Path(raw)
        assert target.is_absolute()
        assert not target.exists(), 'target must be absent at spawn'
        denied.append((str(target), raw.endswith('/')))
assert denied
prompt = agent_prompt()
assert 'filesystem denied' in prompt
child = r'''
import json, os, pathlib, sys
rules = [(pathlib.Path(path).resolve(), recursive) for path, recursive in json.loads(sys.argv[1])]
root = pathlib.Path(sys.argv[2])
base = root / 'real' if (root / 'alias').is_symlink() else root
def audit(event, args):
    if event != 'open' or not isinstance(args[0], (str, bytes, os.PathLike)):
        return
    path, mode, flags = args
    reading = ('r' in mode or '+' in mode) if isinstance(mode, str) else not flags & os.O_WRONLY
    if reading:
        candidate = pathlib.Path(path).resolve()
        for denied, recursive in rules:
            if candidate == denied or (recursive and denied in candidate.parents):
                raise PermissionError(str(candidate))
sys.addaudithook(audit)
for name in ['private', 'private-copy']:
    directory = base / name
    directory.mkdir()
    (directory / 'child.txt').write_text('created by child', encoding='utf-8')
observed = []
for name in ['private', 'private-copy']:
    try:
        content = (base / name / 'child.txt').read_text(encoding='utf-8')
        assert content == 'created by child'
        observed.append(name + '=read')
    except PermissionError:
        observed.append(name + '=denied')
if (root / 'alias').is_symlink():
    try:
        (root / 'alias' / 'private' / 'child.txt').read_text(encoding='utf-8')
        alias_read = True
    except PermissionError:
        alias_read = False
    assert alias_read == (observed[0] == 'private=read')
(root / 'runtime' / 'observed.txt').write_text('\n'.join(observed), encoding='utf-8')
'''
subprocess.run([sys.executable, '-c', child, json.dumps(denied), env('RHEI_ROOT')], check=True)
result('Future directory reads attempted.\n')
"#,
        );
        adapter_settings(&dir, &agent, true);
        assert_success(&run_cli("run", &plan_path, &machine, &["--no-tui", "--no-callbacks"]));
        let expected = if recursive {
            "private=denied\nprivate-copy=read"
        } else {
            "private=read\nprivate-copy=read"
        };
        assert_eq!(fs::read_to_string(dir.join("runtime/observed.txt")).unwrap(), expected);
    }
}
