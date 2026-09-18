// Exact-match token pricing and integer cost calculation.

// §AR-source-file-size.3 §FS-rhei-cost-accounting.5

fn price_tokens(
    price_book: &PriceBook,
    provider: Option<&str>,
    model: Option<&str>,
    tokens: &AccountingTokens,
) -> AccountingPricing {
    let priceable_measured = [
        tokens.input.total.value,
        tokens.input.cached_read.value,
        tokens.input.cache_write.value,
        tokens.output.total.value,
        tokens.output.cached_read.value,
        tokens.output.cache_write.value,
    ]
    .into_iter()
    .flatten()
    .count();
    if priceable_measured == 0 && tokens.total.value.is_none() {
        return AccountingPricing {
            status: "not-applicable".to_string(),
            currency: None,
            amount_micro: None,
            priced_amount_micro: None,
            price_book_id: None,
        };
    }
    if priceable_measured == 0 {
        return AccountingPricing {
            status: "unpriced".to_string(),
            currency: Some(price_book.currency.clone()),
            amount_micro: None,
            priced_amount_micro: None,
            price_book_id: Some(price_book.price_book_id.clone()),
        };
    }

    let Some(entry) = price_entry(price_book, provider, model) else {
        return AccountingPricing {
            status: "unpriced".to_string(),
            currency: Some(price_book.currency.clone()),
            amount_micro: None,
            priced_amount_micro: None,
            price_book_id: Some(price_book.price_book_id.clone()),
        };
    };
    let mut amount = 0u64;
    amount = amount.saturating_add(price_dimension(
        uncached_input_tokens(tokens),
        entry.input_total_micro,
    ));
    amount = amount.saturating_add(price_dimension(
        tokens.input.cached_read.value,
        entry.input_cached_read_micro,
    ));
    amount = amount.saturating_add(price_dimension(
        tokens.input.cache_write.value,
        entry.input_cache_write_micro,
    ));
    amount = amount.saturating_add(price_dimension(tokens.output.total.value, entry.output_total_micro));
    AccountingPricing {
        status: "priced".to_string(),
        currency: Some(price_book.currency.clone()),
        amount_micro: Some(amount),
        priced_amount_micro: Some(amount),
        price_book_id: Some(price_book.price_book_id.clone()),
    }
}

/// What the full input rate applies to: `input.total` less the two cache
/// dimensions, which are parts of it and each carry a rate of their own.
/// Charging the whole at the full rate and the parts at theirs charges every
/// cached token twice. An unavailable dimension subtracts nothing, and the
/// subtraction saturates rather than underflowing when a provider reports
/// parts larger than the whole. §FS-rhei-cost-accounting.5
fn uncached_input_tokens(tokens: &AccountingTokens) -> Option<u64> {
    let input_total = tokens.input.total.value?;
    Some(
        input_total
            .saturating_sub(tokens.input.cached_read.value.unwrap_or(0))
            .saturating_sub(tokens.input.cache_write.value.unwrap_or(0)),
    )
}

fn price_dimension(tokens: Option<u64>, price_micro: u64) -> u64 {
    let Some(tokens) = tokens else { return 0 };
    // §FS-rhei-cost-accounting.5: Cost uses integer micro-unit arithmetic.
    let amount = (tokens as u128 * price_micro as u128) / PRICE_UNIT_TOKENS as u128;
    u64::try_from(amount).unwrap_or(u64::MAX)
}

fn price_entry<'a>(
    price_book: &'a PriceBook,
    provider: Option<&str>,
    model: Option<&str>,
) -> Option<&'a PriceBookEntry> {
    let provider = provider?;
    let model = model?;
    price_book
        .entries
        .iter()
        .find(|entry| entry.provider == provider && entry.model == model)
}
