//! Wire types. Weight travels as decimal kilograms and durations in the
//! input grammar; grams and minutes never leave the server unlabelled.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::app::day::{DayRow, DayView, Totals};
use crate::db::{
    Activity, Food, FoodWithUsage, Household, Meal, Target, User, UserLocation, Weight,
};
use crate::domain::duration::format_minutes;
use crate::domain::expenditure::Estimate;
use crate::domain::units::format_kg;

#[derive(Serialize, ToSchema)]
pub struct UserDto {
    pub id: i32,
    pub name: Option<String>,
    pub email: Option<String>,
    pub household_id: Option<i32>,
    pub day_boundary_minutes: i32,
    pub height_mm: Option<i32>,
    pub born_on: Option<NaiveDate>,
    pub sex: Option<String>,
    #[schema(value_type = String)]
    pub activity_factor: Decimal,
}

impl From<User> for UserDto {
    fn from(u: User) -> Self {
        Self {
            id: u.id,
            name: u.name,
            email: u.email,
            household_id: u.household_id,
            day_boundary_minutes: u.day_boundary_minutes,
            height_mm: u.height_mm,
            born_on: u.born_on,
            sex: u.sex,
            activity_factor: u.activity_factor,
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct MemberDto {
    pub id: i32,
    pub name: Option<String>,
    pub email: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct HouseholdDto {
    pub id: i32,
    pub name: String,
    pub members: Vec<MemberDto>,
}

impl HouseholdDto {
    pub fn from(h: Household, members: Vec<User>) -> Self {
        Self {
            id: h.id,
            name: h.name,
            members: members
                .into_iter()
                .map(|u| MemberDto {
                    id: u.id,
                    name: u.name,
                    email: u.email,
                })
                .collect(),
        }
    }
}

#[derive(Deserialize, ToSchema, Default)]
pub struct ProfilePatchInput {
    pub name: Option<String>,
    /// Minutes after midnight the day starts; default 240 (04:00)
    pub day_boundary_minutes: Option<i32>,
    #[serde(default, with = "::serde_with::rust::double_option")]
    pub height_mm: Option<Option<i32>>,
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<String>)]
    pub born_on: Option<Option<NaiveDate>>,
    /// female or male, for the Mifflin-St Jeor seed
    #[serde(default, with = "::serde_with::rust::double_option")]
    pub sex: Option<Option<String>>,
    /// Mifflin multiplier, 1.00 to 2.50
    pub activity_factor: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct LocationDto {
    pub id: i32,
    pub user_id: i32,
    /// RFC 3339, UTC; the origin row is far in the past
    #[schema(value_type = String, format = DateTime)]
    pub valid_from: DateTime<Utc>,
    pub timezone: String,
    pub origin: bool,
}

impl From<UserLocation> for LocationDto {
    fn from(l: UserLocation) -> Self {
        let origin = l.is_origin();
        Self {
            id: l.id,
            user_id: l.user_id,
            valid_from: l.valid_from,
            timezone: l.timezone,
            origin,
        }
    }
}

#[derive(Deserialize, ToSchema)]
pub struct LocationInput {
    /// RFC 3339; default now
    #[schema(value_type = Option<String>, format = DateTime)]
    pub valid_from: Option<DateTime<Utc>>,
    pub timezone: String,
}

#[derive(Deserialize, ToSchema, Default)]
pub struct LocationPatchInput {
    #[schema(value_type = Option<String>, format = DateTime)]
    pub valid_from: Option<DateTime<Utc>>,
    pub timezone: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct TargetDto {
    pub id: i32,
    pub user_id: i32,
    pub valid_from: NaiveDate,
    pub kcal: i32,
    pub protein_g: Option<i32>,
    /// Goal weight in kilograms
    pub weight_kg: Option<String>,
}

impl From<Target> for TargetDto {
    fn from(t: Target) -> Self {
        Self {
            id: t.id,
            user_id: t.user_id,
            valid_from: t.valid_from,
            kcal: t.kcal,
            protein_g: t.protein_g,
            weight_kg: t.weight_g.map(format_kg),
        }
    }
}

#[derive(Deserialize, ToSchema)]
pub struct TargetInput {
    /// Default today
    pub valid_from: Option<NaiveDate>,
    pub kcal: i32,
    pub protein_g: Option<i32>,
    pub weight_kg: Option<String>,
}

#[derive(Deserialize, ToSchema, Default)]
pub struct TargetPatchInput {
    pub valid_from: Option<NaiveDate>,
    pub kcal: Option<i32>,
    #[serde(default, with = "::serde_with::rust::double_option")]
    pub protein_g: Option<Option<i32>>,
    #[serde(default, with = "::serde_with::rust::double_option")]
    pub weight_kg: Option<Option<String>>,
}

#[derive(Serialize, ToSchema)]
pub struct FoodDto {
    pub id: i32,
    pub name: String,
    pub aliases: Vec<String>,
    pub portion: String,
    pub kcal: i32,
    pub protein_g: Option<i32>,
    pub created_by: i32,
    #[schema(value_type = String, format = DateTime)]
    pub created_at: DateTime<Utc>,
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: DateTime<Utc>,
    #[schema(value_type = Option<String>, format = DateTime)]
    pub archived_at: Option<DateTime<Utc>>,
    /// Non-voided meals logged against it
    pub uses: i64,
}

impl FoodDto {
    pub fn from(f: Food, uses: i64) -> Self {
        Self {
            id: f.id,
            name: f.name,
            aliases: f.aliases,
            portion: f.portion,
            kcal: f.kcal,
            protein_g: f.protein_g,
            created_by: f.created_by,
            created_at: f.created_at,
            updated_at: f.updated_at,
            archived_at: f.archived_at,
            uses,
        }
    }
}

impl From<FoodWithUsage> for FoodDto {
    fn from(f: FoodWithUsage) -> Self {
        Self::from(f.food, f.uses)
    }
}

#[derive(Deserialize, ToSchema)]
pub struct FoodInput {
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    /// What one portion is: "1 bowl (350 g)"
    pub portion: String,
    pub kcal: i32,
    pub protein_g: Option<i32>,
}

#[derive(Deserialize, ToSchema, Default)]
pub struct FoodPatchInput {
    pub name: Option<String>,
    pub aliases: Option<Vec<String>>,
    pub portion: Option<String>,
    pub kcal: Option<i32>,
    #[serde(default, with = "::serde_with::rust::double_option")]
    pub protein_g: Option<Option<i32>>,
}

#[derive(Serialize, ToSchema)]
pub struct MealDto {
    pub id: i32,
    pub user_id: i32,
    #[schema(value_type = String, format = DateTime)]
    pub eaten_at: DateTime<Utc>,
    pub timezone: String,
    pub day: NaiveDate,
    pub day_override: bool,
    pub description: String,
    pub kcal: i32,
    pub protein_g: Option<i32>,
    pub source: String,
    pub food_id: Option<i32>,
    pub portions: Option<String>,
    pub created_by: i32,
    pub created_via: String,
    pub voided: bool,
}

impl From<Meal> for MealDto {
    fn from(m: Meal) -> Self {
        Self {
            id: m.id,
            user_id: m.user_id,
            eaten_at: m.eaten_at,
            timezone: m.timezone,
            day: m.day,
            day_override: m.day_override,
            description: m.description,
            kcal: m.kcal,
            protein_g: m.protein_g,
            source: m.source,
            food_id: m.food_id,
            portions: m.portions.map(|p| p.normalize().to_string()),
            created_by: m.created_by,
            created_via: m.created_via,
            voided: m.voided_at.is_some(),
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct WeightDto {
    pub id: i32,
    pub user_id: i32,
    #[schema(value_type = String, format = DateTime)]
    pub measured_at: DateTime<Utc>,
    pub timezone: String,
    pub day: NaiveDate,
    pub day_override: bool,
    pub weight_kg: String,
    pub weight_g: i32,
    pub created_by: i32,
    pub created_via: String,
    pub voided: bool,
}

impl From<Weight> for WeightDto {
    fn from(w: Weight) -> Self {
        Self {
            id: w.id,
            user_id: w.user_id,
            measured_at: w.measured_at,
            timezone: w.timezone,
            day: w.day,
            day_override: w.day_override,
            weight_kg: format_kg(w.weight_g),
            weight_g: w.weight_g,
            created_by: w.created_by,
            created_via: w.created_via,
            voided: w.voided_at.is_some(),
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct ActivityDto {
    pub id: i32,
    pub user_id: i32,
    #[schema(value_type = String, format = DateTime)]
    pub started_at: DateTime<Utc>,
    pub timezone: String,
    pub day: NaiveDate,
    pub day_override: bool,
    pub kind: String,
    pub minutes: i32,
    /// 45m, 1h, 1h30
    pub duration: String,
    pub kcal: Option<i32>,
    pub note: String,
    pub created_by: i32,
    pub created_via: String,
    pub voided: bool,
}

impl From<Activity> for ActivityDto {
    fn from(a: Activity) -> Self {
        Self {
            id: a.id,
            user_id: a.user_id,
            started_at: a.started_at,
            timezone: a.timezone,
            day: a.day,
            day_override: a.day_override,
            kind: a.kind,
            minutes: a.minutes,
            duration: format_minutes(a.minutes),
            kcal: a.kcal,
            note: a.note,
            created_by: a.created_by,
            created_via: a.created_via,
            voided: a.voided_at.is_some(),
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct DayDto {
    pub day: NaiveDate,
    pub user_id: i32,
    pub logged: bool,
    pub meals: Vec<MealDto>,
    pub weights: Vec<WeightDto>,
    pub activities: Vec<ActivityDto>,
    pub totals: Totals,
    pub target: Option<TargetDto>,
    pub balance: Option<i32>,
    /// The expenditure estimate as of this day and its basis
    pub expenditure: Estimate,
    /// kcal minus the estimate, on a logged day with an estimate
    pub balance_vs_expenditure: Option<i32>,
}

impl From<DayView> for DayDto {
    fn from(v: DayView) -> Self {
        Self {
            day: v.day,
            user_id: v.user_id,
            logged: v.logged,
            meals: v.meals.into_iter().map(Into::into).collect(),
            weights: v.weights.into_iter().map(Into::into).collect(),
            activities: v.activities.into_iter().map(Into::into).collect(),
            totals: v.totals,
            target: v.target.map(Into::into),
            balance: v.balance,
            expenditure: v.expenditure,
            balance_vs_expenditure: v.balance_vs_expenditure,
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct DaysDto {
    pub user_id: i32,
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub days: Vec<DayRow>,
}

#[derive(Deserialize, ToSchema)]
pub struct TokenInput {
    pub name: String,
    /// Comma-separated: read, write, edit, delegate
    pub scopes: String,
    /// The person a personal token acts as; default the caller. Not for delegate tokens.
    pub user_id: Option<i32>,
}

#[derive(Serialize, ToSchema)]
pub struct TokenDto {
    pub id: i32,
    pub name: String,
    pub scopes: Vec<String>,
    pub user_id: Option<i32>,
    pub created_by: Option<i32>,
    #[schema(value_type = String, format = DateTime)]
    pub created_at: DateTime<Utc>,
    #[schema(value_type = Option<String>, format = DateTime)]
    pub last_used_at: Option<DateTime<Utc>>,
    #[schema(value_type = Option<String>, format = DateTime)]
    pub revoked_at: Option<DateTime<Utc>>,
}

impl From<crate::db::ApiToken> for TokenDto {
    fn from(t: crate::db::ApiToken) -> Self {
        Self {
            id: t.id,
            name: t.name,
            scopes: t.scopes,
            user_id: t.user_id,
            created_by: t.created_by,
            created_at: t.created_at,
            last_used_at: t.last_used_at,
            revoked_at: t.revoked_at,
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct TokenCreated {
    pub id: i32,
    pub name: String,
    pub scopes: Vec<String>,
    pub user_id: Option<i32>,
    /// Shown once, never stored
    pub secret: String,
}
