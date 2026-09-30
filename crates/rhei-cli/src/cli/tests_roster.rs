mod roster_unit_tests {
    use super::*;

    fn write_json(path: &Path, body: &str) {
        fs::create_dir_all(path.parent().expect("fixture parent")).expect("create fixture parent");
        fs::write(path, body).expect("write fixture");
    }

    fn layered_roster() -> (tempfile::TempDir, MergedRoster) {
        let root = tempfile::tempdir().expect("root");
        let plan_root = root.path().join("project");
        fs::create_dir_all(&plan_root).expect("project root");
        let global_path = home_dir().expect("home").join(".config/rhei/settings.json");
        write_json(
            &global_path,
            r#"{
              "agents": {
                "shared": {
                  "command": ["global"],
                  "stdin_prompt": false,
                  "session": null
                }
              },
              "models": {
                "review": {
                  "provider": "openai",
                  "model": "global-model",
                  "default_agent": "shared",
                  "agents": {
                    "shared": {
                      "args": ["--global"],
                      "autonomous_args": ["--auto"],
                      "timeout": "10m"
                    },
                    "sparse": {}
                  }
                }
              },
              "defaults": {
                "model": "review",
                "agent": "shared",
                "agent_mode": "safe",
                "agent_timeout": "20m",
                "program_timeout": "5m",
                "attempts": 3,
                "mcp_servers": ["tools"],
                "skills": ["reviewing"]
              }
            }"#,
        );
        write_json(
            &plan_root.join(PROJECT_SETTINGS_RELATIVE_PATH),
            r#"{
              "agents": {
                "shared": {"command": ["project"], "modes": {"safe": ["--safe"]}},
                "project-only": {"command": ["project-only"]}
              },
              "models": {
                "review": {
                  "model": "project-model",
                  "agents": {"shared": {"args": [], "timeout": null}}
                }
              },
              "defaults": {"agent": null, "agent_mode": null, "skills": []}
            }"#,
        );
        let roster = load_merged_roster(&plan_root, false).expect("merge roster");
        (root, roster)
    }

    /// Origins are recorded by the same wholesale and field-level decisions
    /// that merge every roster field. §FS-rhei-agents.1.1.7
    #[test]
    fn merged_roster_records_all_field_origins_and_selected_sources() {
        let _home = TempHome::new();
        let (root, roster) = layered_roster();
        let project_root = root.path().join("project");

        assert_eq!(roster.provenance.agents["shared"], RosterOrigin::Project);
        assert_eq!(roster.provenance.agents["project-only"], RosterOrigin::Project);
        assert_eq!(roster.provenance.agents["codex"], RosterOrigin::BuiltIn);
        assert_eq!(
            roster.agent_fields["shared"],
            BTreeSet::from(["command".to_string(), "modes".to_string()])
        );

        let model = &roster.provenance.models["review"];
        assert_eq!(model.fields["provider"], RosterOrigin::Global);
        assert_eq!(model.fields["model"], RosterOrigin::Project);
        assert_eq!(model.fields["default_agent"], RosterOrigin::Global);
        assert_eq!(model.agents["shared"]["args"], RosterOrigin::Project);
        assert_eq!(model.agents["shared"]["autonomous_args"], RosterOrigin::Global);
        assert_eq!(model.agents["shared"]["timeout"], RosterOrigin::Project);
        assert!(model.agents["sparse"].is_empty());

        for field in [
            "model",
            "agent_timeout",
            "program_timeout",
            "attempts",
            "mcp_servers",
        ] {
            assert_eq!(roster.provenance.defaults[field], RosterOrigin::Global);
        }
        for field in ["agent", "agent_mode", "skills"] {
            assert_eq!(roster.provenance.defaults[field], RosterOrigin::Project);
        }
        assert_eq!(
            fs::canonicalize(roster.sources.global.as_ref().expect("global source")).unwrap(),
            fs::canonicalize(home_dir().unwrap().join(".config/rhei/settings.json")).unwrap()
        );
        assert_eq!(
            fs::canonicalize(&roster.sources.project.as_ref().expect("project source").0).unwrap(),
            fs::canonicalize(project_root.join(PROJECT_SETTINGS_RELATIVE_PATH)).unwrap()
        );
    }

    /// Serialization retains explicit scalar/list clears, omits unsupplied
    /// values, and is deterministic. §FS-rhei-agents.1.1.7
    #[test]
    fn roster_json_serializes_complete_values_clears_and_omissions_deterministically() {
        let _home = TempHome::new();
        let (root, roster) = layered_roster();
        let payload = roster_json_value(&root.path().join("project"), &roster).expect("payload");
        let again = roster_json_value(&root.path().join("project"), &roster).expect("payload");

        assert_eq!(
            serde_json::to_string_pretty(&payload).unwrap(),
            serde_json::to_string_pretty(&again).unwrap()
        );
        assert_eq!(
            payload["agents"]["shared"],
            serde_json::json!({"command": ["project"], "modes": {"safe": ["--safe"]}})
        );
        assert!(payload["agents"]["pi"].get("modes").is_none());
        assert_eq!(
            payload["models"]["review"]["agents"]["shared"]["args"],
            serde_json::json!([])
        );
        assert_eq!(
            payload["models"]["review"]["agents"]["shared"]["autonomous_args"],
            serde_json::json!(["--auto"])
        );
        assert!(payload["models"]["review"]["agents"]["shared"]["timeout"].is_null());
        assert_eq!(
            payload["models"]["review"]["agents"]["sparse"],
            serde_json::json!({})
        );
        assert!(payload["defaults"]["agent"].is_null());
        assert!(payload["defaults"]["agent_mode"].is_null());
        assert_eq!(payload["defaults"]["skills"], serde_json::json!([]));
    }

    /// Text output follows the documented section order and exposes field
    /// source annotations without repeating transport detail. §FS-rhei-agents.1.1.7
    #[test]
    fn roster_text_is_ordered_and_annotates_model_and_binding_fields() {
        let _home = TempHome::new();
        let (root, roster) = layered_roster();
        let payload = roster_json_value(&root.path().join("project"), &roster).expect("payload");
        let text = render_roster_text(&payload);
        let positions = ["Sources", "Defaults", "Agents", "Models"]
            .map(|heading| text.find(heading).expect("section"));
        assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
        for needle in [
            "shared [project] modes: safe",
            "provider=\"openai\" [global]",
            "model=\"project-model\" [project]",
            "args=[] [project]",
            "timeout=null [project]",
        ] {
            assert!(text.contains(needle), "missing {needle:?}:\n{text}");
        }
    }

    /// Settings-only validation rejects structurally invalid registries before
    /// either roster renderer can run. §FS-rhei-agents.1.1.7
    #[test]
    fn roster_intrinsic_validation_covers_agents_models_and_tooling() {
        let mut settings = RheiSettings { agents: built_in_agents(), ..Default::default() };
        settings.agents.insert("empty".to_string(), CustomAgentProfile::default());
        settings.models.insert("bad".to_string(), ModelProfile::default());
        settings.mcp_servers.insert("bad".to_string(), McpServerProfile::default());
        let errors = validate_intrinsic_settings(&settings);
        for needle in ["empty 'command'", "missing required field 'provider'", "exactly one"] {
            assert!(errors.iter().any(|error| error.contains(needle)), "{errors:?}");
        }
    }

    /// A roster whose only interesting content is `defaults`, each tier's block
    /// written verbatim so a test can tell "absent", "authored", and "explicit
    /// null" apart. §FS-rhei-agents.1.3
    fn roster_with_defaults(
        global_defaults: &str,
        project_defaults: Option<&str>,
    ) -> (tempfile::TempDir, std::path::PathBuf, MergedRoster) {
        let root = tempfile::tempdir().expect("root");
        let plan_root = root.path().join("project");
        fs::create_dir_all(&plan_root).expect("project root");
        write_json(
            &home_dir().expect("home").join(".config/rhei/settings.json"),
            &format!("{{ \"defaults\": {global_defaults} }}"),
        );
        if let Some(defaults) = project_defaults {
            write_json(
                &plan_root.join(PROJECT_SETTINGS_RELATIVE_PATH),
                &format!("{{ \"defaults\": {defaults} }}"),
            );
        }
        let roster = load_merged_roster(&plan_root, false).expect("merge roster");
        (root, plan_root, roster)
    }

    /// `defaults.prices` is read from the block it sits in, carries the tier
    /// that declared it, and is rendered as authored: the roster resolves no
    /// path and canonicalizes nothing.
    /// §FS-rhei-agents.1.1.1 §FS-rhei-agents.1.1.7 §FS-rhei-cost-accounting.5.1
    #[test]
    fn roster_reads_a_machine_price_book_and_renders_it_as_authored() {
        let _home = TempHome::new();
        let (_root, plan_root, roster) = roster_with_defaults(
            r#"{ "agent_timeout": "30m", "prices": "~/books/machine.json" }"#,
            None,
        );

        assert_eq!(
            roster.provenance.defaults.get("agent_timeout"),
            Some(&RosterOrigin::Global),
            "the fixture's own block was not read"
        );
        assert_eq!(
            roster.provenance.defaults.get("prices"),
            Some(&RosterOrigin::Global),
            "defaults.prices was dropped from the block beside it: {:?}",
            roster.provenance.defaults
        );
        let payload = roster_json_value(&plan_root, &roster).expect("payload");
        assert_eq!(payload["defaults"]["prices"], serde_json::json!("~/books/machine.json"));
        let text = render_roster_text(&payload);
        assert!(
            text.contains("prices: \"~/books/machine.json\" [global]"),
            "roster text was:\n{text}"
        );
    }

    /// An authored project path replaces the machine book and an explicit
    /// `null` clears it — the ordinary `defaults` merge, and not a ceiling the
    /// machine tier holds. §FS-rhei-agents.1.3
    #[test]
    fn a_project_price_book_replaces_the_machine_one_and_null_clears_it() {
        let _home = TempHome::new();
        let (_replacing_root, plan_root, replaced) = roster_with_defaults(
            r#"{ "prices": "~/books/machine.json" }"#,
            Some(r#"{ "prices": "books/project.json" }"#),
        );
        assert_eq!(
            replaced.provenance.defaults.get("prices"),
            Some(&RosterOrigin::Project),
            "the project file did not win: {:?}",
            replaced.provenance.defaults
        );
        let payload = roster_json_value(&plan_root, &replaced).expect("payload");
        assert_eq!(payload["defaults"]["prices"], serde_json::json!("books/project.json"));

        let (_clearing_root, cleared_root, cleared) = roster_with_defaults(
            r#"{ "prices": "~/books/machine.json" }"#,
            Some(r#"{ "prices": null }"#),
        );
        assert_eq!(
            cleared.provenance.defaults.get("prices"),
            Some(&RosterOrigin::Project),
            "the explicit clear was not attributed: {:?}",
            cleared.provenance.defaults
        );
        // An explicit clear renders as `null`; an absent key renders as no key
        // at all, and the two send a reader to different files.
        // §FS-rhei-agents.1.1.7
        let payload = roster_json_value(&cleared_root, &cleared).expect("payload");
        let defaults = payload["defaults"].as_object().expect("defaults object");
        assert!(defaults.contains_key("prices"), "the clear was dropped: {defaults:?}");
        assert!(defaults["prices"].is_null(), "the clear did not render as null: {defaults:?}");
    }

    /// The path is fixed when settings merge and read only when a run prices,
    /// so a machine whose book is misspelled still merges and still prints.
    /// §FS-rhei-agents.1.3 §FS-rhei-cost-accounting.5.1
    #[test]
    fn a_price_book_path_that_names_nothing_still_merges_and_prints() {
        let _home = TempHome::new();
        let (_root, plan_root, roster) =
            roster_with_defaults(r#"{ "prices": "books/never-written.json" }"#, None);

        let payload = roster_json_value(&plan_root, &roster).expect("payload");
        assert_eq!(payload["defaults"]["prices"], serde_json::json!("books/never-written.json"));
    }
}
