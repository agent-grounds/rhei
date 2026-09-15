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
}
