// §AR-source-file-size.3: which plan a bare command lands on when the target is
// omitted, and what the commands do once it has landed. State-machine
// resolution is a sibling; fixtures live in `common.rs`.

#[test]
fn omitted_plan_target_resolves_from_current_directory() {
    // §FS-rhei-panta.6: invoked inside a project, a command with no target
    // operates on the whole project.
    let project = create_panta_project(
        "panta-cwd-resolve",
        "# Panta: Cwd\n",
        &[
            ("auth.rhei.md", "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n"),
            (
                "billing.rhei.md",
                "# Rhei: Billing\n\n## Tasks\n\n### Task 1: Invoice\n**State:** pending\n",
            ),
        ],
        WORKSPACE_STATE_MACHINE,
    );
    let output = rhei_command()
        .arg("list")
        .current_dir(&project)
        .output()
        .expect("list runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("auth.1") && stdout.contains("billing.1"),
        "bare `rhei list` inside a project should list it\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Nested inside the project (a subdirectory with no plan of its own), the
    // upward walk still finds the project.
    let nested = project.join("notes");
    fs::create_dir_all(&nested).expect("mkdir nested");
    let output = rhei_command()
        .arg("list")
        .current_dir(&nested)
        .output()
        .expect("list runs");
    assert!(
        output.status.success() && String::from_utf8_lossy(&output.stdout).contains("auth.1"),
        "the upward walk should find the enclosing project"
    );

    // A lone `.rhei.md` in the directory resolves to that rhei.
    let dir = unique_temp_dir("cwd-lone-rhei");
    fs::write(
        dir.join("plan.rhei.md"),
        "# Rhei: Lone\n\n## Tasks\n\n### Task 1: Alpha\n**State:** pending\n",
    )
    .expect("write plan");
    let output = rhei_command()
        .arg("list")
        .current_dir(&dir)
        .output()
        .expect("list runs");
    assert!(
        output.status.success() && String::from_utf8_lossy(&output.stdout).contains("plan.1"),
        "a lone rhei file should resolve"
    );

    // Several bare rheis with no manifest are ambiguous, and the error names
    // both fixes.
    fs::write(
        dir.join("second.rhei.md"),
        "# Rhei: Second\n\n## Tasks\n\n### Task 1: Beta\n**State:** pending\n",
    )
    .expect("write second plan");
    let output = rhei_command()
        .arg("list")
        .current_dir(&dir)
        .output()
        .expect("list runs");
    assert!(!output.status.success(), "ambiguous directory must fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("plan.rhei.md")
            && stderr.contains("second.rhei.md")
            && stderr.contains("index.panta.md"),
        "ambiguity error should name the candidates and the project fix: {stderr}"
    );
}

#[test]
fn omitted_plan_target_resolves_conventional_panta_child() {
    // §FS-rhei-panta.6: the `panta/` child rhei init creates resolves from
    // the host directory and anywhere under it.
    let host = unique_temp_dir("panta-child-resolve");
    let project = host.join("panta");
    fs::create_dir_all(&project).expect("mkdir panta");
    fs::write(project.join("index.panta.md"), "# Panta: Child\n").expect("write manifest");
    fs::write(
        project.join("auth.rhei.md"),
        "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n",
    )
    .expect("write auth");
    let nested = host.join("src/deep");
    fs::create_dir_all(&nested).expect("mkdir nested");

    for cwd in [host.to_path_buf(), nested.clone()] {
        let output = rhei_command()
            .arg("list")
            .current_dir(&cwd)
            .output()
            .expect("list runs");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success() && stdout.contains("auth.1"),
            "bare list from {} should resolve the panta/ child: {stdout}\nstderr: {}",
            cwd.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn empty_project_is_valid_and_list_exits_successfully() {
    let host = unique_temp_dir("empty-project-ok");
    assert!(
        rhei_command()
            .arg("init")
            .arg("--no-agents")
            .current_dir(&host)
            .output()
            .expect("init runs")
            .status
            .success(),
        "init should succeed"
    );

    // §FS-rhei-panta.6: a just-initialized project lists successfully and
    // says how to grow, instead of erroring.
    let output = rhei_command()
        .arg("list")
        .current_dir(&host)
        .output()
        .expect("list runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "empty project must not be a list error\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("no tickets yet") && stdout.contains("index.panta.md"),
        "empty listing should say how to grow the project: {stdout}"
    );

    // Machine consumers get an empty array, not prose.
    let output = rhei_command()
        .arg("list")
        .arg("--json")
        .current_dir(&host)
        .output()
        .expect("list --json runs");
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "[]");
}

#[test]
fn empty_project_validate_warns_that_discovery_found_nothing() {
    let host = unique_temp_dir("empty-project-validate");
    fs::write(host.join("index.panta.md"), "# Panta: Empty\n").expect("write manifest");
    // A plan missing the `.rhei.md` suffix is invisible to discovery; validate
    // must not report the project green without saying so. §FS-rhei-panta.6
    fs::write(
        host.join("auth.md"),
        "# Rhei: Auth\n\n## Tasks\n\n### Task 1: A\n**State:** pending\n",
    )
    .expect("write misnamed plan");

    let output = rhei_command()
        .arg("validate")
        .current_dir(&host)
        .output()
        .expect("validate runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "an empty project validates successfully\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("holds no rheis") && stdout.contains("*.rhei.md"),
        "validate should warn that discovery found nothing: {stdout}"
    );
}

#[test]
fn reset_never_infers_an_omitted_target() {
    // §FS-rhei-panta.6: reset destroys runtime state, so it is excluded from
    // omitted-target resolution even inside a resolvable project.
    let project = create_panta_project(
        "reset-explicit-target",
        "# Panta: Reset\n",
        &[(
            "auth.rhei.md",
            "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** in-progress\n",
        )],
        WORKSPACE_STATE_MACHINE,
    );
    let output = rhei_command()
        .arg("reset")
        .current_dir(&project)
        .output()
        .expect("reset runs");
    assert!(!output.status.success(), "bare `rhei reset` must refuse");
    let stderr: String = String::from_utf8_lossy(&output.stderr)
        .chars()
        .filter(|ch| *ch != '│' && *ch != '\n')
        .collect();
    let stderr = stderr.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        stderr.contains("never infers its target"),
        "the refusal should say why and how: {stderr}"
    );
    let plan = fs::read_to_string(project.join("auth.rhei.md")).expect("plan intact");
    assert!(
        plan.contains("**State:** in-progress"),
        "a refused reset must not touch ticket states: {plan}"
    );

    // The explicit form still works. The ledger records where Task 1 started,
    // which is what reset returns it to. §FS-rhei-reset.2.2
    let runtime = project.join("runtime");
    fs::create_dir_all(&runtime).expect("create runtime dir");
    fs::write(runtime.join("state-transitions.log"), "auth.1 pending@in-progress\n")
        .expect("write ledger");
    let output = rhei_command()
        .arg("reset")
        .arg("-y")
        .arg(&project)
        .output()
        .expect("reset runs");
    assert!(
        output.status.success(),
        "explicit reset should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan = fs::read_to_string(project.join("auth.rhei.md")).expect("plan reset");
    assert!(plan.contains("**State:** pending"), "explicit reset applies: {plan}");
}

#[test]
fn omitted_target_counts_workspace_rheis_and_skips_dotfiles() {
    // A bare file next to a workspace directory is ambiguous — the walk counts
    // rheis exactly as project discovery does. §FS-rhei-panta.6
    let dir = unique_temp_dir("cwd-workspace-ambiguity");
    fs::write(
        dir.join("auth.rhei.md"),
        "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n",
    )
    .expect("write auth");
    let ws = dir.join("billing");
    fs::create_dir_all(ws.join("tasks")).expect("mkdir workspace");
    fs::write(ws.join("index.rhei.md"), "# Rhei: Billing\n").expect("write index");
    fs::write(ws.join("tasks/one.md"), "### Task 1: Invoice\n**State:** pending\n")
        .expect("write task");

    let output = rhei_command()
        .arg("list")
        .current_dir(&dir)
        .output()
        .expect("list runs");
    assert!(!output.status.success(), "file + workspace rhei must be ambiguous");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("auth.rhei.md") && stderr.contains("billing"),
        "ambiguity error should name both rheis: {stderr}"
    );

    // A lone workspace directory resolves; a hidden dotfile is not a rhei and
    // neither creates ambiguity nor resolves. §FS-rhei-panta.6
    let dir = unique_temp_dir("cwd-workspace-lone");
    let ws = dir.join("billing");
    fs::create_dir_all(ws.join("tasks")).expect("mkdir workspace");
    fs::write(ws.join("index.rhei.md"), "# Rhei: Billing\n").expect("write index");
    fs::write(ws.join("tasks/one.md"), "### Task 1: Invoice\n**State:** pending\n")
        .expect("write task");
    fs::write(dir.join("._junk.rhei.md"), "AppleDouble metadata, not markdown")
        .expect("write dotfile");

    let output = rhei_command()
        .arg("list")
        .current_dir(&dir)
        .output()
        .expect("list runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("billing.1"),
        "the workspace rhei should resolve despite the dotfile\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn omitted_target_never_adopts_a_loose_plan_from_an_ancestor() {
    // §FS-rhei-panta.6: a loose plan resolves in the invocation directory
    // only; ancestors are adopted solely through explicit manifests.
    let parent = unique_temp_dir("ancestor-loose-plan");
    fs::write(
        parent.join("notes.rhei.md"),
        "# Rhei: Notes\n\n## Tasks\n\n### Task 1: Idea\n**State:** pending\n",
    )
    .expect("write stray plan");
    let nested = parent.join("some/unrelated");
    fs::create_dir_all(&nested).expect("mkdir nested");

    let output = rhei_command()
        .arg("list")
        .current_dir(&nested)
        .output()
        .expect("list runs");
    assert!(!output.status.success(), "the stray ancestor plan must not resolve");
    let stderr: String = String::from_utf8_lossy(&output.stderr)
        .chars()
        .filter(|ch| *ch != '│' && *ch != '\n')
        .collect();
    let stderr = stderr.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        stderr.contains("no Rhei plan found"),
        "the error should say nothing was found: {stderr}"
    );

    fs::remove_dir_all(parent).expect("cleanup");
}

#[test]
fn empty_project_reset_is_a_noop_success() {
    // §FS-rhei-panta.6: an empty project — exactly what `rhei init` creates —
    // is a valid state for every command; reset has nothing to rewrite and
    // reports a no-op instead of failing on the project directory.
    let dir = unique_temp_dir("panta-empty-reset");
    fs::write(dir.join("index.panta.md"), "# Panta: Empty\n").expect("write manifest");

    let output = rhei_command()
        .arg("reset")
        .arg("-y")
        .arg(&dir)
        .output()
        .expect("reset runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "reset on an empty project should succeed\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("Reset 0 task(s)"), "reset should report a no-op: {stdout}");
}

/// Basin tickets keep runtime metadata in the project manifest, so commands
/// that advance a ticket can read and write it. Parsing their bare task files
/// as whole plans made the whole basin unworkable. §FS-rhei-panta.6.1
#[test]
fn basin_tickets_transition_and_complete() {
    let project = create_panta_project(
        "panta-basin-transition",
        "# Panta: Basin Work\n",
        &[
            ("auth.rhei.md", "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n"),
            ("basin/quick.md", "### Task 1: Fix typo\n**State:** pending\n"),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let transition = rhei_command()
        .arg("transition")
        .arg(&project)
        .args(["--task", "basin.1", "--from", "pending", "--to", "in-progress"])
        .output()
        .expect("transition command should run");
    assert!(
        transition.status.success(),
        "basin ticket should transition\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&transition.stdout),
        String::from_utf8_lossy(&transition.stderr)
    );

    let complete = rhei_command()
        .arg("complete")
        .arg(&project)
        .args(["--task", "basin.1", "--result", "done"])
        .output()
        .expect("complete command should run");
    assert!(
        complete.status.success(),
        "basin ticket should complete\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&complete.stdout),
        String::from_utf8_lossy(&complete.stderr)
    );

    let listed = rhei_command()
        .arg("list")
        .arg(&project)
        .args(["--rhei", "basin"])
        .output()
        .expect("list command should run");
    let stdout = String::from_utf8_lossy(&listed.stdout);
    assert!(
        stdout.contains("Task basin.1: Fix typo [completed]"),
        "basin ticket should be completed, got:\n{stdout}"
    );
}

/// §FS-rhei-reset.1.2: `--dry-run` reports the damage and changes nothing.
#[test]
fn reset_dry_run_changes_nothing() {
    let project = create_panta_project(
        "panta-reset-dry-run",
        "# Panta: Reset\n",
        &[(
            "auth.rhei.md",
            "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** completed\n",
        )],
        WORKSPACE_STATE_MACHINE,
    );
    let plan = project.join("auth.rhei.md");
    let before = fs::read_to_string(&plan).expect("read plan before reset");

    let output = rhei_command()
        .arg("reset")
        .arg(&project)
        .arg("--dry-run")
        .output()
        .expect("reset command should run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "dry run should succeed\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("Would reset") && stdout.contains("Dry run"),
        "dry run should preview the reset, got:\n{stdout}"
    );
    assert_eq!(
        fs::read_to_string(&plan).expect("read plan after dry run"),
        before,
        "dry run must not rewrite the plan"
    );
}
