//! Equivalent file arguments render the same static view. §FS-rhei-viz.7.2.1

use std::fs;
use std::path::Path;

use serde_json::Value;

use super::*;

const PLAN: &str =
    "# Rhei: Path spellings\n\n## Tasks\n\n### Task a: Render this task\n**State:** done\n";
const MACHINE: &str = r#"
name: path-control
version: 1
states:
  open:
    description: Waiting
  done:
    final: true
transitions:
  - from: open
    to: done
profiles:
  work:
    initial: open
    allowed: [open, done]
node_policy:
  root: work
  default: work
"#;

fn render(cwd: &Path, home: &Path, input: &Path, explicit_machine: bool) -> Value {
    let output = home.join("view.html");
    // Reusing an output cannot let an earlier successful control mask a failed render.
    if output.exists() {
        fs::remove_file(&output).expect("remove previous visualization");
    }
    let mut cmd = rhei_command(home);
    cmd.current_dir(cwd)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("NO_COLOR", "1")
        .args(["viz"])
        .arg(input)
        .arg("--output")
        .arg(&output);
    if explicit_machine {
        cmd.arg("--state-machine").arg(cwd.join("control.yaml"));
    }
    let result = CliRun::from(&cmd.output().expect("run viz"));
    eprintln!(
        "viz {} (explicit machine: {explicit_machine}): {}\n{}{}",
        input.display(),
        result.status,
        result.stdout,
        result.stderr
    );
    assert_success(&result);
    let html = fs::read_to_string(&output).expect("read visualization");
    let boot = html.split_once("const BOOT = ").expect("embedded static model").1;
    let boot = boot
        .lines()
        .next()
        .expect("embedded static model line")
        .strip_suffix(';')
        .expect("end of embedded static model");
    let bundle: Value = serde_json::from_str(boot).expect("static model is JSON");
    let plans = bundle.as_object().expect("plan bundle");
    assert_eq!(plans.len(), 1);
    let plan = plans.values().next().unwrap().clone();
    assert_eq!(plan["plan_title"], "Path spellings");
    assert_eq!(plan["tasks"].as_array().unwrap().len(), 1);
    assert_eq!(plan["tasks"][0]["id"], "plan.a");
    assert_eq!(plan["tasks"][0]["title"], "Render this task");
    assert_eq!(plan["tasks"][0]["state"], "done");
    assert_eq!(plan["tasks"][0]["history"], serde_json::json!([{"from": "open", "to": "done"}]));
    let machine = if explicit_machine { "path-control" } else { "path-sibling" };
    assert_eq!(plan["machine"]["name"], machine);
    eprintln!("rendered plan, task, {machine} machine, and plan-local history");
    plan
}

fn equivalent_file_paths(explicit_machine: bool) {
    let dir = unique_temp_dir("rhei-643-viz-file-paths");
    let home = dir.join("home");
    let work = dir.join("work");
    fs::create_dir_all(work.join("runtime")).unwrap();
    let plan = write_fixture_file(&work, "plan.rhei.md", PLAN);
    write_fixture_file(&work, "control.yaml", MACHINE);
    write_fixture_file(&work, "states.yaml", &MACHINE.replace("path-control", "path-sibling"));
    write_fixture_file(&work.join("runtime"), "state-transitions.log", "plan.a open@done\n");
    // The nested control must read the plan's siblings, rather than the caller's.
    write_fixture_file(&dir, "states.yaml", &MACHINE.replace("path-control", "wrong-directory"));
    write_fixture_file(&dir, "control.yaml", MACHINE);

    let dot = render(&work, &home, Path::new("./plan.rhei.md"), explicit_machine);
    let absolute = render(&work, &home, &plan, explicit_machine);
    assert_eq!(absolute, dot);
    let nested = render(&dir, &home, Path::new("work/plan.rhei.md"), explicit_machine);
    assert_eq!(nested, dot);
    let bare = render(&work, &home, Path::new("plan.rhei.md"), explicit_machine);
    assert_eq!(bare, dot);
}

/// Port the intake reproducer's explicit-machine comparison to the CLI harness.
/// §FS-rhei-viz.7.2.1
#[test]
fn viz_equivalent_file_paths_with_explicit_machine() {
    equivalent_file_paths(true);
}

/// The same spellings discover the sibling machine and preserve a nonempty parent.
/// §FS-rhei-viz.7.2.1
#[test]
fn viz_equivalent_file_paths_with_sibling_machine() {
    equivalent_file_paths(false);
}
