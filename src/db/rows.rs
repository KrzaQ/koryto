use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// How an entry was created; stored on every meal, weight and activity.
pub const VIA_WEB: &str = "web";
pub const VIA_MCP: &str = "mcp";

/// Where a meal's number came from.
pub const SOURCE_ESTIMATE: &str = "estimate";
pub const SOURCE_MANUAL: &str = "manual";
pub const SOURCE_LABEL: &str = "label";
pub const SOURCE_FOOD: &str = "food";
pub const SOURCES: [&str; 4] = [SOURCE_ESTIMATE, SOURCE_MANUAL, SOURCE_LABEL, SOURCE_FOOD];
/// A session's kcal computed from its kind's MET rate, not given by hand.
pub const SOURCE_MET: &str = "met";

/// The origin location row of every user: far enough back that no entry
/// can precede it. Postgres `-infinity` would do, but sqlx cannot decode it.
pub fn origin() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("0001-01-01T00:00:00Z")
        .expect("origin")
        .with_timezone(&Utc)
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, PartialEq)]
pub struct Household {
    pub id: i32,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, PartialEq)]
pub struct User {
    pub id: i32,
    pub subject: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub household_id: Option<i32>,
    pub day_boundary_minutes: i32,
    pub height_mm: Option<i32>,
    pub born_on: Option<NaiveDate>,
    pub sex: Option<String>,
    pub activity_factor: Decimal,
    pub created_at: DateTime<Utc>,
    pub last_login_at: Option<DateTime<Utc>>,
}

impl User {
    /// Name, else email, else the subject: what the UI and MCP show.
    pub fn display(&self) -> &str {
        self.name
            .as_deref()
            .or(self.email.as_deref())
            .unwrap_or(&self.subject)
    }
}

/// `Some(None)` clears a nullable column; `None` leaves it alone.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct ProfilePatch {
    pub name: Option<String>,
    pub day_boundary_minutes: Option<i32>,
    pub height_mm: Option<Option<i32>>,
    pub born_on: Option<Option<NaiveDate>>,
    pub sex: Option<Option<String>>,
    pub activity_factor: Option<Decimal>,
}

#[derive(Debug, Clone, FromRow, Serialize, PartialEq)]
pub struct UserLocation {
    pub id: i32,
    pub user_id: i32,
    pub valid_from: DateTime<Utc>,
    pub timezone: String,
}

impl UserLocation {
    pub fn is_origin(&self) -> bool {
        self.valid_from == origin()
    }
}

#[derive(Debug, Clone, FromRow, Serialize, PartialEq)]
pub struct Target {
    pub id: i32,
    pub user_id: i32,
    pub valid_from: NaiveDate,
    pub kcal: i32,
    pub protein_g: Option<i32>,
    pub weight_g: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct NewTarget {
    pub user_id: i32,
    pub valid_from: NaiveDate,
    pub kcal: i32,
    pub protein_g: Option<i32>,
    pub weight_g: Option<i32>,
}

#[derive(Debug, Default, Clone)]
pub struct TargetPatch {
    pub valid_from: Option<NaiveDate>,
    pub kcal: Option<i32>,
    pub protein_g: Option<Option<i32>>,
    pub weight_g: Option<Option<i32>>,
}

#[derive(Debug, Clone, FromRow, Serialize, PartialEq)]
pub struct Food {
    pub id: i32,
    pub household_id: i32,
    pub name: String,
    pub aliases: Vec<String>,
    pub portion: String,
    pub kcal: i32,
    pub protein_g: Option<i32>,
    pub created_by: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub archived_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct NewFood {
    pub household_id: i32,
    pub name: String,
    pub aliases: Vec<String>,
    pub portion: String,
    pub kcal: i32,
    pub protein_g: Option<i32>,
    pub created_by: i32,
}

#[derive(Debug, Default, Clone)]
pub struct FoodPatch {
    pub name: Option<String>,
    pub aliases: Option<Vec<String>>,
    pub portion: Option<String>,
    pub kcal: Option<i32>,
    pub protein_g: Option<Option<i32>>,
}

/// A food with how many non-voided meals reference it.
#[derive(Debug, Clone, FromRow, Serialize, PartialEq)]
pub struct FoodWithUsage {
    #[sqlx(flatten)]
    #[serde(flatten)]
    pub food: Food,
    pub uses: i64,
}

#[derive(Debug, Clone, FromRow, Serialize, PartialEq)]
pub struct Meal {
    pub id: i32,
    pub user_id: i32,
    pub eaten_at: DateTime<Utc>,
    pub timezone: String,
    pub day: NaiveDate,
    pub day_override: bool,
    pub description: String,
    pub kcal: i32,
    pub protein_g: Option<i32>,
    pub source: String,
    pub food_id: Option<i32>,
    pub portions: Option<Decimal>,
    pub created_by: i32,
    pub created_via: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub voided_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct NewMeal {
    pub user_id: i32,
    pub eaten_at: DateTime<Utc>,
    pub timezone: String,
    pub day: NaiveDate,
    pub day_override: bool,
    pub description: String,
    pub kcal: i32,
    pub protein_g: Option<i32>,
    pub source: String,
    pub food_id: Option<i32>,
    pub portions: Option<Decimal>,
    pub created_by: i32,
    pub created_via: String,
}

/// Time fields are patched together: the caller has already recomputed the
/// day for a new instant or zone.
#[derive(Debug, Default, Clone)]
pub struct MealPatch {
    pub eaten_at: Option<DateTime<Utc>>,
    pub timezone: Option<String>,
    pub day: Option<NaiveDate>,
    pub day_override: Option<bool>,
    pub description: Option<String>,
    pub kcal: Option<i32>,
    pub protein_g: Option<Option<i32>>,
    pub source: Option<String>,
    pub food_id: Option<Option<i32>>,
    pub portions: Option<Option<Decimal>>,
}

#[derive(Debug, Clone, FromRow, Serialize, PartialEq)]
pub struct Weight {
    pub id: i32,
    pub user_id: i32,
    pub measured_at: DateTime<Utc>,
    pub timezone: String,
    pub day: NaiveDate,
    pub day_override: bool,
    pub weight_g: i32,
    pub created_by: i32,
    pub created_via: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub voided_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct NewWeight {
    pub user_id: i32,
    pub measured_at: DateTime<Utc>,
    pub timezone: String,
    pub day: NaiveDate,
    pub day_override: bool,
    pub weight_g: i32,
    pub created_by: i32,
    pub created_via: String,
}

#[derive(Debug, Default, Clone)]
pub struct WeightPatch {
    pub measured_at: Option<DateTime<Utc>>,
    pub timezone: Option<String>,
    pub day: Option<NaiveDate>,
    pub day_override: Option<bool>,
    pub weight_g: Option<i32>,
}

#[derive(Debug, Clone, FromRow, Serialize, PartialEq)]
pub struct Activity {
    pub id: i32,
    pub user_id: i32,
    pub started_at: DateTime<Utc>,
    pub timezone: String,
    pub day: NaiveDate,
    pub day_override: bool,
    pub kind: String,
    pub minutes: i32,
    pub kcal: Option<i32>,
    /// Where the kcal came from: "manual" (given) or "met" (from the kind's rate)
    pub source: String,
    pub activity_kind_id: Option<i32>,
    pub note: String,
    pub created_by: i32,
    pub created_via: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub voided_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct NewActivity {
    pub user_id: i32,
    pub started_at: DateTime<Utc>,
    pub timezone: String,
    pub day: NaiveDate,
    pub day_override: bool,
    pub kind: String,
    pub minutes: i32,
    pub kcal: Option<i32>,
    pub source: String,
    pub activity_kind_id: Option<i32>,
    pub note: String,
    pub created_by: i32,
    pub created_via: String,
}

#[derive(Debug, Default, Clone)]
pub struct ActivityPatch {
    pub started_at: Option<DateTime<Utc>>,
    pub timezone: Option<String>,
    pub day: Option<NaiveDate>,
    pub day_override: Option<bool>,
    pub kind: Option<String>,
    pub minutes: Option<i32>,
    pub kcal: Option<Option<i32>>,
    pub source: Option<String>,
    pub activity_kind_id: Option<Option<i32>>,
    pub note: Option<String>,
}

/// A kind of sport and what it costs: MET is the multiple of resting
/// metabolism it demands. Reference data, shared by every household.
#[derive(Debug, Clone, FromRow, Serialize, PartialEq)]
pub struct ActivityKind {
    pub id: i32,
    pub name: String,
    pub aliases: Vec<String>,
    pub met: Decimal,
    pub note: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub archived_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct NewActivityKind {
    pub name: String,
    pub aliases: Vec<String>,
    pub met: Decimal,
    pub note: String,
}

#[derive(Debug, Default, Clone)]
pub struct ActivityKindPatch {
    pub name: Option<String>,
    pub aliases: Option<Vec<String>>,
    pub met: Option<Decimal>,
    pub note: Option<String>,
}

/// The three entry tables, for code that treats them alike (void, recompute).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    Meal,
    Weight,
    Activity,
}

impl EntryKind {
    pub fn table(self) -> &'static str {
        match self {
            Self::Meal => "meals",
            Self::Weight => "weights",
            Self::Activity => "activities",
        }
    }

    pub fn instant_column(self) -> &'static str {
        match self {
            Self::Meal => "eaten_at",
            Self::Weight => "measured_at",
            Self::Activity => "started_at",
        }
    }
}

impl std::str::FromStr for EntryKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "meal" | "meals" => Ok(Self::Meal),
            "weight" | "weights" | "weigh-in" => Ok(Self::Weight),
            "activity" | "activities" | "sport" => Ok(Self::Activity),
            other => Err(format!("{other:?} is not meal, weight or activity")),
        }
    }
}

/// One row of an entry table as the recompute sees it.
#[derive(Debug, Clone, FromRow, PartialEq)]
pub struct EntryTime {
    pub id: i32,
    pub instant: DateTime<Utc>,
    pub timezone: String,
    pub day: NaiveDate,
}

/// Meal totals for one day, non-voided rows only.
#[derive(Debug, Clone, FromRow, Serialize, PartialEq)]
pub struct MealDayTotals {
    pub day: NaiveDate,
    pub kcal: i32,
    /// Sum over the meals that have protein; NULL when none do.
    pub protein_g: Option<i32>,
    pub meals: i32,
    pub meals_without_protein: i32,
}

#[derive(Debug, Clone, FromRow, Serialize, PartialEq)]
pub struct ActivityDayTotals {
    pub day: NaiveDate,
    pub minutes: i32,
    pub activities: i32,
    /// Sum over the entries that carry kcal; None when none do
    pub kcal: Option<i32>,
}

/// The day's weight: the earliest non-voided reading.
#[derive(Debug, Clone, FromRow, Serialize, PartialEq)]
pub struct DayWeight {
    pub day: NaiveDate,
    pub weight_g: i32,
}

#[derive(Debug, Clone, FromRow, Serialize, PartialEq)]
pub struct ApiToken {
    pub id: i32,
    pub name: String,
    #[serde(skip)]
    pub token_hash: String,
    pub scopes: Vec<String>,
    /// The person a personal token acts as; NULL for a delegate token.
    pub user_id: Option<i32>,
    /// NULL for a token made from the CLI.
    pub created_by: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}
