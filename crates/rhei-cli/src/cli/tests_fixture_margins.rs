// A bound a test puts on a fixture is the binary's one margin unless it is the
// behaviour under test. agent-grounds/rhei#483: five tests whose 5 s and 10 s
// margins a stalled windows-latest runner crossed failed in one run, each
// reporting the timeout of an agent that had done nothing wrong.
//
// The bounds are read from the tests' own source, as the diagnostics are by
// `every_miette_site_carries_help`: a margin is a property of every test that runs
// a fixture rather than of any one of them, so one written too short tomorrow
// fails here by file, line and function.

// §REQ-cross-platform.8

    /// A bound a test may write shorter than [`FIXTURE_MARGIN`]: the part and the
    /// function it is written in, and why it is not a margin.
    struct ShortBound {
        file: &'static str,
        function: &'static str,
        why: &'static str,
    }

    /// Bounds that are the behaviour under test (§REQ-cross-platform.8.1).
    const BEHAVIOUR_BOUNDS: &[ShortBound] = &[
        ShortBound {
            file: "tests_agent_execution_validation.rs",
            function: "fake_agent_timeout_keeps_output_and_writes_footer",
            why: "the 1 s timeout is meant to fire on a fixture that sleeps past it",
        },
        ShortBound {
            file: "tests_snapshots_gc.rs",
            function: "agent_spawn_outcome_carries_resolved_timeout",
            why: "the outcome must carry back the 1 s the agent was given, and fire it",
        },
    ];

    /// Bounds no fixture runs inside: a value no spawn reads, or a wait that is
    /// the code's own (§REQ-cross-platform.8.1).
    const NO_FIXTURE_INSIDE: &[ShortBound] = &[
        ShortBound {
            file: "tests_usage_report.rs",
            function: "attempt_identity_is_stable_across_streamed_final_and_durable_reporting",
            why: "the agent is never spawned; its timeout is data",
        },
        ShortBound {
            file: "tests_headless_launch_registry.rs",
            function: "an_unwritable_registry_is_warned_about_with_its_reason_and_without_a_wait",
            why: "it times the launcher's own announcement, which spawns nothing",
        },
        ShortBound {
            file: "tests_file_locks.rs",
            function: "issue_390_a_refusal_that_outlasts_the_bound_is_reported_with_the_plan_intact",
            why: "it times the plan writer's own retries, which spawn nothing",
        },
    ];

    /// One bound a test part writes as a number.
    struct WrittenBound {
        file: String,
        line: usize,
        function: String,
        bound: Duration,
        text: String,
    }

    /// `n` of `unit`, as Rhei's durations and `Duration::from_*` spell them.
    fn written_duration(n: &str, unit: &str) -> Duration {
        let n: u64 = n.replace('_', "").parse().expect("a bound is a whole number");
        match unit {
            "ms" | "millis" => Duration::from_millis(n),
            "s" | "secs" => Duration::from_secs(n),
            "m" => Duration::from_secs(n * 60),
            "h" => Duration::from_secs(n * 3600),
            other => panic!("no unit {other}"),
        }
    }

    /// Every bound the lib binary's `tests_*.rs` parts write as a number: an
    /// agent's `timeout_secs`; an `agent_timeout`, `program_timeout`,
    /// `callback_timeout` or profile `"timeout"` in the settings and states a test
    /// writes; and a ceiling on how long something took. A bound taken from a
    /// named value is not a number here, which is how a margin passes.
    fn written_bounds() -> Vec<WrittenBound> {
        let patterns = [
            r"timeout_secs:\s*Some\(\s*(?P<n>[0-9][0-9_]*)\s*\)",
            r#"(?:\b(?:agent|program|callback)_timeout|"timeout")"?\s*:\s*"?(?P<n>[0-9][0-9_]*)(?P<unit>ms|s|m|h)\b"#,
            r"<=?\s*(?:std::time::)?Duration::from_(?P<unit>secs|millis)\(\s*(?P<n>[0-9][0-9_]*)\s*\)",
        ]
        .map(|pattern| regex::Regex::new(pattern).expect("bound pattern"));
        let function = regex::Regex::new(
            r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:const\s+)?fn\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)",
        )
        .expect("function pattern");

        let parts = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("cli");
        let mut files: Vec<_> = fs::read_dir(&parts)
            .expect("read the cli parts")
            .map(|entry| entry.expect("cli part").file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with("tests_") && name.ends_with(".rs"))
            .filter(|name| name != "tests_fixture_margins.rs")
            .collect();
        files.sort();

        let mut bounds = Vec::new();
        for file in files {
            let text = fs::read_to_string(parts.join(&file)).expect("read a test part");
            let mut current = String::from("<file scope>");
            for (index, line) in text.lines().enumerate() {
                if line.trim_start().starts_with("//") {
                    continue;
                }
                if let Some(found) = function.captures(line) {
                    current = found["name"].to_string();
                }
                for pattern in &patterns {
                    for found in pattern.captures_iter(line) {
                        let unit = found.name("unit").map_or("s", |unit| unit.as_str());
                        bounds.push(WrittenBound {
                            file: file.clone(),
                            line: index + 1,
                            function: current.clone(),
                            bound: written_duration(&found["n"], unit),
                            text: found[0].to_string(),
                        });
                    }
                }
            }
        }
        bounds
    }

    fn names(entry: &ShortBound, bound: &WrittenBound) -> bool {
        entry.file == bound.file && entry.function == bound.function
    }

    /// §REQ-cross-platform.8: no bound a test puts on a fixture is shorter than
    /// [`FIXTURE_MARGIN`], unless it is the behaviour under test or no fixture
    /// runs inside it (§REQ-cross-platform.8.1). Every bound written as a number
    /// is read, so a 5 s margin added tomorrow fails here by name, and a listed
    /// exception that no longer matches a short bound fails as stale.
    #[test]
    #[ignore = "red until #483 gives every fixture margin FIXTURE_MARGIN; implement removes this"]
    fn no_fixture_bound_is_shorter_than_the_margin_unless_it_is_the_behaviour() {
        let bounds = written_bounds();
        assert!(bounds.len() > 10, "the scan read only {} bounds; has the pattern changed?", bounds.len());
        let listed = || BEHAVIOUR_BOUNDS.iter().chain(NO_FIXTURE_INSIDE);
        let short: Vec<&WrittenBound> =
            bounds.iter().filter(|bound| bound.bound < FIXTURE_MARGIN).collect();

        let margins: Vec<String> = short
            .iter()
            .filter(|bound| !listed().any(|entry| names(entry, bound)))
            .map(|bound| {
                format!(
                    "  {}:{} in {}: {:?} `{}`",
                    bound.file, bound.line, bound.function, bound.bound, bound.text
                )
            })
            .collect();
        let stale: Vec<String> = listed()
            .filter(|entry| !short.iter().any(|bound| names(entry, bound)))
            .map(|entry| {
                format!("  {} in {}, listed because {}", entry.file, entry.function, entry.why)
            })
            .collect();

        assert!(
            margins.is_empty() && stale.is_empty(),
            "fixture bounds shorter than FIXTURE_MARGIN ({FIXTURE_MARGIN:?}) that are listed neither \
             as the behaviour nor as holding no fixture:\n{}\n\
             listed exceptions with no such bound left:\n{}\n\
             A margin takes FIXTURE_MARGIN. A bound that is the behaviour under test goes in \
             BEHAVIOUR_BOUNDS, and one no fixture runs inside goes in NO_FIXTURE_INSIDE, each \
             with its reason.",
            if margins.is_empty() { "  (none)".to_string() } else { margins.join("\n") },
            if stale.is_empty() { "  (none)".to_string() } else { stale.join("\n") },
        );
    }

    /// §REQ-cross-platform.8.2: the inherited-pipe tests pin an absence — the
    /// spawn did not wait for the pipe a grandchild still holds — so the
    /// grandchild holds it for longer than the margin. A hold the margin outlasts
    /// ends before a spawn that waited for it can be told from one that did not.
    #[test]
    #[ignore = "red until #483 holds the inherited pipe past FIXTURE_MARGIN; implement removes this"]
    fn the_inherited_pipe_is_held_past_the_margin() {
        assert!(
            INHERITED_PIPE_HOLD > FIXTURE_MARGIN,
            "the grandchild holds the pipe {INHERITED_PIPE_HOLD:?}, inside the {FIXTURE_MARGIN:?} \
             margin, so a spawn that waits for its EOF is not told apart"
        );
    }

    /// §REQ-cross-platform.8: a test build's snapshot redactor timeout bounds a
    /// Python fixture's start like any agent timeout, and no test means it to
    /// fire, so it is the margin.
    #[test]
    #[ignore = "red until #483 makes the test-build redactor timeout FIXTURE_MARGIN; implement removes this"]
    fn the_test_build_snapshot_redactor_timeout_is_the_margin() {
        assert_eq!(
            SNAPSHOT_REDACTOR_TIMEOUT, FIXTURE_MARGIN,
            "the test build's snapshot redactor timeout is a margin around a Python fixture"
        );
    }
