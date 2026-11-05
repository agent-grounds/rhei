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
            "roster_command.rs",
            "rhei roster <RHEI_PLAN>",
            "the refusal is about the target given, so the form is the remedy",
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

    /// Every `rhei <verb>` help text may keep, and why the line is right as it stands.
    /// A site not listed here prints the command's target through `help_target.rs`.
    const BARE_VERB_STANDS: &[(&str, &str, &str)] = &[
        (
            "budget_forget.rs",
            "rhei budget adjust",
            "prints the root `forget` was pointed at, through `budget_target_argument`",
        ),
        (
            "budget_forget.rs",
            "rhei budget show",
            "prints the root `forget` was pointed at, through `budget_target_argument`",
        ),
        (
            "budget_command.rs",
            "rhei transition",
            "names the kind of charge an edge is, and runs nothing",
        ),
        (
            "error_guidance.rs",
            "rhei states",
            "re-checks the machine file it names and nothing else; a plan on the line could refuse first",
        ),
        (
            "retry_command.rs",
            "rhei transition",
            "names the operator's forced-recovery ceremony, which an agent must not run; not a line to paste",
        ),
        (
            "roster_command.rs",
            "rhei roster",
            "the refusal is about the target given, so the form is the remedy",
        ),
        ("run_frontend.rs", "rhei run", "the dashboard keeps only the message; this help is never shown"),
        ("run_frontend.rs", "rhei states", "the dashboard keeps only the message; this help is never shown"),
        ("run_frontend.rs", "rhei transition", "the dashboard keeps only the message; this help is never shown"),
        (
            "run_registry.rs",
            "rhei run",
            "`--headless` stands between the verb and the path it prints, which is the one it looked for",
        ),
        ("templates_instantiate.rs", "rhei run", "`--help` needs no target"),
        (
            "templates_union_host.rs",
            "rhei list",
            "`--into` resolves from where it was typed, so a bare list there shows what it can name",
        ),
        ("viz_command.rs", "rhei viz", "asks for another target, which is the refusal, and runs nothing"),
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

    /// Every source file of every crate, test fixtures aside.
    fn crate_sources() -> Vec<std::path::PathBuf> {
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
        assert!(
            sources.len() > 300,
            "the scan found only {} files; has the layout changed?",
            sources.len()
        );
        sources
    }

    /// Fails when a printed command holds a target placeholder at a site the list
    /// above does not name, and when the list names a site that no longer has one.
    #[test]
    fn help_target_placeholders_stand_only_where_there_is_no_target() {
        let sources = crate_sources();

        let continuation = regex::Regex::new(r"\\\r?\n\s*").expect("continuation pattern");
        let placeholder = regex::Regex::new(
            // A placeholder in clap's spelling, `<RHEI_PLAN>` or `<TARGET>`, asks for the same.
            r#"\brhei [a-z][a-z-]*\b[^"`\n]*?<(?i:plan|plan-or-project|plan-or-directory|project|rhei|rhei_plan|target|workspace)>"#,
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

    /// `code` up to the comma or closing bracket that ends the expression it starts
    /// with, reading past whatever a string or a nested bracket holds.
    fn expression_from(code: &str) -> &str {
        let (mut depth, mut in_string, mut escaped) = (0usize, false, false);
        for (at, ch) in code.char_indices() {
            match ch {
                _ if escaped => escaped = false,
                '\\' if in_string => escaped = true,
                '"' => in_string = !in_string,
                _ if in_string => {}
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' | ',' if depth == 0 => return &code[..at],
                ')' | ']' | '}' => depth -= 1,
                _ => {}
            }
        }
        code
    }

    /// The help text in `code`: each `help =` expression, and the body of each helper
    /// that builds one. Messages and prompt text are not read.
    fn help_text(code: &str) -> Vec<&str> {
        let starts =
            regex::Regex::new(r"\bhelp\s*=\s*|\bfn \w*_help\b[^{;]*\{").expect("help pattern");
        starts.find_iter(code).map(|start| expression_from(&code[start.end()..])).collect()
    }

    /// Fails when help text spells a command taking a target at a site the list above
    /// does not name, and when the list names a site that no longer does. A help that
    /// prints the target goes through `help_target.rs`, which is what makes any such
    /// literal a line with no target in it. §FS-rhei-errors.6
    #[test]
    fn help_target_bare_verbs_stand_only_where_listed() {
        let continuation = regex::Regex::new(r"\\\r?\n\s*").expect("continuation pattern");
        let bare = regex::Regex::new(
            r"\brhei (?:list|states|validate|show|release|next|complete|transition|seal|run|render|retry|cost|archive|verify|roster|summary|viz|budget (?:init|show|adjust|forget)|snapshot (?:list|show|gc|continue))\b",
        )
        .expect("verb pattern");

        let mut offenders = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for path in crate_sources() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_owned();
            let source = std::fs::read_to_string(&path).expect("read source file");
            let code = printed_text(&source, &continuation);
            for found in help_text(&code).iter().flat_map(|text| bare.find_iter(text)) {
                let allowed = BARE_VERB_STANDS
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
            "{} help(s) name a command with no target; print it through help_target.rs \
             (§FS-rhei-errors.1.2.1), or list the site with why the line is right:\n  {}",
            offenders.len(),
            offenders.join("\n  ")
        );
        for (file, command, _) in BARE_VERB_STANDS {
            assert!(
                seen.contains(&(*file, *command)),
                "{file} no longer prints `{command}`, so it should leave the list"
            );
        }
    }

    /// A worker's thread starts with no target, and no route reaches these helps from
    /// a test, so what is held is that each file spawning one carries it across.
    /// §FS-rhei-errors.1.2.1
    #[test]
    fn help_target_each_parallel_spawn_carries_the_target_to_its_worker() {
        let cli = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/cli");
        for name in ["run_parallel_spawn.rs", "run_parallel_program_spawn.rs"] {
            let source = std::fs::read_to_string(cli.join(name)).expect("read the spawn file");
            assert!(
                source.contains("inherit_help_target("),
                "{name} spawns a worker that does not inherit the command's help target"
            );
        }
    }
}
