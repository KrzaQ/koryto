//! What a session costs, from a rate instead of a guess. MET is the multiple
//! of resting metabolism an activity demands, so an hour of it burns
//! `MET × weight` kcal in total — but the person's base expenditure already
//! pays for the resting hour underneath it. Only the excess belongs to the
//! session, hence `(MET − 1)`. Counting the whole thing would hand a two-hour
//! walk about 200 kcal it has already been given once.

use rust_decimal::prelude::ToPrimitive;
use rust_decimal::{Decimal, RoundingStrategy};

/// `(met − 1) × kg × hours`, rounded, never negative. None when the MET is
/// at or below rest: standing about earns nothing.
pub fn kcal(met: Decimal, weight_g: i32, minutes: i32) -> Option<i32> {
    if minutes <= 0 || weight_g <= 0 {
        return None;
    }
    let excess = met - Decimal::ONE;
    if excess <= Decimal::ZERO {
        return None;
    }
    let kg = Decimal::from(weight_g) / Decimal::from(1000);
    let hours = Decimal::from(minutes) / Decimal::from(60);
    (excess * kg * hours)
        .round_dp_with_strategy(0, RoundingStrategy::MidpointAwayFromZero)
        .to_i32()
        .filter(|k| *k > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dec(s: &str) -> Decimal {
        s.parse().unwrap()
    }

    #[test]
    fn a_walk_costs_what_it_costs_above_sitting_still() {
        // 10 km in 2 h at 104 kg: 3.5 MET is 728 kcal in total, of which the
        // base already covers 208. The session is worth 520.
        assert_eq!(kcal(dec("3.5"), 104_000, 120), Some(520));
        // The same walk on a lighter body burns less.
        assert_eq!(kcal(dec("3.5"), 70_000, 120), Some(350));
        // An hour's swim at 6 MET.
        assert_eq!(kcal(dec("6"), 104_000, 60), Some(520));
        // Half an hour of it is half as much.
        assert_eq!(kcal(dec("6"), 104_000, 30), Some(260));
    }

    #[test]
    fn nothing_to_earn_and_nothing_to_divide_by() {
        // At rest, or below it, a session is worth nothing rather than a
        // negative number.
        assert_eq!(kcal(dec("1"), 104_000, 60), None);
        assert_eq!(kcal(dec("0.5"), 104_000, 60), None);
        // Without a weight there is nothing to multiply.
        assert_eq!(kcal(dec("6"), 0, 60), None);
        assert_eq!(kcal(dec("6"), 104_000, 0), None);
        // A minute of gentle yoga rounds to nothing, not to zero kcal logged.
        assert_eq!(kcal(dec("1.01"), 60_000, 1), None);
    }
}
