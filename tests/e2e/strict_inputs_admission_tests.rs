//! Live admission under `--strict-inputs`: a program in member `a` edits member
//! `b`'s machine and then lays member `c`. The enclosing run seals `c` on
//! arrival as its own generation 1 and runs it, while `b`, whose sealed machine
//! changed, is not re-adopted and its ticket never runs the edited command.
//! §FS-rhei-seal.8.6 §FS-rhei-panta.6.2

use std::fs;
use std::path::Path;

use super::strict_inputs_support::*;
use super::*;

fn lay_follow_template(project: &Path) -> std::path::PathBuf {
    let template = project.join(".agent-grounds/rhei/templates/follow-on");
    fs::create_dir_all(template.join("tasks")).unwrap();
    let program = write_python_agent(
        project,
        "follow.py",
        "write(pathlib.Path.cwd() / 'follow-ran', 'x')\nresult('follow-on ran\\n')\n",
    );
    write_fixture_file(
        &template,
        "template.yaml",
        "name: follow-on\nversion: 1.0.0\ndescription: Laid mid-run\n",
    );
    write_fixture_file(&template, "index.rhei.md", "# Rhei: Follow\n");
    write_fixture_file(
        &template,
        "states.yaml",
        &drift_machine("follow-on", &fixture_command(&program), &MachineShape::default()),
    );
    write_fixture_file(
        &template,
        "tasks/01-drift.md",
        "### Task 1: Laid mid-run\n**State:** step\n",
    );
    template
}

#[test]
#[ignore = "red until #521: admission re-adopts a drifted member's machine; implement removes this"]
fn admission_seals_a_new_member_and_refuses_a_member_whose_machine_changed() {
    let dir = unique_temp_dir("strict-admission");
    let project = dir.join("project");
    let home = dir.join("home");
    fs::create_dir_all(&project).unwrap();
    write_fixture_file(&project, "index.panta.md", "# Panta: Strict admission\n");
    let template = lay_follow_template(&project);
    let shape = MachineShape::default();
    let a = lay_member(&project, "a", "### Task 1: Edit b, lay c\n**State:** step\n", &shape);
    let b = lay_member(
        &project,
        "b",
        "### Task 1: Runs after a\n**State:** step\n**Prior:** a.1\n",
        &shape,
    );
    let quote = |path: &Path| serde_json::to_string(&path.display().to_string()).unwrap();
    on_ticket(
        &a,
        1,
        &format!(
            "import subprocess\n\
             machine = pathlib.Path({b_machine})\n\
             write(machine, machine.read_text(encoding='utf-8').replace('laid-machine', 'edited-machine'))\n\
             done = subprocess.run([{rhei}, 'instantiate', {template}, '--output', {c}], cwd={project})\n\
             assert done.returncode == 0\n",
            b_machine = quote(&b.join("states.yaml")),
            rhei = quote(&rhei_binary()),
            template = quote(&template),
            c = quote(&project.join("c")),
            project = quote(&project),
        ),
    );

    let run = rhei_at(&home, &project, &["run", ".", "--no-tui", "--strict-inputs"]);
    assert_refused(&run, "states.yaml");
    let c = project.join("c");
    assert!(c.join("follow-ran").is_file(), "the admitted member ran\nstderr:\n{}", run.stderr);
    assert_eq!(task_state(&c, 1), "completed");
    let sealed = read_json(&generation(&c, "c", 1));
    assert_eq!(sealed["generation"], 1, "sealed on arrival as its own generation 1: {sealed:#}");
    assert_eq!(sealed["rhei"], "c", "{sealed:#}");
    assert!(generation(&a, "a", 1).is_file() && generation(&b, "b", 1).is_file());
    assert_eq!(task_state(&a, 1), "completed");
    assert_eq!(task_state(&b, 1), "step", "the drifted member starts nothing new");
    assert!(!evidence(&b).contains("argv=edited-machine"), "{}", evidence(&b));
    assert_eq!(spawned(&b, "b.1"), Vec::<String>::new());
}
