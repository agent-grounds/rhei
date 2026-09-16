//! Plan-time rejection of task-export declarations that cannot form the
//! handoff their metadata promises.

use super::new_tests::{new_run, project_with_rhei};
use super::*;

/// Validation is the user-facing boundary: it rejects both a misspelled name
/// and a merely transitive ordering path in one read-only pass.
// §FS-rhei-plan-language.3.12.1 §FS-rhei-validate.4.3
#[test]
fn validate_rejects_unmatched_exports_and_transitive_only_ordering() {
    let plan = r#"# Rhei: Invalid exports

## Tasks

### Task 1: Produce
**State:** completed
**Provides:** api-contract, error-codes

### Task 2: Middle
**State:** completed
**Prior:** 1

### Task 3: Consume
**State:** pending
**Prior:** 2
**Consumes:** 1:api-contarct
"#;
    let (_dir, plan_path, machine_path) = setup_single_file("export-validation", plan);

    let result = run_cli("validate", &plan_path, &machine_path, &[]);
    assert!(
        !result.status.success(),
        "invalid task exports must make validation fail\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    assert_stderr_contains(&result, "Task plan.3 consumes export 'api-contarct' from Task plan.1");
    assert_stderr_contains(&result, "Available exports: api-contract, error-codes");
    assert_stderr_contains(&result, "must list Task plan.1 directly in **Prior:**");
}

/// `rhei new` authors only the fields requested. Its ordinary post-write
/// validation rejects an incomplete export pair and rolls the insertion back;
/// it never synthesizes `Prior` from `Consumes`.
// §FS-rhei-new.1.3 §FS-rhei-new.6 §FS-rhei-plan-language.3.12.1
#[test]
fn new_rolls_back_a_consumer_created_without_its_direct_prior() {
    let dir = project_with_rhei("export-new-direct-prior");
    assert_success(&new_run(&["new", "Producer", "--under", "auth", "--provides", "api"], &dir));
    let path = dir.join("auth.rhei.md");
    let before = std::fs::read_to_string(&path).expect("plan before rejected create");

    let created =
        new_run(&["new", "Consumer", "--under", "auth", "--consumes", "auth.1:api"], &dir);
    assert!(!created.status.success(), "an incomplete handoff must fail validation");
    assert_stderr_contains(&created, "must list Task auth.1 directly in **Prior:**");
    assert_eq!(
        std::fs::read_to_string(&path).expect("plan after rejected create"),
        before,
        "failed post-write validation rolls the create back"
    );
}
