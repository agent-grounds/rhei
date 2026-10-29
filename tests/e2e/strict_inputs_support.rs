//! A directory workspace laid the way a template lays one: its own
//! `states.yaml`, and a program state that runs `scripts/step.py` beside it.
//! The script appends one evidence line per ticket naming the bytes it ran
//! (`mark=`) and the command the machine gave it (`argv=`), then runs the
//! ticket's own action file, `on-<n>.py` at the workspace root, which is where
//! a case makes a ticket edit an execution input mid-run. The action files sit
//! outside `scripts/`, so they are never sealed themselves. §FS-rhei-seal.2.1
//! Python, not a shell script: §REQ-cross-platform.4

use std::fs;
use std::path::{Path, PathBuf};

use super::python_fixture::fixture_command_with_args;
use super::*;

const STEP: &str = r#"MARK = 'laid'
ws = pathlib.Path(__file__).resolve().parent.parent
local = os.environ['RHEI_TASK_ID_LOCAL']
append(ws / 'evidence.log', 'ticket ' + local + ' mark=' + MARK + ' argv=' + ' '.join(sys.argv[1:]) + '\n')

def wait_for(name, seconds=30.0):
    deadline = time.monotonic() + seconds
    while not (ws / name).exists():
        if time.monotonic() >= deadline:
            raise RuntimeError('barrier was not released: ' + name)
        time.sleep(0.02)

action = ws / ('on-' + local + '.py')
if action.exists():
    exec(action.read_text(encoding='utf-8'))
result('step ran\n')
"#;

/// The action that edits the sealed script the way an editor saves it: a new
/// file moved over the old one. Idempotent, so a resumed ticket re-running it
/// changes no bytes.
pub const EDIT_SCRIPT: &str = r#"script = ws / 'scripts' / 'step.py'
text = script.read_text(encoding='utf-8')
if "MARK = 'laid'" in text:
    temp = ws / 'scripts' / 'step.tmp'
    write(temp, text.replace("MARK = 'laid'", "MARK = 'edited'"))
    os.replace(temp, script)
"#;

pub struct Drift {
    pub _dir: TestDir,
    pub ws: PathBuf,
    pub home: PathBuf,
}

/// What a case varies on the one-state machine.
#[derive(Default)]
pub struct MachineShape<'a> {
    /// `concurrent: true` on the program state, so `--parallel` holds two.
    pub concurrent: bool,
    /// A `cli:` callback on the `step -> completed` edge, already YAML-encoded.
    pub on_enter: Option<&'a str>,
}

pub fn drift_machine(name: &str, command: &str, shape: &MachineShape) -> String {
    let concurrent = if shape.concurrent { "    concurrent: true\n" } else { "" };
    let on_enter = shape.on_enter.map(|cb| format!("    on_enter: {cb}\n")).unwrap_or_default();
    format!(
        "name: {name}\nversion: 1\nstates:\n  step:\n    initial: true\n    description: Run the \
         step script\n{concurrent}    program:\n      command: {command}\n    program_timeout: \
         60s\n  completed:\n    final: true\n    description: Done\ntransitions:\n  - from: \
         step\n    to: completed\n    exit_code: 0\n{on_enter}"
    )
}

/// Lay rhei `<id>` at `<parent>/<id>`, with `tasks` as its one task file.
pub fn lay_member(parent: &Path, id: &str, tasks: &str, shape: &MachineShape) -> PathBuf {
    lay_member_files(parent, id, &[tasks], shape)
}

/// [`lay_member`] with one task file per entry, `tasks/<nn>-drift.md`: a
/// `--parallel` run holds two workers only when they live in different files.
pub fn lay_member_files(parent: &Path, id: &str, tasks: &[&str], shape: &MachineShape) -> PathBuf {
    let ws = parent.join(id);
    fs::create_dir_all(ws.join("tasks")).expect("workspace directories");
    fs::create_dir_all(ws.join("scripts")).expect("scripts directory");
    write_fixture_file(&ws, "index.rhei.md", &format!("# Rhei: Drift {id}\n"));
    for (index, text) in tasks.iter().enumerate() {
        write_fixture_file(&ws.join("tasks"), &format!("{:02}-drift.md", index + 1), text);
    }
    let step = write_python_agent(&ws.join("scripts"), "step.py", STEP);
    let command = fixture_command_with_args(&step, &["laid-machine"]);
    write_fixture_file(&ws, "states.yaml", &drift_machine(&format!("drift-{id}"), &command, shape));
    ws
}

/// A standalone workspace `drift` under a fresh temporary directory, with an
/// isolated `HOME` whose user settings file does not exist. §REQ-test-isolation
pub fn lay_drift(prefix: &str, tasks: &[&str], shape: &MachineShape) -> Drift {
    let dir = unique_temp_dir(prefix);
    let ws = lay_member_files(&dir, "drift", tasks, shape);
    let home = dir.join("home");
    Drift { ws, home, _dir: dir }
}

/// Give ticket `<n>` of the workspace at `ws` an action to run after its
/// evidence line.
pub fn on_ticket(ws: &Path, n: u32, body: &str) {
    write_fixture_file(ws, &format!("on-{n}.py"), body);
}

/// `rhei <args>` from inside the workspace, under the case's `HOME`.
pub fn rhei_at(home: &Path, cwd: &Path, args: &[&str]) -> CliRun {
    let output = rhei_command(home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("rhei should start");
    CliRun::from(&output)
}

impl Drift {
    pub fn rhei(&self, args: &[&str]) -> CliRun {
        rhei_at(&self.home, &self.ws, args)
    }

    /// `rhei run <ws> --no-tui <extra…>`.
    pub fn run(&self, extra: &[&str]) -> CliRun {
        let ws = self.ws.display().to_string();
        let mut args = vec!["run", ws.as_str(), "--no-tui"];
        args.extend_from_slice(extra);
        self.rhei(&args)
    }

    pub fn seal(&self) -> CliRun {
        self.rhei(&["seal", &self.ws.display().to_string()])
    }

    pub fn evidence(&self) -> String {
        evidence(&self.ws)
    }

    pub fn generation(&self, n: u32) -> PathBuf {
        generation(&self.ws, "drift", n)
    }
}

pub fn evidence(ws: &Path) -> String {
    fs::read_to_string(ws.join("evidence.log")).unwrap_or_default()
}

/// `runtime/seal/<rhei>/<n>.json` under the rhei's execution root. §FS-rhei-seal.3.1
pub fn generation(root: &Path, rhei: &str, n: u32) -> PathBuf {
    root.join("runtime").join("seal").join(rhei).join(format!("{n}.json"))
}

/// The `**State:**` of `### Task <n>:` in the workspace's task files.
pub fn task_state(ws: &Path, n: u32) -> String {
    let mut files: Vec<PathBuf> = fs::read_dir(ws.join("tasks"))
        .expect("tasks directory")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    files.sort();
    let text: String = files.iter().map(|file| fs::read_to_string(file).unwrap()).collect();
    let heading = format!("### Task {n}:");
    let body = text.split(&heading).nth(1).unwrap_or_else(|| panic!("no {heading} in:\n{text}"));
    let line = body
        .lines()
        .find(|line| line.starts_with("**State:**"))
        .unwrap_or_else(|| panic!("no state for {heading} in:\n{text}"));
    line.trim_start_matches("**State:**").split_whitespace().next().unwrap_or("").to_string()
}

/// Whether any log or spawn record was written for ticket `<id>`: a refused
/// start charges nothing, so it leaves neither. §FS-rhei-seal.8.4
pub fn spawned(root: &Path, id: &str) -> Vec<String> {
    let prefix = format!("task-{id}-");
    ["logs", "spawns"]
        .iter()
        .filter_map(|dir| fs::read_dir(root.join("runtime").join(dir)).ok())
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(&prefix))
        .collect()
}

pub fn read_json(path: &Path) -> serde_json::Value {
    let text = fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("no JSON at {}: {err}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|err| panic!("{}: {err}", path.display()))
}

/// Assert the strict run refused: exit `1`, and stderr names the changed file
/// and the remedy. §FS-rhei-seal.8.4
pub fn assert_refused(run: &CliRun, file: &str) {
    assert_eq!(
        run.status.code(),
        Some(1),
        "a refused start exits 1\nstdout:\n{}\nstderr:\n{}",
        run.stdout,
        run.stderr
    );
    for expected in [file, "rhei seal"] {
        assert!(
            run.stderr.contains(expected),
            "the refusal should name `{expected}`\nstderr:\n{}",
            run.stderr
        );
    }
}

pub fn ticket_chain(count: u32) -> String {
    let mut plan = String::new();
    for n in 1..=count {
        let prior = if n > 1 { format!("**Prior:** {}\n", n - 1) } else { String::new() };
        plan.push_str(&format!("### Task {n}: Step {n}\n**State:** step\n{prior}\n"));
    }
    plan
}
