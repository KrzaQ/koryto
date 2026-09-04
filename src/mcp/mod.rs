//! The MCP endpoint at /mcp: how meals, weights and sport get logged from
//! Open WebUI and Claude Code. Bearer tokens only; every tool acts as the
//! token's user (or the delegated one) and sees their household.

#[cfg(test)]
mod tests;

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, NaiveDate, Utc};
use rmcp::handler::server::tool::{Extension, ToolRouter};
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::model::{ErrorData, Implementation, ServerCapabilities, ServerInfo};
use rmcp::transport::StreamableHttpService;
use rmcp::transport::streamable_http_server::StreamableHttpServerConfig;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::{ServerHandler, schemars, tool, tool_handler, tool_router};
use serde::{Deserialize, Serialize};

use crate::app::entries::{
    self, ActivityInput, ActivityPatchInput, MealInput, MealPatchInput, WeightInput,
    WeightPatchInput,
};
use crate::app::{AppError, day as appday, scope, stats, time};
use crate::db::{
    Activity, Db, EntryKind, FoodPatch, Meal, NewFood, NewTarget, SOURCE_ESTIMATE, User, Weight,
};
use crate::domain::day;
use crate::domain::duration::format_minutes;
use crate::domain::expenditure::Estimate;
use crate::domain::units::{format_kg, parse_kg};
use crate::domain::when::When;
use crate::http::AppState;
use crate::http::auth::{self, Principal};
use crate::http::error::ApiError;

const INSTRUCTIONS: &str = "Calorie, protein, weight and sport log for a household. You act as one \
person and can see and log for everyone in their household; call whoami first to learn who that is, \
what zone and day they are on, and their target. A day runs from the person's day boundary (default \
04:00) to the next, on the clock of wherever they are, so a 01:00 snack belongs to the evening before. \
Weights are kilograms (\"82.4\"); kcal and protein grams are integers; durations are \"45m\", \"1h30\". \
Logging a meal: search_foods first, because a saved food gives the same number every time; log a hit \
with food and portions and no confirmation is needed. Otherwise estimate kcal and protein from the \
description, show the person the description and both numbers, and call log_meal with confirmed=true \
only after they agree; offer add_food when something sounds like it will recur. One dinner for several \
people is one call with for_users. Weights and sport are the person's own numbers: log them without \
asking. Sport kcal is added to that day's expenditure, so a 600 kcal swim means 600 kcal more to eat \
that day; log the number the person gives, or an estimate they agree to, and leave it out otherwise. \
The budget question (\"can I have a beer?\") is balance_vs_expenditure on get_day: below zero is room \
left. void_entry is the undo. Travel: set_location when someone says where they are; days recompute. \
log_* and add_food and set_location need the write scope; update_*, void_entry, set_target and the \
food edits need edit.";

#[derive(Clone)]
pub struct KorytoMcp {
    state: AppState,
    #[allow(dead_code)] // read by the tool_handler macro through Self::tool_router()
    tool_router: ToolRouter<Self>,
}

fn principal(parts: &http::request::Parts) -> Result<&Principal, ErrorData> {
    parts
        .extensions
        .get::<Principal>()
        .ok_or_else(|| ErrorData::invalid_request("no bearer token", None))
}

fn require_write(parts: &http::request::Parts) -> Result<&Principal, ErrorData> {
    let p = principal(parts)?;
    if p.can_write() {
        Ok(p)
    } else {
        Err(ErrorData::invalid_request(
            "this token has read scope only; logging needs read,write",
            None,
        ))
    }
}

fn require_edit(parts: &http::request::Parts) -> Result<&Principal, ErrorData> {
    let p = principal(parts)?;
    if p.can_edit() {
        Ok(p)
    } else {
        Err(ErrorData::invalid_request(
            "this token cannot change existing entries; that needs the edit scope",
            None,
        ))
    }
}

fn app_err(e: AppError) -> ErrorData {
    match e {
        AppError::NotFound => ErrorData::invalid_params("not found", None),
        AppError::Forbidden(m) | AppError::BadRequest(m) | AppError::Conflict(m) => {
            ErrorData::invalid_params(m, None)
        }
        AppError::Db(e) => ErrorData::internal_error(e.to_string(), None),
        AppError::Other(e) => ErrorData::internal_error(format!("{e:#}"), None),
    }
}

fn db_err(e: crate::db::DbError) -> ErrorData {
    app_err(e.into())
}

fn when(s: Option<&str>) -> Result<Option<When>, ErrorData> {
    s.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            s.parse().map_err(|e: crate::domain::when::WhenError| {
                ErrorData::invalid_params(e.to_string(), None)
            })
        })
        .transpose()
}

fn date(s: Option<&str>) -> Result<Option<NaiveDate>, ErrorData> {
    s.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .map_err(|_| ErrorData::invalid_params(format!("{s:?} is not YYYY-MM-DD"), None))
        })
        .transpose()
}

fn local(instant: DateTime<Utc>, zone: &str) -> String {
    let tz: chrono_tz::Tz = zone.parse().unwrap_or(chrono_tz::UTC);
    instant
        .with_timezone(&tz)
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
}

// ----- parameters ------------------------------------------------------------

#[derive(Deserialize, schemars::JsonSchema)]
pub struct DayParam {
    /// YYYY-MM-DD; default today on the person's clock
    pub date: Option<String>,
    /// Whose day: a household member by name or email; default you
    pub user: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct RangeParam {
    /// First day, YYYY-MM-DD
    pub from: String,
    /// Last day, YYYY-MM-DD (inclusive)
    pub to: String,
    /// Whose data: a household member by name or email; default you
    pub user: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SearchFoodsParam {
    /// Fragment of the name or an alias; empty lists the most used
    pub query: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct AddFoodParam {
    pub name: String,
    /// What one portion is, e.g. "1 bowl (350 g)" or "1 slice"
    pub portion: String,
    /// kcal per portion
    pub kcal: i32,
    /// Protein grams per portion
    pub protein_g: Option<i32>,
    /// Other names people use for it
    pub aliases: Option<Vec<String>>,
    /// Must be true. Set it only after the person has seen the name, portion and numbers and agreed.
    pub confirmed: Option<bool>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct LogMealParam {
    /// What was eaten, as the person said it. Defaults to the food's name.
    pub description: Option<String>,
    /// Your estimate, or the label; required without a food
    pub kcal: Option<i32>,
    pub protein_g: Option<i32>,
    /// estimate (default), manual (the person gave the number) or label (read off packaging)
    pub source: Option<String>,
    /// A saved food by name or alias; its numbers times portions are used
    pub food: Option<String>,
    /// Decimal portion count of the food, default 1
    pub portions: Option<String>,
    /// When: RFC 3339, or YYYY-MM-DD HH:MM on the person's current clock; default now
    pub eaten_at: Option<String>,
    /// Household members by name or email; default you. One entry per person.
    pub for_users: Option<Vec<String>>,
    /// Required when the number is an estimate: true only after the person has seen the description, kcal and protein and agreed
    pub confirmed: Option<bool>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct LogWeightParam {
    /// Kilograms, e.g. "82.4"
    pub weight_kg: String,
    /// RFC 3339 or YYYY-MM-DD HH:MM; default now
    pub measured_at: Option<String>,
    /// Household member by name or email; default you
    pub for_user: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct LogActivityParam {
    /// run, gym, cycling, swim, walk, ...
    pub kind: String,
    /// "45", "45m", "1h", "1h30", "1:30"
    pub duration: String,
    /// The number the person gives or agrees to; it is added to that day's expenditure
    pub kcal: Option<i32>,
    pub note: Option<String>,
    /// RFC 3339 or YYYY-MM-DD HH:MM; default now
    pub started_at: Option<String>,
    pub for_user: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SetLocationParam {
    /// IANA zone, e.g. America/New_York
    pub timezone: String,
    /// Since when, RFC 3339; default now. Use it when someone says they arrived earlier.
    pub from: Option<String>,
    pub for_user: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct UpdateMealParam {
    /// The meal id, as returned by log_meal or get_day
    pub id: i32,
    pub description: Option<String>,
    /// An explicit number unlinks the meal from its food
    pub kcal: Option<i32>,
    pub protein_g: Option<i32>,
    pub source: Option<String>,
    /// Relink to a saved food by name
    pub food: Option<String>,
    pub portions: Option<String>,
    pub eaten_at: Option<String>,
    /// Count it for this day instead: YYYY-MM-DD
    pub day: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct UpdateWeightParam {
    pub id: i32,
    pub weight_kg: Option<String>,
    pub measured_at: Option<String>,
    pub day: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct UpdateActivityParam {
    pub id: i32,
    pub kind: Option<String>,
    pub duration: Option<String>,
    pub kcal: Option<i32>,
    pub note: Option<String>,
    pub started_at: Option<String>,
    pub day: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct VoidParam {
    /// meal, weight or activity
    pub kind: String,
    pub id: i32,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct SetTargetParam {
    /// Daily kcal target
    pub kcal: i32,
    pub protein_g: Option<i32>,
    /// Goal weight in kilograms
    pub weight_kg: Option<String>,
    /// From which day, YYYY-MM-DD; default today
    pub from: Option<String>,
    pub for_user: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct UpdateFoodParam {
    pub id: i32,
    pub name: Option<String>,
    pub portion: Option<String>,
    pub kcal: Option<i32>,
    pub protein_g: Option<i32>,
    pub aliases: Option<Vec<String>>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct FoodIdParam {
    pub id: i32,
}

// ----- outputs ---------------------------------------------------------------

#[derive(Serialize, schemars::JsonSchema)]
pub struct PersonOut {
    pub id: i32,
    pub name: String,
    pub email: Option<String>,
}

impl From<&User> for PersonOut {
    fn from(u: &User) -> Self {
        Self {
            id: u.id,
            name: u.display().to_string(),
            email: u.email.clone(),
        }
    }
}

#[derive(Serialize, schemars::JsonSchema)]
pub struct TargetOut {
    pub since: String,
    pub kcal: i32,
    pub protein_g: Option<i32>,
    pub weight_kg: Option<String>,
}

impl From<crate::db::Target> for TargetOut {
    fn from(t: crate::db::Target) -> Self {
        Self {
            since: t.valid_from.to_string(),
            kcal: t.kcal,
            protein_g: t.protein_g,
            weight_kg: t.weight_g.map(format_kg),
        }
    }
}

#[derive(Serialize, schemars::JsonSchema)]
pub struct WhoamiOut {
    pub you: PersonOut,
    pub household: Option<String>,
    pub members: Vec<PersonOut>,
    /// IANA zone you are on right now
    pub timezone: String,
    /// Today on your clock and day boundary
    pub today: String,
    /// The day starts this many minutes after midnight
    pub day_boundary_minutes: i32,
    pub target: Option<TargetOut>,
    /// Today's estimated expenditure and where the number comes from
    pub expenditure: Estimate,
    pub scopes: Vec<String>,
}

#[derive(Serialize, schemars::JsonSchema)]
pub struct MealOut {
    pub id: i32,
    pub user: String,
    /// RFC 3339 on the clock it was logged under
    pub eaten_at: String,
    pub day: String,
    pub description: String,
    pub kcal: i32,
    pub protein_g: Option<i32>,
    pub source: String,
    pub food_id: Option<i32>,
    pub portions: Option<String>,
    pub voided: bool,
}

impl MealOut {
    fn from(m: Meal, user: &str) -> Self {
        Self {
            id: m.id,
            user: user.to_string(),
            eaten_at: local(m.eaten_at, &m.timezone),
            day: m.day.to_string(),
            description: m.description,
            kcal: m.kcal,
            protein_g: m.protein_g,
            source: m.source,
            food_id: m.food_id,
            portions: m.portions.map(|p| p.normalize().to_string()),
            voided: m.voided_at.is_some(),
        }
    }
}

#[derive(Serialize, schemars::JsonSchema)]
pub struct WeightOut {
    pub id: i32,
    pub user: String,
    pub measured_at: String,
    pub day: String,
    pub weight_kg: String,
    pub voided: bool,
}

impl WeightOut {
    fn from(w: Weight, user: &str) -> Self {
        Self {
            id: w.id,
            user: user.to_string(),
            measured_at: local(w.measured_at, &w.timezone),
            day: w.day.to_string(),
            weight_kg: format_kg(w.weight_g),
            voided: w.voided_at.is_some(),
        }
    }
}

#[derive(Serialize, schemars::JsonSchema)]
pub struct ActivityOut {
    pub id: i32,
    pub user: String,
    pub started_at: String,
    pub day: String,
    pub kind: String,
    pub duration: String,
    pub minutes: i32,
    pub kcal: Option<i32>,
    pub note: String,
    pub voided: bool,
}

impl ActivityOut {
    fn from(a: Activity, user: &str) -> Self {
        Self {
            id: a.id,
            user: user.to_string(),
            started_at: local(a.started_at, &a.timezone),
            day: a.day.to_string(),
            kind: a.kind,
            duration: format_minutes(a.minutes),
            minutes: a.minutes,
            kcal: a.kcal,
            note: a.note,
            voided: a.voided_at.is_some(),
        }
    }
}

#[derive(Serialize, schemars::JsonSchema)]
pub struct DayOut {
    pub day: String,
    pub user: String,
    pub logged: bool,
    pub kcal: i32,
    /// Sum over meals that have protein; see meals_without_protein
    pub protein_g: Option<i32>,
    pub meals_without_protein: i32,
    pub sport_minutes: i32,
    /// Sum of the day's sport kcal; null when no entry carries a number
    pub sport_kcal: Option<i32>,
    pub target_kcal: Option<i32>,
    /// kcal minus target, on a logged day with a target
    pub balance: Option<i32>,
    /// Estimated expenditure for this day (base plus the day's sport), and its basis
    pub expenditure: Estimate,
    /// kcal minus the estimate, on a logged day with an estimate; below zero is room left
    pub balance_vs_expenditure: Option<i32>,
    pub meals: Vec<MealOut>,
    pub weights: Vec<WeightOut>,
    pub activities: Vec<ActivityOut>,
}

#[derive(Serialize, schemars::JsonSchema)]
pub struct LogMealOut {
    pub meals: Vec<MealOut>,
    /// Each person's day so far, after this meal
    pub days: Vec<DayTotalsOut>,
}

#[derive(Serialize, schemars::JsonSchema)]
pub struct DayTotalsOut {
    pub user: String,
    pub day: String,
    pub kcal: i32,
    pub protein_g: Option<i32>,
    pub target_kcal: Option<i32>,
    pub balance: Option<i32>,
}

#[derive(Serialize, schemars::JsonSchema)]
pub struct DayRowOut {
    pub day: String,
    pub logged: bool,
    pub kcal: Option<i32>,
    pub protein_g: Option<i32>,
    pub sport_minutes: i32,
    pub sport_kcal: Option<i32>,
    pub weight_kg: Option<String>,
    pub trend_kg: Option<String>,
    pub target_kcal: Option<i32>,
    pub balance: Option<i32>,
}

#[derive(Serialize, schemars::JsonSchema)]
pub struct SummaryOut {
    pub user: String,
    pub from: String,
    pub to: String,
    pub days: usize,
    pub logged_days: usize,
    pub mean_kcal: Option<i32>,
    pub mean_protein_g: Option<i32>,
    pub mean_balance: Option<i32>,
    /// Mean expenditure (base plus sport) over logged days with an estimate
    pub mean_expenditure: Option<i32>,
    /// Mean intake minus expenditure over logged days with an estimate; below zero is a deficit
    pub mean_balance_vs_expenditure: Option<i32>,
    pub sport_minutes: i32,
    pub sport_kcal: i32,
    pub weight_first_kg: Option<String>,
    pub weight_last_kg: Option<String>,
    pub trend_first_kg: Option<String>,
    pub trend_last_kg: Option<String>,
    /// Trend change over the range, grams (negative is loss)
    pub trend_delta_g: Option<i32>,
    /// Estimated daily expenditure as of the last day, and its basis
    pub expenditure: Estimate,
    pub rows: Vec<DayRowOut>,
}

#[derive(Serialize, schemars::JsonSchema)]
pub struct FoodOut {
    pub id: i32,
    pub name: String,
    pub aliases: Vec<String>,
    pub portion: String,
    pub kcal: i32,
    pub protein_g: Option<i32>,
    pub uses: i64,
    pub archived: bool,
}

impl FoodOut {
    fn from(f: crate::db::Food, uses: i64) -> Self {
        Self {
            id: f.id,
            name: f.name,
            aliases: f.aliases,
            portion: f.portion,
            kcal: f.kcal,
            protein_g: f.protein_g,
            uses,
            archived: f.archived_at.is_some(),
        }
    }
}

#[derive(Serialize, schemars::JsonSchema)]
pub struct ListOut<T> {
    pub items: Vec<T>,
}

#[derive(Serialize, schemars::JsonSchema)]
pub struct LocationOut {
    pub user: String,
    pub timezone: String,
    pub since: String,
    /// Entries whose day moved
    pub entries_recomputed: usize,
    pub today: String,
}

#[derive(Serialize, schemars::JsonSchema)]
pub struct DoneOut {
    pub done: bool,
    pub message: String,
}

// ----- tools -----------------------------------------------------------------

#[tool_router]
impl KorytoMcp {
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }

    fn db(&self) -> &Db {
        &self.state.db
    }

    async fn member(&self, actor: &User, who: Option<&str>) -> Result<User, ErrorData> {
        match who.map(str::trim).filter(|w| !w.is_empty()) {
            None => scope::member(self.db(), actor, None).await.map_err(app_err),
            Some(w) => scope::member_named(self.db(), actor, w)
                .await
                .map_err(|e| match e {
                    AppError::NotFound => ErrorData::invalid_params(
                        format!("nobody in your household matches {w:?}"),
                        None,
                    ),
                    e => app_err(e),
                }),
        }
    }

    async fn today_for(&self, user: &User) -> Result<NaiveDate, ErrorData> {
        let tz = time::current_zone(self.db(), user).await.map_err(db_err)?;
        Ok(day::today(tz, user.day_boundary_minutes))
    }

    async fn day_out(&self, user: &User, d: NaiveDate) -> Result<DayOut, ErrorData> {
        let v = appday::day_view(self.db(), user, d, false)
            .await
            .map_err(app_err)?;
        let name = user.display();
        Ok(DayOut {
            day: d.to_string(),
            user: name.to_string(),
            logged: v.logged,
            kcal: v.totals.kcal,
            protein_g: v.totals.protein_g,
            meals_without_protein: v.totals.meals_without_protein,
            sport_minutes: v.totals.sport_minutes,
            sport_kcal: v.totals.sport_kcal,
            target_kcal: v.target.as_ref().map(|t| t.kcal),
            balance: v.balance,
            expenditure: v.expenditure,
            balance_vs_expenditure: v.balance_vs_expenditure,
            meals: v
                .meals
                .into_iter()
                .map(|m| MealOut::from(m, name))
                .collect(),
            weights: v
                .weights
                .into_iter()
                .map(|w| WeightOut::from(w, name))
                .collect(),
            activities: v
                .activities
                .into_iter()
                .map(|a| ActivityOut::from(a, name))
                .collect(),
        })
    }

    async fn food_named(&self, actor: &User, name: &str) -> Result<crate::db::Food, ErrorData> {
        let household = scope::household_of(actor).map_err(app_err)?;
        self.db()
            .find_food(household, name)
            .await
            .map_err(db_err)?
            .ok_or_else(|| {
                ErrorData::invalid_params(
                    format!("no saved food matches {name:?}; search_foods lists them, or estimate and log without a food"),
                    None,
                )
            })
    }

    #[tool(
        description = "Who you are acting as, the household members you can see and log for, the zone and day you are on, the target in force and today's expenditure estimate. Call it first."
    )]
    async fn whoami(
        &self,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<Json<WhoamiOut>, ErrorData> {
        let p = principal(&parts)?;
        let user = p.user();
        let tz = time::current_zone(self.db(), user).await.map_err(db_err)?;
        let today = day::today(tz, user.day_boundary_minutes);
        let (household, members) = match user.household_id {
            Some(id) => (
                Some(self.db().get_household(id).await.map_err(db_err)?.name),
                self.db()
                    .household_members(id)
                    .await
                    .map_err(db_err)?
                    .iter()
                    .map(PersonOut::from)
                    .collect(),
            ),
            None => (None, vec![]),
        };
        let target = self
            .db()
            .target_for(user.id, today)
            .await
            .map_err(db_err)?
            .map(TargetOut::from);
        let expenditure = stats::expenditure_on(self.db(), user, today)
            .await
            .map_err(app_err)?;
        Ok(Json(WhoamiOut {
            you: PersonOut::from(user),
            household,
            members,
            timezone: tz.name().to_string(),
            today: today.to_string(),
            day_boundary_minutes: user.day_boundary_minutes,
            target,
            expenditure,
            scopes: p.scopes(),
        }))
    }

    #[tool(
        description = "One person's day: meals, weigh-ins, sport, totals, target and balance. Today on their clock unless a date is given."
    )]
    async fn get_day(
        &self,
        Parameters(p): Parameters<DayParam>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<Json<DayOut>, ErrorData> {
        let actor = principal(&parts)?.user();
        let user = self.member(actor, p.user.as_deref()).await?;
        let d = match date(p.date.as_deref())? {
            Some(d) => d,
            None => self.today_for(&user).await?,
        };
        Ok(Json(self.day_out(&user, d).await?))
    }

    #[tool(
        description = "A range of days for one person: per-day intake, protein, sport, weight and trend, plus averages over logged days, the trend change, the mean balance against expenditure and the estimated daily expenditure with its basis (adaptive from the data, or the Mifflin-St Jeor seed until there is enough). Unlogged days are gaps, not zeros."
    )]
    async fn get_summary(
        &self,
        Parameters(p): Parameters<RangeParam>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<Json<SummaryOut>, ErrorData> {
        let actor = principal(&parts)?.user();
        let user = self.member(actor, p.user.as_deref()).await?;
        let from = date(Some(&p.from))?.expect("given");
        let to = date(Some(&p.to))?.expect("given");
        let s = stats::summary(self.db(), &user, from, to)
            .await
            .map_err(app_err)?;
        Ok(Json(SummaryOut {
            user: user.display().to_string(),
            from: from.to_string(),
            to: to.to_string(),
            days: s.days,
            logged_days: s.logged_days,
            mean_kcal: s.mean_kcal,
            mean_protein_g: s.mean_protein_g,
            mean_balance: s.mean_balance,
            mean_expenditure: s.mean_expenditure,
            mean_balance_vs_expenditure: s.mean_balance_vs_expenditure,
            sport_minutes: s.sport_minutes,
            sport_kcal: s.sport_kcal,
            weight_first_kg: s.weight.first_g.map(format_kg),
            weight_last_kg: s.weight.last_g.map(format_kg),
            trend_first_kg: s.weight.trend_first_g.map(format_kg),
            trend_last_kg: s.weight.trend_last_g.map(format_kg),
            trend_delta_g: s.weight.trend_delta_g,
            expenditure: s.expenditure,
            rows: s
                .rows
                .into_iter()
                .map(|r| DayRowOut {
                    day: r.day.to_string(),
                    logged: r.logged,
                    kcal: r.kcal,
                    protein_g: r.protein_g,
                    sport_minutes: r.sport_minutes,
                    sport_kcal: r.sport_kcal,
                    weight_kg: r.weight_g.map(format_kg),
                    trend_kg: r.trend_g.map(format_kg),
                    target_kcal: r.target_kcal,
                    balance: r.balance,
                })
                .collect(),
        }))
    }

    #[tool(
        description = "The household's saved foods matching a name or alias fragment, most used first; empty query lists them all. Check here before estimating a meal."
    )]
    async fn search_foods(
        &self,
        Parameters(p): Parameters<SearchFoodsParam>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<Json<ListOut<FoodOut>>, ErrorData> {
        let actor = principal(&parts)?.user();
        let household = scope::household_of(actor).map_err(app_err)?;
        let items = self
            .db()
            .search_foods(household, p.query.as_deref().unwrap_or(""), false)
            .await
            .map_err(db_err)?
            .into_iter()
            .map(|f| FoodOut::from(f.food, f.uses))
            .collect();
        Ok(Json(ListOut { items }))
    }

    #[tool(
        description = "Save a food for the household so it always gets the same number: a name, what one portion is, kcal and protein per portion. Show the person all of it and pass confirmed=true only after they agree. Needs the write scope."
    )]
    async fn add_food(
        &self,
        Parameters(p): Parameters<AddFoodParam>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<Json<FoodOut>, ErrorData> {
        let actor = require_write(&parts)?.user();
        if p.confirmed != Some(true) {
            return Err(ErrorData::invalid_params(
                "not saved: show the person the name, portion, kcal and protein, and call again with confirmed=true once they agree",
                None,
            ));
        }
        let household = scope::household_of(actor).map_err(app_err)?;
        if p.name.trim().is_empty() || p.portion.trim().is_empty() {
            return Err(ErrorData::invalid_params(
                "name and portion cannot be empty",
                None,
            ));
        }
        if p.kcal < 0 || p.protein_g.is_some_and(|x| x < 0) {
            return Err(ErrorData::invalid_params(
                "kcal and protein_g cannot be negative",
                None,
            ));
        }
        let f = self
            .db()
            .insert_food(NewFood {
                household_id: household,
                name: p.name,
                aliases: p
                    .aliases
                    .unwrap_or_default()
                    .into_iter()
                    .map(|a| a.trim().to_string())
                    .filter(|a| !a.is_empty())
                    .collect(),
                portion: p.portion,
                kcal: p.kcal,
                protein_g: p.protein_g,
                created_by: actor.id,
            })
            .await
            .map_err(db_err)?;
        Ok(Json(FoodOut::from(f, 0)))
    }

    #[tool(
        description = "Log a meal for one or more household members, now unless eaten_at is given. With a saved food (by name or alias) and portions the numbers are exact and no confirmation is needed. Without one, kcal is required: show the person the description, kcal and protein you are about to record and pass confirmed=true only after they agree. Returns the entries and each person's day so far. Needs the write scope."
    )]
    async fn log_meal(
        &self,
        Parameters(p): Parameters<LogMealParam>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<Json<LogMealOut>, ErrorData> {
        let principal = require_write(&parts)?;
        let actor = principal.user();
        let food = match p.food.as_deref().map(str::trim).filter(|f| !f.is_empty()) {
            Some(name) => Some(self.food_named(actor, name).await?),
            None => None,
        };
        let is_estimate = food.is_none()
            && p.source
                .as_deref()
                .map(|s| s.trim().eq_ignore_ascii_case(SOURCE_ESTIMATE))
                .unwrap_or(true);
        if is_estimate && p.confirmed != Some(true) {
            return Err(ErrorData::invalid_params(
                "not logged: this is an estimate, so show the person the description, kcal and protein and call again with confirmed=true once they agree (or log it against a saved food)",
                None,
            ));
        }
        let mut user_ids = Vec::new();
        for who in p.for_users.unwrap_or_default() {
            user_ids.push(self.member(actor, Some(&who)).await?.id);
        }
        let meals = entries::log_meals(
            self.db(),
            actor,
            principal.via(),
            MealInput {
                user_ids,
                eaten_at: when(p.eaten_at.as_deref())?,
                timezone: None,
                day: None,
                description: p.description,
                kcal: p.kcal,
                protein_g: p.protein_g,
                source: p.source,
                food_id: food.map(|f| f.id),
                portions: p.portions,
            },
        )
        .await
        .map_err(app_err)?;
        let mut out = LogMealOut {
            meals: Vec::new(),
            days: Vec::new(),
        };
        for m in meals {
            let user = self.db().get_user(m.user_id).await.map_err(db_err)?;
            let name = user.display().to_string();
            let d = self.day_out(&user, m.day).await?;
            out.days.push(DayTotalsOut {
                user: name.clone(),
                day: d.day,
                kcal: d.kcal,
                protein_g: d.protein_g,
                target_kcal: d.target_kcal,
                balance: d.balance,
            });
            out.meals.push(MealOut::from(m, &name));
        }
        Ok(Json(out))
    }

    #[tool(
        description = "Record a weigh-in in kilograms, now unless measured_at is given. The person's own number: no confirmation needed. Needs the write scope."
    )]
    async fn log_weight(
        &self,
        Parameters(p): Parameters<LogWeightParam>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<Json<WeightOut>, ErrorData> {
        let principal = require_write(&parts)?;
        let actor = principal.user();
        let user = self.member(actor, p.for_user.as_deref()).await?;
        let w = entries::log_weight(
            self.db(),
            actor,
            principal.via(),
            WeightInput {
                user_id: Some(user.id),
                measured_at: when(p.measured_at.as_deref())?,
                timezone: None,
                day: None,
                weight_kg: p.weight_kg,
            },
        )
        .await
        .map_err(app_err)?;
        Ok(Json(WeightOut::from(w, user.display())))
    }

    #[tool(
        description = "Record sport: a kind and a duration, now unless started_at is given. kcal, when the person gives it or agrees to an estimate, is added to that day's expenditure and so to what they may eat. Needs the write scope."
    )]
    async fn log_activity(
        &self,
        Parameters(p): Parameters<LogActivityParam>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<Json<ActivityOut>, ErrorData> {
        let principal = require_write(&parts)?;
        let actor = principal.user();
        let user = self.member(actor, p.for_user.as_deref()).await?;
        let a = entries::log_activity(
            self.db(),
            actor,
            principal.via(),
            ActivityInput {
                user_id: Some(user.id),
                started_at: when(p.started_at.as_deref())?,
                timezone: None,
                day: None,
                kind: p.kind,
                duration: p.duration,
                kcal: p.kcal,
                note: p.note,
            },
        )
        .await
        .map_err(app_err)?;
        Ok(Json(ActivityOut::from(a, user.display())))
    }

    #[tool(
        description = "Where someone is from now on (or since `from`), as an IANA zone. Every later entry gets its day on that clock, and existing entries recompute. Needs the write scope."
    )]
    async fn set_location(
        &self,
        Parameters(p): Parameters<SetLocationParam>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<Json<LocationOut>, ErrorData> {
        let actor = require_write(&parts)?.user();
        let user = self.member(actor, p.for_user.as_deref()).await?;
        let tz = day::parse_tz(&p.timezone).map_err(|e| ErrorData::invalid_params(e, None))?;
        let from = match when(p.from.as_deref())? {
            None => Utc::now(),
            Some(When::Instant(i)) => i,
            Some(When::Wall(w)) => crate::domain::when::resolve_wall(w, tz),
        };
        self.db()
            .insert_location(user.id, from, tz.name())
            .await
            .map_err(db_err)?;
        let changed = time::recompute_days(self.db(), &user)
            .await
            .map_err(|e| ErrorData::internal_error(format!("{e:#}"), None))?;
        Ok(Json(LocationOut {
            user: user.display().to_string(),
            timezone: tz.name().to_string(),
            since: local(from, tz.name()),
            entries_recomputed: changed,
            today: day::today(tz, user.day_boundary_minutes).to_string(),
        }))
    }

    #[tool(
        description = "Change a meal: description, numbers, food link, portions, time or the day it counts for. An explicit kcal unlinks it from its food. Needs the edit scope."
    )]
    async fn update_meal(
        &self,
        Parameters(p): Parameters<UpdateMealParam>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<Json<MealOut>, ErrorData> {
        let actor = require_edit(&parts)?.user();
        let food_id = match p.food.as_deref().map(str::trim).filter(|f| !f.is_empty()) {
            Some(name) => Some(Some(self.food_named(actor, name).await?.id)),
            None => None,
        };
        let m = entries::update_meal(
            self.db(),
            actor,
            p.id,
            MealPatchInput {
                eaten_at: when(p.eaten_at.as_deref())?,
                timezone: None,
                day: date(p.day.as_deref())?.map(Some),
                description: p.description,
                kcal: p.kcal,
                protein_g: p.protein_g.map(Some),
                source: p.source,
                food_id,
                portions: p.portions,
            },
        )
        .await
        .map_err(app_err)?;
        let user = self.db().get_user(m.user_id).await.map_err(db_err)?;
        Ok(Json(MealOut::from(m, user.display())))
    }

    #[tool(description = "Change a weigh-in's kilograms, time or day. Needs the edit scope.")]
    async fn update_weight(
        &self,
        Parameters(p): Parameters<UpdateWeightParam>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<Json<WeightOut>, ErrorData> {
        let actor = require_edit(&parts)?.user();
        let w = entries::update_weight(
            self.db(),
            actor,
            p.id,
            WeightPatchInput {
                measured_at: when(p.measured_at.as_deref())?,
                timezone: None,
                day: date(p.day.as_deref())?.map(Some),
                weight_kg: p.weight_kg,
            },
        )
        .await
        .map_err(app_err)?;
        let user = self.db().get_user(w.user_id).await.map_err(db_err)?;
        Ok(Json(WeightOut::from(w, user.display())))
    }

    #[tool(
        description = "Change a sport entry's kind, duration, kcal, note, time or day. Needs the edit scope."
    )]
    async fn update_activity(
        &self,
        Parameters(p): Parameters<UpdateActivityParam>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<Json<ActivityOut>, ErrorData> {
        let actor = require_edit(&parts)?.user();
        let a = entries::update_activity(
            self.db(),
            actor,
            p.id,
            ActivityPatchInput {
                started_at: when(p.started_at.as_deref())?,
                timezone: None,
                day: date(p.day.as_deref())?.map(Some),
                kind: p.kind,
                duration: p.duration,
                kcal: p.kcal.map(Some),
                note: p.note,
            },
        )
        .await
        .map_err(app_err)?;
        let user = self.db().get_user(a.user_id).await.map_err(db_err)?;
        Ok(Json(ActivityOut::from(a, user.display())))
    }

    #[tool(
        description = "The undo: hide a meal, weigh-in or sport entry from every total. Nothing is deleted; the web UI can bring it back. Needs the edit scope."
    )]
    async fn void_entry(
        &self,
        Parameters(p): Parameters<VoidParam>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<Json<DoneOut>, ErrorData> {
        let actor = require_edit(&parts)?.user();
        let kind: EntryKind = p
            .kind
            .parse()
            .map_err(|e: String| ErrorData::invalid_params(e, None))?;
        entries::void(self.db(), actor, kind, p.id)
            .await
            .map_err(app_err)?;
        Ok(Json(DoneOut {
            done: true,
            message: format!("{} {} voided", p.kind.trim().to_lowercase(), p.id),
        }))
    }

    #[tool(
        description = "Set a person's daily kcal target (and optionally protein and a goal weight) from a day on; earlier days keep the old target. Needs the edit scope."
    )]
    async fn set_target(
        &self,
        Parameters(p): Parameters<SetTargetParam>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<Json<TargetOut>, ErrorData> {
        let actor = require_edit(&parts)?.user();
        let user = self.member(actor, p.for_user.as_deref()).await?;
        if p.kcal <= 0 || p.protein_g.is_some_and(|x| x <= 0) {
            return Err(ErrorData::invalid_params(
                "kcal and protein_g must be positive",
                None,
            ));
        }
        let weight_g = p
            .weight_kg
            .as_deref()
            .map(|k| parse_kg(k).map_err(|e| ErrorData::invalid_params(e.to_string(), None)))
            .transpose()?;
        let valid_from = match date(p.from.as_deref())? {
            Some(d) => d,
            None => self.today_for(&user).await?,
        };
        let t = self
            .db()
            .insert_target(NewTarget {
                user_id: user.id,
                valid_from,
                kcal: p.kcal,
                protein_g: p.protein_g,
                weight_g,
            })
            .await
            .map_err(db_err)?;
        Ok(Json(TargetOut::from(t)))
    }

    #[tool(
        description = "Change a saved food's name, portion, numbers or aliases. Past meals keep their numbers. Needs the edit scope."
    )]
    async fn update_food(
        &self,
        Parameters(p): Parameters<UpdateFoodParam>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<Json<FoodOut>, ErrorData> {
        let actor = require_edit(&parts)?.user();
        scope::food(self.db(), actor, p.id).await.map_err(app_err)?;
        if p.kcal.is_some_and(|k| k < 0) || p.protein_g.is_some_and(|x| x < 0) {
            return Err(ErrorData::invalid_params(
                "kcal and protein_g cannot be negative",
                None,
            ));
        }
        let f = self
            .db()
            .update_food(
                p.id,
                FoodPatch {
                    name: p.name,
                    aliases: p.aliases,
                    portion: p.portion,
                    kcal: p.kcal,
                    protein_g: p.protein_g.map(Some),
                },
            )
            .await
            .map_err(db_err)?;
        Ok(Json(FoodOut::from(f, 0)))
    }

    #[tool(
        description = "Retire a saved food: it stops matching, past meals keep their numbers. Needs the edit scope."
    )]
    async fn archive_food(
        &self,
        Parameters(p): Parameters<FoodIdParam>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<Json<FoodOut>, ErrorData> {
        let actor = require_edit(&parts)?.user();
        scope::food(self.db(), actor, p.id).await.map_err(app_err)?;
        let f = self
            .db()
            .set_food_archived(p.id, true)
            .await
            .map_err(db_err)?;
        Ok(Json(FoodOut::from(f, 0)))
    }
}

#[tool_handler]
impl ServerHandler for KorytoMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(
                Implementation::new("koryto", env!("CARGO_PKG_VERSION")).with_title("koryto"),
            )
            .with_instructions(INSTRUCTIONS)
    }
}

// ----- mounting --------------------------------------------------------------

/// Bearer only: no cookies, no dev-mode fallback. The principal is stored in
/// the request extensions, which rmcp forwards to the tools.
async fn require_bearer(State(state): State<AppState>, mut req: Request, next: Next) -> Response {
    match auth::bearer(&state, req.headers()).await {
        Ok(Some(p)) => {
            req.extensions_mut().insert(p);
            next.run(req).await
        }
        Ok(None) => ApiError::unauthorized().into_response(),
        Err(e) => e.into_response(),
    }
}

pub fn router(state: AppState) -> Router<AppState> {
    let mut hosts: Vec<String> = vec!["localhost".into(), "127.0.0.1".into(), "::1".into()];
    if let Some(h) = state.config.public_url.host_str() {
        hosts.push(h.to_string());
        if let Some(p) = state.config.public_url.port() {
            hosts.push(format!("{h}:{p}"));
        }
    }
    let config = StreamableHttpServerConfig::default()
        .with_json_response(true)
        .with_allowed_hosts(hosts);
    let factory_state = state.clone();
    let service: StreamableHttpService<KorytoMcp, LocalSessionManager> = StreamableHttpService::new(
        move || Ok(KorytoMcp::new(factory_state.clone())),
        Arc::new(LocalSessionManager::default()),
        config,
    );
    Router::new()
        .route_service("/mcp", service)
        .layer(middleware::from_fn_with_state(state, require_bearer))
}

#[allow(dead_code)]
fn _body_is_axum(_: Body) {}
