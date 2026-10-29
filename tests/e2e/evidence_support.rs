//! The workspaces the evidence and verify scenarios run in, and the readers of
//! what a run leaves under `runtime/evidence/`.
//!
//! Every record is written by a real `rhei run` over a single-file plan: a
//! program step, then an agent step on a stub CLI that saves the prompt it was
//! handed and answers `--version`. The stubs are Python, so they run on every
//! platform the suite does. §REQ-cross-platform.4 §FS-rhei-evidence

use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::{
    fixture_command, fixture_command_with_args, rhei_command, run_cli, unique_temp_dir,
    write_python_agent, CliRun, TestDir,
};

/// The stub agent CLI. On `--version` it prints a version and counts the probe
/// in `version-probes.txt`; otherwise it saves the prompt it read, byte for
/// byte, as `seen-prompt-<task>-<state>.md` beside itself, and finishes.
const STUB_AGENT: &str = r#"here = pathlib.Path(__file__).resolve().parent
if '--version' in sys.argv[1:]:
    append(str(here / 'version-probes.txt'), ' '.join(sys.argv[1:]) + '\n')
    print((here / 'stub-version.txt').read_text(encoding='utf-8').strip() if (here / 'stub-version.txt').exists() else 'stub-cli 1.2.3')
    sys.exit(0)
prompt = agent_prompt()
task = agent_context('RHEI_TASK_ID', 'unknown')
with open(here / ('seen-prompt-' + task + '-' + os.environ.get('RHEI_STATE', '') + '.md'), 'w', encoding='utf-8', newline='') as handle:
    handle.write(prompt)
print('stub agent ran', flush=True)
result('## Result\n\nStub agent finished.\n')
"#;

/// The program step: prints where it ran and succeeds.
const STUB_PROGRAM: &str = r#"print('program ran in ' + os.getcwd(), flush=True)
result('## Result\n\nProgram finished.\n')
"#;

/// A program step, then an agent step on `mock`.
pub const PROBE_PLAN: &str = r#"# Rhei: Evidence probe

## Tasks

### Task 1: Probe
**State:** measure
"#;

/// The machine of [`PROBE_PLAN`], with the program's command spliced in.
pub fn probe_machine(program_command: &str, extra_program: &str) -> String {
    format!(
        r#"name: evidence-probe
version: 1
states:
  measure:
    initial: true
    description: A program step
    program:
      command: {program_command}
{extra_program}    program_timeout: 60s
  work:
    description: An agent step
    agent: mock
    agent_timeout: 60s
    attempts: 1
  done:
    description: Done
    final: true
transitions:
  - {{ from: measure, to: work, description: Measured, exit_code: 0 }}
  - {{ from: work, to: done, description: Worked }}
"#
    )
}

/// One standalone workspace: a plan, its machine, the stubs and the settings.
pub struct EvidenceWorkspace {
    /// Held so the tree outlives the test that built it.
    pub _dir: TestDir,
    pub root: PathBuf,
    pub plan: PathBuf,
    pub machine: PathBuf,
    pub agent: PathBuf,
    pub program: PathBuf,
}

/// How the `mock` profile is declared: the stub's command plus `extra`
/// (`stdin_prompt`, `version_args`, …) merged over it, and any top-level
/// settings blocks (`evidence`, `snapshots`, `mcp_servers`) beside `agents`.
pub struct Profile {
    pub fields: serde_json::Value,
    pub settings: serde_json::Value,
}

impl Default for Profile {
    fn default() -> Self {
        Profile {
            fields: serde_json::json!({ "stdin_prompt": true, "timeout": "60s" }),
            settings: serde_json::json!({}),
        }
    }
}

/// The probe workspace: [`PROBE_PLAN`] over [`probe_machine`], `mock` declared
/// as `profile` says, and the program run as `[python, prog.py, --flag]`.
pub fn evidence_workspace(prefix: &str, profile: Profile) -> EvidenceWorkspace {
    evidence_workspace_with(prefix, profile, "")
}

/// [`evidence_workspace`] with extra lines under the program's `program:` block
/// (each indented six spaces), such as an `env:` map.
pub fn evidence_workspace_with(prefix: &str, profile: Profile, extra: &str) -> EvidenceWorkspace {
    let dir = unique_temp_dir(prefix);
    let root = dir.to_path_buf();
    let agent = write_python_agent(&root, "stub-agent.py", STUB_AGENT);
    let program = write_python_agent(&root, "prog.py", STUB_PROGRAM);
    write_settings(&root, &agent, profile);
    let plan = root.join("plan.rhei.md");
    fs::write(&plan, PROBE_PLAN).expect("write plan");
    let machine = root.join("states.yaml");
    let command = fixture_command_with_args(&program, &["--flag"]);
    fs::write(&machine, probe_machine(&command, extra)).expect("write machine");
    EvidenceWorkspace { _dir: dir, root, plan, machine, agent, program }
}

fn write_settings(root: &Path, agent: &Path, profile: Profile) {
    let command: serde_json::Value =
        serde_json::from_str(&fixture_command(agent)).expect("fixture command is JSON");
    let mut mock = serde_json::json!({ "command": command });
    merge(&mut mock, &profile.fields);
    let mut settings = serde_json::json!({
        "defaults": { "agent": "mock", "agent_timeout": "60s" },
        "agents": { "mock": mock },
    });
    merge(&mut settings, &profile.settings);
    let settings_dir = root.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings dir");
    fs::write(settings_dir.join("settings.json"), settings.to_string()).expect("write settings");
}

/// Shallow-merge the keys of `extra` into `base`; a `null` value removes one.
fn merge(base: &mut serde_json::Value, extra: &serde_json::Value) {
    let (Some(base), Some(extra)) = (base.as_object_mut(), extra.as_object()) else { return };
    for (key, value) in extra {
        if value.is_null() && key != "version_args" {
            base.remove(key);
        } else {
            base.insert(key.clone(), value.clone());
        }
    }
}

impl EvidenceWorkspace {
    /// `rhei run` to completion. The run itself must succeed: a test that pins
    /// evidence must not pass or fail on a broken fixture.
    pub fn run(&self) -> CliRun {
        let result = run_cli("run", &self.plan, &self.machine, &["--no-tui", "--no-callbacks"]);
        assert!(
            result.status.success(),
            "fixture: the probe run failed\nstdout:\n{}\nstderr:\n{}",
            result.stdout,
            result.stderr
        );
        result
    }

    /// `rhei <args…>` with the workspace's isolated home, in the workspace.
    pub fn rhei(&self, args: &[&str]) -> CliRun {
        let output = rhei_command(self.root.join(".home"))
            .current_dir(&self.root)
            .args(args)
            .output()
            .expect("rhei runs");
        CliRun::from(&output)
    }

    /// `rhei verify <root> [args…]`.
    pub fn verify(&self, args: &[&str]) -> CliRun {
        let root = self.root.display().to_string();
        let mut all = vec!["verify", root.as_str()];
        all.extend_from_slice(args);
        self.rhei(&all)
    }

    /// `rhei verify <root> --json [args…]`, parsed. Fails on unparsable stdout.
    pub fn verify_json(&self, args: &[&str]) -> (CliRun, serde_json::Value) {
        let mut all = vec!["--json"];
        all.extend_from_slice(args);
        let result = self.verify(&all);
        let parsed = serde_json::from_str(&result.stdout).unwrap_or_else(|err| {
            panic!("verify --json prints one object: {err}\n{}\n{}", result.stdout, result.stderr)
        });
        (result, parsed)
    }

    pub fn evidence_dir(&self) -> PathBuf {
        self.root.join("runtime/evidence")
    }

    /// The prompt the stub agent saved for `task` in `state`.
    pub fn seen_prompt(&self, task: &str, state: &str) -> Vec<u8> {
        let path = self.root.join(format!("seen-prompt-{task}-{state}.md"));
        fs::read(&path).unwrap_or_else(|err| panic!("fixture: the stub saved no prompt: {err}"))
    }
}

/// Every run directory under `runtime/evidence/runs/`, sorted.
pub fn run_dirs(evidence: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(evidence.join("runs")) else { return Vec::new() };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort();
    dirs
}

/// Every `*.start.json` or `*.end.json` record of every run, parsed.
pub fn records(evidence: &Path, phase: &str) -> Vec<serde_json::Value> {
    let suffix = format!(".{phase}.json");
    let mut found = Vec::new();
    for dir in run_dirs(evidence) {
        let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
            .expect("read run dir")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.to_string_lossy().ends_with(&suffix))
            .collect();
        paths.sort();
        for path in paths {
            let text = fs::read_to_string(&path).expect("read record");
            found.push(
                serde_json::from_str(&text)
                    .unwrap_or_else(|err| panic!("{}: {err}", path.display())),
            );
        }
    }
    found
}

/// The one start record of `kind`, failing with what was there otherwise.
pub fn one_start(evidence: &Path, kind: &str) -> serde_json::Value {
    let starts: Vec<serde_json::Value> =
        records(evidence, "start").into_iter().filter(|r| r["kind"] == kind).collect();
    assert_eq!(
        starts.len(),
        1,
        "expected one `{kind}` start record under {}; found {starts:#?}",
        evidence.display()
    );
    starts.into_iter().next().expect("one record")
}

/// The end record answering for `start`.
pub fn end_for(evidence: &Path, start: &serde_json::Value) -> serde_json::Value {
    let id_field = if start["kind"] == "callback" { "callback_id" } else { "invocation_id" };
    records(evidence, "end")
        .into_iter()
        .find(|end| end[id_field] == start[id_field])
        .unwrap_or_else(|| panic!("no end record for {}", start[id_field]))
}

/// The manifest of the one run, parsed, and its path.
pub fn only_manifest(evidence: &Path) -> (PathBuf, serde_json::Value) {
    let dirs = run_dirs(evidence);
    assert_eq!(dirs.len(), 1, "expected one run directory under {}", evidence.display());
    let path = dirs[0].join("manifest.json");
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("the run wrote no manifest at {}: {err}", path.display()));
    (path, serde_json::from_str(&text).expect("manifest parses"))
}

/// The stored object for `hex`, if one is there.
pub fn object(evidence: &Path, hex: &str) -> Option<Vec<u8>> {
    fs::read(evidence.join("objects/sha256").join(hex)).ok()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    Sha256::digest(bytes).iter().fold(String::new(), |mut hex, byte| {
        let _ = write!(hex, "{byte:02x}");
        hex
    })
}

/// Every file under `dir`, recursively, read as bytes, with its path.
pub fn every_file(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else { return out };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            out.extend(every_file(&path));
        } else if let Ok(bytes) = fs::read(&path) {
            out.push((path, bytes));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Fail naming the file when `needle` appears in any byte under `dir`.
pub fn assert_nowhere_under(dir: &Path, needle: &str, what: &str) {
    for (path, bytes) in every_file(dir) {
        let text = String::from_utf8_lossy(&bytes);
        assert!(!text.contains(needle), "{what} ({needle}) appears in {}", path.display());
    }
}

/// Fail unless `dir` exists and holds at least one file: the precondition of
/// any assertion that something is absent from it.
pub fn assert_store_exists(dir: &Path) {
    assert!(
        !every_file(dir).is_empty(),
        "the run left no evidence under {}: nothing records what its processes were given",
        dir.display()
    );
}

/// The `$id`'s published schema, read from the checkout.
pub fn published_schema(schema_id: &str) -> serde_json::Value {
    let path =
        super::repo_root().join("crates/rhei-cli/schemas").join(format!("{schema_id}.schema.json"));
    serde_json::from_str(&fs::read_to_string(&path).expect("schema source")).expect("schema JSON")
}

/// Validate `instance` against the published `schema_id`, compiled the way the
/// accounting contract tests compile theirs: without the relative `$id`.
pub fn assert_valid(schema_id: &str, instance: &serde_json::Value) {
    let mut schema = published_schema(schema_id);
    schema.as_object_mut().expect("schema object").remove("$id");
    let compiled = jsonschema::JSONSchema::compile(&schema)
        .unwrap_or_else(|err| panic!("compile {schema_id}: {err}"));
    if let Err(errors) = compiled.validate(instance) {
        let errors: Vec<String> = errors.map(|error| error.to_string()).collect();
        panic!("{schema_id} rejects {instance:#}: {errors:#?}");
    };
}
