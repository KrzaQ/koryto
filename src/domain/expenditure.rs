//! How much a person actually burns. Over a trailing window, intake minus
//! the energy stored as weight change is expenditure; that number needs no
//! exercise estimate and improves the longer someone logs. Until the window
//! has enough data the Mifflin-St Jeor equation times an activity factor
//! stands in, and the result says which it is.

use chrono::NaiveDate;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::{Decimal, RoundingStrategy};
use serde::Serialize;

use super::trend::round_div;

/// The trailing window, in days including the end day.
pub const WINDOW_DAYS: i64 = 28;
/// Logged days the window needs before the adaptive number is shown.
pub const MIN_LOGGED_DAYS: usize = 14;
/// Days between the first and last weigh-in in the window it needs.
pub const MIN_WEIGHT_SPAN_DAYS: i64 = 10;
/// Energy per kilogram of body weight, the usual working figure.
pub const KCAL_PER_KG: i64 = 7700;

/// One day of the window: `kcal` is None on an unlogged day, `trend_g` is
/// the weight trend on a day with a reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DayRow {
    pub day: NaiveDate,
    pub kcal: Option<i32>,
    pub trend_g: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub height_mm: Option<i32>,
    pub born_on: Option<NaiveDate>,
    pub sex: Option<Sex>,
    pub activity_factor: Decimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Sex {
    Female,
    Male,
}

impl std::str::FromStr for Sex {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "female" | "f" => Ok(Self::Female),
            "male" | "m" => Ok(Self::Male),
            other => Err(format!("{other:?} is not female or male")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Basis {
    /// Derived from intake and the weight trend over the window.
    Adaptive,
    /// Mifflin-St Jeor times the activity factor; not enough data yet.
    Seed,
    /// Nothing to go on: no weight, or no profile for the seed.
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Estimate {
    pub kcal: Option<i32>,
    pub basis: Basis,
    pub logged_days: usize,
    pub weight_span_days: i64,
    /// What the seed would say, for the UI to show next to the adaptive number.
    pub seed_kcal: Option<i32>,
}

/// `rows` is the window in day order, at most [`WINDOW_DAYS`] long; `latest_trend_g`
/// is the most recent trend weight known at all (it may be older than the window).
pub fn estimate(
    rows: &[DayRow],
    latest_trend_g: Option<i32>,
    end: NaiveDate,
    profile: &Profile,
) -> Estimate {
    let logged: Vec<i64> = rows.iter().filter_map(|r| r.kcal.map(i64::from)).collect();
    let weigh_ins: Vec<(NaiveDate, i32)> = rows
        .iter()
        .filter_map(|r| r.trend_g.map(|t| (r.day, t)))
        .collect();
    let span = match (weigh_ins.first(), weigh_ins.last()) {
        (Some(a), Some(b)) => (b.0 - a.0).num_days(),
        _ => 0,
    };
    let seed_kcal = seed(profile, latest_trend_g, end);

    if logged.len() >= MIN_LOGGED_DAYS && span >= MIN_WEIGHT_SPAN_DAYS {
        let mean_intake = round_div(logged.iter().sum::<i64>(), logged.len() as i64);
        let delta_g = i64::from(weigh_ins.last().expect("span > 0").1)
            - i64::from(weigh_ins.first().expect("span > 0").1);
        // Energy stored per day: Δgrams × kcal/kg ÷ 1000 ÷ days.
        let stored_per_day = round_div(delta_g * KCAL_PER_KG, 1000 * span);
        return Estimate {
            kcal: Some((mean_intake - stored_per_day).max(0) as i32),
            basis: Basis::Adaptive,
            logged_days: logged.len(),
            weight_span_days: span,
            seed_kcal,
        };
    }
    Estimate {
        kcal: seed_kcal,
        basis: if seed_kcal.is_some() {
            Basis::Seed
        } else {
            Basis::None
        },
        logged_days: logged.len(),
        weight_span_days: span,
        seed_kcal,
    }
}

/// Mifflin-St Jeor: `10 kg + 6.25 cm − 5 age + 5` (male) or `− 161` (female),
/// times the activity factor. None when a piece of the profile is missing.
pub fn seed(profile: &Profile, weight_g: Option<i32>, on: NaiveDate) -> Option<i32> {
    let weight_g = i64::from(weight_g?);
    let height_mm = i64::from(profile.height_mm?);
    let age = years_between(profile.born_on?, on);
    let offset = match profile.sex? {
        Sex::Male => 5,
        Sex::Female => -161,
    };
    // 10 × kg = weight_g / 100; 6.25 × cm = height_mm × 5 / 8.
    let bmr = round_div(weight_g, 100) + round_div(height_mm * 5, 8) - 5 * age + offset;
    let tdee = Decimal::from(bmr) * profile.activity_factor;
    tdee.round_dp_with_strategy(0, RoundingStrategy::MidpointAwayFromZero)
        .to_i32()
        .map(|k| k.max(0))
}

fn years_between(born: NaiveDate, on: NaiveDate) -> i64 {
    use chrono::Datelike;
    let mut years = i64::from(on.year() - born.year());
    if (on.month(), on.day()) < (born.month(), born.day()) {
        years -= 1;
    }
    years.max(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }
    fn dec(s: &str) -> Decimal {
        s.parse().unwrap()
    }
    fn profile() -> Profile {
        Profile {
            height_mm: Some(1800),
            born_on: Some(d("1990-06-15")),
            sex: Some(Sex::Male),
            activity_factor: dec("1.40"),
        }
    }
    fn window(
        n_days: i64,
        kcal: impl Fn(i64) -> Option<i32>,
        trend: impl Fn(i64) -> Option<i32>,
    ) -> Vec<DayRow> {
        (0..n_days)
            .map(|i| DayRow {
                day: d("2026-09-01") + chrono::Duration::days(i),
                kcal: kcal(i),
                trend_g: trend(i),
            })
            .collect()
    }

    #[test]
    fn mifflin_for_both_sexes_and_missing_fields() {
        // 80 kg, 180 cm, 36 years old on 2026-09-28: 800 + 1125 − 180 + 5 = 1750 × 1.4 = 2450.
        assert_eq!(seed(&profile(), Some(80000), d("2026-09-28")), Some(2450));
        let mut f = profile();
        f.sex = Some(Sex::Female);
        // 1750 − 166 = 1584 × 1.4 = 2217.6 -> 2218.
        assert_eq!(seed(&f, Some(80000), d("2026-09-28")), Some(2218));
        // The day before the birthday is a year younger.
        assert_eq!(seed(&profile(), Some(80000), d("2026-06-14")), Some(2457));
        let mut p = profile();
        p.height_mm = None;
        assert_eq!(seed(&p, Some(80000), d("2026-09-28")), None);
        assert_eq!(seed(&profile(), None, d("2026-09-28")), None);
        assert_eq!("F".parse::<Sex>(), Ok(Sex::Female));
        assert!("x".parse::<Sex>().is_err());
    }

    #[test]
    fn below_the_threshold_it_is_the_seed_or_nothing() {
        let rows = window(
            28,
            |i| (i < 10).then_some(2000),
            |i| (i % 7 == 0).then_some(80000),
        );
        let e = estimate(&rows, Some(80000), d("2026-09-28"), &profile());
        assert_eq!(e.basis, Basis::Seed);
        assert_eq!(e.kcal, Some(2450));
        assert_eq!(e.logged_days, 10);
        assert_eq!(e.weight_span_days, 21);

        // Enough logged days but weigh-ins too close together.
        let rows = window(28, |_| Some(2000), |i| (i == 0 || i == 5).then_some(80000));
        let e = estimate(&rows, Some(80000), d("2026-09-28"), &profile());
        assert_eq!(e.basis, Basis::Seed);
        assert_eq!(e.weight_span_days, 5);

        let mut p = profile();
        p.born_on = None;
        let e = estimate(&rows, Some(80000), d("2026-09-28"), &p);
        assert_eq!((e.basis, e.kcal), (Basis::None, None));
    }

    #[test]
    fn adaptive_intake_minus_stored_energy() {
        // Steady 2000 kcal a day, weight flat: expenditure is 2000.
        let rows = window(28, |_| Some(2000), |i| (i % 7 == 0).then_some(80000));
        let e = estimate(&rows, Some(80000), d("2026-09-28"), &profile());
        assert_eq!((e.basis, e.kcal), (Basis::Adaptive, Some(2000)));
        assert_eq!(e.seed_kcal, Some(2450));

        // Lost 700 g over 21 days on 2000 a day: 700 × 7.7 / 21 ≈ 257 more burnt.
        let rows = window(
            28,
            |_| Some(2000),
            |i| match i {
                0 => Some(80000),
                7 => Some(79800),
                14 => Some(79500),
                21 => Some(79300),
                _ => None,
            },
        );
        let e = estimate(&rows, Some(79300), d("2026-09-28"), &profile());
        assert_eq!(e.kcal, Some(2257));

        // Gained 700 g on the same intake: 1743.
        let rows = window(
            28,
            |_| Some(2000),
            |i| match i {
                0 => Some(80000),
                21 => Some(80700),
                _ => None,
            },
        );
        assert_eq!(
            estimate(&rows, Some(80700), d("2026-09-28"), &profile()).kcal,
            Some(1743)
        );
    }

    #[test]
    fn a_flight_day_is_just_one_more_logged_day() {
        // Twenty-seven ordinary days and one 30-hour day with five meals.
        let rows = window(
            28,
            |i| Some(if i == 10 { 3600 } else { 2000 }),
            |i| (i % 7 == 0).then_some(80000),
        );
        let e = estimate(&rows, Some(80000), d("2026-09-28"), &profile());
        // (27 × 2000 + 3600) / 28 = 2057.
        assert_eq!(e.kcal, Some(2057));
        // Unlogged days are gaps, not zeros.
        let rows = window(
            28,
            |i| (i != 10).then_some(2000),
            |i| (i % 7 == 0).then_some(80000),
        );
        assert_eq!(
            estimate(&rows, Some(80000), d("2026-09-28"), &profile()).kcal,
            Some(2000)
        );
    }
}
