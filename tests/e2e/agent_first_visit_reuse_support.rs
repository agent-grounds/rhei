//! The shared-output revision fixture from #755, inside an owning rhei.
// §FS-rhei-agent-visit-pairing.6

use super::agent_reentry_support::{spawn_lines, write_settings};
use super::*;
use std::fs;
use std::path::PathBuf;

pub(super) const ARGS: &[&str] = &["--no-tui", "--no-dashboard", "--no-callbacks"];
pub(super) const OUTPUTS: &[&str] = &["runtime/design.md", "runtime/design.json"];

pub(super) struct Revision {
    pub dir: TestDir,
    pub project: PathBuf,
    pub owner: PathBuf,
    pub machine: PathBuf,
}

impl Revision {
    pub fn new(label: &str, sibling_writer: bool) -> Self {
        let dir = unique_temp_dir(&format!("first-visit-reuse-{label}"));
        let project = dir.join("project");
        let owner = project.join("design");
        fs::create_dir_all(owner.join("tasks")).expect("owner workspace");
        fs::write(project.join("index.panta.md"), "# Panta: Revision\n").unwrap();
        fs::write(owner.join("index.rhei.md"), "# Rhei: Design\n").unwrap();
        let state = if sibling_writer { "prepare" } else { "verdict" };
        fs::write(
            owner.join("tasks/design.md"),
            format!("### Task design: Revise the design\n**State:** {state}\n"),
        )
        .unwrap();
        if sibling_writer {
            fs::write(
                owner.join("tasks/writer.md"),
                "### Task writer: Write the first design\n**State:** verdict\n",
            )
            .unwrap();
        }
        fs::write(
            owner.join("tasks/other.md"),
            "### Task other: Unrelated work\n**State:** hold\n",
        )
        .unwrap();
        // Another rhei has identically named artifacts. They must never be
        // reported as this invocation's outputs or reached by its reset.
        let neighbour = project.join("neighbour");
        fs::create_dir_all(neighbour.join("tasks")).unwrap();
        fs::write(neighbour.join("index.rhei.md"), "# Rhei: Neighbour\n").unwrap();
        fs::write(neighbour.join("tasks/held.md"), "### Task held: Stay here\n**State:** hold\n")
            .unwrap();
        for path in OUTPUTS {
            super::agent_reentry_support::seed_output(&neighbour, path);
        }
        let machine = write_fixture_file(&owner, "states.yaml", MACHINE);
        let agent = write_python_agent(&dir, "mock-agent.py", AGENT);
        write_settings(&project, &agent, false);
        Self { dir, project, owner, machine }
    }

    pub fn run(&self, parallel: &str) -> CliRun {
        let mut args = ARGS.to_vec();
        args.extend(["--parallel", parallel]);
        let run = run_cli("run", &self.project, &self.machine, &args);
        assert_success(&run);
        run
    }

    pub fn transition(&self, task: &str, from: &str, to: &str) {
        assert_success(&run_transition(&self.project, &self.machine, task, from, to));
    }

    pub fn spawns(&self) -> Vec<String> {
        spawn_lines(&self.owner)
    }
}

const AGENT: &str = r#"root = pathlib.Path(env('RHEI_ROOT'))
state = env('RHEI_STATE')
append(root / 'runtime' / 'spawn-count.log', env('RHEI_TASK_ID') + ' ' + state + '\n')
if state in ('verdict', 'revise'):
    version = 'v1' if state == 'verdict' else 'v2'
    write(root / 'runtime' / 'design.md', '# Design ' + version + '\n')
    write(root / 'runtime' / 'design.json', '{"version":"' + version + '"}\n')
result('done in ' + state + '\n')
"#;

const MACHINE: &str = r#"name: first-visit-reuse
version: 1
states:
  prepare: { gating: true, description: Prepare for owner review }
  verdict:
    initial: true
    description: Write v1
    agent: mock
    outputs:
      - { name: design, path: runtime/design.md }
      - { name: data, path: runtime/design.json }
  approve-design: { gating: true, description: Owner review }
  revise:
    description: Revise in place
    agent: mock
    inputs:
      - { name: design, path: runtime/design.md }
    outputs:
      - { name: design, path: runtime/design.md }
      - { name: data, path: runtime/design.json }
  hold: { gating: true, description: Held }
  work: { description: Unrelated work, agent: mock }
  completed: { final: true, description: Done }
  cancelled: { final: true, description: Dropped }
transitions:
  - { from: prepare, to: approve-design }
  - { from: verdict, to: approve-design }
  - { from: approve-design, to: completed }
  - { from: approve-design, to: revise }
  - { from: revise, to: approve-design }
  - { from: hold, to: work }
  - { from: work, to: completed }
  - { from: "*", to: cancelled }
"#;
