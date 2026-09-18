use super::*;

/// §FS-rhei-plan-language.3.14: both task snapshot forms are metadata, not
/// prose, and the parser accepts all three lineage axes.
#[test]
fn snapshot_prior_parses_task_inheritance_metadata() {
    for value in ["reviewed from self", "reviewed from ancestor", "reviewed from prior", "none"] {
        let input = format!(
            "# Rhei: Example\n## Tasks\n\n### Task 1: Alpha\n**State:** pending\n**Inherits:** {value}\n"
        );

        parse(&input).unwrap_or_else(|err| panic!("{value:?} should parse: {}", err.message));
    }
}

/// §FS-rhei-plan-language.3.14: malformed and repeated controls are diagnosed
/// as inheritance metadata rather than silently becoming task prose.
#[test]
fn snapshot_prior_rejects_malformed_and_duplicate_task_inheritance_metadata() {
    let malformed = r#"# Rhei: Example
## Tasks

### Task 1: Alpha
**State:** pending
**Inherits:** reviewed beside prior
"#;
    let err = parse(malformed).expect_err("malformed inheritance rule");
    assert!(
        err.message.contains("Malformed **Inherits:**"),
        "unexpected diagnostic: {}",
        err.message
    );

    let duplicate = r#"# Rhei: Example
## Tasks

### Task 1: Alpha
**State:** pending
**Inherits:** reviewed from prior
**Inherits:** none
"#;
    let err = parse(duplicate).expect_err("duplicate inheritance rule");
    assert!(err.message.contains("more than once"), "unexpected diagnostic: {}", err.message);
}

/// §FS-rhei-plan-language.3.14: the closed metadata block has one canonical
/// order, immediately after Prior and before export/execution metadata.
#[test]
fn snapshot_prior_task_inheritance_metadata_has_canonical_order() {
    let before_prior = r#"# Rhei: Example
## Tasks

### Task 1: Alpha
**State:** pending
**Inherits:** reviewed from prior
**Prior:** Task 2

### Task 2: Beta
**State:** pending
"#;
    let err = parse(before_prior).expect_err("inheritance before Prior");
    assert!(err.message.contains("after **Prior:**"), "unexpected diagnostic: {}", err.message);

    let after_provides = r#"# Rhei: Example
## Tasks

### Task 1: Alpha
**State:** pending
**Provides:** report
**Inherits:** reviewed from prior
"#;
    let err = parse(after_provides).expect_err("inheritance after Provides");
    assert!(err.message.contains("before **Provides:**"), "unexpected diagnostic: {}", err.message);
}
