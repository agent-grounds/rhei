    // Link integrity stops at the edge of a code region: a fenced block or an
    // inline code span holds illustrations, not references.
    // §FS-rhei-plan-language.3.6

    /// §FS-rhei-plan-language.3.6: a content section that quotes the shape of a
    /// task body names files that were never meant to resolve.
    #[test]
    fn a_link_inside_a_fenced_content_section_is_not_checked() {
        let dir = tempfile::tempdir().expect("tmpdir");

        let input = "# Rhei: Link demo\n\n## How To Write A Task\n\n\
                     Example of the body we want:\n\n\
                     ```markdown\n\
                     See [the design note](docs/design.md) for the rationale.\n\
                     ```\n\n\
                     ## Tasks\n\n\
                     ### Task 1: Do it\n**State:** pending\n";
        let rhei = parse(input).expect("parse ok");
        let report = Validator::new(sample_machine()).validate_with_base(&rhei, Some(dir.path()));

        assert!(!report.has_errors(), "fenced example should not be checked: {:?}", report.errors);
    }

    /// §FS-rhei-plan-language.3.6: the same rule for a task body, which has
    /// carried fenced examples since long before content sections did.
    #[test]
    fn a_link_inside_a_fenced_task_body_is_not_checked() {
        let dir = tempfile::tempdir().expect("tmpdir");

        let input = "# Rhei: Link demo\n\n## Tasks\n\n\
                     ### Task 1: Do it\n**State:** pending\n\n\
                     Example:\n\n\
                     ```markdown\n\
                     See [the design note](docs/design.md).\n\
                     ```\n";
        let rhei = parse(input).expect("parse ok");
        let report = Validator::new(sample_machine()).validate_with_base(&rhei, Some(dir.path()));

        assert!(!report.has_errors(), "fenced example should not be checked: {:?}", report.errors);
    }

    /// §FS-rhei-plan-language.3.6: skipping code must not skip prose — a real
    /// reference outside every fence is still a reference.
    #[test]
    fn a_broken_link_outside_a_fence_still_fails() {
        let dir = tempfile::tempdir().expect("tmpdir");

        let input = "# Rhei: Link demo\n\n## Overview\n\n\
                     ```markdown\n\
                     See [an example](docs/example.md).\n\
                     ```\n\n\
                     Really see [the spec](specs/nonexistent.md).\n\n\
                     ## Tasks\n\n\
                     ### Task 1: Do it\n**State:** pending\n";
        let rhei = parse(input).expect("parse ok");
        let report = Validator::new(sample_machine()).validate_with_base(&rhei, Some(dir.path()));

        let joined = report.errors.join("\n");
        assert!(joined.contains("nonexistent.md"), "prose link should fail; got:\n{joined}");
        assert!(!joined.contains("docs/example.md"), "fenced link was checked; got:\n{joined}");
    }

    /// §FS-rhei-plan-language.3.6: a fence nobody closed runs to the end of the
    /// text it opened, exactly as a renderer treats it.
    #[test]
    fn an_unclosed_fence_runs_to_the_end_of_the_body() {
        let dir = tempfile::tempdir().expect("tmpdir");

        let input = "# Rhei: Link demo\n\n## Tasks\n\n\
                     ### Task 1: Do it\n**State:** pending\n\n\
                     ```markdown\n\
                     See [an example](docs/example.md).\n\n\
                     And [another](docs/other.md).\n";
        let rhei = parse(input).expect("parse ok");
        let report = Validator::new(sample_machine()).validate_with_base(&rhei, Some(dir.path()));

        assert!(!report.has_errors(), "unclosed fence should swallow both: {:?}", report.errors);
    }

    /// §FS-rhei-plan-language.3.6: an inline code span is code too, and a real
    /// link beside one on the same line is still checked.
    #[test]
    fn an_inline_code_span_hides_its_link_and_nothing_else() {
        let links = extract_markdown_links("Write `[text](target.md)`, then read [it](real.md).");

        assert_eq!(links, vec![("it".to_string(), "real.md".to_string())]);
    }

    /// §FS-rhei-plan-language.3.6: a closing run shorter than the opening one
    /// does not close the block, so a fence quoting a fence stays code.
    #[test]
    fn a_shorter_run_inside_a_longer_fence_does_not_close_it() {
        let text = "````markdown\n```\n[a](a.md)\n```\n````\n[b](b.md)\n";

        assert_eq!(extract_markdown_links(text), vec![("b".to_string(), "b.md".to_string())]);
    }

    /// §FS-rhei-plan-language.3.6: tildes fence as backticks do, and a tilde
    /// run does not close a backtick block.
    #[test]
    fn tilde_fences_are_code_and_do_not_close_backtick_blocks() {
        let tildes = "~~~\n[a](a.md)\n~~~\n[b](b.md)\n";
        assert_eq!(extract_markdown_links(tildes), vec![("b".to_string(), "b.md".to_string())]);

        let mixed = "```\n[a](a.md)\n~~~\n[b](b.md)\n";
        assert!(extract_markdown_links(mixed).is_empty());
    }

    /// §FS-rhei-plan-language.3.6: an unmatched backtick run is literal text,
    /// not the start of a span that swallows the rest of the line.
    #[test]
    fn an_unclosed_backtick_run_leaves_the_line_alone() {
        let links = extract_markdown_links("A ` stray tick and [a link](a.md).");

        assert_eq!(links, vec![("a link".to_string(), "a.md".to_string())]);
    }
