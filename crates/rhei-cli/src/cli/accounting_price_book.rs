// Owned price books: the built-in default, caller input validation, and
// durable copying. Invocation pricing remains beside token accounting.

// §AR-source-file-size.3 §FS-rhei-cost-accounting.5.1

const ACCOUNTING_PRICES_SCHEMA: &str = "rhei.accounting.prices.v1";
const PRICE_BOOK_ID: &str = "builtin-2026-05-20";
const PRICE_UNIT_TOKENS: u64 = 1_000_000;

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
struct PriceBook {
    schema: String,
    price_book_id: String,
    currency: String,
    entries: Vec<PriceBookEntry>,
    /// Caller metadata retained through every durable copy but ignored by
    /// price-book semantics. §FS-rhei-cost-accounting.5.1
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    extensions: BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
struct PriceBookEntry {
    provider: String,
    model: String,
    effective_at: String,
    unit: String,
    input_total_micro: u64,
    input_cached_read_micro: u64,
    input_cache_write_micro: u64,
    output_total_micro: u64,
    /// Caller metadata retained through every durable copy but ignored by
    /// matching and pricing. §FS-rhei-cost-accounting.5.1
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    extensions: BTreeMap<String, serde_json::Value>,
}

fn builtin_price_entries() -> Vec<PriceBookEntry> {
    vec![PriceBookEntry {
        provider: "anthropic".to_string(),
        model: "claude-sonnet-4-6".to_string(),
        effective_at: "2026-05-20T00:00:00Z".to_string(),
        unit: "1m_tokens".to_string(),
        input_total_micro: 3_000_000,
        input_cached_read_micro: 300_000,
        input_cache_write_micro: 3_750_000,
        output_total_micro: 15_000_000,
        extensions: BTreeMap::new(),
    }]
}

fn builtin_price_book() -> PriceBook {
    PriceBook {
        schema: ACCOUNTING_PRICES_SCHEMA.to_string(),
        price_book_id: PRICE_BOOK_ID.to_string(),
        currency: "USD".to_string(),
        entries: builtin_price_entries(),
        extensions: BTreeMap::new(),
    }
}

fn load_price_book(path: &Path) -> MietteResult<PriceBook> {
    // Read exactly once before agent execution. §FS-rhei-cost-accounting.5.1
    let text = fs::read_to_string(path).map_err(|err| {
        miette!(
            help = "pass a readable local JSON file using rhei.accounting.prices.v1",
            "failed to read price book '{}': {err}", path.display()
        )
    })?;
    let price_book: PriceBook = serde_json::from_str(&text).map_err(|err| {
        miette!(
            help = "provide schema, price_book_id, currency, and entries in the rhei.accounting.prices.v1 JSON shape",
            "failed to parse price book '{}': {err}", path.display()
        )
    })?;
    validate_price_book(path, &price_book)?;
    Ok(price_book)
}

fn validate_price_book(path: &Path, price_book: &PriceBook) -> MietteResult<()> {
    let invalid = |problem: String| {
        miette!(
            help = "use a non-empty id and currency, unique provider/model entries, and unit `1m_tokens`",
            "invalid price book '{}': {problem}", path.display()
        )
    };
    if price_book.schema != ACCOUNTING_PRICES_SCHEMA {
        return Err(invalid(format!(
            "unsupported schema '{}'; expected '{}'",
            price_book.schema, ACCOUNTING_PRICES_SCHEMA
        )));
    }
    if price_book.price_book_id.trim().is_empty() {
        return Err(invalid("price_book_id must not be empty".to_string()));
    }
    if price_book.currency.trim().is_empty() {
        return Err(invalid("currency must not be empty".to_string()));
    }
    let mut matches = BTreeSet::new();
    for entry in &price_book.entries {
        if entry.provider.trim().is_empty() || entry.model.trim().is_empty() {
            return Err(invalid("entry provider and model must not be empty".to_string()));
        }
        if entry.effective_at.trim().is_empty() {
            return Err(invalid(format!(
                "effective_at must not be empty for {}/{}",
                entry.provider, entry.model
            )));
        }
        if entry.unit != "1m_tokens" {
            return Err(invalid(format!(
                "unsupported unit '{}' for {}/{}",
                entry.unit, entry.provider, entry.model
            )));
        }
        if !matches.insert((&entry.provider, &entry.model)) {
            return Err(invalid(format!(
                "duplicate provider/model entry for {}/{}",
                entry.provider, entry.model
            )));
        }
    }
    Ok(())
}

fn write_price_book(accounting_root: &Path, price_book: &PriceBook) -> MietteResult<()> {
    let path = accounting_root.join("prices.json");
    write_json_atomic(&path, price_book)
}

fn generated_price_book_archive_path(accounting_root: &Path, price_book: &PriceBook) -> PathBuf {
    accounting_root.join("price-books").join(format!("{}.json", price_book.price_book_id))
}

/// Check immutable generated-book bytes in every root before the current
/// snapshot in any root is changed. §FS-rhei-cost-accounting.5.1
fn validate_generated_price_book_archive(
    accounting_root: &Path,
    price_book: &PriceBook,
) -> MietteResult<()> {
    let path = generated_price_book_archive_path(accounting_root, price_book);
    let existing = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(file_io_report(&path, "failed to read generated price-book archive", err)),
    };
    let expected = serde_json::to_vec_pretty(price_book).map_err(|err| {
        miette!("failed to serialize generated price-book archive '{}': {err}", path.display())
    })?;
    if existing != expected {
        return Err(miette!(
            help = "preserve the existing archive under its content-derived id and repair the conflicting file before retrying",
            "generated price-book archive '{}' already exists with different bytes",
            path.display()
        ));
    }
    Ok(())
}

/// Publish the inspectable current snapshot and reuse or create its immutable
/// generated archive. §FS-rhei-cost-accounting.5.1
fn write_generated_price_book(accounting_root: &Path, price_book: &PriceBook) -> MietteResult<()> {
    write_price_book(accounting_root, price_book)?;
    let archive = generated_price_book_archive_path(accounting_root, price_book);
    if !archive.exists() {
        write_json_atomic(&archive, price_book)?;
    }
    Ok(())
}

/// Generated archives are trusted only when their declared id is the digest
/// of their canonical semantic content. §FS-rhei-cost-accounting.5.2
fn generated_archive_is_valid(path: &Path, price_book: &PriceBook) -> bool {
    price_book.price_book_id.starts_with("profiles-sha256-")
        && path.file_stem().and_then(OsStr::to_str) == Some(price_book.price_book_id.as_str())
        && validate_price_book(path, price_book).is_ok()
        && generated_price_book_id(price_book)
            .is_ok_and(|generated| generated == price_book.price_book_id)
}

/// Refuse conflicts over the same root union and shared-root task scope that
/// inspection reads, before currency checks or any accounting mutation.
// §FS-rhei-panta.6.5 §FS-rhei-cost-accounting.11
fn validate_accounting_identity(roots: &[AccountingRoot], scope: &RheiScope) -> MietteResult<()> {
    let inspection = read_cost_inspection_over(roots, scope);
    if let Some(error) = inspection.identity_conflicts.first() {
        return Err(miette!(
            help = "repair or remove the conflicting invocation record before starting another run",
            "accounting identity conflict: {error}"
        ));
    }
    Ok(())
}

/// Currency is a per-root invariant, including records outside a narrowed task
/// scope. Identity has already been checked over the scoped union separately.
// §FS-rhei-cost-accounting.5.1 §FS-rhei-cost-accounting.11
fn validate_price_book_currency(
    accounting_root: &Path,
    price_book: &PriceBook,
) -> MietteResult<()> {
    let (records, errors, _) = read_accounting_root(accounting_root);
    if let Some(error) = errors.first() {
        return Err(miette!(
            help = "repair or remove the unreadable invocation record before starting another run",
            "cannot verify selected currency '{}' in accounting root '{}': {error}",
            price_book.currency,
            accounting_root.display()
        ));
    }
    for (_, record) in records {
        let Some(record_currency) = record.pricing.currency else {
            continue;
        };
        if record_currency != price_book.currency {
            return Err(miette!(
                help = "reuse a price book with the durable currency or start from a new accounting root; existing invocations are not converted",
                "selected price book currency '{}' conflicts with durable currency '{}' in accounting root '{}'",
                price_book.currency,
                record_currency,
                accounting_root.display()
            ));
        }
    }
    Ok(())
}
