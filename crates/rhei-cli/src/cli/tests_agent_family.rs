// What a declared family is, unit by unit: the fallback the merge applies, the
// accessor every behavioural branch now reads, and the two artifact readers
// that had to learn the word.
//
// The end-to-end proof is `tests/e2e/agent_family_tests.rs`; these are the
// cases a whole run cannot show cheaply — a written `false` beating an
// inherited `true`, a written empty `modes` leaving none, and a record from
// before the field existed.

// §AR-source-file-size.3 §FS-rhei-agents.1.1.2

mod agent_family {
    use super::super::*;

    /// One entry, merged the way `load_merged_roster` merges it: the profile
    /// the registry ends up with, and the field set `rhei roster` reports.
    /// §FS-rhei-agents.1.3
    fn resolve(id: &str, entry: serde_json::Value) -> (CustomAgentProfile, BTreeSet<String>) {
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
        (
            agents.remove(id).expect("the entry survives the merge"),
            agent_fields.remove(id).expect("the entry has a field set"),
        )
    }

    /// A field the entry writes wins; a field it omits is the family's.
    // §FS-rhei-agents.1.1.2
    #[test]
    fn a_written_field_wins_and_an_omitted_one_is_inherited() {
        let (profile, supplied) = resolve(
            "cld",
            serde_json::json!({ "family": "codex", "command": ["wrapper"] }),
        );
        assert_eq!(profile.command, vec!["wrapper".to_string()]);
        assert_eq!(profile.mcp_flag.as_deref(), Some("--mcp"));
        assert_eq!(profile.model_flag.as_deref(), Some("--model"));
        // The roster reports the resolved profile, so an inherited field is in
        // its field set beside the written ones. §FS-rhei-agents.1.1.7
        assert!(supplied.contains("family"));
        assert!(supplied.contains("command"));
        assert!(supplied.contains("mcp_flag"));
    }

    /// Presence is read from the document, so a written `false` overrides an
    /// inherited `true` rather than being indistinguishable from silence.
    // §FS-rhei-agents.1.1.2
    #[test]
    fn a_written_false_beats_an_inherited_true() {
        let (inherited, _) = resolve("cld", serde_json::json!({ "family": "codex" }));
        assert!(inherited.stdin_prompt, "the codex family pipes its prompt");

        let (written, _) =
            resolve("cld", serde_json::json!({ "family": "codex", "stdin_prompt": false }));
        assert!(!written.stdin_prompt, "a written false is not silence");
    }

    /// `command` is required only when `family` is absent: a bare family is a
    /// complete profile, that built-in under another id.
    // §FS-rhei-agents.1.1.2
    #[test]
    fn an_omitted_command_is_inherited_whole() {
        let (profile, supplied) = resolve("gem", serde_json::json!({ "family": "gemini" }));
        assert_eq!(profile.command, vec!["gemini".to_string()]);
        assert!(supplied.contains("command"));
    }

    /// `modes` is replaced whole, never merged: a written one-key map is the
    /// only mode, and a written empty map leaves none.
    // §FS-rhei-agents.1.1.2
    #[test]
    fn modes_are_replaced_whole_and_an_empty_map_leaves_none() {
        let (replaced, _) = resolve(
            "cld",
            serde_json::json!({ "family": "codex", "modes": { "yolo": ["--wide-open"] } }),
        );
        assert_eq!(replaced.modes.len(), 1);
        assert_eq!(replaced.modes["yolo"], vec!["--wide-open".to_string()]);

        let (cleared, supplied) =
            resolve("cld", serde_json::json!({ "family": "codex", "modes": {} }));
        assert!(cleared.modes.is_empty(), "a written empty map is not an omission");
        assert!(supplied.contains("modes"), "and the roster still reports it");
    }

    /// `effort` and `session` are replaced whole too — `session` above all,
    /// because a wrapper usually exists to move the configuration home the
    /// family's block names. §FS-rhei-agents.1.1.2
    #[test]
    fn effort_and_session_replace_whole() {
        let (inherited, _) = resolve("cdx", serde_json::json!({ "family": "codex" }));
        assert_eq!(
            inherited.session.as_ref().and_then(|block| block["layout"]["dir_template"].as_str()),
            Some("~/.codex/sessions")
        );
        assert!(inherited.effort.is_some());

        let (restated, _) = resolve(
            "cdx",
            serde_json::json!({
                "family": "codex",
                "session": { "layout": { "kind": "FlatById", "dir_template": "~/.cdx/sessions" } },
                "effort": { "values": { "low": "low" }, "args": ["--effort", "{value}"] },
            }),
        );
        assert_eq!(
            restated.session.as_ref().and_then(|block| block["layout"]["dir_template"].as_str()),
            Some("~/.cdx/sessions")
        );
        assert_eq!(
            restated.effort.as_ref().map(|effort| effort.values.len()),
            Some(1),
            "a written effort block is the whole block"
        );
    }

    /// An entry whose id is the built-in it names inherits: it is the one case
    /// where wholesale replacement and `family` could both claim the profile,
    /// and the operator wrote `family`. §FS-rhei-agents.2
    #[test]
    fn an_entry_may_declare_the_built_in_it_replaces() {
        let (profile, _) = resolve(
            "claude-code",
            serde_json::json!({ "family": "claude-code", "command": ["wrapper"] }),
        );
        assert_eq!(profile.command, vec!["wrapper".to_string()]);
        assert_eq!(profile.prompt_flag.as_deref(), Some("-p"));
    }

    /// An unrecognized family inherits nothing; validation is what reports it,
    /// so the merge neither guesses nor fails. §FS-rhei-agents.1.1.2
    #[test]
    fn an_unknown_family_inherits_nothing_and_is_a_validation_error() {
        let (profile, _) =
            resolve("cld", serde_json::json!({ "family": "claude", "command": ["wrapper"] }));
        assert_eq!(profile.prompt_flag, None);

        let mut settings = RheiSettings::default();
        settings.agents.insert("cld".to_string(), profile);
        let errors = validate_intrinsic_settings(&settings);
        assert!(
            errors.iter().any(|error| error.contains("agent 'cld' declares family 'claude'")),
            "{errors:?}"
        );
        assert!(errors.iter().any(|error| error.contains("claude-code, codex")), "{errors:?}");
    }

    /// A built-in agent is its own family, and a profile that declares none is
    /// itself. §FS-rhei-agents.1.1.2
    #[test]
    fn every_built_in_is_its_own_family_and_a_bare_profile_is_itself() {
        for (id, profile) in built_in_agents() {
            assert_eq!(resolved_agent_family(&profile, &id), id);
        }
        let bare = CustomAgentProfile { command: vec!["wrapper".to_string()], ..Default::default() };
        assert_eq!(resolved_agent_family(&bare, "cld"), "cld");

        let declared = CustomAgentProfile {
            family: Some("claude-code".to_string()),
            ..Default::default()
        };
        assert_eq!(resolved_agent_family(&declared, "cld"), "claude-code");
    }

    /// Extraction is keyed on the family: the three that have extractors, the
    /// three that do not, and a profile that declared none.
    // §FS-rhei-cost-accounting.4
    #[test]
    fn extractor_resolution_follows_the_family() {
        assert_eq!(agent_usage_extractor("claude-code"), Some(AgentUsageExtractor::Claude));
        assert_eq!(agent_usage_extractor("codex"), Some(AgentUsageExtractor::Codex));
        assert_eq!(agent_usage_extractor("pi"), Some(AgentUsageExtractor::Pi));
        for unmeasured in ["gemini", "cursor", "kilocode"] {
            assert_eq!(agent_usage_extractor(unmeasured), None, "{unmeasured}");
        }
        // The compatibility floor: an id that is nobody's family.
        assert_eq!(agent_usage_extractor("cld"), None);
    }

    /// The record's family is provenance a reader may or may not find, and it
    /// never decides the convention — that is the record's own field.
    // §FS-rhei-cost-accounting.3 §FS-rhei-cost-accounting.3.6
    #[test]
    fn a_record_reads_the_same_with_and_without_a_family() {
        let base = serde_json::json!({
            "schema": ACCOUNTING_INVOCATION_SCHEMA,
            "invocation_id": "1::work::cld::visit-1",
            "task_id": "1",
            "state": "work",
            "visit": 1,
            "agent": "cld",
            "started_at": "2026-09-28T10:00:00Z",
            "ended_at": "2026-09-28T10:00:01Z",
            "extraction_status": "measured",
            "scope": "aggregate-agent-process",
            "token_convention": TOKEN_CONVENTION_INCLUDES_CACHE,
            "tokens": {},
            "pricing": { "status": "unpriced" },
        });
        let without: AccountingInvocationRecord =
            serde_json::from_value(base.clone()).expect("a record without the field parses");
        assert_eq!(without.agent_family, None);
        assert_eq!(record_token_convention(&without), TokenConvention::IncludesCache);

        let mut raw = base;
        raw["agent_family"] = serde_json::json!("claude-code");
        raw["token_convention"] = serde_json::json!(TOKEN_CONVENTION_EXCLUDES_CACHE);
        let with: AccountingInvocationRecord =
            serde_json::from_value(raw).expect("a record with the field parses");
        assert_eq!(with.agent_family.as_deref(), Some("claude-code"));
        assert_eq!(with.agent, "cld", "the family never displaces the profile's own id");
        // Stated wins over anything the family might have implied.
        assert_eq!(record_token_convention(&with), TokenConvention::ExcludesCache);
    }
}
