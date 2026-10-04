// §AR-source-file-size.3: `rhei init` workspace scaffolding cases, split from
// the validation cases that read what it writes. The agent-discovery note has
// cases of its own in `workspace_validation_init_agents_note.rs`.

#[test]
fn init_creates_project_with_manifest_gitignore_and_agents_note() {
    let dir = unique_temp_dir("init-fresh");
    let host = dir.join("my-cool_project");
    fs::create_dir_all(host.join(".git")).expect("mark repo root");

    // §FS-rhei-init.2: manifest in panta/, ignore rules, agent note at the
    // host, empty-project hint.
    let output = rhei_command()
        .arg("init")
        .arg(&host)
        .output()
        .expect("init runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "init should succeed: {stdout}");
    assert!(
        stdout.contains("Initialized Panta project \"My Cool Project\" at panta/")
            && stdout.contains("no rheis yet"),
        "init should report the derived title, location, and empty state: {stdout}"
    );
    assert_eq!(
        fs::read_to_string(host.join("panta/index.panta.md")).expect("manifest"),
        "# Panta: My Cool Project\n"
    );
    // §FS-rhei-init.3: the project folder is ignored at the host; generated
    // output stays self-contained inside it.
    let host_ignore = fs::read_to_string(host.join(".gitignore")).expect("host gitignore");
    assert!(
        host_ignore.lines().any(|line| line.trim() == "panta/"),
        "host gitignore should ignore the project folder: {host_ignore}"
    );
    let project_ignore =
        fs::read_to_string(host.join("panta/.gitignore")).expect("project gitignore");
    assert!(
        project_ignore.contains("runtime/") && project_ignore.contains(".rhei/cache/"),
        "project gitignore should cover generated output: {project_ignore}"
    );
    let agents = fs::read_to_string(host.join("AGENTS.md")).expect("agents note");
    assert!(
        agents.contains("<!-- rhei:begin -->")
            && agents.contains("lives in `panta/`")
            && agents.contains("<!-- rhei:end -->"),
        "AGENTS.md should carry the marked note naming the location: {agents}"
    );

    // §FS-rhei-init.2: an existing project is refused untouched.
    let output = rhei_command()
        .arg("init")
        .arg(&host)
        .output()
        .expect("init runs");
    assert!(!output.status.success(), "re-init must fail");
    let stderr: String = String::from_utf8_lossy(&output.stderr)
        .chars()
        .filter(|ch| *ch != '│' && *ch != '\n')
        .collect();
    let stderr = stderr.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(stderr.contains("already a Panta project"), "refusal should say why: {stderr}");
}

#[test]
fn init_adopts_existing_bare_rheis_and_unblocks_bare_commands() {
    let dir = unique_temp_dir("init-adopt");
    fs::create_dir_all(dir.join(".git")).expect("mark repo root");
    fs::write(
        dir.join("auth.rhei.md"),
        "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n",
    )
    .expect("write auth");
    fs::write(
        dir.join("billing.rhei.md"),
        "# Rhei: Billing\n\n## Tasks\n\n### Task 1: Invoice\n**State:** pending\n",
    )
    .expect("write billing");
    // An existing AGENTS.md is appended to, not clobbered.
    fs::write(dir.join("AGENTS.md"), "# House rules\n\nBe kind.\n").expect("write agents");

    // The ambiguity error names `rhei init` as the fix. §FS-rhei-panta.6
    let output = rhei_command()
        .arg("list")
        .current_dir(&dir)
        .output()
        .expect("list runs");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("rhei init"),
        "ambiguity error should point at init"
    );

    // §FS-rhei-init.2: default mode refuses to shadow existing plans and
    // names both fixes.
    let output = rhei_command()
        .arg("init")
        .current_dir(&dir)
        .output()
        .expect("init runs");
    assert!(!output.status.success(), "default init over bare rheis must refuse");
    let refusal: String = String::from_utf8_lossy(&output.stderr)
        .chars()
        .filter(|ch| *ch != '│' && *ch != '\n')
        .collect();
    let refusal = refusal.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        refusal.contains("--here") && refusal.contains("auth.rhei.md"),
        "refusal should name the stranded rheis and the adoption flag: {refusal}"
    );

    // §FS-rhei-init.5: adoption (--here) reports the discovered rheis.
    let output = rhei_command()
        .arg("init")
        .args(["--here", "--title", "Adopted"])
        .current_dir(&dir)
        .output()
        .expect("init runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "init should succeed: {stdout}");
    assert!(
        stdout.contains("Initialized Panta project \"Adopted\" with 2 rheis: auth, billing"),
        "init should report discovered rheis: {stdout}"
    );
    let agents = fs::read_to_string(dir.join("AGENTS.md")).expect("agents note");
    assert!(
        agents.starts_with("# House rules") && agents.contains("<!-- rhei:begin -->"),
        "existing AGENTS.md content should be preserved: {agents}"
    );

    // The bare invocation now resolves the new project.
    let output = rhei_command()
        .arg("list")
        .current_dir(&dir)
        .output()
        .expect("list runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("auth.1") && stdout.contains("billing.1"),
        "bare list should work after init: {stdout}"
    );
}

#[test]
fn init_no_agents_skips_note_and_bad_plans_surface_as_warning() {
    let dir = unique_temp_dir("init-warn");
    fs::write(
        dir.join("My Plan.rhei.md"),
        "# Rhei: Broken\n\n## Tasks\n\n### Task 1: Alpha\n**State:** pending\n",
    )
    .expect("write bad plan");

    // §FS-rhei-init.5: a discovery failure is a warning, not an init failure.
    let output = rhei_command()
        .arg("init")
        .args(["--here", "--no-agents"])
        .current_dir(&dir)
        .output()
        .expect("init runs");
    assert!(output.status.success(), "init should still succeed");
    assert!(dir.join("index.panta.md").is_file(), "manifest should be written");
    assert!(!dir.join("AGENTS.md").exists(), "--no-agents should skip the note");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("does not load cleanly") && stderr.contains("My Plan"),
        "load error should surface as a warning: {stderr}"
    );
}

#[test]
fn init_leaves_the_manifest_bare_over_rhei_declared_machines() {
    let dir = unique_temp_dir("init-declared-machines");
    fs::write(dir.join("states.yaml"), WORKSPACE_STATE_MACHINE).expect("write machine");
    fs::write(
        dir.join("auth.rhei.md"),
        "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n",
    )
    .expect("write auth");
    fs::write(
        dir.join("billing.rhei.md"),
        "# Rhei: Billing\n\n## Tasks\n\n### Task 1: Invoice\n**State:** pending\n",
    )
    .expect("write billing");

    // §FS-rhei-init.2: the manifest stays bare — each rhei keeps the machine
    // it declares, so nothing needs hoisting into the project default.
    let output = rhei_command()
        .arg("init")
        .args(["--here", "--no-agents"])
        .current_dir(&dir)
        .output()
        .expect("init runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "init should succeed: {stdout}");
    assert!(
        stdout.contains("with 2 rheis: auth, billing"),
        "init should discover both rheis: {stdout}"
    );
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("does not load cleanly"),
        "rhei-declared machines load without any project default"
    );
    let manifest = fs::read_to_string(dir.join("index.panta.md")).expect("manifest");
    assert!(!manifest.contains("**States:**"), "the manifest stays bare: {manifest}");
}

#[test]
fn init_force_overwrites_manifest_without_duplicating_companions() {
    let dir = unique_temp_dir("init-force");
    fs::create_dir_all(dir.join(".git")).expect("mark repo root");
    let run_init = |args: &[&str]| {
        rhei_command()
            .arg("init")
            .args(args)
            .current_dir(&dir)
            .output()
            .expect("init runs")
    };
    assert!(run_init(&[]).status.success(), "first init should succeed");

    // §FS-rhei-init.2: --force rewrites the manifest in place…
    let output = run_init(&["--force", "--title", "Renamed"]);
    assert!(
        output.status.success(),
        "forced re-init should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(dir.join("panta/index.panta.md")).expect("manifest"),
        "# Panta: Renamed\n"
    );

    // …and the idempotent companion files are updated, never duplicated.
    let agents = fs::read_to_string(dir.join("AGENTS.md")).expect("agents note");
    assert_eq!(
        agents.matches("<!-- rhei:begin -->").count(),
        1,
        "AGENTS.md block must not duplicate: {agents}"
    );
    let host_ignore = fs::read_to_string(dir.join(".gitignore")).expect("host gitignore");
    assert_eq!(
        host_ignore.matches("panta/").count(),
        1,
        "host gitignore entry must not duplicate: {host_ignore}"
    );
    let project_ignore =
        fs::read_to_string(dir.join("panta/.gitignore")).expect("project gitignore");
    assert_eq!(
        project_ignore.matches("runtime/").count(),
        1,
        "project gitignore entries must not duplicate: {project_ignore}"
    );
}

#[test]
fn init_force_without_here_refuses_when_the_host_is_the_project() {
    let dir = unique_temp_dir("init-force-host-project");
    let run_init = |args: &[&str]| {
        rhei_command()
            .arg("init")
            .args(args)
            .current_dir(&dir)
            .output()
            .expect("init runs")
    };
    assert!(run_init(&["--here", "--no-agents"]).status.success(), "adoption succeeds");

    // §FS-rhei-init.2: force means re-initialize, never nest a shadowed
    // `panta/` project inside the existing one.
    let output = run_init(&["--force"]);
    assert!(!output.status.success(), "default-mode --force over a --here project must refuse");
    let stderr: String = String::from_utf8_lossy(&output.stderr)
        .chars()
        .filter(|ch| *ch != '│' && *ch != '\n')
        .collect();
    let stderr = stderr.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        stderr.contains("--force --here"),
        "the refusal should name the re-init path: {stderr}"
    );
    assert!(!dir.join("panta").exists(), "no shadowed nested project may be created");

    assert!(
        run_init(&["--force", "--here", "--no-agents"]).status.success(),
        "--force --here re-initializes the host project"
    );
}

#[test]
fn init_loads_a_mixed_declared_and_silent_machine_set_cleanly() {
    let dir = unique_temp_dir("init-mixed-machines");
    fs::write(dir.join("states.yaml"), WORKSPACE_STATE_MACHINE).expect("write machine");
    fs::write(
        dir.join("auth.rhei.md"),
        "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n",
    )
    .expect("write auth");
    fs::write(
        dir.join("billing.rhei.md"),
        "# Rhei: Billing\n\n## Tasks\n\n### Task 1: Invoice\n**State:** pending\n",
    )
    .expect("write billing");

    // §FS-rhei-init.2: a silent rhei runs the built-in default while a
    // declaring sibling keeps its own machine — no conflict to surface.
    let output = rhei_command()
        .arg("init")
        .args(["--here", "--no-agents"])
        .current_dir(&dir)
        .output()
        .expect("init runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "init should succeed: {stdout}");
    assert!(
        stdout.contains("with 2 rheis: auth, billing"),
        "both rheis should load despite differing machines: {stdout}"
    );
    let manifest = fs::read_to_string(dir.join("index.panta.md")).expect("manifest");
    assert!(!manifest.contains("**States:**"), "the manifest stays bare: {manifest}");
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("does not load cleanly"),
        "a mixed declared/silent set loads cleanly"
    );
}

#[test]
fn init_here_refuses_to_shadow_an_existing_panta_child_project() {
    // §FS-rhei-init.2: adopting the host must not shadow a default-mode
    // project at panta/ — target resolution prefers the host manifest, so
    // the child project would become unreachable by inference.
    let dir = unique_temp_dir("init-here-shadow");
    fs::create_dir_all(dir.join(".git")).expect("mark repo root");
    let first = rhei_command()
        .arg("init")
        .current_dir(&dir)
        .output()
        .expect("init runs");
    assert!(first.status.success(), "default init should succeed");

    for args in [&["--here"][..], &["--here", "--force"][..]] {
        let output = rhei_command()
            .arg("init")
            .args(args)
            .current_dir(&dir)
            .output()
            .expect("init runs");
        assert!(
            !output.status.success(),
            "--here over an existing panta/ project must refuse ({args:?})"
        );
        let stderr: String = String::from_utf8_lossy(&output.stderr)
            .chars()
            .filter(|ch| *ch != '│' && *ch != '\n')
            .collect();
        let stderr = stderr.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            stderr.contains("would shadow") && stderr.contains("panta"),
            "refusal should explain the shadowing: {stderr}"
        );
    }
    assert!(
        !dir.join("index.panta.md").exists(),
        "the refused adoption must not write a host manifest"
    );
}
