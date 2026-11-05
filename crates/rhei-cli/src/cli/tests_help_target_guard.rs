// A help prints its command's target, and this is what keeps that true of the
// help nobody has written yet. §FS-rhei-errors.1.2.1 §FS-rhei-errors.6

mod help_target_guard {
    /// Every printed command that may keep a target placeholder, and why it has
    /// no target to print. A site not listed here prints the one it was given.
    const NO_TARGET_TO_PRINT: &[(&str, &str, &str)] = &[
        (
            "cli_dispatch.rs",
            "rhei reset <plan-or-project>",
            "reset never infers its target, and this refusal is about omitting it; §FS-rhei-panta.6",
        ),
        (
            "runs_command.rs",
            "rhei run --headless <plan>",
            "a success-path hint with no run on the machine, so nothing to name",
        ),
        (
            "run_prompt_navigation.rs",
            "rhei render <plan>",
            "prompt text a worker reads beside the plan's path, not a diagnostic",
        ),
        (
            "templates_union.rs",
            "rhei instantiate <template> [inputs] --into <rhei>",
            "teaches the form that replaces a removed manifest key; that form's rhei is not chosen yet",
        ),
        (
            "templates_union.rs",
            "rhei instantiate --into <rhei>",
            "names the call-site form in a refusal about a manifest field, which has no target",
        ),
        (
            "templates_union_host.rs",
            "rhei new --under <rhei>",
            "the reader chooses which rhei the ticket goes to; `--into basin` was what was refused",
        ),
    ];

    /// Source with comments gone and string continuations joined, so a command
    /// a literal wraps over two source lines reads as the one command it prints.
    fn printed_text(source: &str, continuation: &regex::Regex) -> String {
        let code: Vec<&str> =
            source.lines().filter(|line| !line.trim_start().starts_with("//")).collect();
        continuation.replace_all(&code.join("\n"), "").into_owned()
    }

    fn rust_sources(dir: &std::path::Path, found: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("read source dir") {
            let path = entry.expect("dir entry").path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_owned();
            if path.is_dir() {
                rust_sources(&path, found);
            } else if name.ends_with(".rs") && !name.starts_with("tests_") {
                // Test fixtures construct text to assert on, not to show.
                found.push(path);
            }
        }
    }

    /// Fails when a printed command holds a target placeholder at a site the list
    /// above does not name, and when the list names a site that no longer has one.
    #[test]
    #[ignore = "red until #555: helps print `rhei list <plan>` on commands that know their target; implement removes this"]
    fn help_target_placeholders_stand_only_where_there_is_no_target() {
        let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("the crates directory")
            .to_path_buf();
        let mut sources = Vec::new();
        for entry in std::fs::read_dir(&crates).expect("read crates dir") {
            let source_dir = entry.expect("dir entry").path().join("src");
            if source_dir.is_dir() {
                rust_sources(&source_dir, &mut sources);
            }
        }
        assert!(sources.len() > 300, "the scan found only {} files; has the layout changed?", sources.len());

        let continuation = regex::Regex::new(r"\\\r?\n\s*").expect("continuation pattern");
        let placeholder = regex::Regex::new(
            r#"\brhei [a-z][a-z-]*\b[^"`\n]*?<(?:plan|plan-or-project|plan-or-directory|project|rhei|target|workspace)>"#,
        )
        .expect("placeholder pattern");

        let mut offenders = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for path in &sources {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_owned();
            let source = std::fs::read_to_string(path).expect("read source file");
            for found in placeholder.find_iter(&printed_text(&source, &continuation)) {
                let allowed = NO_TARGET_TO_PRINT
                    .iter()
                    .find(|(file, command, _)| *file == name && found.as_str() == *command);
                match allowed {
                    Some((file, command, _)) => {
                        seen.insert((*file, *command));
                    }
                    None => offenders.push(format!("{name}: {}", found.as_str())),
                }
            }
        }
        offenders.sort();
        assert!(
            offenders.is_empty(),
            "{} printed command(s) hold a target placeholder at a site with no reason to; print \
             the target (§FS-rhei-errors.1.2.1), or list the site with why it has none:\n  {}",
            offenders.len(),
            offenders.join("\n  ")
        );
        for (file, command, _) in NO_TARGET_TO_PRINT {
            assert!(
                seen.contains(&(*file, *command)),
                "{file} no longer prints `{command}`, so it should leave the list"
            );
        }
    }
}
