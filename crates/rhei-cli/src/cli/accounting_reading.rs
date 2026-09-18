// Reading a stored invocation record into today's token convention: which
// convention it follows, what its dimensions mean under it, and what its money
// is once every cached token is charged exactly once.

// Nothing here writes: a stored record stays as it was computed.
// §AR-source-file-size.3 §FS-rhei-cost-accounting.5.2

/// The convention Rhei stamps on every record it writes: `input.total` is the
/// whole, and the two cache dimensions are parts of it.
/// §FS-rhei-cost-accounting.3.6
const TOKEN_CONVENTION_INCLUDES_CACHE: &str = "input-total-includes-cache";

/// The convention a provider that reports a cache-free input count follows, and
/// that a `claude-code` or `pi` record written before §FS-rhei-cost-accounting.3.1
/// was stated is still in. Rhei never writes it, but a reader has to name it.
const TOKEN_CONVENTION_EXCLUDES_CACHE: &str = "input-total-excludes-cache";

/// What a record's `input.total` counts. §FS-rhei-cost-accounting.3.6
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TokenConvention {
    /// Today's convention: the cache dimensions sit inside `input.total`.
    IncludesCache,
    /// `input.total` counts neither cache dimension, so a reading adds them in.
    ExcludesCache,
    /// Nothing states the convention and nothing implies it. The record is read
    /// as stored, because nothing about it is known to be wrong, and no
    /// aggregate holding it reports `complete`. §FS-rhei-cost-accounting.6.2
    Unknown,
}

/// The convention a record follows: what it says, or — for a record written
/// before the field existed — what its own `agent` implies.
/// §FS-rhei-cost-accounting.3.6
fn record_token_convention(record: &AccountingInvocationRecord) -> TokenConvention {
    if let Some(stated) = record.token_convention.as_deref() {
        return if stated == TOKEN_CONVENTION_INCLUDES_CACHE {
            TokenConvention::IncludesCache
        } else if stated == TOKEN_CONVENTION_EXCLUDES_CACHE {
            TokenConvention::ExcludesCache
        } else {
            // A convention this build has never heard of is not one it may
            // assume the meaning of.
            TokenConvention::Unknown
        };
    }
    match record.agent.as_str() {
        "codex" => TokenConvention::IncludesCache,
        "claude-code" | "pi" => TokenConvention::ExcludesCache,
        _ => TokenConvention::Unknown,
    }
}

/// The price books a reading can reach for a record whose money it has to
/// recompute: the built-in book, the `prices.json` sitting beside the records,
/// and valid content-addressed generated archives in that same accounting
/// root. Selection never fetches a book over the network.
// §FS-rhei-cost-accounting.5.2
#[derive(Clone, Debug)]
struct ReachablePriceBooks {
    beside_records: Option<PriceBook>,
    generated_archives: BTreeMap<String, PriceBook>,
    builtin: PriceBook,
}

impl ReachablePriceBooks {
    /// What is reachable from one accounting root. A `prices.json` that will
    /// not read or will not parse is simply not reachable; it is not an error,
    /// because nothing has asked for it yet.
    fn beside(accounting_root: &Path) -> Self {
        let beside_records = fs::read_to_string(accounting_root.join("prices.json"))
            .ok()
            .and_then(|text| serde_json::from_str::<PriceBook>(&text).ok());
        let mut generated_archives = BTreeMap::new();
        if let Ok(entries) = fs::read_dir(accounting_root.join("price-books")) {
            for path in entries.filter_map(Result::ok).map(|entry| entry.path()) {
                let Some(book) = fs::read_to_string(&path)
                    .ok()
                    .and_then(|text| serde_json::from_str::<PriceBook>(&text).ok())
                else {
                    continue;
                };
                if generated_archive_is_valid(&path, &book) {
                    generated_archives.insert(book.price_book_id.clone(), book);
                }
            }
        }
        Self { beside_records, generated_archives, builtin: builtin_price_book() }
    }

    /// What is reachable while a run holds its own selected book: that book is
    /// the `prices.json` the run wrote beside the records it is writing.
    /// §FS-rhei-cost-accounting.5.1
    fn with_selected(price_book: &PriceBook) -> Self {
        Self {
            beside_records: Some(price_book.clone()),
            generated_archives: BTreeMap::new(),
            builtin: builtin_price_book(),
        }
    }

    /// Only the built-in book, where a reading has no accounting root behind
    /// it — which in practice is a test that builds an inspection by hand.
    #[cfg(test)]
    fn builtin_only() -> Self {
        Self {
            beside_records: None,
            generated_archives: BTreeMap::new(),
            builtin: builtin_price_book(),
        }
    }

    /// The book a record names, when it is one of the reachable ones.
    fn named(&self, price_book_id: Option<&str>) -> Option<&PriceBook> {
        let id = price_book_id?;
        if let Some(book) = self.beside_records.as_ref().filter(|book| book.price_book_id == id) {
            return Some(book);
        }
        if let Some(book) = self.generated_archives.get(id) {
            return Some(book);
        }
        (self.builtin.price_book_id == id).then_some(&self.builtin)
    }
}

/// One stored record, read into §FS-rhei-cost-accounting.3.1's convention.
///
/// This is what a recomputation produces, never what is on disk: `tokens`,
/// `amount_micro`, and `priced_amount_micro` stay as they were written
/// (§FS-rhei-cost-accounting.5.1).
struct RecordReading<'a> {
    /// The record as stored, for everything a reading does not restate: its
    /// identity, its agent, and the model it named.
    record: &'a AccountingInvocationRecord,
    tokens: AccountingTokens,
    pricing: AccountingPricing,
    /// The record's convention could not be established, so no aggregate over
    /// it may claim to be complete. §FS-rhei-cost-accounting.6.2
    convention_unknown: bool,
}

/// Read one stored record: restate its tokens where it counted its cache
/// dimensions outside `input.total`, and recompute its money where the stored
/// amount charged its cached tokens twice. §FS-rhei-cost-accounting.5.2
fn read_stored_record<'a>(
    record: &'a AccountingInvocationRecord,
    books: &ReachablePriceBooks,
) -> RecordReading<'a> {
    let (convention, tokens) = read_stored_tokens(record);
    let pricing = reread_pricing(record, convention, &tokens, books);
    RecordReading {
        record,
        tokens,
        pricing,
        convention_unknown: convention == TokenConvention::Unknown,
    }
}

/// Read one stored record through a caller-selected replacement book. Token
/// repair is shared with ordinary historical reading, but stored money is not:
/// the explicit book alone prices the in-memory result.
/// §FS-rhei-cost-accounting.5.1 §FS-rhei-cost-accounting.5.2
fn read_stored_record_with_price_book<'a>(
    record: &'a AccountingInvocationRecord,
    price_book: &PriceBook,
) -> RecordReading<'a> {
    let (convention, tokens) = read_stored_tokens(record);
    let pricing = price_tokens(
        price_book,
        record.provider.as_deref(),
        record.model.as_deref(),
        &tokens,
    );
    RecordReading {
        record,
        tokens,
        pricing,
        convention_unknown: convention == TokenConvention::Unknown,
    }
}

/// Restate a durable record into today's token convention without consulting
/// any price book. Both ordinary rereading and explicit repricing start here.
/// §FS-rhei-cost-accounting.5.2
fn read_stored_tokens(
    record: &AccountingInvocationRecord,
) -> (TokenConvention, AccountingTokens) {
    let convention = record_token_convention(record);
    let tokens = match convention {
        TokenConvention::ExcludesCache => restate_tokens(&record.tokens),
        TokenConvention::IncludesCache | TokenConvention::Unknown => record.tokens.clone(),
    };
    (convention, tokens)
}

/// `input.total` becomes every input token the provider counted, and the whole
/// becomes that plus the output. An input total the record never measured has
/// nothing to restate, and restating needs no price book.
/// §FS-rhei-cost-accounting.5.2
fn restate_tokens(tokens: &AccountingTokens) -> AccountingTokens {
    let mut restated = tokens.clone();
    let Some(input_total) = tokens.input.total.value else {
        return restated;
    };
    let inclusive = input_total
        .saturating_add(tokens.input.cached_read.value.unwrap_or(0))
        .saturating_add(tokens.input.cache_write.value.unwrap_or(0));
    restated.input.total.value = Some(inclusive);
    if restated.total.value.is_some() {
        // An unavailable output contributes nothing, as it does everywhere
        // else. §FS-rhei-cost-accounting.5
        restated.total.value =
            Some(inclusive.saturating_add(tokens.output.total.value.unwrap_or(0)));
    }
    restated
}

/// What the record's money is once every cached token is charged once.
///
/// The stored amount is already what §FS-rhei-cost-accounting.5 computes unless
/// it was written under the old formula for a record whose cache dimensions sit
/// inside `input.total`: there, and only there, was the cached half charged at
/// the full input rate and again at its own.
// §FS-rhei-cost-accounting.5.2
fn reread_pricing(
    record: &AccountingInvocationRecord,
    convention: TokenConvention,
    tokens: &AccountingTokens,
    books: &ReachablePriceBooks,
) -> AccountingPricing {
    if !stored_amount_charges_cache_twice(record, convention) {
        return record.pricing.clone();
    }
    match books.named(record.pricing.price_book_id.as_deref()) {
        Some(book) => {
            price_tokens(book, record.provider.as_deref(), record.model.as_deref(), tokens)
        }
        // Out of reach. A known over-charge is no more a lower bound than it
        // is an amount, so it is neither reported nor carried forward: the
        // doubt reaches the reader as coverage. §FS-rhei-cost-accounting.6.2
        None => AccountingPricing {
            status: "unpriced".to_string(),
            currency: record.pricing.currency.clone(),
            amount_micro: None,
            priced_amount_micro: None,
            price_book_id: record.pricing.price_book_id.clone(),
        },
    }
}

/// Whether this record's stored amount charged its cached tokens twice: it
/// counts them inside `input.total`, it was priced before the formula that
/// subtracts them existed, and it has some to charge.
///
/// A record that states its convention was priced under §FS-rhei-cost-accounting.5
/// already, so recomputing it would return the amount it stores and could only
/// lose it where the book is out of reach. A record whose cache dimensions are
/// zero or unavailable needs no correction either: the recomputation and the
/// stored amount agree, and it stays priced whether or not its book is
/// reachable. §FS-rhei-cost-accounting.5.2
fn stored_amount_charges_cache_twice(
    record: &AccountingInvocationRecord,
    convention: TokenConvention,
) -> bool {
    if convention != TokenConvention::IncludesCache || record.token_convention.is_some() {
        return false;
    }
    let cached_read = record.tokens.input.cached_read.value.unwrap_or(0);
    let cache_write = record.tokens.input.cache_write.value.unwrap_or(0);
    cached_read.saturating_add(cache_write) > 0
}
