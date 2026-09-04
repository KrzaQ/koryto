//! The weight trend: an exponential moving average over day weights, so the
//! daily noise of the scale does not hide the direction. Updated only on
//! days with a reading; gaps neither decay nor advance it.

use chrono::NaiveDate;

/// One in ten of each new reading's deviation moves the trend.
const ALPHA_DENOMINATOR: i64 = 10;

/// `(day, weight_g)` in day order -> `(day, trend_g)` for the same days.
/// Integer arithmetic in milligrams so the result is deterministic.
pub fn trend(points: &[(NaiveDate, i32)]) -> Vec<(NaiveDate, i32)> {
    let mut out = Vec::with_capacity(points.len());
    let mut t_mg: Option<i64> = None;
    for &(day, w) in points {
        let w_mg = i64::from(w) * 1000;
        let next = match t_mg {
            None => w_mg,
            Some(t) => t + round_div(w_mg - t, ALPHA_DENOMINATOR),
        };
        t_mg = Some(next);
        out.push((day, round_div(next, 1000) as i32));
    }
    out
}

/// Integer division rounded to nearest, away from zero on ties.
pub fn round_div(n: i64, d: i64) -> i64 {
    let q = n / d;
    let r = n % d;
    if 2 * r.abs() >= d.abs() {
        q + n.signum() * d.signum()
    } else {
        q
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    #[test]
    fn seeds_with_the_first_reading_and_smooths_after() {
        let pts = vec![
            (d("2026-09-01"), 82000),
            (d("2026-09-02"), 83000),
            (d("2026-09-05"), 81000), // a gap does not matter
        ];
        let t = trend(&pts);
        assert_eq!(t[0], (d("2026-09-01"), 82000));
        assert_eq!(t[1], (d("2026-09-02"), 82100));
        assert_eq!(t[2], (d("2026-09-05"), 81990));
        assert!(trend(&[]).is_empty());
    }

    #[test]
    fn a_single_outlier_barely_moves_it() {
        let mut pts: Vec<(NaiveDate, i32)> = (1..=10)
            .map(|i| (d(&format!("2026-09-{i:02}")), 80000))
            .collect();
        pts.push((d("2026-09-11"), 83000));
        let t = trend(&pts);
        assert_eq!(t.last().unwrap().1, 80300);
    }

    #[test]
    fn rounding() {
        assert_eq!(round_div(5, 10), 1);
        assert_eq!(round_div(4, 10), 0);
        assert_eq!(round_div(-5, 10), -1);
        assert_eq!(round_div(-4, 10), 0);
        assert_eq!(round_div(15, 10), 2);
    }
}
