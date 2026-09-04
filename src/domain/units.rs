//! Weight is stored as integer grams and shown as decimal kilograms;
//! portions are a decimal count. This module owns those conversions.

use rust_decimal::prelude::ToPrimitive;
use rust_decimal::{Decimal, RoundingStrategy};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum UnitError {
    #[error("cannot read {0:?} as kilograms (try 82.4)")]
    Weight(String),
    #[error("weight must be between 20 and 400 kg")]
    WeightRange,
    #[error("cannot read {0:?} as a portion count (try 1, 0.5 or 1.5)")]
    Portions(String),
}

/// `82.4`, `82,4`, `82.40 kg`, `82` -> grams, rounded to the gram.
pub fn parse_kg(input: &str) -> Result<i32, UnitError> {
    let s = input
        .trim()
        .to_ascii_lowercase()
        .trim_end_matches("kg")
        .trim()
        .replace(',', ".");
    let err = || UnitError::Weight(input.to_string());
    let kg: Decimal = s.parse().map_err(|_| err())?;
    let grams = (kg * Decimal::from(1000))
        .round()
        .to_i64()
        .ok_or_else(err)?;
    if !(20_000..=400_000).contains(&grams) {
        return Err(UnitError::WeightRange);
    }
    Ok(grams as i32)
}

/// Kilograms with up to two decimals, trailing zeros trimmed: `82.4`, `82`.
pub fn format_kg(grams: i32) -> String {
    (Decimal::from(grams) / Decimal::from(1000))
        .round_dp(2)
        .normalize()
        .to_string()
}

/// Positive, at most two decimals.
pub fn parse_portions(input: &str) -> Result<Decimal, UnitError> {
    let err = || UnitError::Portions(input.to_string());
    let p: Decimal = input.trim().replace(',', ".").parse().map_err(|_| err())?;
    if p <= Decimal::ZERO || p.scale() > 2 || p > Decimal::from(9999) {
        return Err(err());
    }
    Ok(p.normalize())
}

/// `round(per_portion * portions)` as an integer.
pub fn scale(per_portion: i32, portions: Decimal) -> i32 {
    (Decimal::from(per_portion) * portions)
        .round_dp_with_strategy(0, RoundingStrategy::MidpointAwayFromZero)
        .to_i32()
        .unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kilograms() {
        assert_eq!(parse_kg("82.4"), Ok(82400));
        assert_eq!(parse_kg("82,4"), Ok(82400));
        assert_eq!(parse_kg(" 82.45 kg"), Ok(82450));
        assert_eq!(parse_kg("82"), Ok(82000));
        assert_eq!(parse_kg("82.4567"), Ok(82457));
        assert_eq!(parse_kg("10"), Err(UnitError::WeightRange));
        assert!(parse_kg("heavy").is_err());
        assert_eq!(format_kg(82400), "82.4");
        assert_eq!(format_kg(82000), "82");
        assert_eq!(format_kg(82457), "82.46");
    }

    #[test]
    fn portions_and_scaling() {
        assert_eq!(parse_portions("1").unwrap().to_string(), "1");
        assert_eq!(parse_portions("1.50").unwrap().to_string(), "1.5");
        assert_eq!(parse_portions("0,5").unwrap().to_string(), "0.5");
        assert!(parse_portions("0").is_err());
        assert!(parse_portions("1.234").is_err());
        assert!(parse_portions("lots").is_err());
        assert_eq!(scale(520, parse_portions("1.5").unwrap()), 780);
        assert_eq!(scale(333, parse_portions("0.5").unwrap()), 167);
        assert_eq!(scale(24, parse_portions("1.5").unwrap()), 36);
    }
}
