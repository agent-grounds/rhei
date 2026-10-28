// Price-book coverage: the one warning per book, its pairs, and the origin a
// selected book is named by. §FS-rhei-validate.4.6 §FS-rhei-budgets.5.2

fn coverage_target(provider: Option<&str>, model: Option<&str>) -> ResolvedAgent {
    ResolvedAgent {
        agent: AgentConfig::from("claude-code"),
        profile: built_in_agents().remove("claude-code").expect("claude-code"),
        mode: None,
        target: None,
        model: model.map(str::to_owned),
        model_provider: provider.map(str::to_owned),
        model_name: model.map(str::to_owned),
        timeout_secs: None,
        autonomous_args: Vec::new(),
    }
}

/// One line for the book, never one per pair or per invocation: the pairs
/// deduplicated and ascending, a covered pair and an unresolved one left out.
/// §FS-rhei-validate.4.6
#[test]
fn coverage_warns_once_per_book_listing_each_uncovered_pair_once_in_order() {
    let book = builtin_price_book();
    let origin = PriceBookOrigin::of(&book, None);
    let targets = [
        coverage_target(Some("openai"), Some("gpt-9")),
        coverage_target(Some("anthropic"), Some("claude-opus-9")),
        coverage_target(Some("openai"), Some("gpt-9")),
        coverage_target(Some("anthropic"), Some("claude-sonnet-4-6")),
        coverage_target(None, Some("mystery")),
    ];

    let warning = price_book_coverage_warning(&book, &origin, &targets);

    assert_eq!(
        warning.as_deref(),
        Some(
            "price book builtin-2026-05-20 (built-in) has no entry for anthropic/claude-opus-9, \
             openai/gpt-9: their invocations will be measured but unpriced, and each is charged \
             the spend reserve. Add an entry for each pair to price them."
        )
    );
    assert_eq!(
        price_book_coverage_warning(&book, &origin, &targets[3..]),
        None,
        "a book that prices every resolved pair says nothing"
    );
}

/// A caller's book is named by the path it was read from, the built-in book
/// by its place, and any other book was composed from profile prices; the
/// selection itself is unchanged. §FS-rhei-budgets.5.2 §FS-rhei-cost-accounting.5.1
#[test]
fn a_selected_book_is_named_by_where_it_came_from() {
    let builtin = builtin_price_book();
    let custom = custom_luna_price_book();
    let file = PriceBookOrigin::of(&custom, Some(Path::new("/work/prices.json")));
    assert_eq!((file.source, file.path.as_deref()), ("file", Some("/work/prices.json")));
    assert_eq!(PriceBookOrigin::of(&builtin, None).source, "builtin");
    assert_eq!(PriceBookOrigin::of(&custom, None).source, "profiles");

    let context = unpriced_pricing_context(Some("openai"), Some("gpt-5.6-luna"), &custom, &file);
    assert_eq!(context.missing_entry, Some(false), "the book has the entry");
    let context = unpriced_pricing_context(Some("openai"), None, &custom, &file);
    assert_eq!(context.missing_entry, None, "an unknown pair cannot be checked");
    assert_eq!(context.price_book_id.as_deref(), Some("fixture-luna-2026-09-01"));
}
