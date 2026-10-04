// §AR-source-file-size.3: what a project says when something is wrong — errors,
// near-miss suggestions, and code frames. What it renders when nothing is wrong
// is a sibling; fixtures live in `common.rs`.
#[test]
fn invalid_derived_rhei_id_error_states_rule_and_suggests_rename() {
    let dir = unique_temp_dir("invalid-rhei-id");
    fs::write(
        dir.join("My Plan.rhei.md"),
        "# Rhei: Spaces\n\n## Tasks\n\n### Task 1: Alpha\n**State:** pending\n",
    )
    .expect("write plan");

    // §AR-rhei-panta.3: the derived id is a load error with the rule and a
    // concrete rename, on every command including read-only ones.
    let output = rhei_command()
        .arg("list")
        .arg(dir.join("My Plan.rhei.md"))
        .output()
        .expect("list runs");
    assert!(!output.status.success(), "invalid derived id must fail");
    // miette wraps report lines, so collapse the decoration before matching.
    let stderr: String = String::from_utf8_lossy(&output.stderr)
        .chars()
        .filter(|ch| *ch != '│' && *ch != '\n')
        .collect();
    let stderr = stderr.split_whitespace().collect::<Vec<_>>().join(" ");
    // The wrap may split the suggested filename, so drop spaces entirely for
    // the rename fragment.
    let compact: String = stderr.chars().filter(|ch| !ch.is_whitespace()).collect();
    assert!(
        stderr.contains("rhei id 'My Plan'")
            && stderr.contains("must start with a letter")
            && compact.contains("Renamethefileto`My-Plan.rhei.md`"),
        "error should state the rule and suggest a rename: {stderr}"
    );
}

#[test]
fn duplicate_rhei_id_error_names_both_sources() {
    let project = create_panta_project(
        "panta-dup-id",
        "# Panta: Duplicate\n",
        &[
            ("auth.rhei.md", "# Rhei: Auth A\n\n## Tasks\n\n### Task 1: A\n**State:** pending\n"),
            ("auth/index.rhei.md", "# Rhei: Auth B\n\n## Notes\nWorkspace variant.\n"),
            ("auth/tasks/one.md", "### Task 1: B\n**State:** pending\n"),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let err = workspace::load_panta_project(&project).expect_err("duplicate id should fail");
    assert!(
        err.message.contains("duplicate rhei id 'auth'")
            && err.message.contains("auth.rhei.md")
            && err.message.matches("auth").count() >= 2
            && err.message.contains("Rename"),
        "error should name both colliding sources and the fix: {}",
        err.message
    );
}

#[test]
fn implicit_panta_rejects_basin_named_single_file_rhei() {
    let dir = unique_temp_dir("implicit-basin");
    fs::write(
        dir.join("basin.rhei.md"),
        "# Rhei: Basin\n\n## Tasks\n\n### Task 1: Alpha\n**State:** pending\n",
    )
    .expect("write plan");

    // §FS-rhei-panta.4: the reservation also guards the implicit-Panta path.
    let output = rhei_command()
        .arg("list")
        .arg(dir.join("basin.rhei.md"))
        .output()
        .expect("list runs");
    assert!(!output.status.success(), "basin id must be rejected on the implicit path");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("`basin` is reserved") && stderr.contains("Rename"),
        "error should state the reservation and the fix: {stderr}"
    );
}

#[test]
fn unknown_task_id_error_suggests_closest_qualified_ids() {
    let dir = unique_temp_dir("unknown-task-hint");
    fs::write(
        dir.join("plan.rhei.md"),
        "# Rhei: Hints\n\n## Tasks\n\n### Task cache-key: Alpha\n**State:** pending\n",
    )
    .expect("write plan");

    let output = rhei_command()
        .arg("next")
        .arg(dir.join("plan.rhei.md"))
        .args(["--task", "cache", "--no-callbacks"])
        .output()
        .expect("next runs");
    assert!(!output.status.success(), "unknown task must fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("task 'cache' not found") && stderr.contains("plan.cache-key"),
        "error should suggest the closest qualified id: {stderr}"
    );
}

#[test]
fn missing_input_artifact_error_names_pre_qualification_file() {
    let dir = unique_temp_dir("legacy-artifact-hint");
    fs::write(
        dir.join("plan.rhei.md"),
        "# Rhei: Legacy\n\n## Tasks\n\n### Task 1: Alpha\n**State:** pending\n",
    )
    .expect("write plan");
    fs::write(dir.join("states.yaml"), PANTA_INPUT_STATE_MACHINE).expect("write machine");
    // The artifact exists under its pre-qualification (rhei-local) name only.
    fs::create_dir_all(dir.join("runtime")).expect("mkdir runtime");
    fs::write(dir.join("runtime/1.md"), "brief\n").expect("write legacy artifact");

    let output = rhei_command()
        .arg("next")
        .arg(dir.join("plan.rhei.md"))
        .args(["--task", "1", "--no-callbacks"])
        .output()
        .expect("next runs");
    assert!(!output.status.success(), "missing qualified input must fail");
    let stderr: String = String::from_utf8_lossy(&output.stderr)
        .chars()
        .filter(|ch| *ch != '│' && *ch != '\n')
        .collect();
    let stderr = stderr.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        stderr.contains("Missing required input artifact: brief (runtime/plan.1.md)")
            && stderr.contains("pre-qualification artifact exists at 'runtime/1.md'"),
        "error should name the legacy file and the rename: {stderr}"
    );
}

#[test]
fn ambiguous_rhei_local_shorthand_names_qualified_candidates() {
    let project = create_panta_project(
        "panta-ambiguous-shorthand",
        "# Panta: Ambiguous\n",
        &[
            ("auth.rhei.md", "# Rhei: Auth\n\n## Tasks\n\n### Task 1: Login\n**State:** pending\n"),
            (
                "billing.rhei.md",
                "# Rhei: Billing\n\n## Tasks\n\n### Task 1: Invoice\n**State:** pending\n",
            ),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    // §FS-rhei-panta.6: a shorthand matching more than one rhei is an error
    // that names the qualified candidates.
    let output = rhei_command()
        .arg("next")
        .arg(&project)
        .args(["--task", "1", "--no-callbacks"])
        .output()
        .expect("next runs");
    assert!(!output.status.success(), "ambiguous shorthand must fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("ambiguous across rheis")
            && stderr.contains("auth.1")
            && stderr.contains("billing.1"),
        "error should name the qualified candidates: {stderr}"
    );

    // A --rhei narrowing disambiguates the same shorthand.
    let output = rhei_command()
        .arg("next")
        .arg(&project)
        .args(["--task", "1", "--rhei", "auth", "--peek", "--no-callbacks"])
        .output()
        .expect("next runs");
    assert!(
        output.status.success(),
        "narrowed shorthand should resolve\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("auth.1"),
        "resolved ticket should be qualified in output"
    );
}

/// A parse error in a rhei inside a project keeps the code frame that file
/// gets when validated directly — the project form is the one `rhei init`
/// steers new authors toward. §FS-rhei-panta.6
#[test]
fn project_parse_errors_keep_their_line_and_code_frame() {
    let project = create_panta_project(
        "panta-parse-frame",
        "# Panta: Broken\n",
        &[("broken.rhei.md", "# Onboarding\n\n## Tasks\n\n### Task 1: One\n**State:** pending\n")],
        WORKSPACE_STATE_MACHINE,
    );

    let output = rhei_command()
        .arg("validate")
        .arg(&project)
        .output()
        .expect("validate command should run");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "validate should fail on the malformed rhei");
    for fragment in ["PARSE ERROR", "line 1", "# Onboarding", "broken.rhei.md"] {
        assert!(
            stderr.contains(fragment),
            "project parse error should include {fragment:?}, got:\n{stderr}"
        );
    }
}

/// Reaching a plan through its project must not cost the author diagnostics.
/// Task headings under a content section fail first as "Metadata field appears
/// outside a task", on a line that is not the mistake; only the structural
/// diagnostic explains it, and it must survive both scopes.
// §FS-rhei-validate.4.2: both scopes report every problem in the file.
#[test]
fn project_scoped_parse_errors_match_file_scoped_ones() {
    let plan = "# Rhei: A\n\n## Overview\n\n### Task 1: one\n\n**State:** pending\n\n\
                ### Task 2: two\n\n**State:** pending\n";
    let project = create_panta_project(
        "panta-parse-parity",
        "# Panta: Parity\n",
        &[("c.rhei.md", plan)],
        WORKSPACE_STATE_MACHINE,
    );

    let run = |target: &Path| -> String {
        let output = rhei_command()
            .arg("validate")
            .arg(target)
            .output()
            .expect("validate command should run");
        assert!(!output.status.success(), "the malformed plan must fail validation");
        let stderr: String = String::from_utf8_lossy(&output.stderr)
            .chars()
            .filter(|ch| *ch != '│' && *ch != '\n')
            .collect();
        normalize_for_assertions(&stderr)
    };

    let by_project = run(&project);
    let by_file = run(&project.join("c.rhei.md"));

    for scope in [&by_project, &by_file] {
        assert!(scope.contains("(3 problems)"), "every problem must be reported, got:\n{scope}");
        assert!(
            scope.contains("Tasks section must be the final '##' chapter"),
            "the structural diagnostic explains the mistake, got:\n{scope}"
        );
        assert!(
            scope.contains("line 7:") && scope.contains("line 11:"),
            "each problem keeps its line, got:\n{scope}"
        );
    }
}

/// §FS-rhei-validate.3: a `**Prior:**` naming an unknown rhei is reported under
/// the id the author wrote — never re-qualified with the citing rhei — and names
/// both the unknown rhei and the near miss.
#[test]
fn missing_prior_names_the_unknown_rhei_and_suggests_the_near_miss() {
    let project = create_panta_project(
        "panta-prior-typo",
        "# Panta: Typo\n",
        &[
            (
                "onboarding.rhei.md",
                "# Rhei: Onboarding\n\n## Tasks\n\n### Task 1: Research\n**State:** pending\n",
            ),
            (
                "billing.rhei.md",
                "# Rhei: Billing\n\n## Tasks\n\n### Task 1: Pick\n**State:** pending\n\
                 **Prior:** onbaording.1\n",
            ),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let output = rhei_command()
        .arg("validate")
        .arg(&project)
        .output()
        .expect("validate command should run");
    // miette wraps report lines, so collapse the decoration before matching.
    let stderr: String = String::from_utf8_lossy(&output.stderr)
        .chars()
        .filter(|ch| *ch != '│' && *ch != '\n')
        .collect();
    let stderr = normalize_for_assertions(&stderr);
    assert!(!output.status.success(), "validate should fail on the unknown rhei");
    assert!(
        stderr.contains("no rhei named 'onbaording'"),
        "error should name the unknown rhei, got:\n{stderr}"
    );
    assert!(
        stderr.contains("Did you mean 'onboarding.1'?"),
        "error should suggest the near miss, got:\n{stderr}"
    );
    assert!(
        stderr.contains("missing Task onbaording.1"),
        "error should quote the authored id, got:\n{stderr}"
    );
    assert!(
        !stderr.contains("billing.onbaording.1"),
        "error must not re-qualify the authored id with the citing rhei, got:\n{stderr}"
    );
}

/// §FS-rhei-validate.3: a correction is only offered when it resolves to some
/// other ticket. A one-character rhei name is within one edit of every other, so
/// the near-miss search must not propose the citing task as its own prior.
#[test]
fn missing_prior_never_suggests_the_citing_task_as_its_own_prior() {
    let project = create_panta_project(
        "panta-prior-self-suggest",
        "# Panta: Short\n",
        &[
            ("b.rhei.md", "# Rhei: B\n\n## Tasks\n\n### Task 1: Other\n**State:** pending\n"),
            (
                "a.rhei.md",
                "# Rhei: A\n\n## Tasks\n\n### Task 1: Pick\n**State:** pending\n\
                 **Prior:** c.1\n",
            ),
        ],
        WORKSPACE_STATE_MACHINE,
    );

    let output = rhei_command()
        .arg("validate")
        .arg(&project)
        .output()
        .expect("validate command should run");
    let stderr: String = String::from_utf8_lossy(&output.stderr)
        .chars()
        .filter(|ch| *ch != '│' && *ch != '\n')
        .collect();
    let stderr = normalize_for_assertions(&stderr);
    assert!(!output.status.success(), "validate should fail on the unknown rhei");
    assert!(
        stderr.contains("missing Task c.1"),
        "error should quote the authored id, got:\n{stderr}"
    );
    assert!(!stderr.contains("Did you mean"), "no correction should be offered, got:\n{stderr}");
    assert!(
        stderr.contains("rhei 'a' has no ticket 'c.1'"),
        "error should rule out the local-id reading too, got:\n{stderr}"
    );
}
