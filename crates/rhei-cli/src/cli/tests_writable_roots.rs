// The writable-root grant, unit by unit: the field as settings read it, how a
// family hands it on, which directories one launch is granted and in what
// order, and where the pairs land on the command line.
//
// The end-to-end proof is `tests/e2e/codex_writable_roots_tests.rs`, which runs
// a fake Codex that writes only under its roots; these are the cases a whole
// run cannot show cheaply — a spelling kept byte for byte, a second spelling of
// one directory dropped, a pair that must not land past the separator.

// §AR-source-file-size.3 §FS-rhei-agents.2.2 §FS-rhei-agents.1.1.2

mod writable_roots_tests {
    use super::super::*;

    fn resolve(id: &str, entry: serde_json::Value) -> CustomAgentProfile {
        let mut agents = built_in_agents();
        let mut agent_fields: BTreeMap<String, BTreeSet<String>> = agents
            .iter()
            .map(|(id, profile)| (id.clone(), built_in_agent_fields(profile)))
            .collect();
        agents.insert(
            id.to_string(),
            serde_json::from_value(entry.clone()).expect("an agent entry decodes"),
        );
        agent_fields.insert(id.to_string(), raw_object_keys(&entry));
        apply_agent_family_inheritance(&mut agents, &mut agent_fields);
        agents.remove(id).expect("the entry survives the merge")
    }

    fn flag(profile: &CustomAgentProfile) -> Option<&str> {
        profile.writable_roots.as_ref().map(|adapter| adapter.path_flag.as_str())
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_string()).collect()
    }

    /// The field decodes to its one key, and a key beside it is refused, as
    /// `deny_read`'s is. §FS-rhei-agents.1.1.2
    #[test]
    fn writable_roots_decodes_its_path_flag_and_refuses_another_key() {
        let profile: CustomAgentProfile = serde_json::from_value(serde_json::json!({
            "command": ["agent"],
            "writable_roots": { "path_flag": "--grant" },
        }))
        .expect("a writable_roots block decodes");
        assert_eq!(flag(&profile), Some("--grant"));

        let absent: CustomAgentProfile =
            serde_json::from_value(serde_json::json!({ "command": ["agent"] })).unwrap();
        assert_eq!(absent.writable_roots, None);
        assert!(
            serde_json::to_value(&absent).unwrap().get("writable_roots").is_none(),
            "an absent block is not serialized"
        );

        let extra = serde_json::from_value::<CustomAgentProfile>(serde_json::json!({
            "command": ["agent"],
            "writable_roots": { "path_flag": "--grant", "roots": ["/x"] },
        }));
        assert!(extra.is_err(), "writable_roots has one field");
    }

    /// An empty flag is a settings error naming the field's full path.
    /// §FS-rhei-agents.1.1.2
    #[test]
    fn an_empty_writable_roots_path_flag_is_a_settings_error() {
        for path_flag in ["", "  "] {
            let mut settings = RheiSettings::default();
            settings.agents.insert(
                "cdx".to_string(),
                resolve(
                    "cdx",
                    serde_json::json!({
                        "family": "codex",
                        "writable_roots": { "path_flag": path_flag },
                    }),
                ),
            );
            let errors = validate_intrinsic_settings(&settings);
            assert!(
                errors.iter().any(|error| error.contains("agents.cdx.writable_roots.path_flag")),
                "{path_flag:?}: {errors:?}"
            );
        }

        let mut settings = RheiSettings::default();
        settings
            .agents
            .insert("cdx".to_string(), resolve("cdx", serde_json::json!({ "family": "codex" })));
        assert!(validate_intrinsic_settings(&settings).is_empty());
    }

    /// `codex` is the only built-in with the grant, and `yolo` stays its first
    /// mode, so a bare `codex` still resolves to it. §FS-rhei-agents.2 §FS-rhei-agents.1.4.1
    #[test]
    fn only_the_built_in_codex_declares_writable_roots_and_yolo_stays_first() {
        let agents = built_in_agents();
        for (id, profile) in &agents {
            let expected = (id == "codex").then_some("--add-dir");
            assert_eq!(flag(profile), expected, "built-in {id}");
        }
        let codex = &agents["codex"];
        let names: Vec<&str> = codex.modes.keys().map(String::as_str).collect();
        assert_eq!(names, ["yolo", "workspace"]);
        assert_eq!(
            codex.modes["yolo"],
            strings(&[
                "--sandbox",
                "danger-full-access",
                "--skip-git-repo-check",
                "-c",
                "approval_policy=\"never\""
            ])
        );
        assert_eq!(
            codex.modes["workspace"],
            strings(&[
                "--sandbox",
                "workspace-write",
                "--skip-git-repo-check",
                "-c",
                "approval_policy=\"never\"",
                "-c",
                "sandbox_workspace_write.network_access=true",
            ])
        );
        assert!(built_in_agent_fields(codex).contains("writable_roots"));
    }

    /// A `family: codex` entry inherits the grant, writing only `modes` leaves
    /// it inherited, a written block replaces it whole, and a same-id `codex`
    /// without `family` replaces the built-in and gets none. §FS-rhei-agents.1.1.2
    #[test]
    fn writable_roots_follows_the_family_and_is_replaced_whole() {
        let inherited = resolve("cdx", serde_json::json!({ "family": "codex" }));
        assert_eq!(flag(&inherited), Some("--add-dir"));
        assert!(inherited.modes.contains_key("workspace"));

        let own_modes = resolve(
            "cdx",
            serde_json::json!({
                "family": "codex",
                "modes": { "xhigh": ["--sandbox", "workspace-write"] },
            }),
        );
        assert_eq!(flag(&own_modes), Some("--add-dir"), "writing modes does not drop the grant");
        assert_eq!(own_modes.modes.len(), 1, "modes are still replaced whole");

        let replaced = resolve(
            "cdx",
            serde_json::json!({ "family": "codex", "writable_roots": { "path_flag": "--grant" } }),
        );
        assert_eq!(flag(&replaced), Some("--grant"));

        let same_id = resolve("codex", serde_json::json!({ "command": ["codex", "exec"] }));
        assert_eq!(flag(&same_id), None, "a same-id entry replaces the built-in wholesale");
        assert!(same_id.modes.is_empty());
    }

    /// Roots keep their order, a second spelling of one directory is dropped
    /// for the earlier one, and nothing about a spelling is rewritten. The
    /// names are ones every platform allows. §FS-rhei-agents.2.2 §REQ-cross-platform.3
    #[test]
    fn roots_keep_their_order_and_spelling_and_drop_a_second_spelling() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let plan = dir.path().join("plans dir ü").join("it's 'quoted'");
        let checkout = dir.path().join("checkout");
        std::fs::create_dir_all(&plan).unwrap();
        std::fs::create_dir_all(&checkout).unwrap();
        let trailing = PathBuf::from(format!("{}/", checkout.display()));
        let dotted = checkout.join(".");
        let state = dir.path().join("state").join("rhei");

        let roots = distinct_absolute_roots([
            plan.clone(),
            plan.clone(),
            checkout.clone(),
            trailing,
            dotted,
            state.clone(),
            state.clone(),
        ]);
        assert_eq!(roots, vec![plan.clone(), checkout.clone(), state.clone()]);

        let (args, left_out) = writable_root_args("--add-dir", &roots);
        assert_eq!(
            args,
            vec![
                "--add-dir".to_string(),
                plan.display().to_string(),
                "--add-dir".to_string(),
                checkout.display().to_string(),
                "--add-dir".to_string(),
                state.display().to_string(),
            ]
        );
        assert!(left_out.is_empty());
        assert!(args[1].contains("plans dir ü") && args[1].contains("it's 'quoted'"));
    }

    /// A double quote is kept too, where the file system allows it in a name;
    /// Unix only, because Windows refuses `"` in one. §FS-rhei-agents.2.2
    /// §REQ-cross-platform.3
    #[cfg(unix)]
    #[test]
    fn a_double_quote_in_a_unix_root_is_kept() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let plan = dir.path().join("plans dir ü").join("it's \"quoted\"");
        std::fs::create_dir_all(&plan).unwrap();
        let roots = distinct_absolute_roots([plan.clone(), plan.join(".")]);
        assert_eq!(roots, vec![plan.clone()]);
        let (args, _) = writable_root_args("--add-dir", &roots);
        assert_eq!(args, vec!["--add-dir".to_string(), plan.display().to_string()]);
        assert!(args[1].contains("it's \"quoted\""));
    }

    /// A root with no Unicode spelling is left out of the pairs, never granted
    /// under a lossy name, and the launch warns naming it; a git directory
    /// reported in such bytes keeps them. Unix only, because a Windows name is
    /// not bytes; the odd name is never created, since not every Unix file
    /// system accepts it. §FS-rhei-agents.2.2 §REQ-cross-platform.3
    #[cfg(unix)]
    #[test]
    fn a_root_with_no_unicode_spelling_is_left_out_with_a_warning() {
        use std::os::unix::ffi::OsStrExt;
        let dir = tempfile::tempdir().expect("tmpdir");
        let plain = dir.path().join("plan");
        std::fs::create_dir_all(&plain).unwrap();
        let odd = dir.path().join(std::ffi::OsStr::from_bytes(b"state\xfd")).join(".git");

        let mut stdout = plain.as_os_str().as_bytes().to_vec();
        stdout.push(b'\n');
        stdout.extend_from_slice(odd.as_os_str().as_bytes());
        stdout.extend_from_slice(b"\r\n");
        assert_eq!(git_dirs_from_rev_parse(&stdout), vec![plain.clone(), odd.clone()]);

        let (args, left_out) = writable_root_args("--add-dir", &[plain.clone(), odd.clone()]);
        assert_eq!(args, vec!["--add-dir".to_string(), plain.display().to_string()]);
        assert_eq!(left_out, vec![odd.clone()]);
        let warning = left_out_root_warning(&odd);
        assert!(warning.contains(r"state\xFD"), "{warning}");

        // Through the adapter: the launch's own root has the odd name.
        let launch = LaunchRoots { rhei_root: &odd, project_root: &odd, checkout_root: &plain };
        let sink = Arc::new(Messages::default());
        let argv = composed(&agent_with_path_adapters(
            &codex_agent(),
            &ResolvedExclusions::default(),
            launch,
            &(sink.clone() as Arc<dyn rhei_tui::EventSink>),
        ));
        assert!(argv.iter().all(|arg| !arg.contains('\u{FFFD}')), "no lossy spelling: {argv:?}");
        let granted = plain.display().to_string();
        assert!(argv.windows(2).any(|w| w == ["--add-dir", granted.as_str()]), "{argv:?}");
        let warned = sink.0.lock().unwrap().clone();
        assert_eq!(warned, vec![left_out_root_warning(&odd)]);
    }

    /// One directory reached through a link is one root, kept as first spelled.
    /// §FS-rhei-agents.2.2
    #[cfg(unix)]
    #[test]
    fn a_link_to_an_earlier_root_is_the_same_root() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let real = dir.path().join("real");
        std::fs::create_dir_all(&real).unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert_eq!(distinct_absolute_roots([link.clone(), real.clone()]), vec![link]);
    }

    /// A relative root — a plan named relative to where `rhei run` started —
    /// is made absolute against that directory and not otherwise rewritten.
    /// §FS-rhei-agents.2.2
    #[test]
    fn a_relative_root_is_made_absolute_against_the_working_directory() {
        let relative = PathBuf::from("plans dir ü").join("plan");
        let roots = distinct_absolute_roots([relative.clone()]);
        let cwd = std::env::current_dir().expect("cwd");
        assert_eq!(roots, vec![cwd.join(&relative)]);
        assert!(roots[0].is_absolute());
    }

    /// No `~` or `$HOME` expansion: such a root is a relative path like any
    /// other, joined to the working directory as written. §FS-rhei-agents.2.2
    #[test]
    fn a_tilde_or_home_variable_is_not_expanded() {
        let cwd = std::env::current_dir().expect("cwd");
        let roots = distinct_absolute_roots([PathBuf::from("~/.cargo"), PathBuf::from("$HOME/x")]);
        assert_eq!(roots, vec![cwd.join("~/.cargo"), cwd.join("$HOME/x")]);
    }

    /// A backslash is part of a Unix file name, so it is kept as written.
    /// §FS-rhei-agents.2.2
    #[cfg(unix)]
    #[test]
    fn a_backslash_in_a_unix_root_is_kept() {
        let root = PathBuf::from(r"/srv/plans\ü\plan");
        assert_eq!(distinct_absolute_roots([root.clone()]), vec![root.clone()]);
        assert_eq!(
            writable_root_args("--add-dir", &[root]).0,
            strings(&["--add-dir", r"/srv/plans\ü\plan"])
        );
    }

    /// Windows separators stay as resolved, and both spellings of one
    /// directory are one root. §FS-rhei-agents.2.2 §REQ-cross-platform
    #[cfg(windows)]
    #[test]
    fn windows_separators_are_kept_and_one_directory_is_one_root() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let native = dir.path().join("plans dir ü");
        std::fs::create_dir_all(&native).unwrap();
        let forward = PathBuf::from(native.display().to_string().replace('\\', "/"));
        assert_eq!(distinct_absolute_roots([native.clone(), forward]), vec![native.clone()]);
        assert_eq!(
            writable_root_args("--add-dir", &[native.clone()]).0,
            vec!["--add-dir".to_string(), native.display().to_string()]
        );
    }

    /// One launch: the rhei root, the project around it, the checkout, its git
    /// directory and common directory, and `rhei/` under each state base, in
    /// that order and once each. §FS-rhei-agents.2.2
    #[test]
    fn a_launch_grants_its_roots_in_order() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let project = dir.path().join("project");
        let member = project.join("alpha");
        let checkout = dir.path().join("wt");
        let common = dir.path().join("main").join(".git");
        let git_dir = common.join("worktrees").join("wt");
        let state = dir.path().join("state").join("rhei");
        for path in [&member, &checkout, &git_dir, &state] {
            std::fs::create_dir_all(path).unwrap();
        }
        let launch =
            LaunchRoots { rhei_root: &member, project_root: &project, checkout_root: &checkout };
        let git_dirs =
            git_dirs_from_rev_parse(format!("{}\n{}\n", git_dir.display(), common.display()).as_bytes());
        // Both resolvers normally answer with one base, which is granted once.
        let roots =
            ordered_writable_roots(launch, git_dirs.clone(), vec![state.clone(), state.clone()]);
        assert_eq!(
            roots,
            vec![member.clone(), project.clone(), checkout.clone(), git_dir, common, state.clone()]
        );

        // A standalone rhei is its own project, and outside git there are no
        // git directories; a plain checkout's git dir is its common dir.
        let standalone =
            LaunchRoots { rhei_root: &member, project_root: &member, checkout_root: &checkout };
        let plain = checkout.join(".git");
        let dirs = git_dirs_from_rev_parse(format!("{0}\n{0}\n", plain.display()).as_bytes());
        assert_eq!(dirs.len(), 2);
        let roots = ordered_writable_roots(standalone, dirs, vec![state.clone()]);
        assert_eq!(roots, vec![member.clone(), checkout.clone(), plain, state.clone()]);
        let roots = ordered_writable_roots(standalone, Vec::new(), vec![state.clone()]);
        assert_eq!(roots, vec![member, checkout, state]);
    }

    /// A launch outside any repository adds no git directory, and the state
    /// base it grants is the one the root guards resolve under.
    /// §FS-rhei-agents.2.2 §FS-rhei-recover.4
    #[test]
    fn a_launch_grants_the_root_guard_state_base() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let state = rhei_core::root_access::guard_state_base().expect("state base").join("rhei");
        let launch = LaunchRoots {
            rhei_root: dir.path(),
            project_root: dir.path(),
            checkout_root: dir.path(),
        };
        let roots = launch_writable_roots(launch);
        assert!(roots_are_one_directory(&roots[0], dir.path()), "{roots:?}");
        assert!(roots.iter().any(|root| roots_are_one_directory(root, &state)), "{roots:?}");
    }

    /// Every message a launch emits, as text.
    #[cfg(unix)]
    #[derive(Default)]
    struct Messages(std::sync::Mutex<Vec<String>>);

    #[cfg(unix)]
    impl rhei_tui::EventSink for Messages {
        fn emit(&self, event: rhei_tui::RunEvent) {
            if let rhei_tui::RunEvent::Message { text, .. } = event {
                self.0.lock().unwrap().push(text);
            }
        }
    }

    fn quiet() -> Arc<dyn rhei_tui::EventSink> {
        Arc::new(rhei_tui::NullSink)
    }

    /// The built-in `codex` profile in its `workspace` mode, read on stdin.
    #[cfg(unix)]
    fn codex_agent() -> ResolvedAgent {
        let mut profile = resolve("cdx", serde_json::json!({ "family": "codex" }));
        profile.stdin_prompt = true;
        ResolvedAgent {
            agent: AgentConfig::from("cdx"),
            profile,
            mode: Some("workspace".to_string()),
            target: None,
            model: None,
            model_provider: None,
            model_name: None,
            timeout_secs: Some(60),
            autonomous_args: Vec::new(),
        }
    }

    fn composed(resolved: &ResolvedAgent) -> Vec<String> {
        let runtime_dir = tempfile::tempdir().expect("tmpdir");
        build_agent_command(
            resolved,
            "prompt",
            Path::new("/tmp/workspace"),
            None,
            None,
            "task-1",
            "working",
            1,
            1,
            &ResolvedTooling::default(),
            runtime_dir.path(),
            &[],
        )
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect()
    }

    /// The pairs follow the `deny_read` pairs and precede the mode flags and
    /// the separator; mode tokens, a grant written into a mode included, pass
    /// through literally. §FS-rhei-agents.2.2
    #[test]
    fn writable_root_pairs_follow_deny_read_and_precede_the_mode_and_separator() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let plan = dir.path().join("plan");
        std::fs::create_dir_all(&plan).unwrap();
        let mut profile = resolve(
            "cdx",
            serde_json::json!({
                "family": "codex",
                "command": ["codex", "exec"],
                "deny_read": { "path_flag": "--deny" },
                "modes": { "workspace": ["--sandbox", "workspace-write", "--add-dir", "~/.cargo"] },
            }),
        );
        profile.stdin_prompt = true;
        let resolved = ResolvedAgent {
            agent: AgentConfig::from("cdx"),
            profile,
            mode: Some("workspace".to_string()),
            target: None,
            model: None,
            model_provider: None,
            model_name: None,
            timeout_secs: Some(60),
            autonomous_args: Vec::new(),
        };
        let excluded = dir.path().join("secret");
        let exclusions = ResolvedExclusions {
            entries: vec![ResolvedExclusion {
                authored: "checkout=secret".into(),
                logical: excluded.clone(),
                canonical: excluded.clone(),
                recursive: false,
            }],
            ..Default::default()
        };
        let launch = LaunchRoots { rhei_root: &plan, project_root: &plan, checkout_root: &plan };
        let argv = composed(&agent_with_path_adapters(&resolved, &exclusions, launch, &quiet()));

        let at = |needle: &str| argv.iter().position(|arg| arg == needle).unwrap();
        let separator = at("--");
        let sandbox = at("--sandbox");
        let deny = at("--deny");
        let first_root = at("--add-dir");
        assert!(deny < first_root, "{argv:?}");
        assert_eq!(argv[first_root + 1], plan.display().to_string(), "{argv:?}");
        let root_flags: Vec<usize> = argv
            .iter()
            .enumerate()
            .filter(|(_, arg)| *arg == "--add-dir")
            .map(|(i, _)| i)
            .collect();
        // Every computed pair is ahead of the mode; the mode's own grant follows it.
        let computed: Vec<&usize> = root_flags.iter().filter(|index| **index < sandbox).collect();
        assert!(!computed.is_empty(), "{argv:?}");
        assert!(root_flags.iter().all(|index| *index < separator), "no root past --: {argv:?}");
        assert!(
            argv.windows(4).any(|w| w == ["--sandbox", "workspace-write", "--add-dir", "~/.cargo"]),
            "mode tokens pass through literally: {argv:?}"
        );
        assert_eq!(argv.last().map(String::as_str), Some("--"));

        // The same profile with no grant gets no root arguments at all.
        let mut bare = resolved.clone();
        bare.profile.writable_roots = None;
        let argv = composed(&agent_with_path_adapters(&bare, &exclusions, launch, &quiet()));
        assert_eq!(argv.iter().filter(|arg| *arg == "--add-dir").count(), 1, "{argv:?}");
    }
}
