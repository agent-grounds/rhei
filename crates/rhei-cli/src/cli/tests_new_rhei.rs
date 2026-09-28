/// Unit coverage for prospective-workspace admission and create ownership.
/// §FS-rhei-new.2.1.1 §FS-rhei-new.5.1
mod new_rhei_destination_tests {
    use super::super::*;

    fn classify(root: &Path, directory_layout: bool) -> NewRheiDestination {
        classify_new_rhei_destination(root, "billing", directory_layout).expect("classification")
    }

    #[test]
    fn a_missing_destination_is_vacant() {
        let project = tempfile::tempdir().expect("tempdir");
        assert_eq!(classify(project.path(), true), NewRheiDestination::Vacant);
    }

    #[test]
    fn an_empty_directory_is_adoptable() {
        let project = tempfile::tempdir().expect("tempdir");
        fs::create_dir(project.path().join("billing")).expect("prospective workspace");
        assert_eq!(classify(project.path(), true), NewRheiDestination::AdoptableDirectory);
    }

    #[test]
    fn an_authored_machine_bundle_is_adoptable() {
        let project = tempfile::tempdir().expect("tempdir");
        let billing = project.path().join("billing");
        fs::create_dir_all(billing.join("prompt_templates")).expect("prompt directory");
        fs::write(billing.join("states.yaml"), "not parsed here\n").expect("machine");
        fs::write(billing.join("prompt_templates/review.md"), "Review\n").expect("prompt");
        assert_eq!(classify(project.path(), true), NewRheiDestination::AdoptableDirectory);
    }

    /// A prior failed create or dry run leaves the exact empty coordination
    /// entry behind; retry must admit and reuse it.
    // §FS-rhei-new.2.1.1
    #[test]
    fn issue_95_exact_empty_index_sidecar_is_adoptable() {
        let project = tempfile::tempdir().expect("tempdir");
        let billing = project.path().join("billing");
        fs::create_dir(&billing).expect("prospective workspace");
        fs::write(billing.join("index.rhei.md.lock"), b"").expect("coordination sidecar");

        assert_eq!(classify(project.path(), true), NewRheiDestination::AdoptableDirectory);
    }

    /// Similar names and objects that cannot be the empty regular sidecar stay
    /// authored obstructions; the allowlist is exact.
    // §FS-rhei-new.2.1.1
    #[test]
    fn issue_95_nonempty_and_nonregular_index_sidecars_are_obstructions() {
        let nonempty = tempfile::tempdir().expect("tempdir");
        let billing = nonempty.path().join("billing");
        fs::create_dir(&billing).expect("prospective workspace");
        let sidecar = billing.join("index.rhei.md.lock");
        fs::write(&sidecar, b"not coordination\n").expect("nonempty sidecar");
        assert_eq!(classify(nonempty.path(), true), NewRheiDestination::Occupied(sidecar));

        let nonregular = tempfile::tempdir().expect("tempdir");
        let billing = nonregular.path().join("billing");
        let sidecar = billing.join("index.rhei.md.lock");
        fs::create_dir_all(&sidecar).expect("sidecar-shaped directory");
        assert_eq!(classify(nonregular.path(), true), NewRheiDestination::Occupied(sidecar));
    }

    #[test]
    fn prompt_templates_without_a_machine_are_occupied() {
        let project = tempfile::tempdir().expect("tempdir");
        let prompts = project.path().join("billing/prompt_templates");
        fs::create_dir_all(&prompts).expect("prompt directory");
        assert_eq!(classify(project.path(), true), NewRheiDestination::Occupied(prompts));
    }

    #[test]
    fn actual_rheis_collide_in_both_requested_layouts() {
        let single = tempfile::tempdir().expect("tempdir");
        let single_path = single.path().join("billing.rhei.md");
        fs::write(&single_path, "# Rhei: Existing\n").expect("single-file rhei");
        for directory_layout in [false, true] {
            assert_eq!(
                classify(single.path(), directory_layout),
                NewRheiDestination::ExistingRhei(single_path.clone())
            );
        }

        let workspace = tempfile::tempdir().expect("tempdir");
        let workspace_path = workspace.path().join("billing");
        fs::create_dir(&workspace_path).expect("workspace");
        fs::write(workspace_path.join("index.rhei.md"), "# Rhei: Existing\n")
            .expect("workspace index");
        for directory_layout in [false, true] {
            assert_eq!(
                classify(workspace.path(), directory_layout),
                NewRheiDestination::ExistingRhei(workspace_path.clone())
            );
        }
    }

    #[test]
    fn an_adoptable_directory_conflicts_with_single_file_layout() {
        let project = tempfile::tempdir().expect("tempdir");
        let billing = project.path().join("billing");
        fs::create_dir(&billing).expect("prospective workspace");
        fs::write(billing.join("states.yaml"), "authored\n").expect("machine");
        assert_eq!(
            classify(project.path(), false),
            NewRheiDestination::LayoutConflict { directory: billing, obstruction: None }
        );
    }

    #[test]
    fn unrelated_workspace_content_names_the_obstruction() {
        let project = tempfile::tempdir().expect("tempdir");
        let notes = project.path().join("billing/notes.md");
        fs::create_dir(notes.parent().expect("parent")).expect("prospective workspace");
        fs::write(&notes, "authored notes\n").expect("notes");
        assert_eq!(classify(project.path(), true), NewRheiDestination::Occupied(notes));
    }

    #[test]
    fn ownership_excludes_a_pre_existing_root() {
        let project = tempfile::tempdir().expect("tempdir");
        let root = project.path().join("billing");
        let tasks = root.join("tasks");
        fs::create_dir(&root).expect("prospective workspace");
        fs::write(root.join("states.yaml"), "authored\n").expect("machine");
        assert_eq!(
            invocation_created_directories(&[root, tasks.clone()]).expect("ownership"),
            vec![tasks]
        );
    }

    #[test]
    fn ownership_includes_every_directory_the_invocation_will_create() {
        let project = tempfile::tempdir().expect("tempdir");
        let root = project.path().join("billing");
        let tasks = root.join("tasks");
        assert_eq!(
            invocation_created_directories(&[root.clone(), tasks.clone()]).expect("ownership"),
            vec![root, tasks]
        );
    }
}

/// Unit coverage for the tip that replaces the argument parser's `--` advice on
/// a hyphen-leading option value.
/// §FS-rhei-new.3.4.1
mod new_description_hyphen_value_tests {
    use super::super::*;

    fn matched(argv: &[&str]) -> Option<&'static str> {
        hyphen_value_option(argv.iter().copied()).map(|found| found.option.name)
    }

    fn tip_for(name: &str) -> &'static str {
        HYPHEN_VALUE_OPTIONS
            .iter()
            .find(|option| option.name == name)
            .expect("a covered option")
            .tip
    }

    /// One row per covered option, each matched by its own hyphen-leading value.
    #[test]
    fn each_covered_option_matches_its_own_hyphen_leading_value() {
        assert_eq!(
            matched(&["rhei", "new", "t", "--under", "auth", "--description", "- Context: x."]),
            Some("--description")
        );
        assert_eq!(
            matched(&["rhei", "new", "t", "--description-file", "-x/body.md"]),
            Some("--description-file")
        );
    }

    /// The tips name the spellings that keep the value, and neither carries a
    /// `-- ` run of its own — the parser's advice must not come back in rhei's
    /// words.
    #[test]
    fn each_tip_names_the_spelling_that_keeps_the_value() {
        let description = tip_for("--description");
        assert!(description.contains("--description="), "got: {description}");
        assert!(description.contains("--description-file -"), "got: {description}");

        let file = tip_for("--description-file");
        assert!(file.contains("--description-file="), "got: {file}");

        for option in &HYPHEN_VALUE_OPTIONS {
            assert!(!option.tip.contains("-- "), "got: {}", option.tip);
        }
    }

    /// A hyphen-leading `TITLE` is a different mistake with a different answer,
    /// and it is the command line that tells the two apart.
    #[test]
    fn a_hyphen_leading_title_matches_no_option() {
        assert_eq!(matched(&["rhei", "new", "- leading", "--under", "auth"]), None);
    }

    /// `-` is the spelling the tip recommends, so it must never raise it.
    #[test]
    fn a_value_of_exactly_one_hyphen_matches_nothing() {
        assert_eq!(matched(&["rhei", "new", "t", "--description-file", "-"]), None);
        assert_eq!(matched(&["rhei", "new", "t", "--description", "-"]), None);
    }

    #[test]
    fn an_option_with_nothing_after_it_matches_nothing() {
        assert_eq!(matched(&["rhei", "new", "t", "--description"]), None);
    }

    /// After a bare `--` every token is a value, so nothing there was taken for
    /// another flag.
    #[test]
    fn an_occurrence_after_a_bare_double_dash_matches_nothing() {
        assert_eq!(matched(&["rhei", "new", "t", "--", "--description", "- x"]), None);
    }

    #[test]
    fn the_tip_takes_the_slot_the_parsers_own_tip_stood_in() {
        let rendered = "\
error: unexpected argument '- ' found

  tip: to pass '- ' as a value, use '-- - '

Usage: rhei new [OPTIONS] <TITLE>

For more information, try '--help'.
";
        assert_eq!(
            parser_tip_replaced(rendered, "attach it as --description=<text>.", "- "),
            "\
error: unexpected argument '- ' found

  tip: attach it as --description=<text>.

Usage: rhei new [OPTIONS] <TITLE>

For more information, try '--help'.
"
        );
    }

    /// The parser offers its `--` advice only where the command takes
    /// positionals, so a rendering without it still has to carry rhei's.
    #[test]
    fn a_refusal_carrying_no_tip_gets_one_above_the_usage_line() {
        let rendered = "error: unexpected argument '-x' found\n\nUsage: rhei new [OPTIONS]\n";
        assert_eq!(
            parser_tip_replaced(rendered, "attach it as --description-file=<path>.", "-x"),
            "error: unexpected argument '-x' found\n\n  tip: attach it as \
             --description-file=<path>.\n\nUsage: rhei new [OPTIONS]\n"
        );
    }

    /// The refusal the parser really raises for `argv`, put through the tip
    /// substitution: `None` means the parser's own message stands. Parsed
    /// rather than hand-rolled, because which token the parser refuses first is
    /// the whole question here.
    fn refusal_for(argv: &[&str]) -> Option<String> {
        match Cli::try_parse_from(argv) {
            Ok(_) => panic!("expected a refusal for {argv:?}"),
            Err(err) => hyphen_value_refusal(&err, argv.iter().copied()),
        }
    }

    /// The command lines the ticket reports: the value is what the parser
    /// refused, so rhei's tip takes the slot.
    #[test]
    fn a_refusal_about_the_value_gets_the_options_tip() {
        let bullets =
            refusal_for(&["rhei", "new", "t", "--under", "auth", "--description", "- Context: x."])
                .expect("a substituted refusal");
        assert!(bullets.contains("--description=<text>"), "got: {bullets}");
        assert!(!bullets.contains("-- - "), "got: {bullets}");

        let path = refusal_for(&["rhei", "new", "t", "--description-file", "-x/body.md"])
            .expect("a substituted refusal");
        assert!(path.contains("--description-file=<path>"), "got: {path}");
    }

    /// A value that is itself flag-shaped is still the value: the token the
    /// parser named may stand where the value stands and nowhere else.
    #[test]
    fn a_value_that_looks_like_a_flag_does_not_suppress_its_own_tip() {
        let refusal = refusal_for(&["rhei", "new", "t", "--description", "-x"])
            .expect("a substituted refusal");
        assert!(refusal.contains("--description=<text>"), "got: {refusal}");
    }

    /// An unknown flag earlier on the line is refused first, and the parser's
    /// advice about it is the advice the caller needs.
    #[test]
    fn a_refusal_about_another_flag_keeps_the_parsers_message() {
        assert_eq!(
            refusal_for(&["rhei", "new", "t", "--under", "auth", "--bogus", "--description", "-x"]),
            None
        );
        assert_eq!(refusal_for(&["rhei", "new", "t", "-x", "--description", "-xyz"]), None);
    }

    /// On a command that declares no such option it is the option's own name
    /// the parser refuses, so there is no value to advise about — and advice
    /// about one would be refused a second time when it was followed.
    #[test]
    fn a_command_without_the_option_keeps_the_parsers_message() {
        assert_eq!(refusal_for(&["rhei", "list", "--description", "-x"]), None);
        assert_eq!(refusal_for(&["rhei", "intervene", "--description", "-x"]), None);
    }

    /// A hyphen-leading `TITLE` is refused before the value is reached, and its
    /// refusal is the parser's own.
    #[test]
    fn a_hyphen_leading_title_alone_keeps_the_parsers_message() {
        assert_eq!(refusal_for(&["rhei", "new", "- leading", "--under", "auth"]), None);
    }

    /// Both mistakes at once is settled by which of them the parser reached.
    ///
    /// This test used to assert the opposite — that the option's tip is printed
    /// whenever both mistakes are on the line — on the reasoning that the token
    /// the parser named, `- `, is no token of the command line. That reasoning
    /// was the plan's extrapolation and it does not hold: the parser names a
    /// token rather than quoting it, so the title `- leading` is *also* named
    /// `- `. The rule is now the parser's own order. Where the two are
    /// distinguishable the title was refused and keeps its own message; where
    /// they are indistinguishable the earlier token wins, which is the title
    /// again.
    #[test]
    fn both_mistakes_at_once_are_settled_by_which_was_refused() {
        assert_eq!(
            refusal_for(&["rhei", "new", "- leading", "--under", "auth", "--description", "-x"]),
            None
        );
        assert_eq!(
            refusal_for(&[
                "rhei",
                "new",
                "- leading",
                "--under",
                "auth",
                "--description",
                "- Context: x.",
            ]),
            None
        );
    }

    /// The parser names a long option truncated at its first `=` and a token
    /// beginning with a single `-` as `-` plus its first character, so the
    /// comparison is against that name and not against the token. Cut on a
    /// `char`: a multibyte first character must not panic.
    #[test]
    fn the_parsers_name_for_a_token_is_not_the_token() {
        assert_eq!(parser_name_for("--kindd=task"), "--kindd");
        assert_eq!(parser_name_for("--bogus"), "--bogus");
        assert_eq!(parser_name_for("-xy"), "-x");
        assert_eq!(parser_name_for("-éxy"), "-é");
        assert_eq!(parser_name_for("- Context: x."), "- ");
        assert_eq!(parser_name_for("-"), "-");
        assert_eq!(parser_name_for("t"), "t");
    }

    /// The spellings the parser does not name verbatim: a doubled letter in a
    /// long option and a cluster of unknown short flags are refusals about
    /// *that* token, and taking the parser's advice away from them would take
    /// away the only advice that fixes the line.
    #[test]
    fn a_refusal_the_parser_did_not_name_verbatim_keeps_the_parsers_message() {
        assert_eq!(
            refusal_for(&[
                "rhei",
                "new",
                "t",
                "--under",
                "auth",
                "--kindd=task",
                "--description",
                "- Context: x.",
            ]),
            None
        );
        assert_eq!(refusal_for(&["rhei", "list", "--bogus=1", "--description", "-z"]), None);
        assert_eq!(refusal_for(&["rhei", "list", "-xy", "--description", "-z"]), None);
        assert_eq!(refusal_for(&["rhei", "intervene", "-xy", "--description", "-z"]), None);
    }

    /// A multibyte cluster reaches the same comparison, and the answer is the
    /// parser's message rather than a panic.
    #[test]
    fn a_multibyte_first_character_is_cut_on_a_char_boundary() {
        assert_eq!(refusal_for(&["rhei", "list", "-éxy", "--description", "-z"]), None);
        let refusal = refusal_for(&["rhei", "new", "t", "--under", "auth", "--description", "-éxy"])
            .expect("a substituted refusal");
        assert!(refusal.contains("--description=<text>"), "got: {refusal}");
    }

    /// An earlier token the parser would name the same way is the one it
    /// refused, so the value's tip is withheld: the name alone cannot tell
    /// `rhei list -x --description -x` apart, but the order can.
    #[test]
    fn an_earlier_token_with_the_same_name_keeps_the_parsers_message() {
        assert_eq!(refusal_for(&["rhei", "list", "-x", "--description", "-x"]), None);
    }

    /// Both options at once is an error the caller must fix either way, but the
    /// value that was refused is still the one answered for: the match is
    /// positional and by name rather than "this token appears nowhere else".
    #[test]
    fn a_value_repeated_later_on_the_line_keeps_its_tip() {
        let refusal = refusal_for(&[
            "rhei",
            "new",
            "t",
            "--under",
            "auth",
            "--description",
            "-x",
            "--description-file",
            "-x",
        ])
        .expect("a substituted refusal");
        assert!(refusal.contains("--description=<text>"), "got: {refusal}");
    }

    /// The parser quotes the refused token inside its tip, so a token carrying a
    /// blank line makes a tip several paragraphs long. All of it goes: half a
    /// tip left behind is the `-- ` advice this refusal exists to remove,
    /// standing without the `tip:` label that says it is advice.
    #[test]
    fn a_refused_token_carrying_a_blank_line_takes_its_whole_tip_with_it() {
        let argv = ["rhei", "new", "t", "--under", "auth", "--description", "--bo\n\nq"];
        let refusal = refusal_for(&argv).expect("a substituted refusal");
        assert!(refusal.contains("--description=<text>"), "got: {refusal}");
        assert!(!refusal.contains("as a value, use"), "got: {refusal}");
        assert!(!refusal.contains("-- --bo"), "got: {refusal}");
    }

    /// A `tip:` line inside the sentence the parser echoed back is part of what
    /// was refused, not the parser's advice, and dropping it would leave the
    /// message no longer saying what was refused.
    #[test]
    fn a_tip_line_inside_the_error_sentence_is_kept() {
        let rendered = "\
error: unexpected argument '--bo
tip: eaten' found

  tip: to pass '--bo
tip: eaten' as a value, use '-- --bo
tip: eaten'

Usage: rhei new [OPTIONS] <TITLE>
";
        assert_eq!(
            parser_tip_replaced(rendered, "attach it as --description=<text>.", "--bo\ntip: eaten"),
            "\
error: unexpected argument '--bo
tip: eaten' found

  tip: attach it as --description=<text>.

Usage: rhei new [OPTIONS] <TITLE>
"
        );
    }
}
