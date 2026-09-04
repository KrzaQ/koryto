//! Logging and changing meals, weigh-ins and sport for anyone in the
//! household. Every input is validated here once, for the API and MCP alike.

use chrono::{DateTime, NaiveDate, Utc};
use chrono_tz::Tz;
use rust_decimal::Decimal;
use serde::Deserialize;
use utoipa::ToSchema;

use super::scope;
use super::time::{self, Resolved};
use super::{AppError, AppResult, bad};
use crate::db::{
    Activity, ActivityPatch, Db, EntryKind, Food, Meal, MealPatch, NewActivity, NewMeal, NewWeight,
    SOURCE_ESTIMATE, SOURCE_FOOD, SOURCE_MANUAL, SOURCES, User, Weight, WeightPatch,
};
use crate::domain::day::parse_tz;
use crate::domain::duration::parse_minutes;
use crate::domain::units::{parse_kg, parse_portions, scale};
use crate::domain::when::When;

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct MealInput {
    /// Whose intake; empty or absent means the caller's. One entry per person.
    #[serde(default)]
    pub user_ids: Vec<i32>,
    /// RFC 3339 with an offset, or a wall-clock time (YYYY-MM-DD HH:MM) on the person's current clock; default now
    #[schema(value_type = Option<String>)]
    pub eaten_at: Option<When>,
    /// IANA zone to compute the day in, instead of the location history
    pub timezone: Option<String>,
    /// Count this meal for this day whatever the clock says
    pub day: Option<NaiveDate>,
    /// Required unless a food is given
    pub description: Option<String>,
    /// Required unless a food is given
    pub kcal: Option<i32>,
    pub protein_g: Option<i32>,
    /// estimate (default), manual or label; a food sets it to food
    pub source: Option<String>,
    /// A saved food: kcal and protein come from it times `portions`
    pub food_id: Option<i32>,
    /// Decimal portion count, default 1
    pub portions: Option<String>,
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct MealPatchInput {
    #[schema(value_type = Option<String>)]
    pub eaten_at: Option<When>,
    pub timezone: Option<String>,
    /// Set the accounting day by hand; null goes back to the computed day
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<String>)]
    pub day: Option<Option<NaiveDate>>,
    pub description: Option<String>,
    /// An explicit number unlinks the meal from its food
    pub kcal: Option<i32>,
    #[serde(default, with = "::serde_with::rust::double_option")]
    pub protein_g: Option<Option<i32>>,
    pub source: Option<String>,
    /// Link to a food (its numbers replace the meal's); null unlinks
    #[serde(default, with = "::serde_with::rust::double_option")]
    pub food_id: Option<Option<i32>>,
    pub portions: Option<String>,
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct WeightInput {
    /// Whose weight; default the caller
    pub user_id: Option<i32>,
    #[schema(value_type = Option<String>)]
    pub measured_at: Option<When>,
    pub timezone: Option<String>,
    pub day: Option<NaiveDate>,
    /// Kilograms, e.g. "82.4"
    pub weight_kg: String,
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct WeightPatchInput {
    #[schema(value_type = Option<String>)]
    pub measured_at: Option<When>,
    pub timezone: Option<String>,
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<String>)]
    pub day: Option<Option<NaiveDate>>,
    pub weight_kg: Option<String>,
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct ActivityInput {
    pub user_id: Option<i32>,
    #[schema(value_type = Option<String>)]
    pub started_at: Option<When>,
    pub timezone: Option<String>,
    pub day: Option<NaiveDate>,
    /// run, gym, cycling, ...
    pub kind: String,
    /// 45, 45m, 1h, 1h30, 1:30
    pub duration: String,
    /// Informational only; never enters the balance
    pub kcal: Option<i32>,
    pub note: Option<String>,
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct ActivityPatchInput {
    #[schema(value_type = Option<String>)]
    pub started_at: Option<When>,
    pub timezone: Option<String>,
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<String>)]
    pub day: Option<Option<NaiveDate>>,
    pub kind: Option<String>,
    pub duration: Option<String>,
    #[serde(default, with = "::serde_with::rust::double_option")]
    pub kcal: Option<Option<i32>>,
    pub note: Option<String>,
}

fn zone_override(tz: &Option<String>) -> AppResult<Option<Tz>> {
    tz.as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(|s| parse_tz(s).map_err(bad))
        .transpose()
}

fn non_negative(value: Option<i32>, what: &str) -> AppResult<Option<i32>> {
    match value {
        Some(v) if v < 0 => Err(bad(format!("{what} cannot be negative"))),
        v => Ok(v),
    }
}

fn check_source(source: Option<&str>) -> AppResult<String> {
    let s = source.unwrap_or(SOURCE_ESTIMATE).trim().to_lowercase();
    if s == SOURCE_FOOD {
        return Err(bad(
            "source \"food\" comes from giving a food, not from naming it",
        ));
    }
    if !SOURCES.contains(&s.as_str()) {
        return Err(bad(format!(
            "source must be estimate, manual or label, not {s:?}"
        )));
    }
    Ok(s)
}

fn check_text(text: Option<&str>, what: &str) -> AppResult<String> {
    let t = text.unwrap_or("").trim();
    if t.is_empty() {
        return Err(bad(format!("{what} cannot be empty")));
    }
    Ok(t.to_string())
}

/// A wall-clock time is read on the person's current clock (or the explicit
/// zone); an instant stays as it is; nothing means now.
async fn instant_of(
    db: &Db,
    user: &User,
    when: Option<When>,
    zone: Option<Tz>,
) -> AppResult<DateTime<Utc>> {
    Ok(match when {
        None => Utc::now(),
        Some(When::Instant(i)) => i,
        Some(When::Wall(w)) => {
            let tz = match zone {
                Some(tz) => tz,
                None => time::current_zone(db, user).await?,
            };
            crate::domain::when::resolve_wall(w, tz)
        }
    })
}

async fn resolve_new(
    db: &Db,
    user: &User,
    when: Option<When>,
    timezone: &Option<String>,
    day: Option<NaiveDate>,
) -> AppResult<Resolved> {
    let zone = zone_override(timezone)?;
    let instant = instant_of(db, user, when, zone).await?;
    Ok(time::resolve(db, user, Some(instant), zone, day).await?)
}

/// The time fields an entry has now, for [`resolve_patch`].
struct Current {
    instant: DateTime<Utc>,
    day: NaiveDate,
    day_override: bool,
}

/// The time fields a patch may change.
struct TimePatch<'a> {
    when: Option<When>,
    timezone: &'a Option<String>,
    day: Option<Option<NaiveDate>>,
}

/// Where a patch moves an entry in time: None when the patch does not touch
/// time at all. A hand-set day survives unless the patch sets `day`.
async fn resolve_patch(
    db: &Db,
    user: &User,
    current: Current,
    patch: TimePatch<'_>,
) -> AppResult<Option<Resolved>> {
    if patch.when.is_none() && patch.timezone.is_none() && patch.day.is_none() {
        return Ok(None);
    }
    let zone = zone_override(patch.timezone)?;
    let instant = match patch.when {
        None => current.instant,
        Some(w) => instant_of(db, user, Some(w), zone).await?,
    };
    let keep = match patch.day {
        Some(d) => d,
        None if current.day_override => Some(current.day),
        None => None,
    };
    Ok(Some(
        time::resolve(db, user, Some(instant), zone, keep).await?,
    ))
}

struct Numbers {
    description: String,
    kcal: i32,
    protein_g: Option<i32>,
    source: String,
    food_id: Option<i32>,
    portions: Option<Decimal>,
}

fn from_food(food: &Food, portions: Option<&str>, description: Option<&str>) -> AppResult<Numbers> {
    let portions = match portions {
        Some(p) => parse_portions(p).map_err(|e| bad(e.to_string()))?,
        None => Decimal::ONE,
    };
    Ok(Numbers {
        description: description
            .map(str::trim)
            .filter(|d| !d.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| food.name.clone()),
        kcal: scale(food.kcal, portions),
        protein_g: food.protein_g.map(|p| scale(p, portions)),
        source: SOURCE_FOOD.into(),
        food_id: Some(food.id),
        portions: Some(portions),
    })
}

// ----- meals ---------------------------------------------------------------

pub async fn log_meals(db: &Db, actor: &User, via: &str, input: MealInput) -> AppResult<Vec<Meal>> {
    let mut users = Vec::new();
    if input.user_ids.is_empty() {
        users.push(scope::member(db, actor, None).await?);
    }
    for id in &input.user_ids {
        if users.iter().any(|u: &User| u.id == *id) {
            continue;
        }
        users.push(scope::member(db, actor, Some(*id)).await?);
    }
    let numbers = match input.food_id {
        Some(id) => {
            let food = scope::food(db, actor, id).await?;
            if food.archived_at.is_some() {
                return Err(bad(format!("{} is archived", food.name)));
            }
            from_food(
                &food,
                input.portions.as_deref(),
                input.description.as_deref(),
            )?
        }
        None => {
            if input.portions.is_some() {
                return Err(bad("portions only make sense with a food"));
            }
            Numbers {
                description: check_text(input.description.as_deref(), "description")?,
                kcal: non_negative(input.kcal, "kcal")?
                    .ok_or_else(|| bad("kcal is required without a food"))?,
                protein_g: non_negative(input.protein_g, "protein_g")?,
                source: check_source(input.source.as_deref())?,
                food_id: None,
                portions: None,
            }
        }
    };
    let mut out = Vec::with_capacity(users.len());
    for user in &users {
        let r = resolve_new(db, user, input.eaten_at, &input.timezone, input.day).await?;
        out.push(
            db.insert_meal(NewMeal {
                user_id: user.id,
                eaten_at: r.instant,
                timezone: r.timezone,
                day: r.day,
                day_override: r.day_override,
                description: numbers.description.clone(),
                kcal: numbers.kcal,
                protein_g: numbers.protein_g,
                source: numbers.source.clone(),
                food_id: numbers.food_id,
                portions: numbers.portions,
                created_by: actor.id,
                created_via: via.into(),
            })
            .await?,
        );
    }
    Ok(out)
}

pub async fn update_meal(db: &Db, actor: &User, id: i32, input: MealPatchInput) -> AppResult<Meal> {
    let owner = scope::owned_entry(db, actor, EntryKind::Meal, id).await?;
    let current = db.get_meal(id).await?;
    let mut patch = MealPatch::default();
    if let Some(r) = resolve_patch(
        db,
        &owner,
        Current {
            instant: current.eaten_at,
            day: current.day,
            day_override: current.day_override,
        },
        TimePatch {
            when: input.eaten_at,
            timezone: &input.timezone,
            day: input.day,
        },
    )
    .await?
    {
        patch.eaten_at = Some(r.instant);
        patch.timezone = Some(r.timezone);
        patch.day = Some(r.day);
        patch.day_override = Some(r.day_override);
    }
    if let Some(d) = input.description.as_deref() {
        patch.description = Some(check_text(Some(d), "description")?);
    }
    // Numbers: a food link wins, then an explicit kcal, then a portion change.
    let food_after: Option<Option<i32>> = match input.food_id {
        Some(f) => Some(f),
        None if input.kcal.is_some() => Some(None),
        None => None,
    };
    match food_after {
        Some(Some(food_id)) => {
            let food = scope::food(db, actor, food_id).await?;
            let portions = input
                .portions
                .clone()
                .or_else(|| current.portions.map(|p| p.to_string()));
            let n = from_food(
                &food,
                portions.as_deref(),
                input.description.as_deref().or(Some(&current.description)),
            )?;
            patch.kcal = Some(n.kcal);
            patch.protein_g = Some(n.protein_g);
            patch.source = Some(n.source);
            patch.food_id = Some(n.food_id);
            patch.portions = Some(n.portions);
            if input.description.is_none() && current.food_id != Some(food.id) {
                patch.description = Some(food.name.clone());
            }
        }
        Some(None) => {
            let kcal = non_negative(input.kcal, "kcal")?;
            patch.kcal = Some(kcal.unwrap_or(current.kcal));
            patch.food_id = Some(None);
            patch.portions = Some(None);
            patch.source = Some(check_source(
                input.source.as_deref().or(Some(SOURCE_MANUAL)),
            )?);
            if let Some(p) = input.protein_g {
                patch.protein_g = Some(non_negative(p, "protein_g")?);
            }
        }
        None => {
            if let Some(p) = input.portions.as_deref() {
                let food_id = current
                    .food_id
                    .ok_or_else(|| bad("portions only make sense with a food"))?;
                let food = scope::food(db, actor, food_id).await?;
                let n = from_food(&food, Some(p), Some(&current.description))?;
                patch.kcal = Some(n.kcal);
                patch.protein_g = Some(n.protein_g);
                patch.portions = Some(n.portions);
            }
            if let Some(p) = input.protein_g {
                patch.protein_g = Some(non_negative(p, "protein_g")?);
            }
            if let Some(s) = input.source.as_deref() {
                if current.food_id.is_some() {
                    return Err(bad(
                        "a meal logged from a food keeps source \"food\"; give kcal to unlink it",
                    ));
                }
                patch.source = Some(check_source(Some(s))?);
            }
        }
    }
    Ok(db.update_meal(id, patch).await?)
}

// ----- weights -------------------------------------------------------------

pub async fn log_weight(db: &Db, actor: &User, via: &str, input: WeightInput) -> AppResult<Weight> {
    let user = scope::member(db, actor, input.user_id).await?;
    let weight_g = parse_kg(&input.weight_kg).map_err(|e| bad(e.to_string()))?;
    let r = resolve_new(db, &user, input.measured_at, &input.timezone, input.day).await?;
    Ok(db
        .insert_weight(NewWeight {
            user_id: user.id,
            measured_at: r.instant,
            timezone: r.timezone,
            day: r.day,
            day_override: r.day_override,
            weight_g,
            created_by: actor.id,
            created_via: via.into(),
        })
        .await?)
}

pub async fn update_weight(
    db: &Db,
    actor: &User,
    id: i32,
    input: WeightPatchInput,
) -> AppResult<Weight> {
    let owner = scope::owned_entry(db, actor, EntryKind::Weight, id).await?;
    let current = db.get_weight(id).await?;
    let mut patch = WeightPatch::default();
    if let Some(r) = resolve_patch(
        db,
        &owner,
        Current {
            instant: current.measured_at,
            day: current.day,
            day_override: current.day_override,
        },
        TimePatch {
            when: input.measured_at,
            timezone: &input.timezone,
            day: input.day,
        },
    )
    .await?
    {
        patch.measured_at = Some(r.instant);
        patch.timezone = Some(r.timezone);
        patch.day = Some(r.day);
        patch.day_override = Some(r.day_override);
    }
    if let Some(kg) = input.weight_kg.as_deref() {
        patch.weight_g = Some(parse_kg(kg).map_err(|e| bad(e.to_string()))?);
    }
    Ok(db.update_weight(id, patch).await?)
}

// ----- activities ----------------------------------------------------------

pub async fn log_activity(
    db: &Db,
    actor: &User,
    via: &str,
    input: ActivityInput,
) -> AppResult<Activity> {
    let user = scope::member(db, actor, input.user_id).await?;
    let kind = check_text(Some(&input.kind), "kind")?;
    let minutes = parse_minutes(&input.duration).map_err(|e| bad(e.to_string()))?;
    let kcal = non_negative(input.kcal, "kcal")?;
    let r = resolve_new(db, &user, input.started_at, &input.timezone, input.day).await?;
    Ok(db
        .insert_activity(NewActivity {
            user_id: user.id,
            started_at: r.instant,
            timezone: r.timezone,
            day: r.day,
            day_override: r.day_override,
            kind,
            minutes,
            kcal,
            note: input.note.unwrap_or_default(),
            created_by: actor.id,
            created_via: via.into(),
        })
        .await?)
}

pub async fn update_activity(
    db: &Db,
    actor: &User,
    id: i32,
    input: ActivityPatchInput,
) -> AppResult<Activity> {
    let owner = scope::owned_entry(db, actor, EntryKind::Activity, id).await?;
    let current = db.get_activity(id).await?;
    let mut patch = ActivityPatch::default();
    if let Some(r) = resolve_patch(
        db,
        &owner,
        Current {
            instant: current.started_at,
            day: current.day,
            day_override: current.day_override,
        },
        TimePatch {
            when: input.started_at,
            timezone: &input.timezone,
            day: input.day,
        },
    )
    .await?
    {
        patch.started_at = Some(r.instant);
        patch.timezone = Some(r.timezone);
        patch.day = Some(r.day);
        patch.day_override = Some(r.day_override);
    }
    if let Some(k) = input.kind.as_deref() {
        patch.kind = Some(check_text(Some(k), "kind")?);
    }
    if let Some(d) = input.duration.as_deref() {
        patch.minutes = Some(parse_minutes(d).map_err(|e| bad(e.to_string()))?);
    }
    if let Some(k) = input.kcal {
        patch.kcal = Some(non_negative(k, "kcal")?);
    }
    patch.note = input.note;
    Ok(db.update_activity(id, patch).await?)
}

// ----- void ----------------------------------------------------------------

pub async fn void(db: &Db, actor: &User, kind: EntryKind, id: i32) -> AppResult<()> {
    scope::owned_entry(db, actor, kind, id).await?;
    db.void_entry(kind, id).await.map_err(|e| match e {
        crate::db::DbError::NotFound => AppError::Conflict("already voided".into()),
        e => e.into(),
    })
}

pub async fn unvoid(db: &Db, actor: &User, kind: EntryKind, id: i32) -> AppResult<()> {
    scope::owned_entry(db, actor, kind, id).await?;
    db.unvoid_entry(kind, id).await.map_err(|e| match e {
        crate::db::DbError::NotFound => AppError::Conflict("not voided".into()),
        e => e.into(),
    })
}
