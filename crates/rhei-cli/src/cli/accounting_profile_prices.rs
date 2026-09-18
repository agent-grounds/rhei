// Building the effective run-start price book from selected model profiles.

// §AR-source-file-size.3 §FS-rhei-cost-accounting.5.1

#[derive(Clone)]
enum EffectivePriceSource {
    Profile {
        prices: ModelProfilePrices,
        profiles: Vec<String>,
    },
    Builtin(PriceBookEntry),
    Unpriced,
}

#[derive(Clone)]
struct EffectivePricePair {
    source: EffectivePriceSource,
    labels: Vec<String>,
}

/// Resolve the invocations which this run can select from the tasks' current
/// states before any execution surface starts. §FS-rhei-cost-accounting.5.1
fn selected_run_invocations(
    loaded: &LoadedPlan,
    machines: &ExecutionMachines,
    settings: &RheiSettings,
    opts: &RunOptions,
    scope: &RheiScope,
) -> MietteResult<Vec<ResolvedAgent>> {
    if opts.no_agent() {
        return Ok(Vec::new());
    }
    let mut selected = Vec::new();
    for task in &loaded.rhei.tasks {
        if !task_in_rhei_scope(scope, &task.id.to_string()) {
            continue;
        }
        let machine = machines.for_task(&task.id);
        let state = normalized_state_name(task.state.as_str(), machine);
        let Some(definition) = machine.states.get(&state) else { continue };
        if definition.terminal || definition.gating || definition.program.is_some() {
            continue;
        }
        selected.extend(resolve_agent_invocations_for_task(
            machine,
            &state,
            settings,
            opts,
            Some(task),
        )?);
    }
    Ok(selected)
}

/// Compose profile rates with exact built-in and unpriced fallbacks. `None`
/// preserves the built-in book unchanged when no selected profile has rates.
/// §FS-rhei-cost-accounting.5.1
fn profile_price_book(
    settings: &RheiSettings,
    invocations: &[ResolvedAgent],
) -> MietteResult<Option<PriceBook>> {
    let has_profile_prices = invocations.iter().any(|resolved| {
        resolved
            .model_profile_id()
            .and_then(|id| settings.models.get(id))
            .and_then(|profile| profile.prices.as_ref())
            .is_some()
    });
    if !has_profile_prices {
        return Ok(None);
    }

    let builtin = builtin_price_book();
    let mut pairs: BTreeMap<(String, String), EffectivePricePair> = BTreeMap::new();
    for resolved in invocations {
        let provider = resolved.model_provider.as_deref();
        let model = resolved.model_name.as_deref().or(resolved.model.as_deref());
        let profile_id = resolved.model_profile_id();
        let profile = profile_id.and_then(|id| settings.models.get(id));
        let profile_prices = profile.and_then(|item| item.prices.as_ref());
        if let (Some(id), Some(_)) = (profile_id, profile_prices) {
            let declared_provider = profile.and_then(|item| item.provider.as_deref()).unwrap_or("");
            let declared_model = profile.and_then(|item| item.model.as_deref()).unwrap_or("");
            if provider != Some(declared_provider) || model != Some(declared_model) {
                return Err(miette!(
                    help = "use a profile whose declared provider/model pair matches the final execution identity, or remove its authored prices",
                    "priced model profile '{id}' declares {declared_provider}/{declared_model}, but the final invocation resolves to {}/{}",
                    provider.unwrap_or("<none>"),
                    model.unwrap_or("<none>")
                ));
            }
        }
        let (Some(provider), Some(model)) = (provider, model) else { continue };
        let key = (provider.to_string(), model.to_string());
        let (source, label) = match (profile_id, profile.and_then(|item| item.prices.as_ref())) {
            (Some(id), Some(prices)) => {
                (
                    EffectivePriceSource::Profile {
                        prices: prices.clone(),
                        profiles: vec![id.to_string()],
                    },
                    format!("profile '{id}'"),
                )
            }
            _ => match price_entry(&builtin, Some(provider), Some(model)) {
                Some(entry) => (
                    EffectivePriceSource::Builtin(entry.clone()),
                    fallback_label(resolved, "built-in"),
                ),
                None => (EffectivePriceSource::Unpriced, fallback_label(resolved, "unpriced")),
            },
        };
        merge_effective_source(&mut pairs, key, source, label)?;
    }

    let mut currency_sources: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for pair in pairs.values() {
        let currency = match &pair.source {
            EffectivePriceSource::Profile { prices, .. } => Some(prices.currency.as_str()),
            EffectivePriceSource::Builtin(_) => Some(builtin.currency.as_str()),
            EffectivePriceSource::Unpriced => None,
        };
        if let Some(currency) = currency {
            currency_sources
                .entry(currency.to_string())
                .or_default()
                .extend(pair.labels.iter().cloned());
        }
    }
    if currency_sources.len() != 1 {
        let detail = currency_sources
            .iter()
            .map(|(currency, labels)| format!("{currency}: {}", labels.join(", ")))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(miette!(
            help = "select profiles and built-in fallbacks that use one currency, or pass one explicit --prices book",
            "profile price sources use different currencies ({detail})"
        ));
    }
    let currency = currency_sources
        .into_keys()
        .next()
        .expect("a generated book contains at least one priced profile");
    let mut entries = Vec::new();
    for ((provider, model), pair) in pairs {
        match pair.source {
            EffectivePriceSource::Profile { prices, mut profiles } => {
                profiles.sort();
                profiles.dedup();
                let mut extensions = prices.extensions;
                extensions.insert("source".to_string(), serde_json::json!({ "kind": "profile" }));
                extensions.insert("model_profiles".to_string(), serde_json::json!(profiles));
                entries.push(PriceBookEntry {
                    provider,
                    model,
                    effective_at: prices.effective_at,
                    unit: "1m_tokens".to_string(),
                    input_total_micro: prices.input_total_micro,
                    input_cached_read_micro: prices.input_cached_read_micro,
                    input_cache_write_micro: prices.input_cache_write_micro,
                    output_total_micro: prices.output_total_micro,
                    extensions,
                });
            }
            EffectivePriceSource::Builtin(mut entry) => {
                entry.extensions.insert(
                    "source".to_string(),
                    serde_json::json!({
                        "kind": "builtin",
                        "price_book_id": builtin.price_book_id.clone()
                    }),
                );
                entries.push(entry);
            }
            EffectivePriceSource::Unpriced => {}
        }
    }
    let mut book = PriceBook {
        schema: ACCOUNTING_PRICES_SCHEMA.to_string(),
        price_book_id: String::new(),
        currency,
        entries,
        extensions: BTreeMap::new(),
    };
    book.price_book_id = generated_price_book_id(&book)?;
    Ok(Some(book))
}

fn fallback_label(resolved: &ResolvedAgent, kind: &str) -> String {
    match resolved.model_profile_id() {
        Some(id) => format!("profile '{id}' {kind} fallback"),
        None => format!(
            "literal '{}' {kind} source",
            resolved.target.as_ref().map(ExecutionTarget::selector).unwrap_or_else(|| "target".to_string())
        ),
    }
}

fn merge_effective_source(
    pairs: &mut BTreeMap<(String, String), EffectivePricePair>,
    key: (String, String),
    source: EffectivePriceSource,
    label: String,
) -> MietteResult<()> {
    let Some(existing) = pairs.get_mut(&key) else {
        pairs.insert(key, EffectivePricePair { source, labels: vec![label] });
        return Ok(());
    };
    match (&mut existing.source, source) {
        (
            EffectivePriceSource::Profile { prices: left, profiles },
            EffectivePriceSource::Profile { prices: right, profiles: incoming },
        ) if *left == right => profiles.extend(incoming),
        (EffectivePriceSource::Builtin(left), EffectivePriceSource::Builtin(right))
            if *left == right => {}
        (EffectivePriceSource::Unpriced, EffectivePriceSource::Unpriced) => {}
        _ => {
            return Err(miette!(
                help = "give same-pair profiles identical rates and metadata, or pass one explicit --prices book",
                "conflicting price sources for {}/{}: {} and {label}",
                key.0,
                key.1,
                existing.labels.join(", ")
            ));
        }
    }
    existing.labels.push(label);
    Ok(())
}

/// Content identity for a generated profile book. Object keys are recursively
/// canonicalized and the self-referential id is excluded.
/// §FS-rhei-cost-accounting.5.1
fn generated_price_book_id(book: &PriceBook) -> MietteResult<String> {
    let semantic = serde_json::json!({
        "schema": book.schema.clone(),
        "currency": book.currency.clone(),
        "entries": book.entries.clone(),
    });
    let canonical = canonical_json(semantic);
    let bytes = serde_json::to_vec(&canonical)
        .map_err(|err| miette!("failed to serialize generated price-book identity: {err}"))?;
    Ok(format!("profiles-sha256-{:x}", Sha256::digest(bytes)))
}

fn canonical_json(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(object) => serde_json::Value::Object(
            object.into_iter().map(|(key, value)| (key, canonical_json(value))).collect(),
        ),
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(canonical_json).collect())
        }
        scalar => scalar,
    }
}
