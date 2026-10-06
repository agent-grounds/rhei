    // The retirement record judged against the live graph. Included into
    // `mod tests` in `state_machine/mod.rs`, so the indentation is the module's.

    // §FS-rhei-validate.4 §FS-rhei-validate.4.1 §FS-rhei-validate.4.3

    fn retired_report(retired: &str, tasks: &str) -> ValidationReport {
        let input = format!(
            "# Rhei: Retired\n\n---\nmetadata:\n{retired}---\n\n## Tasks\n\n{tasks}"
        );
        let rhei = parse(&input).expect("parse ok");
        validate_with_machine(&rhei, &sample_machine())
    }

    #[test]
    fn a_prior_naming_a_retired_ticket_is_reported_as_retired() {
        let report = retired_report(
            "  retiredTickets:\n    '2': {}\n",
            "### Task 1: Live\n**State:** pending\n**Prior:** 2\n",
        );
        let joined = report.errors.join("\n");
        assert!(joined.contains("depends on retired Task 2"), "{joined}");
        assert!(!joined.contains("missing"), "retired, not missing: {joined}");
        assert!(report.help.iter().any(|help| help.contains("rhei remove")), "{:?}", report.help);
    }

    #[test]
    fn a_consumed_retired_producer_is_reported_once_as_retired() {
        let report = retired_report(
            "  retiredTickets:\n    '2': {}\n",
            "### Task 1: Live\n**State:** pending\n**Consumes:** 2:findings\n",
        );
        let joined = report.errors.join("\n");
        assert!(joined.contains("from retired producer Task 2"), "{joined}");
        assert!(!joined.contains("missing producer"), "{joined}");
    }

    #[test]
    fn a_live_ticket_under_a_retired_id_is_an_error() {
        let report = retired_report(
            "  retiredTickets:\n    '1': {}\n",
            "### Task 1: Resurrected by hand\n**State:** pending\n",
        );
        let joined = report.errors.join("\n");
        assert!(joined.contains("Task 1 is declared") && joined.contains("retired"), "{joined}");
    }

    #[test]
    fn a_malformed_reserved_key_is_an_error() {
        let report = retired_report(
            "  retiredTickets: [1]\n",
            "### Task 1: Live\n**State:** pending\n",
        );
        assert!(report.errors.iter().any(|error| error.contains("retiredTickets")), "{:?}", report.errors);
    }

    #[test]
    fn a_retired_budget_identity_is_never_rebound() {
        let report = retired_report(
            "  retiredTickets:\n    '2':\n      budgetTicketId: u-1\n  tasks:\n    '1':\n      \
             budgetTicketId: u-1\n",
            "### Task 1: Live\n**State:** pending\n",
        );
        let joined = report.errors.join("\n");
        assert!(joined.contains("u-1") && joined.contains("retired Task 2"), "{joined}");
    }

    #[test]
    fn an_identity_binding_that_is_not_retired_is_fine() {
        let report = retired_report(
            "  retiredTickets:\n    '2': {}\n  tasks:\n    '1':\n      budgetTicketId: u-1\n",
            "### Task 1: Live\n**State:** pending\n",
        );
        assert!(!report.has_errors(), "{:?}", report.errors);
    }
