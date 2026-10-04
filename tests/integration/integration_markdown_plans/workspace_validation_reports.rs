// §AR-source-file-size.3: what a healthy project *renders* — list depth, graph,
// scope line, and grouped output. The errors it reports instead are a sibling;
// fixtures live in `common.rs`.

#[test]
fn list_indents_and_reports_depth_rhei_locally_despite_qualified_ids() {
    let dir = unique_temp_dir("list-depth");
    fs::write(
        dir.join("plan.rhei.md"),
        "# Rhei: Depth\n\n## Tasks\n\n### Task 1: Parent\n**State:** pending\n\n#### Task 1.1: Child\n**State:** pending\n",
    )
    .expect("write plan");

    // §FS-rhei-list.4.1: top-level tickets are flush-left; the qualification
    // segment adds no indentation.
    let output = rhei_command()
        .arg("list")
        .arg(dir.join("plan.rhei.md"))
        .output()
        .expect("list runs");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.lines().any(|line| line.starts_with("Task plan.1: Parent")),
        "top-level ticket must be flush-left: {stdout}"
    );
    assert!(
        stdout.lines().any(|line| line.starts_with("  Task plan.1.1: Child")),
        "child ticket must be indented one level: {stdout}"
    );

    // §FS-rhei-list.4.2: `depth` is 1-based within the owning rhei.
    let output = rhei_command()
        .arg("list")
        .arg(dir.join("plan.rhei.md"))
        .arg("--json")
        .output()
        .expect("list --json runs");
    assert!(output.status.success());
    let payload: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json payload");
    let depths: Vec<(String, u64)> = payload
        .as_array()
        .expect("array")
        .iter()
        .map(|task| {
            (task["id"].as_str().expect("id").to_string(), task["depth"].as_u64().expect("depth"))
        })
        .collect();
    assert_eq!(
        depths,
        vec![("plan.1".to_string(), 1), ("plan.1.1".to_string(), 2)],
        "depth must not count the qualification segment"
    );
}

/// §FS-rhei-viz.7.3: a project renders as one merged graph — every rhei's
/// tickets in one plan, cross-rhei edges intact — and a member rhei renders that
/// graph narrowed to itself, keeping the far end of its cross-rhei priors.
#[test]
fn viz_renders_a_panta_project_as_one_graph_and_narrows_to_a_member() {
    let project = create_panta_project(
        "panta-viz-merged",
        "# Panta: Viz\n",
        &[
            ("auth.rhei.md", "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n"),
            (
                "billing.rhei.md",
                "# Rhei: Billing\n\n## Tasks\n\n### Task 1: Invoice\n**State:** pending\n\
                 **Prior:** auth.1\n",
            ),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let render = |target: &Path, out_name: &str| -> String {
        let out_file = project.join(out_name);
        let output = rhei_command()
            .arg("viz")
            .arg(target)
            .arg("--output")
            .arg(&out_file)
            .output()
            .expect("viz runs");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "viz should render\nstderr: {stderr}");
        assert!(
            !stderr.contains("not Panta-aware"),
            "a project renders as one graph, so nothing is caveated: {stderr}"
        );
        fs::read_to_string(&out_file).expect("read viz page")
    };

    // One page, one plan bundle: both rheis' tickets, and the cross-rhei edge.
    let whole = render(&project, "viz.html");
    assert!(whole.contains("auth.1"), "the project graph holds auth's ticket");
    assert!(whole.contains("billing.1"), "the project graph holds billing's ticket");

    // Narrowed to `billing`: its own ticket, plus the auth ticket its prior
    // points at, so the dependency is scoped rather than erased.
    let narrowed = render(&project.join("billing.rhei.md"), "viz-billing.html");
    assert!(narrowed.contains("billing.1"), "the named rhei's ticket stays");
    assert!(narrowed.contains("auth.1"), "the far end of a cross-rhei prior stays");
}

#[test]
fn scope_report_prints_project_wide_line_and_stays_quiet_for_bare_rhei() {
    let project = create_panta_project(
        "panta-scope-line",
        "# Panta: Scope\n",
        &[
            ("auth.rhei.md", "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n"),
            (
                "billing.rhei.md",
                "# Rhei: Billing\n\n## Tasks\n\n### Task 1: Invoice\n**State:** pending\n",
            ),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    // §FS-rhei-panta.6: an un-narrowed project-scoped reset announces the
    // rheis it will touch before acting.
    let output = rhei_command()
        .arg("reset")
        .arg("-y")
        .arg(&project)
        .output()
        .expect("reset runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "reset should succeed: {stdout}");
    assert!(
        stdout.contains("Scope: `rhei reset` operates project-wide across 2 rheis: auth, billing"),
        "project-wide reset should report its scope: {stdout}"
    );

    // §FS-rhei-panta.6.2: a bare rhei is a one-rhei implicit Panta with no
    // fan-out to report — no scope line.
    let dir = unique_temp_dir("bare-scope-quiet");
    fs::write(
        dir.join("plan.rhei.md"),
        "# Rhei: Quiet\n\n## Tasks\n\n### Task 1: Alpha\n**State:** pending\n",
    )
    .expect("write plan");
    let output = rhei_command()
        .arg("reset")
        .arg("-y")
        .arg(dir.join("plan.rhei.md"))
        .output()
        .expect("reset runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "reset should succeed: {stdout}");
    assert!(!stdout.contains("Scope:"), "one-rhei project must stay quiet: {stdout}");
}

/// §FS-rhei-render.3.4: a merged project renders rhei by rhei, not as one flat
/// task list under a run of headings, and the progress format leads with the
/// completion summary it is named for.
#[test]
fn project_render_groups_tickets_under_their_rhei() {
    let project = create_panta_project(
        "panta-render-groups",
        "# Panta: Store\n",
        &[
            (
                "auth.rhei.md",
                "# Rhei: Authentication\n\n## Overview\n\nWho gets in.\n\n## Tasks\n\n\
                 ### Task 1: Login\n**State:** completed\n",
            ),
            (
                "billing.rhei.md",
                "# Rhei: Billing\n\n## Tasks\n\n### Task 1: Invoice\n**State:** pending\n",
            ),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let render = |format: &str| -> String {
        let output = rhei_command()
            .arg("render")
            .arg(&project)
            .arg("--format")
            .arg(format)
            .arg("--no-color")
            .output()
            .expect("render runs");
        assert!(
            output.status.success(),
            "render should succeed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    };

    let github = render("github");
    // §FS-rhei-render.3.4: the heading carries the rhei id when it differs
    // from the title (`Authentication (auth)`); `Billing` already names its
    // id and stays bare.
    let auth_heading = github.find("## Authentication (auth)").expect("auth heading");
    let billing_heading = github.find("## Billing\n").expect("billing heading");
    let auth_ticket = github.find("auth.1").expect("auth ticket");
    let billing_ticket = github.find("billing.1").expect("billing ticket");
    assert!(
        auth_heading < auth_ticket && auth_ticket < billing_heading,
        "each rhei's tickets belong under its own heading:\n{github}"
    );
    assert!(billing_heading < billing_ticket, "billing's ticket follows its heading:\n{github}");
    assert!(
        !github.contains("Rhei auth / Overview"),
        "the merge prefix is not part of the document:\n{github}"
    );
    assert!(!github.contains("\n\n\n"), "no runs of blank lines:\n{github:?}");

    let progress = render("progress");
    assert!(
        progress.contains("1/2 tickets done (50%)"),
        "progress leads with the completion summary:\n{progress}"
    );
    let auth_heading = progress.find("\nAuthentication (auth)\n").expect("auth group");
    let billing_heading = progress.find("\nBilling\n").expect("billing group");
    assert!(
        auth_heading < progress.find("auth.1").expect("auth ticket")
            && progress.find("auth.1").expect("auth ticket") < billing_heading,
        "progress groups tickets under their rhei too:\n{progress}"
    );
}
