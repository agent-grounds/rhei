// §AR-source-file-size.3: which state machine a project resolves to, and what it
// says when a rhei root's states file is broken.
// Plan-target resolution is a sibling; fixtures live in `common.rs`.

#[test]
fn a_rheis_own_root_machine_governs_it() {
    // A single workspace rhei keeps its machine file in its own root; the
    // project has no root states.yaml.
    let dir = unique_temp_dir("panta-machine-in-rhei-root");
    fs::write(
        dir.join("index.panta.md"),
        "# Panta: Machine In Rhei\n",
    )
    .expect("write manifest");
    let ws = dir.join("flow");
    fs::create_dir_all(ws.join("tasks")).expect("mkdir workspace");
    fs::write(ws.join("index.rhei.md"), "# Rhei: Flow\n")
        .expect("write index");
    fs::write(ws.join("tasks/one.md"), "### Task 1: Alpha\n**State:** pending\n")
        .expect("write task");
    fs::write(ws.join("states.yaml"), WORKSPACE_STATE_MACHINE).expect("write machine");

    // §AR-rhei-panta.4: a states.yaml in a rhei's own root governs that rhei.
    let output =
        rhei_command().arg("list").arg(&dir).output().expect("list runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("flow.1"),
        "project should load with the rhei-root machine file\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn a_rheis_own_machine_beats_the_project_root_file() {
    // §AR-rhei-panta.4: the rhei's own file governs it even when the project
    // root holds a different machine.
    let dir = unique_temp_dir("panta-machine-mismatch-fallback");
    fs::write(
        dir.join("index.panta.md"),
        "# Panta: Mismatch Fallback\n",
    )
    .expect("write manifest");
    fs::write(
        dir.join("states.yaml"),
        "name: unrelated-machine\nversion: 1\nstates:\n  open:\n    description: Open\n    initial: true\n  done:\n    description: Done\n    final: true\ntransitions:\n  - from: open\n    to: done\n",
    )
    .expect("write unrelated machine");
    let ws = dir.join("flow");
    fs::create_dir_all(ws.join("tasks")).expect("mkdir workspace");
    fs::write(ws.join("index.rhei.md"), "# Rhei: Flow\n")
        .expect("write index");
    fs::write(ws.join("tasks/one.md"), "### Task 1: Alpha\n**State:** pending\n")
        .expect("write task");
    fs::write(ws.join("states.yaml"), WORKSPACE_STATE_MACHINE).expect("write machine");

    let output =
        rhei_command().arg("list").arg(&dir).output().expect("list runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("flow.1"),
        "the rhei-root machine file should win over the project root file\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn a_broken_rhei_root_states_file_surfaces() {
    // §AR-rhei-panta.4: an unloadable own-root machine is an error, not a
    // silent fallback to the project default.
    let dir = unique_temp_dir("panta-machine-broken-candidate");
    fs::write(
        dir.join("index.panta.md"),
        "# Panta: Broken Candidate\n",
    )
    .expect("write manifest");
    let ws = dir.join("flow");
    fs::create_dir_all(ws.join("tasks")).expect("mkdir workspace");
    fs::write(ws.join("index.rhei.md"), "# Rhei: Flow\n")
        .expect("write index");
    fs::write(ws.join("tasks/one.md"), "### Task 1: Alpha\n**State:** pending\n")
        .expect("write task");
    fs::write(ws.join("states.yaml"), "name: [unclosed\n").expect("write broken machine");

    let output =
        rhei_command().arg("list").arg(&dir).output().expect("list runs");
    assert!(!output.status.success(), "a broken candidate machine file must fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("failed to parse state machine"),
        "the parse failure should surface, not a misleading not-found: {stderr}"
    );
}

/// §FS-rhei-states-cmd.3: `rhei states` reports the machine the project runs
/// under. Printing the built-in default while the project root held another
/// named every state wrong.
#[test]
fn states_command_resolves_the_projects_machine() {
    let project = create_panta_project(
        "panta-states-cmd",
        "# Panta: Declared\n",
        &[("auth.rhei.md", "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n")],
        WORKSPACE_STATE_MACHINE,
    );

    let output = rhei_command()
        .arg("states")
        .arg(&project)
        .output()
        .expect("states command should run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "states should succeed\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("State machine: workspace-test-machine"),
        "states should report the project machine, got:\n{stdout}"
    );
    assert!(
        stdout.contains("Source: ") && stdout.contains("states.yaml"),
        "states should name the resolved source file, got:\n{stdout}"
    );
}
