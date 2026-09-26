//! One rendering of an amount of money, one reading of one.
//!
//! Every amount this workspace holds is an integer of micro-units — the price
//! book prices in them §FS-rhei-cost-accounting.5 and the project account
//! charges in them §FS-rhei-budgets.1 — so the decimal point belongs in one
//! place. It was in two before the spend dimension needed a third, which is
//! the whole reason this module exists rather than a fourth copy.

/// Micro-units in one unit of currency.
pub const MICRO: u64 = 1_000_000;

/// The most decimal places a written amount may carry, which is the micro-unit
/// itself. §FS-rhei-budgets.2.1
pub const MAX_DECIMALS: u32 = 6;

/// `$25.00` for USD, `25.00 EUR` for anything else.
///
/// Truncated to the currency's minor unit rather than rounded, which is what
/// every cost surface has always printed: a rollup that rounded up would read
/// as more than the archive it was derived from.
/// §FS-rhei-cost-accounting.5 §FS-rhei-budgets.8
pub fn format_micro(value: u64, currency: Option<&str>) -> String {
    let units = value / MICRO;
    let cents = (value % MICRO) / 10_000;
    match currency {
        Some("USD") | None => format!("${units}.{cents:02}"),
        Some(currency) => format!("{units}.{cents:02} {currency}"),
    }
}

/// The bare number a settings key takes, carrying no symbol and no code:
/// `400.00`, `9.50`, `0.0005`.
///
/// Two decimal places unless the amount has a fraction of a cent, because the
/// key is written by a person and `400.000000` is not how a person writes four
/// hundred. §FS-rhei-budgets.2.1
pub fn format_bare(value: u64) -> String {
    let units = value / MICRO;
    let fraction = value % MICRO;
    if fraction % 10_000 == 0 {
        return format!("{units}.{:02}", fraction / 10_000);
    }
    let written = format!("{units}.{fraction:06}");
    written.trim_end_matches('0').to_string()
}

/// Read a settings amount: a positive number of at most six decimal places,
/// in micro-units.
///
/// The error names the key rather than the value, because a settings file has
/// several numbers in it and the one that is wrong is the only thing the
/// reader is missing. Zero and a negative are refused for the same reason the
/// counts refuse them — a bound of nothing admits nothing, which is a way of
/// turning the engine off that nobody asked for — and `"unlimited"` is refused
/// because there is no unbounded state in this design at all.
/// §FS-rhei-budgets.2.1 §REQ-bounded-neural-work.2
pub fn parse_settings_amount(key: &str, value: &serde_json::Value) -> Result<u64, String> {
    let refuse = |why: &str| {
        Err(format!(
            "`defaults.{key}` must be a positive number with at most {MAX_DECIMALS} decimal \
             places, and {why}"
        ))
    };
    let Some(number) = value.as_f64().filter(|number| number.is_finite()) else {
        return refuse(&format!("{value} is not a number"));
    };
    if number <= 0.0 {
        return refuse(&format!("{value} is not positive"));
    }
    let micro = number * MICRO as f64;
    if micro >= u64::MAX as f64 {
        return refuse(&format!("{value} is larger than this ledger can hold"));
    }
    // A seventh decimal place survives the multiplication as a fraction, which
    // is what tells `0.1234567` from `0.123456`.
    if (micro - micro.round()).abs() > f64::EPSILON * micro.max(1.0) {
        return refuse(&format!("{value} carries more than {MAX_DECIMALS} of them"));
    }
    Ok(micro.round() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_amount_is_written_with_its_currency_or_with_none_at_all() {
        assert_eq!(format_micro(25_000_000, Some("USD")), "$25.00");
        assert_eq!(format_micro(25_000_000, None), "$25.00");
        assert_eq!(format_micro(25_000_000, Some("EUR")), "25.00 EUR");
        assert_eq!(format_bare(400_000_000), "400.00");
        assert_eq!(format_bare(0), "0.00");
        assert_eq!(format_bare(1_500_500), "1.5005");
    }

    #[test]
    fn a_settings_amount_takes_a_bare_number_and_refuses_the_four_ways_of_writing_none() {
        assert_eq!(parse_settings_amount("spend_per_day", &json!(400)), Ok(400_000_000));
        assert_eq!(parse_settings_amount("spend_per_day", &json!(400.00)), Ok(400_000_000));
        assert_eq!(parse_settings_amount("spend_per_day", &json!(9.5)), Ok(9_500_000));
        for refused in [json!(0), json!(-5), json!("unlimited"), json!(0.123_456_7)] {
            let error = parse_settings_amount("spend_per_day", &refused)
                .expect_err("a bound of nothing is a settings error");
            assert!(
                error.contains("`defaults.spend_per_day`"),
                "the error names the key that is wrong; got: {error}"
            );
        }
    }
}
