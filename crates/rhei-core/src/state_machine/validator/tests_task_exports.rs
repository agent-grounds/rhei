    fn validate_export_plan(input: &str) -> ValidationReport {
        let rhei = crate::parse(input).expect("export plan should parse");
        validate_with_machine(&rhei, &sample_machine())
    }

    /// A consumed name is a declaration reference, not a best-effort file
    /// lookup, and the diagnostic carries the producer's repair vocabulary.
    // §FS-rhei-validate.4.3 §FS-rhei-plan-language.3.12.1
    #[test]
    fn task_export_validation_rejects_an_unprovided_name_with_available_exports() {
        let report = validate_export_plan(
            r#"# Rhei: Pairing
## Tasks

### Task 1: Produce
**State:** completed
**Provides:** api-contract, error-codes

### Task 2: Consume
**State:** pending
**Prior:** 1
**Consumes:** 1:api-contarct
"#,
        );

        assert_eq!(report.errors.len(), 1, "one bad reference, one error: {report:?}");
        let error = &report.errors[0];
        assert!(
            error.contains("Task 2 consumes export 'api-contarct' from Task 1")
                && error.contains("does not declare it")
                && error.contains("api-contract")
                && error.contains("error-codes"),
            "the pairing error should list the producer's exports; got:\n{error}"
        );
    }

    /// Ordering is authored where readers and scheduling commands see it; a
    /// transitive path is intentionally insufficient.
    // §FS-rhei-validate.4.3 §FS-rhei-plan-language.3.12.1
    #[test]
    fn task_export_validation_requires_the_producer_as_a_direct_prior() {
        let report = validate_export_plan(
            r#"# Rhei: Direct ordering
## Tasks

### Task 1: Produce
**State:** completed
**Provides:** findings

### Task 2: Middle
**State:** completed
**Prior:** 1

### Task 3: Consume
**State:** pending
**Prior:** 2
**Consumes:** 1:findings
"#,
        );

        assert_eq!(report.errors.len(), 1, "one absent direct edge, one error: {report:?}");
        assert!(
            report.errors[0].contains("Task 3 consumes export 'findings' from Task 1")
                && report.errors[0].contains("must list Task 1 directly in **Prior:**"),
            "the direct-edge repair should be explicit; got:\n{}",
            report.errors[0]
        );
    }

    /// Primary diagnostics suppress only their derivatives. A missing Prior
    /// is not repeated through Consumes, while unrelated consumed references
    /// remain visible.
    // §FS-rhei-validate.4.3
    #[test]
    fn task_export_validation_deduplicates_missing_producers_and_collects_independent_errors() {
        let report = validate_export_plan(
            r#"# Rhei: Diagnostic collection
## Tasks

### Task 1: Produce
**State:** completed
**Provides:** report

### Task 2: Consume
**State:** pending
**Prior:** 99, 1
**Consumes:** 99:ghost, 88:missing, 1:typo
"#,
        );
        let joined = report.errors.join("\n");

        assert_eq!(report.errors.len(), 3, "prior, consumes-only producer, and name: {joined}");
        assert_eq!(joined.matches("Task 99").count(), 1, "same missing pair is reported once");
        assert!(joined.contains("Task 88") && joined.contains("missing producer"), "got:\n{joined}");
        assert!(joined.contains("Task 1") && joined.contains("typo"), "got:\n{joined}");
        assert!(
            !joined.contains("list Task 88 directly"),
            "a missing producer must not gain a derivative edge error: {joined}"
        );
    }

    /// The graph's forbidden relationships receive one specific explanation,
    /// without pairing or edge noise layered on top.
    // §FS-rhei-validate.4.3 §FS-rhei-plan-language.3.12.1
    #[test]
    fn task_export_validation_reports_self_and_ancestor_consumption_once_each() {
        let report = validate_export_plan(
            r#"# Rhei: Forbidden producers
## Tasks

### Task 1: Parent
**State:** pending
**Provides:** notes
**Consumes:** 1:notes

#### Task 1.1: Child
**State:** pending
**Consumes:** 1:notes
"#,
        );
        let joined = report.errors.join("\n");

        assert_eq!(report.errors.len(), 2, "one self and one ancestor error: {joined}");
        assert!(joined.contains("Task 1 cannot consume its own export 'notes'"), "got:\n{joined}");
        assert!(
            joined.contains("Task 1.1 cannot consume export 'notes' from ancestor Task 1"),
            "got:\n{joined}"
        );
        assert!(!joined.contains("directly in **Prior:**"), "no derivative errors: {joined}");
    }

    /// Provides may be written for a human, and a valid direct handoff adds no
    /// warning beyond the graph-level visibility advisory.
    // §FS-rhei-validate.4.3
    #[test]
    fn task_export_validation_keeps_unused_and_valid_exports_silent() {
        let report = validate_export_plan(
            r#"# Rhei: Valid exports
## Tasks

### Task 1: Produce
**State:** completed
**Provides:** used, for-humans

### Task 2: Consume
**State:** pending
**Prior:** 1
**Consumes:** 1:used
"#,
        );

        assert!(report.errors.is_empty(), "valid handoff should pass: {report:?}");
        assert_eq!(
            report.warnings,
            vec![CONSUMES_ADVISORY],
            "unused Provides adds no warning beyond the visibility advisory: {report:?}"
        );
    }

    /// Export integrity does not invent another coherence warning for a
    /// terminal consumer whose one direct producer is still non-terminal.
    // §FS-rhei-validate.4.3
    #[test]
    fn task_export_validation_preserves_the_single_prior_coherence_warning() {
        let report = validate_export_plan(
            r#"# Rhei: Coherence
## Tasks

### Task 1: Produce
**State:** pending
**Provides:** report

### Task 2: Consume too early
**State:** completed
**Prior:** 1
**Consumes:** 1:report
"#,
        );

        assert!(report.errors.is_empty(), "the declarations are valid: {report:?}");
        assert_eq!(
            report.warnings,
            vec![
                "Task 2 is 'completed' but its prerequisites are unsatisfied: Task 1 (pending). The plan contradicts its own **Prior:** dependencies.",
                CONSUMES_ADVISORY,
            ],
            "one coherence warning and the independent visibility advisory: {report:?}"
        );
    }
