// §AR-source-file-size.3: cross-project `panta` discovery, qualification, and
// prior validation, split from the workspace cases they share fixtures with.

const PANTA_PROFILE_STATE_MACHINE: &str = r#"name: panta-profile-machine
version: 3.0
states:
  pending:
    description: Task not yet started
  completed:
    description: Task finished
    final: true
transitions:
  - from: pending
    to: completed
profiles:
  top-ticket:
    initial: pending
    allowed: [pending, completed]
  nested-ticket:
    initial: completed
    allowed: [completed]
node_policy:
  root: top-ticket
  default: nested-ticket
  overrides:
    - match:
        level: 1
      profile: top-ticket
"#;

const PANTA_LEVEL_TWO_OVERRIDE_MACHINE: &str = r#"name: panta-level-two-machine
version: 3.0
states:
  pending:
    description: Task not yet started
  completed:
    description: Task finished
    final: true
transitions:
  - from: pending
    to: completed
profiles:
  default-ticket:
    initial: pending
    allowed: [pending, completed]
node_policy:
  root: default-ticket
  default: default-ticket
  overrides:
    - match:
        level: 2
      profile: default-ticket
"#;

const PANTA_INPUT_STATE_MACHINE: &str = r#"name: panta-input-machine
version: 1
states:
  pending:
    description: Needs an input from the owning rhei runtime
    initial: true
    inputs:
      - name: brief
        path: runtime/{task_id}.md
  in-progress:
    description: Task currently being worked on
  completed:
    description: Task finished successfully
    final: true
transitions:
  - from: pending
    to: in-progress
  - from: in-progress
    to: completed
"#;

#[test]
fn panta_project_loads_qualifies_and_validates_cross_rhei_priors() {
    let project = create_panta_project(
        "panta-valid",
        "# Panta: Product Suite\n",
        &[
            (
                "auth.rhei.md",
                "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** completed\n",
            ),
            ("billing/index.rhei.md", "# Rhei: Billing\n\n## Notes\nBilling context.\n"),
            (
                "billing/tasks/invoice.md",
                "### Task 1: Invoice\n**State:** pending\n**Prior:** Task auth.1\n",
            ),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let loaded = workspace::load_panta_project(&project).expect("load panta project");
    assert_eq!(loaded.rhei.title, "Product Suite");
    assert_eq!(loaded.rhei_ids, vec!["auth", "billing"]);
    assert!(loaded.task_sources.contains_key("auth.1"));
    assert!(loaded.task_sources.contains_key("billing.1"));
    assert_eq!(loaded.rhei.tasks[0].id.to_string(), "auth.1");
    assert_eq!(loaded.rhei.tasks[1].id.to_string(), "billing.1");
    assert_eq!(loaded.rhei.tasks[1].prior[0].to_string(), "auth.1");

    let output = rhei_command()
        .arg("validate")
        .arg(&project)
        .output()
        .expect("validate command should run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "validate should succeed for panta project\nstdout: {}\nstderr: {}",
        stdout,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("Validation succeeded"));

    let output = rhei_command()
        .arg("list")
        .arg(project.join("index.panta.md"))
        .output()
        .expect("list command should run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "list should succeed for panta manifest path\nstdout: {}\nstderr: {}",
        stdout,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("Task auth.1: Login [completed]"));
    assert!(stdout.contains("Task billing.1: Invoice [pending] (prior: auth.1)"));
}

#[test]
fn panta_discovery_skips_runtime_artifact_trees() {
    let project = create_panta_project(
        "panta-skip-runtime",
        "# Panta: Runtime Artifacts\n",
        &[
            (
                "auth.rhei.md",
                "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n",
            ),
            (
                "runtime/generated.rhei.md",
                "# Rhei: Generated Artifact\n\n## Tasks\n\n### Task 1: Artifact\n**State:** pending\n",
            ),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    // Runtime artifact trees in the project directory are not rhei discovery inputs. §AR-rhei-panta.1
    let loaded = workspace::load_panta_project(&project).expect("load panta project");
    assert_eq!(loaded.rhei_ids, vec!["auth"]);
    assert!(loaded.task_sources.contains_key("auth.1"));
    assert!(!loaded.task_sources.contains_key("generated.1"));
}

#[test]
fn panta_preserves_ambiguous_local_priors_before_cross_rhei_resolution() {
    let project = create_panta_project(
        "panta-local-prior",
        "# Panta: Ambiguous Local Prior\n",
        &[(
            "auth.rhei.md",
            "# Rhei: Auth\n\n## Tasks\n\n### Task auth: Auth root\n**State:** completed\n\n#### Task auth.1: Local setup\n**State:** completed\n\n### Task 2: Depends locally\n**State:** pending\n**Prior:** Task auth.1\n",
        )],
        WORKSPACE_STATE_MACHINE,
    );

    let loaded = workspace::load_panta_project(&project).expect("load panta project");
    assert_eq!(loaded.rhei.tasks[1].id.to_string(), "auth.2");
    assert_eq!(loaded.rhei.tasks[1].prior[0].to_string(), "auth.auth.1");

    let output = rhei_command()
        .arg("validate")
        .arg(&project)
        .output()
        .expect("validate command should run");
    assert!(
        output.status.success(),
        "validate should resolve ambiguous local prior\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn panta_next_peek_resolves_inputs_from_owning_rhei_root() {
    let project = create_panta_project(
        "panta-peek-input-root",
        "# Panta: Peek Inputs\n",
        &[
            ("auth/index.rhei.md", "# Rhei: Auth\n\n"),
            ("auth/tasks/login.md", "### Task 1: Login\n**State:** pending\n"),
        ],
        PANTA_INPUT_STATE_MACHINE,
    );
    let runtime_dir = project.join("auth/runtime");
    fs::create_dir_all(&runtime_dir).expect("create owning rhei runtime");
    fs::write(runtime_dir.join("auth.1.md"), "ready").expect("write input artifact");

    // Panta readiness checks required inputs at the owning rhei root, not the project root. §AR-rhei-panta.5
    let output = rhei_command()
        .arg("next")
        .arg(&project)
        .arg("--peek")
        .arg("--no-callbacks")
        .output()
        .expect("next --peek command should run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "next --peek should resolve inputs from the child rhei root\nstdout: {}\nstderr: {}",
        stdout,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("auth.1"), "peek should report the claimable ticket: {stdout}");
}

/// The run's ready-set scan resolves a member rhei's required `inputs:` against
/// that rhei's own execution root, exactly as `rhei next --peek` above does.
/// `rhei run` read them from the *project* root instead, so a brief written
/// where every other surface says to write it left the run halting with the
/// ticket reported as never scheduled.
// §AR-rhei-panta.5
#[test]
fn panta_run_dry_run_resolves_inputs_from_owning_rhei_root() {
    let project = create_panta_project(
        "panta-run-input-root",
        "# Panta: Run Inputs\n",
        &[
            ("auth/index.rhei.md", "# Rhei: Auth\n\n"),
            ("auth/tasks/login.md", "### Task 1: Login\n**State:** pending\n"),
        ],
        PANTA_INPUT_STATE_MACHINE,
    );
    let owning_root = project.join("auth");
    let owning_input = owning_root.join("runtime").join("auth.1.md");
    fs::create_dir_all(owning_input.parent().expect("owning runtime parent"))
        .expect("create owning rhei runtime");
    fs::write(&owning_input, "brief").expect("write input artifact");

    // Run from inside the project with a relative target, because that is the
    // invocation that would print a relative path if the halt line rendered one.
    // §FS-rhei-run-report.3.1
    let dry_run = || {
        let output = rhei_command()
            .current_dir(&project)
            .args(["run", ".", "--dry-run"])
            .output()
            .expect("run --dry-run should run");
        String::from_utf8_lossy(&output.stdout).into_owned()
    };

    let stdout = dry_run();
    assert!(
        stdout.contains("would transition: Task auth.1"),
        "an input at the owning rhei root should schedule the ticket: {stdout}"
    );

    // The same file at the project root is not the one this ticket needs, and
    // the halt line names the root the scan looked under. §FS-rhei-run-report.3.1
    let project_input = project.join("runtime").join("auth.1.md");
    fs::create_dir_all(project_input.parent().expect("project runtime parent"))
        .expect("create project runtime");
    fs::rename(&owning_input, &project_input).expect("move the input to the project root");

    let stdout = dry_run();
    assert!(
        !stdout.contains("would transition:"),
        "an input at the project root must not schedule a member's ticket: {stdout}"
    );
    let wanted = Path::new(&rendered_path_text(&owning_root))
        .join("runtime")
        .join("auth.1.md")
        .display()
        .to_string();
    assert!(
        stdout.contains("required input(s) not found: brief") && stdout.contains(&wanted),
        "the halt line should name the absolute path it looked under, {wanted}: {stdout}"
    );
}

#[test]
fn panta_validates_task_links_from_owning_rhei_root() {
    let project = create_panta_project(
        "panta-link-root",
        "# Panta: Link Root\n",
        &[
            (
                "auth.rhei.md",
                "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Read spec\n**State:** pending\n\nSee [spec](docs/spec.md).\n",
            ),
            ("docs/spec.md", "Auth spec\n"),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let output = rhei_command()
        .arg("validate")
        .arg(&project)
        .output()
        .expect("validate command should run");
    assert!(
        output.status.success(),
        "validate should resolve task links relative to rhei root\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn panta_validates_child_rhei_content_links() {
    let project = create_panta_project(
        "panta-child-content-link",
        "# Panta: Child Content Links\n",
        &[
            (
                "auth/index.rhei.md",
                "# Rhei: Auth\n\n## Overview\nSee [missing](docs/missing.md).\n",
            ),
            ("auth/tasks/login.md", "### Task 1: Login\n**State:** pending\n"),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let output = rhei_command()
        .arg("validate")
        .arg(&project)
        .output()
        .expect("validate command should run");
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Project validation checks child rhei content links against the child root. §AR-rhei-panta.5
    assert!(!output.status.success(), "validate should reject broken child rhei content link");
    assert!(
        stderr.contains("section 'Rhei auth / Overview'") && stderr.contains("docs/missing.md"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn panta_explicit_max_levels_one_is_not_raised_to_default() {
    let project = create_panta_project(
        "panta-max-levels",
        "# Panta: Max Levels\n\n---\nstructure:\n  maxLevels: 1\n  nodeKinds: [task]\n---\n",
        &[(
            "auth.rhei.md",
            "# Rhei: Auth\n\n---\nstructure:\n  maxLevels: 1\n  nodeKinds: [task]\n---\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n",
        )],
        PANTA_LEVEL_TWO_OVERRIDE_MACHINE,
    );

    let output = rhei_command()
        .arg("validate")
        .arg(&project)
        .output()
        .expect("validate command should run");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "validate should reject level 2 policy override");
    assert!(
        stderr.contains("match.level is 2") && stderr.contains("levels must be in 1..=1"),
        "unexpected stderr: {stderr}"
    );
}

// A rhei restating the project machine is the degenerate override — the same
// machine governs it either way. §FS-rhei-plan-language.1.3
#[test]
fn panta_rhei_may_restate_the_project_state_machine() {
    let project = create_panta_project(
        "panta-rhei-states-match",
        "# Panta: Product Suite\n",
        &[(
            "auth.rhei.md",
            "# Rhei: Auth\n\n## Tasks\n\n\
             ### Task 1: Login\n**State:** pending\n",
        )],
        WORKSPACE_STATE_MACHINE,
    );

    let loaded = workspace::load_panta_project(&project).expect("restating the default loads");
    assert_eq!(loaded.rhei.tasks[0].id.to_string(), "auth.1");
}

#[test]
fn panta_basin_loads_as_reserved_last_rhei() {
    let project = create_panta_project(
        "panta-basin",
        "# Panta: Captures\n",
        &[
            ("auth.rhei.md", "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n"),
            ("basin/loose.md", "### Task 3: Triage later\n**State:** pending\n"),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let loaded = workspace::load_panta_project(&project).expect("load panta project");
    assert_eq!(loaded.rhei_ids, vec!["auth", "basin"]);
    assert_eq!(loaded.rhei.tasks[0].id.to_string(), "auth.1");
    assert_eq!(loaded.rhei.tasks[1].id.to_string(), "basin.3");
    assert!(loaded.task_sources["basin.3"].ends_with("basin/loose.md"));
}

/// §AR-rhei-panta.1: the basin's manifest is synthetic, so an authored index
/// can never load. Skipping it silently vanished unfiled tickets behind a green
/// validate — what the basin exists to prevent (§FS-rhei-panta.4).
#[test]
fn panta_basin_index_file_is_a_load_error_not_a_silent_skip() {
    let project = create_panta_project(
        "panta-basin-index",
        "# Panta: Captures\n",
        &[
            ("auth.rhei.md", "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n"),
            (
                "basin/index.rhei.md",
                "# Rhei: Basin\n\n## Tasks\n\n### Task 3: Triage later\n**State:** pending\n",
            ),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let error = workspace::load_panta_project(&project)
        .expect_err("an authored basin index must fail the load");
    let message = error.message;
    assert!(
        message.contains("basin/index.rhei.md") || message.contains("basin\\index.rhei.md"),
        "the error must name the offending file, got:\n{message}"
    );
    assert!(
        message.contains("the basin has no authored index"),
        "the error must explain why the file cannot load, got:\n{message}"
    );

    let output = rhei_command()
        .arg("validate")
        .arg(&project)
        .output()
        .expect("validate command should run");
    assert!(
        !output.status.success(),
        "validate must not report success while basin tickets are unloadable\nstdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn panta_basin_ignores_runtime_markdown_artifacts() {
    let project = create_panta_project(
        "panta-basin-runtime",
        "# Panta: Captures\n",
        &[
            ("auth.rhei.md", "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n"),
            ("basin/loose.md", "### Task 3: Triage later\n**State:** pending\n"),
            ("basin/runtime/result.md", "# Runtime Result\n\nNot a task file.\n"),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let loaded = workspace::load_panta_project(&project).expect("load panta project");
    assert_eq!(loaded.rhei_ids, vec!["auth", "basin"]);
    assert!(loaded.task_sources.contains_key("basin.3"));
    // Basin runtime artifacts are ignored rather than parsed as basin tasks. §FS-rhei-panta.2
    assert!(!loaded.task_sources.values().any(|path| path.ends_with("basin/runtime/result.md")));

    let output = rhei_command()
        .arg("validate")
        .arg(&project)
        .output()
        .expect("validate command should run");
    assert!(
        output.status.success(),
        "validate should ignore basin runtime markdown\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn panta_rejects_domain_rhei_named_basin() {
    let project = create_panta_project(
        "panta-basin-reserved",
        "# Panta: Captures\n",
        &[(
            "basin.rhei.md",
            "# Rhei: Basin Domain\n\n## Tasks\n\n### Task 1: Invalid\n**State:** pending\n",
        )],
        WORKSPACE_STATE_MACHINE,
    );

    let err = workspace::load_panta_project(&project).expect_err("reserved basin should fail");
    assert!(
        err.message.contains("`basin` is reserved")
            && err.message.contains("basin.rhei.md")
            && err.message.contains("Rename"),
        "error should state the rule, the offending path, and the fix: {}",
        err.message
    );
}

/// §FS-rhei-plan-language.1.3: the `states.yaml` in a member rhei's own root
/// governs it in place of the project default; the two machines govern side by
/// side.
#[test]
fn panta_child_rhei_state_machine_override_loads_and_validates() {
    let project = create_panta_project(
        "panta-child-states",
        "# Panta: Mixed Machines\n",
        &[
            ("auth.rhei.md", "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n"),
            ("payments/index.rhei.md", "# Rhei: Payments\n"),
            ("payments/tasks/one.md", "### Task 1: Charge\n**State:** open\n"),
            ("payments/states.yaml", CHILD_FLOW_STATE_MACHINE),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let loaded = workspace::load_panta_project(&project).expect("mixed machines load");
    assert!(loaded.rhei_roots["payments"].join("states.yaml").is_file());

    // `pending` exists only in the default machine and `open` only in
    // child-flow, so a green validate proves per-ticket dispatch.
    let output = rhei_command()
        .arg("validate")
        .arg(&project)
        .output()
        .expect("validate command should run");
    assert!(
        output.status.success(),
        "mixed machines should validate\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn panta_profile_resolution_uses_rhei_local_task_depth() {
    let project = create_panta_project(
        "panta-profile-depth",
        "# Panta: Profile Depth\n",
        &[("auth.rhei.md", "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n")],
        PANTA_PROFILE_STATE_MACHINE,
    );

    let output = rhei_command()
        .arg("validate")
        .arg(&project)
        .output()
        .expect("validate command should run");
    assert!(
        output.status.success(),
        "top-level ticket should resolve as level 1 despite project-qualified id\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn panta_transition_routes_rewrite_to_owning_rhei_file() {
    let project = create_panta_project(
        "panta-mutate",
        "# Panta: Mutable\n",
        &[("auth.rhei.md", "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n")],
        WORKSPACE_STATE_MACHINE,
    );

    // Project-scoped mutation targets the qualified ticket id and rewrites the
    // owning rhei file with its rhei-local heading. §FS-rhei-panta.6.1
    let output = rhei_command()
        .arg("transition")
        .arg(&project)
        .arg("--task")
        .arg("auth.1")
        .arg("--from")
        .arg("pending")
        .arg("--to")
        .arg("in-progress")
        .arg("--no-callbacks")
        .output()
        .expect("transition command should run");
    assert!(
        output.status.success(),
        "transition should succeed for Panta projects\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let rewritten = fs::read_to_string(project.join("auth.rhei.md")).expect("read child rhei file");
    assert!(
        rewritten.contains("### Task 1: Login\n**State:** in-progress"),
        "child rhei file should carry the new state under its local heading: {rewritten}"
    );

    // The transition ledger lands in the owning rhei's runtime, keyed by the
    // project-qualified id. §AR-rhei-panta.2
    let ledger = fs::read_to_string(project.join("runtime/state-transitions.log"))
        .expect("read transition ledger");
    assert!(
        ledger.contains("auth.1 pending@in-progress"),
        "ledger should record the qualified ticket id: {ledger}"
    );
}

#[test]
fn panta_next_peek_reads_and_claim_writes_owning_rhei() {
    let project = create_panta_project(
        "panta-next-peek",
        "# Panta: Peek\n",
        &[("auth.rhei.md", "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n")],
        WORKSPACE_STATE_MACHINE,
    );

    // `--peek` does not mutate child rhei files, so it works project-wide. §FS-rhei-panta.6.1
    let output = rhei_command()
        .arg("next")
        .arg(&project)
        .arg("--peek")
        .arg("--no-callbacks")
        .output()
        .expect("next --peek command should run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "next --peek should succeed for Panta projects\nstdout: {}\nstderr: {}",
        stdout,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("auth.1"), "peek should report the claimable ticket: {stdout}");

    // Claim mode writes `**Assignee:**` into the owning rhei's file, resolved
    // through the source map. §FS-rhei-panta.6.1
    let output = rhei_command()
        .arg("next")
        .arg(&project)
        .arg("--no-callbacks")
        .output()
        .expect("next claim command should run");
    assert!(
        output.status.success(),
        "next claim should succeed for Panta projects\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let rewritten = fs::read_to_string(project.join("auth.rhei.md")).expect("read child rhei file");
    assert!(
        rewritten.contains("**Assignee:**"),
        "claim should write the assignee into the owning rhei file: {rewritten}"
    );
}

#[test]
fn panta_rhei_narrowing_scopes_candidates_and_spares_other_rhei_runtime() {
    let project = create_panta_project(
        "panta-narrow",
        "# Panta: Narrow\n",
        &[
            ("auth.rhei.md", "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n"),
            (
                "billing.rhei.md",
                "# Rhei: Billing\n\n## Tasks\n\n### Task 1: Invoice\n**State:** pending\n",
            ),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    // An unknown rhei is rejected and names what is available. §FS-rhei-panta.6
    let output = rhei_command()
        .arg("list")
        .arg(&project)
        .arg("--rhei")
        .arg("nope")
        .output()
        .expect("list runs");
    assert!(!output.status.success(), "unknown rhei must fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown rhei 'nope'") && stderr.contains("auth, billing"),
        "error should name available rheis: {stderr}"
    );

    // `--rhei` narrows the listing to the named rhei.
    let output = rhei_command()
        .arg("list")
        .arg(&project)
        .arg("--rhei")
        .arg("auth")
        .output()
        .expect("list runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("auth.1"), "auth ticket should be listed: {stdout}");
    assert!(!stdout.contains("billing.1"), "billing ticket should be filtered out: {stdout}");

    // Complete one ticket in each rhei so both own runtime artifacts. The
    // fixture machine reaches a terminal state via `in-progress`.
    for task in ["auth.1", "billing.1"] {
        let output = rhei_command()
            .arg("transition")
            .arg(&project)
            .args(["--task", task, "--from", "pending", "--to", "in-progress", "--no-callbacks"])
            .output()
            .expect("transition runs");
        assert!(output.status.success(), "transition {task} should succeed");
        let output = rhei_command()
            .arg("complete")
            .arg(&project)
            .args(["--task", task, "--result", "done", "--no-callbacks"])
            .output()
            .expect("complete runs");
        assert!(
            output.status.success(),
            "complete {task} should succeed\nstderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // A narrowed reset must not destroy the other rhei's runtime state — these
    // sibling single-file rheis share one execution root. §FS-rhei-panta.6.4
    let output = rhei_command()
        .arg("reset")
        .arg("-y")
        .arg(&project)
        .arg("--rhei")
        .arg("auth")
        .output()
        .expect("reset runs");
    assert!(
        output.status.success(),
        "narrowed reset should succeed\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("narrowed to 1 rhei: auth"),
        "reset should report the narrowed scope"
    );

    assert!(
        !project.join("runtime/results/auth.1.md").exists(),
        "in-scope result artifact should be removed"
    );
    assert!(
        project.join("runtime/results/billing.1.md").exists(),
        "out-of-scope rhei must keep its result artifact"
    );
    let billing = fs::read_to_string(project.join("billing.rhei.md")).expect("read billing");
    assert!(billing.contains("**State:** completed"), "billing must stay completed: {billing}");
    let auth = fs::read_to_string(project.join("auth.rhei.md")).expect("read auth");
    assert!(auth.contains("**State:** pending"), "auth must be reset: {auth}");

    // A reset ticket's ledger history goes with it, so the recorded history
    // cannot claim a completion the plan no longer holds. §FS-rhei-panta.6.4
    let ledger = fs::read_to_string(project.join("runtime/state-transitions.log"))
        .expect("read transition ledger");
    assert!(!ledger.contains("auth.1 "), "reset ticket's ledger lines should be pruned: {ledger}");
    assert!(
        ledger.contains("billing.1 "),
        "out-of-scope ticket must keep its ledger lines: {ledger}"
    );

    // The operator is told what a narrowed reset cannot speak for, rather than
    // discovering a silent partial reset later. §FS-rhei-panta.6.4
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("Kept run-scoped output"),
        "narrowed reset should name what it kept"
    );
}

/// A narrowed reset removes every artifact keyed by an in-scope ticket —
/// including declared artifact contracts, whose stale outputs would satisfy a
/// required input on the next run — and nothing else. §FS-rhei-reset.2.1
#[test]
fn panta_narrowed_reset_clears_ticket_owned_artifacts_without_touching_siblings() {
    let project = create_panta_project(
        "panta-narrow-artifacts",
        "# Panta: Narrow Artifacts\n",
        &[
            ("auth.rhei.md", "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n"),
            (
                "billing.rhei.md",
                "# Rhei: Billing\n\n## Tasks\n\n### Task 1: Invoice\n**State:** pending\n",
            ),
        ],
        ARTIFACT_CONTRACT_STATE_MACHINE,
    );

    // Runtime artifacts as the run surfaces write them. `auth.10` guards the
    // prefix match: it must survive a reset narrowed to `auth.1`'s rhei only
    // because it belongs to no in-scope ticket id.
    let runtime = project.join("runtime");
    for dir in
        ["logs", "results", "worktree-refs", "accounting/tasks", "accounting/captures", "notes"]
    {
        fs::create_dir_all(runtime.join(dir)).expect("create runtime dir");
    }
    fs::create_dir_all(runtime.join("snapshot-sessions/auth.1-pending-slug-7"))
        .expect("create snapshot session");
    for file in [
        "logs/task-auth.1-pending.log",
        "logs/task-auth.10-pending.log",
        "logs/task-billing.1-pending.log",
        "results/auth.1.md",
        "results/billing.1.md",
        "worktree-refs/auth.1.yaml",
        "accounting/tasks/auth.1.json",
        "accounting/captures/auth.1-pending-1.json",
        "accounting/captures/billing.1-pending-1.json",
        "notes/auth.1.md",
        "notes/billing.1.md",
    ] {
        fs::write(runtime.join(file), "x").expect("seed runtime artifact");
    }

    let output = rhei_command()
        .arg("reset")
        .arg("-y")
        .arg(&project)
        .args(["--rhei", "auth"])
        .output()
        .expect("reset runs");
    assert!(
        output.status.success(),
        "narrowed reset should succeed\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    for gone in [
        "logs/task-auth.1-pending.log",
        "results/auth.1.md",
        "worktree-refs/auth.1.yaml",
        "accounting/tasks/auth.1.json",
        "accounting/captures/auth.1-pending-1.json",
        "notes/auth.1.md",
        "snapshot-sessions/auth.1-pending-slug-7",
    ] {
        assert!(!runtime.join(gone).exists(), "{gone} is owned by auth.1 and should be removed");
    }
    for kept in [
        "logs/task-auth.10-pending.log",
        "logs/task-billing.1-pending.log",
        "results/billing.1.md",
        "accounting/captures/billing.1-pending-1.json",
        "notes/billing.1.md",
    ] {
        assert!(runtime.join(kept).exists(), "{kept} is not owned by an in-scope ticket");
    }
}

/// `--rhei` narrows candidates but never prior resolution, so the diagnostic
/// must name the out-of-scope prior rather than report the out-of-scope ticket
/// as work in progress. §FS-rhei-panta.6.1
#[test]
fn panta_narrowed_next_explains_a_prior_outside_the_scope() {
    let project = create_panta_project(
        "panta-narrow-blocked",
        "# Panta: Narrow Blocked\n",
        &[
            ("auth.rhei.md", "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n"),
            (
                "billing.rhei.md",
                "# Rhei: Billing\n\n## Tasks\n\n### Task 1: Invoice\n**State:** pending\n\
                 **Prior:** Task auth.1\n",
            ),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let output = rhei_command()
        .arg("next")
        .arg(&project)
        .args(["--rhei", "billing", "--no-callbacks"])
        .output()
        .expect("next runs");
    assert!(!output.status.success(), "the only candidate is blocked");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--rhei scope (billing)")
            && stderr.contains("billing.1")
            && stderr.contains("outside the --rhei scope"),
        "diagnostic should name the scope and the blocking prior outside it: {stderr}"
    );
}

#[test]
fn panta_run_rhei_narrowing_skips_out_of_scope_work_in_agent_mode() {
    let project = create_panta_project(
        "panta-run-narrow",
        "# Panta: Narrow Run\n",
        &[
            ("auth.rhei.md", "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n"),
            (
                "billing.rhei.md",
                "# Rhei: Billing\n\n## Tasks\n\n### Task 1: Invoice\n**State:** pending\n",
            ),
        ],
        AGENT_WORK_STATE_MACHINE,
    );

    // The fake agent records every ticket it is spawned for.
    let script = write_python_agent(
        &project,
        "fake-agent.py",
        // §FS-rhei-states.3.3: `pending -> completed` is terminal, so the agent
        // writes the ticket's result before it exits.
        r#"append(
    pathlib.Path(env('RHEI_ROOT')) / 'agent-invocations.txt',
    env('RHEI_TASK_ID') + '\n',
)
result('## Result\n\nDone.\n')
"#,
    );
    write_run_agent_settings(
        &project,
        &format!(
            r#"{{ "agents": {{ "fake": {{ "command": {}, "stdin_prompt": true, "timeout": "5s" }} }} }}"#,
            fixture_command(&script)
        ),
    );

    // §FS-rhei-run.2.5: the sequential agent loop schedules in-scope work only.
    let output = rhei_command()
        .arg("run")
        .arg(&project)
        .args(["--rhei", "auth", "--no-callbacks"])
        .output()
        .expect("run runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // §FS-rhei-panta.6.1: out-of-scope tickets left non-terminal are not a
    // run failure for a narrowed invocation.
    assert!(
        output.status.success(),
        "narrowed run should succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("narrowed to") && stdout.contains("auth"),
        "run should report its narrowed scope: {stdout}"
    );

    let invocations =
        fs::read_to_string(project.join("agent-invocations.txt")).expect("agent ran for auth");
    assert_eq!(invocations, "auth.1\n", "only the in-scope ticket may spawn an agent");

    let auth = fs::read_to_string(project.join("auth.rhei.md")).expect("read auth");
    assert!(auth.contains("**State:** completed"), "in-scope ticket should finish: {auth}");
    let billing = fs::read_to_string(project.join("billing.rhei.md")).expect("read billing");
    assert!(
        billing.contains("**State:** pending"),
        "out-of-scope ticket must stay untouched: {billing}"
    );
}

#[test]
fn panta_narrowed_reset_clears_workspace_index_metadata_and_legacy_records() {
    // §FS-rhei-panta.6.4: the owning index's runtime ticket metadata is
    // ticket-owned state, and legacy rhei-local records are swept at a root
    // whose every rhei is in scope.
    let project = create_panta_project(
        "panta-narrow-metadata",
        "# Panta: Narrow Metadata\n",
        &[
            (
                "auth/index.rhei.md",
                "# Rhei: Auth\n\n---\nmetadata:\n  tasks:\n    1:\n      stateVisits:\n        in-progress: 2\n---\n\n## Overview\nAuth.\n",
            ),
            ("auth/tasks/one.md", "### Task 1: Login\n**State:** in-progress\n"),
            (
                "billing.rhei.md",
                "# Rhei: Billing\n\n## Tasks\n\n### Task 1: Invoice\n**State:** pending\n",
            ),
        ],
        WORKSPACE_STATE_MACHINE,
    );
    // Legacy pre-qualification runtime state under the workspace rhei's own
    // root, keyed by the rhei-local id.
    let auth_runtime = project.join("auth/runtime");
    fs::create_dir_all(auth_runtime.join("results")).expect("mkdir results");
    fs::write(auth_runtime.join("results/1.md"), "## Result\n\nlegacy\n").expect("seed result");
    fs::write(auth_runtime.join("state-transitions.log"), "1 pending@in-progress\n")
        .expect("seed ledger");
    // A legacy record at the *shared* project root must survive: `billing`
    // also roots there and is out of scope, so a bare local id is ambiguous.
    let project_runtime = project.join("runtime");
    fs::create_dir_all(project_runtime.join("results")).expect("mkdir project results");
    fs::write(project_runtime.join("results/1.md"), "## Result\n\nambiguous\n")
        .expect("seed shared result");

    let output = rhei_command()
        .arg("reset")
        .arg("-y")
        .arg(&project)
        .args(["--rhei", "auth"])
        .output()
        .expect("reset runs");
    assert!(
        output.status.success(),
        "narrowed reset should succeed\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let index = fs::read_to_string(project.join("auth/index.rhei.md")).expect("read index");
    assert!(
        !index.contains("stateVisits"),
        "the in-scope workspace index must lose its runtime visit counters: {index}"
    );
    assert!(
        !auth_runtime.join("results/1.md").exists(),
        "the legacy local-id result at the rhei's own root should be swept"
    );
    assert!(
        !auth_runtime.join("state-transitions.log").exists(),
        "the legacy local-id ledger line should be pruned (file emptied)"
    );
    assert!(
        project_runtime.join("results/1.md").exists(),
        "a local-id record at the shared root is ambiguous and must survive"
    );
}

#[test]
fn panta_run_locks_every_member_rhei_execution_root() {
    // §FS-rhei-run.2.6: a project-level run locks the project root *and*
    // each member rhei's execution root, so a direct `rhei run <member>` and
    // the project run contend on the same lock.
    let project = create_panta_project(
        "panta-run-locks",
        "# Panta: Run Locks\n",
        &[
            ("auth/index.rhei.md", "# Rhei: Auth\n"),
            ("auth/tasks/one.md", "### Task 1: Login\n**State:** pending\n"),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let output = rhei_command()
        .arg("run")
        .arg(&project)
        .arg("--no-callbacks")
        .output()
        .expect("run runs");
    assert!(
        output.status.success(),
        "project run should succeed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(project.join(".rhei/run.lock").is_file(), "the project root must be locked");
    assert!(
        project.join("auth/.rhei/run.lock").is_file(),
        "the member workspace rhei's root must be locked too"
    );
}
