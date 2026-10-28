// Price-book coverage: which targeted provider/model pairs the selected book
// has no exact entry for, told once per book before anything is charged, and
// where the selected book came from, as a receipt and that warning name it.

// §AR-source-file-size.3 §FS-rhei-validate.4.6 §FS-rhei-budgets.5.2

/// Where a run's selected price book came from: `file` for a caller-owned
/// book, with the path it was read from rather than the run's copy;
/// `builtin` for the built-in book; `profiles` for one composed from profile
/// prices. §FS-rhei-budgets.5.2
#[derive(Clone, Debug)]
struct PriceBookOrigin {
    source: &'static str,
    path: Option<String>,
}

impl PriceBookOrigin {
    /// The origin of `book`, given the caller-owned path a run selected it
    /// from, if any. A book that is neither the caller's nor the built-in one
    /// was composed from profile prices. §FS-rhei-cost-accounting.5.1
    fn of(book: &PriceBook, caller: Option<&Path>) -> Self {
        match caller {
            Some(path) => Self { source: "file", path: Some(path.display().to_string()) },
            None if book.price_book_id == PRICE_BOOK_ID => Self { source: "builtin", path: None },
            None => Self { source: "profiles", path: None },
        }
    }
}

/// The warning for the pairs `book` cannot price among `invocations`, or
/// `None` where it prices every one. One per book, never one per pair, the
/// pairs ascending; an invocation whose pair is not resolved statically
/// contributes nothing, because no warning is guessed for it.
/// §FS-rhei-validate.4.6
fn price_book_coverage_warning(
    book: &PriceBook,
    origin: &PriceBookOrigin,
    invocations: &[ResolvedAgent],
) -> Option<String> {
    let uncovered: BTreeSet<String> = invocations
        .iter()
        .filter_map(|resolved| {
            let provider = resolved.model_provider.as_deref()?;
            let model = resolved.model_name.as_deref().or(resolved.model.as_deref())?;
            price_entry(book, Some(provider), Some(model))
                .is_none()
                .then(|| format!("{provider}/{model}"))
        })
        .collect();
    if uncovered.is_empty() {
        return None;
    }
    let book = rhei_core::budget::book_phrase(
        origin.source,
        Some(&book.price_book_id),
        origin.path.as_deref(),
    )?;
    Some(format!(
        "{book} has no entry for {}: their invocations will be measured but unpriced, and each \
         is charged the spend reserve. Add an entry for each pair to price them.",
        uncovered.into_iter().collect::<Vec<_>>().join(", ")
    ))
}

/// The coverage warning `rhei validate` prints, without its `warning: `
/// prefix: the book a run of this plan would select with no `--prices`,
/// checked against every agent invocation the plan can reach. A book that
/// cannot be read, or profile prices that conflict, are said once instead.
/// Advisory: it reads the book and writes nothing. Anything else this check
/// cannot resolve is left to the validation pass, which reports it.
/// §FS-rhei-validate.4.6 §FS-rhei-cost-accounting.5.1
fn validated_price_book_coverage(input: &Path, state_machine: Option<&Path>) -> Vec<String> {
    let workspace_root = execution_workspace_root(input);
    let Ok(settings) = load_merged_settings(&workspace_root) else { return Vec::new() };
    let Ok(loaded) = load_plan_for_validation(input) else { return Vec::new() };
    let Ok(resolved) = resolve_state_machines_for_loaded_plan(input, &loaded, state_machine)
    else {
        return Vec::new();
    };
    let Ok(machines) = ExecutionMachines::build(&resolved, input, &loaded)
        .map(|machines| machines.with_state_machine_override(state_machine))
    else {
        return Vec::new();
    };
    let opts = default_run_options();
    let Ok(invocations) = selected_run_invocations(&loaded, &machines, &settings, &opts, &None)
    else {
        return Vec::new();
    };
    let (book, origin) = match settings.prices.as_ref() {
        Some(declared) => match load_price_book(&declared.path) {
            Ok(book) => {
                let origin = PriceBookOrigin::of(&book, Some(&declared.path));
                (book, origin)
            }
            Err(err) => {
                return vec![format!(
                    "price book {} (from defaults.prices in {}) cannot be read, so the targeted \
                     models' prices are not checked: {err}",
                    declared.path.display(),
                    declared.declared_in.display()
                )]
            }
        },
        None => match profile_price_book(&settings, &invocations) {
            Ok(Some(book)) => (book, PriceBookOrigin { source: "profiles", path: None }),
            Ok(None) => {
                let book = builtin_price_book();
                let origin = PriceBookOrigin::of(&book, None);
                (book, origin)
            }
            Err(err) => {
                return vec![format!(
                    "profile prices conflict, so the targeted models' prices are not checked: {err}"
                )]
            }
        },
    };
    price_book_coverage_warning(&book, &origin, &invocations).into_iter().collect()
}
