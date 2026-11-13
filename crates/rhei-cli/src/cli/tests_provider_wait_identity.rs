mod provider_wait_identity {
    //! The provider a `pi` outage wait is keyed on when the invocation resolves
    //! none, and the refusal recognition it must not widen (agent-grounds/rhei#664).
    // §FS-rhei-agents.2.5 §FS-rhei-agents.2.3 §FS-rhei-run.3.3
    use super::*;

    fn settings() -> RheiSettings {
        RheiSettings { agents: built_in_agents(), ..RheiSettings::default() }
    }

    fn resolved(target: &str) -> ResolvedAgent {
        resolve_target_agent(target, None, &settings()).expect("built-in target")
    }

    fn identity(agent: &str, provider: &str) -> Option<ProviderIdentity> {
        Some(ProviderIdentity { agent: agent.to_string(), provider: provider.to_string() })
    }

    fn key(resolved: &ResolvedAgent) -> Option<ProviderIdentity> {
        resolved_provider_identity(resolved)
    }

    /// A model profile whose concrete model is pi's own `<provider>/<model>`
    /// reference and which names no `provider`.
    fn sol_profile(provider: Option<&str>) -> ModelProfile {
        ModelProfile {
            provider: provider.map(str::to_string),
            model: Some("openai-codex/gpt-6.1-sol".to_string()),
            default_agent: Some("pi".to_string()),
            ..ModelProfile::default()
        }
    }

    fn status(code: i32) -> std::process::ExitStatus {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            std::process::ExitStatus::from_raw(code << 8)
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::ExitStatusExt;
            std::process::ExitStatus::from_raw(code as u32)
        }
    }

    /// The table of §FS-rhei-agents.2.5: the text before the first `/` of the
    /// model rhei hands pi keys the wait when the invocation resolves no
    /// provider; a resolved provider wins; no `/` or an empty half, or another
    /// family, keys nothing. §FS-rhei-agents.2.5 §FS-rhei-run.3.3
    #[test]
    fn a_pi_model_reference_keys_the_wait_when_no_provider_is_resolved() {
        let reference = resolved("pi:openai-codex/gpt-6.1-sol");
        assert_eq!(key(&reference), identity("pi", "openai-codex"), "the incident spelling");
        let respelled = resolved("pi:openai-codex:openai-codex/gpt-6.1-sol");
        assert_eq!(key(&reference), key(&respelled), "both spellings share one wait");

        let routed = resolved("pi:openrouter/vendor/model");
        assert_eq!(key(&routed), identity("pi", "openrouter"), "split at the first `/`");
        let segment = resolved("pi:openai:openai-codex/gpt-6.1-sol");
        assert_eq!(key(&segment), identity("pi", "openai"), "the resolved provider wins");

        // Only the derivation is new: the target's own provider stays unset.
        assert_eq!(reference.model_provider, None, "model_provider is untouched");

        // A `**Model:**` override keeps the target's (absent) provider and hands
        // pi the profile's model, whatever provider the profile names.
        // §FS-rhei-agents.1.4 §FS-rhei-agents.1.5
        let mut with_profile = settings();
        with_profile.models.insert("sol".to_string(), sol_profile(Some("openai")));
        let overridden =
            resolve_target_agent_with_overrides("pi:base", None, &with_profile, None, Some("sol"))
                .expect("an overridden target");
        assert_eq!(key(&overridden), identity("pi", "openai-codex"), "derived from the override");

        // A legacy model profile with a `<p>/<m>` model and no provider.
        let mut legacy = settings();
        legacy.models.insert("sol".to_string(), sol_profile(None));
        legacy.defaults.model = Some("sol".to_string());
        let profiled = resolve_legacy_agent_with_model(None, &legacy, &default_run_options(), None)
            .expect("resolves")
            .expect("an agent");
        assert_eq!(profiled.family(), "pi", "the profile's default agent");
        assert_eq!(key(&profiled), identity("pi", "openai-codex"), "derived from the profile");
    }

    /// What keys nothing, before and after the change. §FS-rhei-agents.2.5
    #[test]
    fn a_pi_model_with_no_provider_half_or_another_family_keys_nothing() {
        assert_eq!(key(&resolved("pi:gpt-6.1-sol")), None, "no `/`");
        for model in ["/gpt-6.1-sol", "openai-codex/"] {
            let mut empty_half = resolved("pi:gpt-6.1-sol");
            empty_half.model = Some(model.to_string());
            empty_half.model_name = Some(model.to_string());
            assert_eq!(key(&empty_half), None, "an empty half: {model}");
        }
        assert_eq!(key(&resolved("claude-code:openai-codex/gpt-6.1-sol")), None, "not pi");
    }

    /// The derived provider keys a wait and never a refusal: a valid session
    /// limit from `pi:openai/gpt-5` stays an ordinary failure, while the same
    /// line from `pi:openai:gpt-5` parks as it does today. Passes before the
    /// fix too; it guards against fixing #664 by widening refusal recognition.
    /// §FS-rhei-agents.2.3 §FS-rhei-agents.2.5
    #[test]
    fn a_provider_derived_for_a_pi_wait_is_not_a_refusal_provider() {
        let observed = std::time::UNIX_EPOCH + Duration::from_secs(1_789_579_200);
        let line = ["You've hit your session limit · resets 10:20pm (Europe/Zurich)".to_string()];
        let refusal = |target: &str| {
            classify_provider_limit(&resolved(target), status(1), false, false, &line, observed)
        };
        assert_eq!(refusal("pi:openai/gpt-5"), None, "a derived provider is never tested");
        let declared = refusal("pi:openai:gpt-5").expect("a declared provider parks");
        assert_eq!(declared.identity, identity("pi", "openai").unwrap());
    }
}
