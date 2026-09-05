//! Postgres access. Every query in the application lives here, so the API,
//! CLI and MCP server share one implementation and one set of tests. The
//! `Db` knows nothing about zones or days: callers pass the resolved values.

mod rows;
#[cfg(test)]
pub mod test_db;
#[cfg(test)]
mod tests;

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

pub use rows::*;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[derive(Clone)]
pub struct Db {
    pool: PgPool,
}

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    Conflict(String),
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

pub type DbResult<T> = std::result::Result<T, DbError>;

fn not_found_or(e: sqlx::Error) -> DbError {
    match e {
        sqlx::Error::RowNotFound => DbError::NotFound,
        other => DbError::Sqlx(other),
    }
}

/// Unique and check violations become conflicts with the given message.
fn conflict_or(e: sqlx::Error, message: impl FnOnce() -> String) -> DbError {
    match &e {
        sqlx::Error::Database(d) if d.is_unique_violation() || d.is_check_violation() => {
            DbError::Conflict(message())
        }
        _ => DbError::Sqlx(e),
    }
}

fn affected(res: sqlx::postgres::PgQueryResult) -> DbResult<()> {
    if res.rows_affected() == 0 {
        Err(DbError::NotFound)
    } else {
        Ok(())
    }
}

const MEAL_COLS: &str = "user_id, eaten_at, timezone, day, day_override, description, kcal, \
                         protein_g, source, food_id, portions, created_by, created_via";
const WEIGHT_COLS: &str =
    "user_id, measured_at, timezone, day, day_override, weight_g, created_by, created_via";
const ACTIVITY_COLS: &str = "user_id, started_at, timezone, day, day_override, kind, minutes, \
                             kcal, source, activity_kind_id, note, created_by, created_via";

impl Db {
    /// Lazy: the first query opens the connection, so `serve` can come up
    /// and answer `/api/health` before (or without) the database.
    pub async fn connect(url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .connect_lazy(url)
            .context("connecting to Postgres")?;
        Ok(Self { pool })
    }

    pub fn from_pool(pool: PgPool) -> Self {
        Self { pool }
    }

    /// For tests that need to age or corrupt rows behind the queries.
    #[cfg(test)]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn migrate(&self) -> Result<()> {
        MIGRATOR
            .run(&self.pool)
            .await
            .context("running migrations")?;
        Ok(())
    }

    /// (version, description, applied) for every known migration.
    pub async fn migration_status(&self) -> Result<Vec<(i64, String, bool)>> {
        let applied: Vec<(i64,)> =
            sqlx::query_as("SELECT version FROM _sqlx_migrations ORDER BY version")
                .fetch_all(&self.pool)
                .await
                .unwrap_or_default();
        Ok(MIGRATOR
            .iter()
            .map(|m| {
                let done = applied.iter().any(|(v,)| *v == m.version);
                (m.version, m.description.to_string(), done)
            })
            .collect())
    }

    // ----- households ------------------------------------------------------

    pub async fn create_household(&self, name: &str) -> DbResult<Household> {
        Ok(
            sqlx::query_as("INSERT INTO households (name) VALUES ($1) RETURNING *")
                .bind(name.trim())
                .fetch_one(&self.pool)
                .await?,
        )
    }

    pub async fn list_households(&self) -> DbResult<Vec<Household>> {
        Ok(sqlx::query_as("SELECT * FROM households ORDER BY id")
            .fetch_all(&self.pool)
            .await?)
    }

    pub async fn get_household(&self, id: i32) -> DbResult<Household> {
        sqlx::query_as("SELECT * FROM households WHERE id = $1")
            .bind(id)
            .fetch_one(&self.pool)
            .await
            .map_err(not_found_or)
    }

    /// By exact name, case-insensitively.
    pub async fn find_household(&self, name: &str) -> DbResult<Household> {
        sqlx::query_as("SELECT * FROM households WHERE lower(name) = lower($1)")
            .bind(name.trim())
            .fetch_one(&self.pool)
            .await
            .map_err(not_found_or)
    }

    pub async fn household_members(&self, household_id: i32) -> DbResult<Vec<User>> {
        Ok(
            sqlx::query_as("SELECT * FROM users WHERE household_id = $1 ORDER BY id")
                .bind(household_id)
                .fetch_all(&self.pool)
                .await?,
        )
    }

    pub async fn rename_household(&self, id: i32, name: &str) -> DbResult<Household> {
        sqlx::query_as("UPDATE households SET name = $2 WHERE id = $1 RETURNING *")
            .bind(id)
            .bind(name.trim())
            .fetch_one(&self.pool)
            .await
            .map_err(not_found_or)
    }

    /// Move a person into `target`, or into a fresh household of their own
    /// when `None`. Entries follow their owner by construction; foods are the
    /// one shared table, so they are handled here, in one transaction:
    ///
    /// - leaving a household that keeps other members forks its foods (a copy
    ///   of each travels with the person, their meals re-pointed at the copy);
    /// - leaving a household that becomes empty moves the foods and deletes it;
    /// - arriving where a food of the same name exists re-points the person's
    ///   meals at that one and drops the incoming duplicate.
    pub async fn move_user(&self, user_id: i32, target: Option<i32>) -> DbResult<User> {
        let mut tx = self.pool.begin().await?;
        let user: User = sqlx::query_as("SELECT * FROM users WHERE id = $1 FOR UPDATE")
            .bind(user_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(not_found_or)?;
        let old = user.household_id;
        if target.is_some() && target == old {
            return Ok(user);
        }
        let target_id = match target {
            Some(id) => {
                sqlx::query_as::<_, Household>("SELECT * FROM households WHERE id = $1")
                    .bind(id)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(not_found_or)?;
                id
            }
            None => {
                let (id,): (i32,) =
                    sqlx::query_as("INSERT INTO households (name) VALUES ($1) RETURNING id")
                        .bind(user.display())
                        .fetch_one(&mut *tx)
                        .await?;
                id
            }
        };
        if let Some(old) = old {
            let (others,): (i64,) =
                sqlx::query_as("SELECT count(*) FROM users WHERE household_id = $1 AND id <> $2")
                    .bind(old)
                    .bind(user_id)
                    .fetch_one(&mut *tx)
                    .await?;
            let foods: Vec<Food> =
                sqlx::query_as("SELECT * FROM foods WHERE household_id = $1 ORDER BY id")
                    .bind(old)
                    .fetch_all(&mut *tx)
                    .await?;
            for f in foods {
                let existing: Option<(i32,)> = sqlx::query_as(
                    "SELECT id FROM foods WHERE household_id = $1 AND lower(name) = lower($2) \
                     AND archived_at IS NULL",
                )
                .bind(target_id)
                .bind(&f.name)
                .fetch_optional(&mut *tx)
                .await?;
                // Which meals to re-point: the person's own when forking,
                // every remaining one when the old household empties.
                let owner_filter = if others > 0 { user_id } else { -1 };
                match (existing, others > 0) {
                    (Some((to,)), fork) => {
                        sqlx::query(
                            "UPDATE meals SET food_id = $2 WHERE food_id = $1 \
                             AND ($3 < 0 OR user_id = $3)",
                        )
                        .bind(f.id)
                        .bind(to)
                        .bind(owner_filter)
                        .execute(&mut *tx)
                        .await?;
                        if !fork {
                            sqlx::query("DELETE FROM foods WHERE id = $1")
                                .bind(f.id)
                                .execute(&mut *tx)
                                .await?;
                        }
                    }
                    (None, true) => {
                        let (copy,): (i32,) = sqlx::query_as(
                            "INSERT INTO foods (household_id, name, aliases, portion, kcal, protein_g, \
                             created_by, created_at, updated_at, archived_at) \
                             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING id",
                        )
                        .bind(target_id)
                        .bind(&f.name)
                        .bind(&f.aliases)
                        .bind(&f.portion)
                        .bind(f.kcal)
                        .bind(f.protein_g)
                        .bind(user_id)
                        .bind(f.created_at)
                        .bind(f.updated_at)
                        .bind(f.archived_at)
                        .fetch_one(&mut *tx)
                        .await?;
                        sqlx::query(
                            "UPDATE meals SET food_id = $2 WHERE food_id = $1 AND user_id = $3",
                        )
                        .bind(f.id)
                        .bind(copy)
                        .bind(user_id)
                        .execute(&mut *tx)
                        .await?;
                    }
                    (None, false) => {
                        sqlx::query("UPDATE foods SET household_id = $2 WHERE id = $1")
                            .bind(f.id)
                            .bind(target_id)
                            .execute(&mut *tx)
                            .await?;
                    }
                }
            }
        }
        let user: User =
            sqlx::query_as("UPDATE users SET household_id = $2 WHERE id = $1 RETURNING *")
                .bind(user_id)
                .bind(target_id)
                .fetch_one(&mut *tx)
                .await?;
        // The old household row can only go once nobody points at it.
        if let Some(old) = old {
            sqlx::query("DELETE FROM households h WHERE h.id = $1 AND NOT EXISTS (SELECT 1 FROM users WHERE household_id = h.id) AND NOT EXISTS (SELECT 1 FROM foods WHERE household_id = h.id)")
                .bind(old)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(user)
    }

    /// Tests only: place a user without the food bookkeeping of [`Db::move_user`].
    #[cfg(test)]
    pub async fn set_user_household(
        &self,
        user_id: i32,
        household_id: Option<i32>,
    ) -> DbResult<User> {
        sqlx::query_as("UPDATE users SET household_id = $2 WHERE id = $1 RETURNING *")
            .bind(user_id)
            .bind(household_id)
            .fetch_one(&self.pool)
            .await
            .map_err(not_found_or)
    }

    // ----- users -----------------------------------------------------------

    /// Login: create or refresh the user row, give a new user the origin
    /// location row in `house_tz`, and a household of their own so they can
    /// keep score alone from the first day.
    pub async fn upsert_user(
        &self,
        subject: &str,
        email: Option<&str>,
        name: Option<&str>,
        house_tz: &str,
    ) -> DbResult<User> {
        let mut tx = self.pool.begin().await?;
        let user: User = sqlx::query_as(
            "INSERT INTO users (subject, email, name, last_login_at) VALUES ($1, $2, $3, now()) \
             ON CONFLICT (subject) DO UPDATE SET email = COALESCE(EXCLUDED.email, users.email), \
             name = COALESCE(EXCLUDED.name, users.name), last_login_at = now() RETURNING *",
        )
        .bind(subject)
        .bind(email)
        .bind(name)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO user_locations (user_id, valid_from, timezone) VALUES ($1, $2, $3) \
             ON CONFLICT (user_id, valid_from) DO NOTHING",
        )
        .bind(user.id)
        .bind(origin())
        .bind(house_tz)
        .execute(&mut *tx)
        .await?;
        let user = if user.household_id.is_none() {
            let (household,): (i32,) =
                sqlx::query_as("INSERT INTO households (name) VALUES ($1) RETURNING id")
                    .bind(user.display())
                    .fetch_one(&mut *tx)
                    .await?;
            sqlx::query_as("UPDATE users SET household_id = $2 WHERE id = $1 RETURNING *")
                .bind(user.id)
                .bind(household)
                .fetch_one(&mut *tx)
                .await?
        } else {
            user
        };
        tx.commit().await?;
        Ok(user)
    }

    /// Case-insensitive; the newest login wins if two subjects share an email.
    pub async fn find_user_by_email(&self, email: &str) -> DbResult<Option<User>> {
        Ok(sqlx::query_as(
            "SELECT * FROM users WHERE lower(email) = lower($1) \
             ORDER BY last_login_at DESC NULLS LAST, id DESC LIMIT 1",
        )
        .bind(email.trim())
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn get_user(&self, id: i32) -> DbResult<User> {
        sqlx::query_as("SELECT * FROM users WHERE id = $1")
            .bind(id)
            .fetch_one(&self.pool)
            .await
            .map_err(not_found_or)
    }

    pub async fn list_users(&self) -> DbResult<Vec<User>> {
        Ok(sqlx::query_as("SELECT * FROM users ORDER BY id")
            .fetch_all(&self.pool)
            .await?)
    }

    pub async fn update_profile(&self, id: i32, p: ProfilePatch) -> DbResult<User> {
        sqlx::query_as(
            "UPDATE users SET \
               name = COALESCE($2, name), \
               day_boundary_minutes = COALESCE($3, day_boundary_minutes), \
               height_mm = CASE WHEN $4 THEN $5 ELSE height_mm END, \
               born_on = CASE WHEN $6 THEN $7 ELSE born_on END, \
               sex = CASE WHEN $8 THEN $9 ELSE sex END, \
               activity_factor = COALESCE($10, activity_factor) \
             WHERE id = $1 RETURNING *",
        )
        .bind(id)
        .bind(p.name)
        .bind(p.day_boundary_minutes)
        .bind(p.height_mm.is_some())
        .bind(p.height_mm.flatten())
        .bind(p.born_on.is_some())
        .bind(p.born_on.flatten())
        .bind(p.sex.is_some())
        .bind(p.sex.flatten())
        .bind(p.activity_factor)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => DbError::NotFound,
            e => conflict_or(e, || "profile value out of range".into()),
        })
    }

    // ----- locations -------------------------------------------------------

    pub async fn list_locations(&self, user_id: i32) -> DbResult<Vec<UserLocation>> {
        Ok(
            sqlx::query_as("SELECT * FROM user_locations WHERE user_id = $1 ORDER BY valid_from")
                .bind(user_id)
                .fetch_all(&self.pool)
                .await?,
        )
    }

    pub async fn get_location(&self, id: i32) -> DbResult<UserLocation> {
        sqlx::query_as("SELECT * FROM user_locations WHERE id = $1")
            .bind(id)
            .fetch_one(&self.pool)
            .await
            .map_err(not_found_or)
    }

    /// The zone in force for a user at `instant`; NotFound only for a user
    /// without an origin row, which `upsert_user` prevents.
    pub async fn zone_at(&self, user_id: i32, instant: DateTime<Utc>) -> DbResult<String> {
        let row: (String,) = sqlx::query_as(
            "SELECT timezone FROM user_locations WHERE user_id = $1 AND valid_from <= $2 \
             ORDER BY valid_from DESC LIMIT 1",
        )
        .bind(user_id)
        .bind(instant)
        .fetch_one(&self.pool)
        .await
        .map_err(not_found_or)?;
        Ok(row.0)
    }

    pub async fn insert_location(
        &self,
        user_id: i32,
        valid_from: DateTime<Utc>,
        timezone: &str,
    ) -> DbResult<UserLocation> {
        sqlx::query_as(
            "INSERT INTO user_locations (user_id, valid_from, timezone) VALUES ($1, $2, $3) \
             RETURNING *",
        )
        .bind(user_id)
        .bind(valid_from)
        .bind(timezone)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| conflict_or(e, || format!("a location already starts at {valid_from}")))
    }

    pub async fn update_location(
        &self,
        id: i32,
        valid_from: Option<DateTime<Utc>>,
        timezone: Option<&str>,
    ) -> DbResult<UserLocation> {
        let current = self.get_location(id).await?;
        if current.is_origin() && valid_from.is_some_and(|v| v != current.valid_from) {
            return Err(DbError::Conflict(
                "the origin location cannot be moved; change its zone instead".into(),
            ));
        }
        sqlx::query_as(
            "UPDATE user_locations SET valid_from = COALESCE($2, valid_from), \
             timezone = COALESCE($3, timezone) WHERE id = $1 RETURNING *",
        )
        .bind(id)
        .bind(valid_from)
        .bind(timezone)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => DbError::NotFound,
            e => conflict_or(e, || "a location already starts at that instant".into()),
        })
    }

    pub async fn delete_location(&self, id: i32) -> DbResult<()> {
        let current = self.get_location(id).await?;
        if current.is_origin() {
            return Err(DbError::Conflict(
                "the origin location cannot be deleted; change its zone instead".into(),
            ));
        }
        affected(
            sqlx::query("DELETE FROM user_locations WHERE id = $1")
                .bind(id)
                .execute(&self.pool)
                .await?,
        )
    }

    // ----- targets ---------------------------------------------------------

    pub async fn list_targets(&self, user_id: i32) -> DbResult<Vec<Target>> {
        Ok(
            sqlx::query_as("SELECT * FROM targets WHERE user_id = $1 ORDER BY valid_from")
                .bind(user_id)
                .fetch_all(&self.pool)
                .await?,
        )
    }

    pub async fn get_target(&self, id: i32) -> DbResult<Target> {
        sqlx::query_as("SELECT * FROM targets WHERE id = $1")
            .bind(id)
            .fetch_one(&self.pool)
            .await
            .map_err(not_found_or)
    }

    /// The target in force on `day`, if any.
    pub async fn target_for(&self, user_id: i32, day: NaiveDate) -> DbResult<Option<Target>> {
        Ok(sqlx::query_as(
            "SELECT * FROM targets WHERE user_id = $1 AND valid_from <= $2 \
             ORDER BY valid_from DESC LIMIT 1",
        )
        .bind(user_id)
        .bind(day)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn insert_target(&self, t: NewTarget) -> DbResult<Target> {
        sqlx::query_as(
            "INSERT INTO targets (user_id, valid_from, kcal, protein_g, weight_g) \
             VALUES ($1, $2, $3, $4, $5) RETURNING *",
        )
        .bind(t.user_id)
        .bind(t.valid_from)
        .bind(t.kcal)
        .bind(t.protein_g)
        .bind(t.weight_g)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| conflict_or(e, || format!("a target already starts on {}", t.valid_from)))
    }

    pub async fn update_target(&self, id: i32, p: TargetPatch) -> DbResult<Target> {
        sqlx::query_as(
            "UPDATE targets SET valid_from = COALESCE($2, valid_from), kcal = COALESCE($3, kcal), \
             protein_g = CASE WHEN $4 THEN $5 ELSE protein_g END, \
             weight_g = CASE WHEN $6 THEN $7 ELSE weight_g END \
             WHERE id = $1 RETURNING *",
        )
        .bind(id)
        .bind(p.valid_from)
        .bind(p.kcal)
        .bind(p.protein_g.is_some())
        .bind(p.protein_g.flatten())
        .bind(p.weight_g.is_some())
        .bind(p.weight_g.flatten())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => DbError::NotFound,
            e => conflict_or(e, || "target conflicts with an existing row".into()),
        })
    }

    pub async fn delete_target(&self, id: i32) -> DbResult<()> {
        affected(
            sqlx::query("DELETE FROM targets WHERE id = $1")
                .bind(id)
                .execute(&self.pool)
                .await?,
        )
    }

    // ----- foods -----------------------------------------------------------

    /// Foods of a household with their usage counts, most used first, then
    /// by name. `query` matches the name or an alias case-insensitively as a
    /// fragment; empty lists everything.
    pub async fn search_foods(
        &self,
        household_id: i32,
        query: &str,
        include_archived: bool,
    ) -> DbResult<Vec<FoodWithUsage>> {
        let q = query.trim();
        Ok(sqlx::query_as(
            "SELECT f.*, \
               (SELECT count(*) FROM meals m WHERE m.food_id = f.id AND m.voided_at IS NULL) AS uses \
             FROM foods f \
             WHERE f.household_id = $1 AND (f.archived_at IS NULL OR $3) \
               AND ($2 = '' OR f.name ILIKE '%' || $2 || '%' \
                    OR EXISTS (SELECT 1 FROM unnest(f.aliases) a WHERE a ILIKE '%' || $2 || '%')) \
             ORDER BY (lower(f.name) = lower($2) OR lower($2) = ANY (SELECT lower(a) FROM unnest(f.aliases) a)) DESC, \
                      uses DESC, lower(f.name), f.id",
        )
        .bind(household_id)
        .bind(q)
        .bind(include_archived)
        .fetch_all(&self.pool)
        .await?)
    }

    /// Exact match on name or alias, case-insensitive, unarchived only.
    pub async fn find_food(&self, household_id: i32, name: &str) -> DbResult<Option<Food>> {
        Ok(sqlx::query_as(
            "SELECT * FROM foods WHERE household_id = $1 AND archived_at IS NULL \
               AND (lower(name) = lower($2) \
                    OR lower($2) = ANY (SELECT lower(a) FROM unnest(aliases) a)) \
             ORDER BY (lower(name) = lower($2)) DESC, id LIMIT 1",
        )
        .bind(household_id)
        .bind(name.trim())
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn get_food(&self, id: i32) -> DbResult<Food> {
        sqlx::query_as("SELECT * FROM foods WHERE id = $1")
            .bind(id)
            .fetch_one(&self.pool)
            .await
            .map_err(not_found_or)
    }

    pub async fn insert_food(&self, f: NewFood) -> DbResult<Food> {
        sqlx::query_as(
            "INSERT INTO foods (household_id, name, aliases, portion, kcal, protein_g, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING *",
        )
        .bind(f.household_id)
        .bind(f.name.trim())
        .bind(&f.aliases)
        .bind(f.portion.trim())
        .bind(f.kcal)
        .bind(f.protein_g)
        .bind(f.created_by)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            conflict_or(e, || {
                format!("a food named {:?} already exists", f.name.trim())
            })
        })
    }

    pub async fn update_food(&self, id: i32, p: FoodPatch) -> DbResult<Food> {
        sqlx::query_as(
            "UPDATE foods SET name = COALESCE($2, name), aliases = COALESCE($3, aliases), \
             portion = COALESCE($4, portion), kcal = COALESCE($5, kcal), \
             protein_g = CASE WHEN $6 THEN $7 ELSE protein_g END, updated_at = now() \
             WHERE id = $1 RETURNING *",
        )
        .bind(id)
        .bind(p.name.map(|n| n.trim().to_string()))
        .bind(p.aliases)
        .bind(p.portion)
        .bind(p.kcal)
        .bind(p.protein_g.is_some())
        .bind(p.protein_g.flatten())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => DbError::NotFound,
            e => conflict_or(e, || "a food with that name already exists".into()),
        })
    }

    pub async fn set_food_archived(&self, id: i32, archived: bool) -> DbResult<Food> {
        sqlx::query_as(
            "UPDATE foods SET archived_at = CASE WHEN $2 THEN COALESCE(archived_at, now()) ELSE NULL END, \
             updated_at = now() WHERE id = $1 RETURNING *",
        )
        .bind(id)
        .bind(archived)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => DbError::NotFound,
            e => conflict_or(e, || "an unarchived food with that name already exists".into()),
        })
    }

    // ----- meals -----------------------------------------------------------

    pub async fn list_meals(
        &self,
        user_id: i32,
        from: NaiveDate,
        to: NaiveDate,
        include_voided: bool,
    ) -> DbResult<Vec<Meal>> {
        Ok(sqlx::query_as(
            "SELECT * FROM meals WHERE user_id = $1 AND day BETWEEN $2 AND $3 \
               AND (voided_at IS NULL OR $4) ORDER BY day, eaten_at, id",
        )
        .bind(user_id)
        .bind(from)
        .bind(to)
        .bind(include_voided)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn get_meal(&self, id: i32) -> DbResult<Meal> {
        sqlx::query_as("SELECT * FROM meals WHERE id = $1")
            .bind(id)
            .fetch_one(&self.pool)
            .await
            .map_err(not_found_or)
    }

    pub async fn insert_meal(&self, m: NewMeal) -> DbResult<Meal> {
        sqlx::query_as(&format!(
            "INSERT INTO meals ({MEAL_COLS}) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) RETURNING *"
        ))
        .bind(m.user_id)
        .bind(m.eaten_at)
        .bind(&m.timezone)
        .bind(m.day)
        .bind(m.day_override)
        .bind(m.description.trim())
        .bind(m.kcal)
        .bind(m.protein_g)
        .bind(&m.source)
        .bind(m.food_id)
        .bind(m.portions)
        .bind(m.created_by)
        .bind(&m.created_via)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| conflict_or(e, || "meal value out of range".into()))
    }

    pub async fn update_meal(&self, id: i32, p: MealPatch) -> DbResult<Meal> {
        sqlx::query_as(
            "UPDATE meals SET eaten_at = COALESCE($2, eaten_at), timezone = COALESCE($3, timezone), \
             day = COALESCE($4, day), day_override = COALESCE($5, day_override), \
             description = COALESCE($6, description), kcal = COALESCE($7, kcal), \
             protein_g = CASE WHEN $8 THEN $9 ELSE protein_g END, source = COALESCE($10, source), \
             food_id = CASE WHEN $11 THEN $12 ELSE food_id END, \
             portions = CASE WHEN $13 THEN $14 ELSE portions END, updated_at = now() \
             WHERE id = $1 RETURNING *",
        )
        .bind(id)
        .bind(p.eaten_at)
        .bind(p.timezone)
        .bind(p.day)
        .bind(p.day_override)
        .bind(p.description.map(|d| d.trim().to_string()))
        .bind(p.kcal)
        .bind(p.protein_g.is_some())
        .bind(p.protein_g.flatten())
        .bind(p.source)
        .bind(p.food_id.is_some())
        .bind(p.food_id.flatten())
        .bind(p.portions.is_some())
        .bind(p.portions.flatten())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => DbError::NotFound,
            e => conflict_or(e, || "meal value out of range".into()),
        })
    }

    // ----- weights ---------------------------------------------------------

    pub async fn list_weights(
        &self,
        user_id: i32,
        from: NaiveDate,
        to: NaiveDate,
        include_voided: bool,
    ) -> DbResult<Vec<Weight>> {
        Ok(sqlx::query_as(
            "SELECT * FROM weights WHERE user_id = $1 AND day BETWEEN $2 AND $3 \
               AND (voided_at IS NULL OR $4) ORDER BY day, measured_at, id",
        )
        .bind(user_id)
        .bind(from)
        .bind(to)
        .bind(include_voided)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn get_weight(&self, id: i32) -> DbResult<Weight> {
        sqlx::query_as("SELECT * FROM weights WHERE id = $1")
            .bind(id)
            .fetch_one(&self.pool)
            .await
            .map_err(not_found_or)
    }

    pub async fn insert_weight(&self, w: NewWeight) -> DbResult<Weight> {
        sqlx::query_as(&format!(
            "INSERT INTO weights ({WEIGHT_COLS}) VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING *"
        ))
        .bind(w.user_id)
        .bind(w.measured_at)
        .bind(&w.timezone)
        .bind(w.day)
        .bind(w.day_override)
        .bind(w.weight_g)
        .bind(w.created_by)
        .bind(&w.created_via)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| conflict_or(e, || "weight out of range (20 to 400 kg)".into()))
    }

    pub async fn update_weight(&self, id: i32, p: WeightPatch) -> DbResult<Weight> {
        sqlx::query_as(
            "UPDATE weights SET measured_at = COALESCE($2, measured_at), \
             timezone = COALESCE($3, timezone), day = COALESCE($4, day), \
             day_override = COALESCE($5, day_override), weight_g = COALESCE($6, weight_g), \
             updated_at = now() WHERE id = $1 RETURNING *",
        )
        .bind(id)
        .bind(p.measured_at)
        .bind(p.timezone)
        .bind(p.day)
        .bind(p.day_override)
        .bind(p.weight_g)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => DbError::NotFound,
            e => conflict_or(e, || "weight out of range (20 to 400 kg)".into()),
        })
    }

    // ----- activities ------------------------------------------------------

    pub async fn list_activities(
        &self,
        user_id: i32,
        from: NaiveDate,
        to: NaiveDate,
        include_voided: bool,
    ) -> DbResult<Vec<Activity>> {
        Ok(sqlx::query_as(
            "SELECT * FROM activities WHERE user_id = $1 AND day BETWEEN $2 AND $3 \
               AND (voided_at IS NULL OR $4) ORDER BY day, started_at, id",
        )
        .bind(user_id)
        .bind(from)
        .bind(to)
        .bind(include_voided)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn get_activity(&self, id: i32) -> DbResult<Activity> {
        sqlx::query_as("SELECT * FROM activities WHERE id = $1")
            .bind(id)
            .fetch_one(&self.pool)
            .await
            .map_err(not_found_or)
    }

    pub async fn insert_activity(&self, a: NewActivity) -> DbResult<Activity> {
        sqlx::query_as(&format!(
            "INSERT INTO activities ({ACTIVITY_COLS}) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) RETURNING *"
        ))
        .bind(a.user_id)
        .bind(a.started_at)
        .bind(&a.timezone)
        .bind(a.day)
        .bind(a.day_override)
        .bind(a.kind.trim().to_lowercase())
        .bind(a.minutes)
        .bind(a.kcal)
        .bind(&a.source)
        .bind(a.activity_kind_id)
        .bind(a.note.trim())
        .bind(a.created_by)
        .bind(&a.created_via)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| conflict_or(e, || "activity value out of range".into()))
    }

    pub async fn update_activity(&self, id: i32, p: ActivityPatch) -> DbResult<Activity> {
        sqlx::query_as(
            "UPDATE activities SET started_at = COALESCE($2, started_at), \
             timezone = COALESCE($3, timezone), day = COALESCE($4, day), \
             day_override = COALESCE($5, day_override), kind = COALESCE($6, kind), \
             minutes = COALESCE($7, minutes), kcal = CASE WHEN $8 THEN $9 ELSE kcal END, \
             source = COALESCE($10, source), \
             activity_kind_id = CASE WHEN $11 THEN $12 ELSE activity_kind_id END, \
             note = COALESCE($13, note), updated_at = now() WHERE id = $1 RETURNING *",
        )
        .bind(id)
        .bind(p.started_at)
        .bind(p.timezone)
        .bind(p.day)
        .bind(p.day_override)
        .bind(p.kind.map(|k| k.trim().to_lowercase()))
        .bind(p.minutes)
        .bind(p.kcal.is_some())
        .bind(p.kcal.flatten())
        .bind(p.source)
        .bind(p.activity_kind_id.is_some())
        .bind(p.activity_kind_id.flatten())
        .bind(p.note.map(|n| n.trim().to_string()))
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => DbError::NotFound,
            e => conflict_or(e, || "activity value out of range".into()),
        })
    }

    // ----- shared entry operations -----------------------------------------

    /// The owner of an entry, for household checks. NotFound for a missing id.
    pub async fn entry_owner(&self, kind: EntryKind, id: i32) -> DbResult<i32> {
        let row: (i32,) = sqlx::query_as(&format!(
            "SELECT user_id FROM {} WHERE id = $1",
            kind.table()
        ))
        .bind(id)
        .fetch_one(&self.pool)
        .await
        .map_err(not_found_or)?;
        Ok(row.0)
    }

    /// Soft delete. Already voided is NotFound so a double undo is visible.
    pub async fn void_entry(&self, kind: EntryKind, id: i32) -> DbResult<()> {
        affected(
            sqlx::query(&format!(
                "UPDATE {} SET voided_at = now(), updated_at = now() \
                 WHERE id = $1 AND voided_at IS NULL",
                kind.table()
            ))
            .bind(id)
            .execute(&self.pool)
            .await?,
        )
    }

    pub async fn unvoid_entry(&self, kind: EntryKind, id: i32) -> DbResult<()> {
        affected(
            sqlx::query(&format!(
                "UPDATE {} SET voided_at = NULL, updated_at = now() \
                 WHERE id = $1 AND voided_at IS NOT NULL",
                kind.table()
            ))
            .bind(id)
            .execute(&self.pool)
            .await?,
        )
    }

    /// Recompute the zone and day of every non-overridden entry of a user
    /// with `f(instant) -> (zone, day)`, in one transaction. Returns how many
    /// rows changed.
    pub async fn recompute_days<F>(&self, user_id: i32, f: F) -> DbResult<usize>
    where
        F: Fn(DateTime<Utc>) -> (String, NaiveDate),
    {
        let mut tx = self.pool.begin().await?;
        let mut changed = 0;
        for kind in [EntryKind::Meal, EntryKind::Weight, EntryKind::Activity] {
            let rows: Vec<EntryTime> = sqlx::query_as(&format!(
                "SELECT id, {} AS instant, timezone, day FROM {} \
                 WHERE user_id = $1 AND NOT day_override ORDER BY id",
                kind.instant_column(),
                kind.table()
            ))
            .bind(user_id)
            .fetch_all(&mut *tx)
            .await?;
            for row in rows {
                let (zone, day) = f(row.instant);
                if zone == row.timezone && day == row.day {
                    continue;
                }
                sqlx::query(&format!(
                    "UPDATE {} SET timezone = $2, day = $3, updated_at = now() WHERE id = $1",
                    kind.table()
                ))
                .bind(row.id)
                .bind(&zone)
                .bind(day)
                .execute(&mut *tx)
                .await?;
                changed += 1;
            }
        }
        tx.commit().await?;
        Ok(changed)
    }

    // ----- per-day aggregates ----------------------------------------------

    pub async fn meal_day_totals(
        &self,
        user_id: i32,
        from: NaiveDate,
        to: NaiveDate,
    ) -> DbResult<Vec<MealDayTotals>> {
        Ok(sqlx::query_as(
            "SELECT day, sum(kcal)::int AS kcal, sum(protein_g)::int AS protein_g, \
                    count(*)::int AS meals, \
                    (count(*) FILTER (WHERE protein_g IS NULL))::int AS meals_without_protein \
             FROM meals WHERE user_id = $1 AND day BETWEEN $2 AND $3 AND voided_at IS NULL \
             GROUP BY day ORDER BY day",
        )
        .bind(user_id)
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await?)
    }

    /// Every kind, archived ones only when asked, best match for `query` first.
    pub async fn search_activity_kinds(
        &self,
        query: &str,
        include_archived: bool,
    ) -> DbResult<Vec<ActivityKind>> {
        let q = query.trim();
        Ok(sqlx::query_as(
            "SELECT * FROM activity_kinds \
             WHERE (archived_at IS NULL OR $2) \
               AND ($1 = '' OR name ILIKE '%' || $1 || '%' \
                    OR EXISTS (SELECT 1 FROM unnest(aliases) a WHERE a ILIKE '%' || $1 || '%')) \
             ORDER BY (lower(name) = lower($1) OR lower($1) = ANY (SELECT lower(a) FROM unnest(aliases) a)) DESC, \
                      lower(name), id",
        )
        .bind(q)
        .bind(include_archived)
        .fetch_all(&self.pool)
        .await?)
    }

    /// Exact match on name or alias, case-insensitive, unarchived only.
    pub async fn find_activity_kind(&self, name: &str) -> DbResult<Option<ActivityKind>> {
        Ok(sqlx::query_as(
            "SELECT * FROM activity_kinds WHERE archived_at IS NULL \
               AND (lower(name) = lower($1) \
                    OR lower($1) = ANY (SELECT lower(a) FROM unnest(aliases) a)) \
             ORDER BY (lower(name) = lower($1)) DESC, id LIMIT 1",
        )
        .bind(name.trim())
        .fetch_optional(&self.pool)
        .await?)
    }

    /// Non-voided sessions whose kcal came from this rate.
    pub async fn activity_kind_uses(&self, id: i32) -> DbResult<i64> {
        let (n,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM activities WHERE activity_kind_id = $1 AND voided_at IS NULL",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await?;
        Ok(n)
    }

    pub async fn get_activity_kind(&self, id: i32) -> DbResult<ActivityKind> {
        sqlx::query_as("SELECT * FROM activity_kinds WHERE id = $1")
            .bind(id)
            .fetch_one(&self.pool)
            .await
            .map_err(not_found_or)
    }

    pub async fn insert_activity_kind(&self, k: NewActivityKind) -> DbResult<ActivityKind> {
        sqlx::query_as(
            "INSERT INTO activity_kinds (name, aliases, met, note) \
             VALUES ($1, $2, $3, $4) RETURNING *",
        )
        .bind(k.name.trim().to_lowercase())
        .bind(&k.aliases)
        .bind(k.met)
        .bind(k.note.trim())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            conflict_or(e, || {
                format!("a sport kind named {:?} already exists", k.name.trim())
            })
        })
    }

    pub async fn update_activity_kind(
        &self,
        id: i32,
        p: ActivityKindPatch,
    ) -> DbResult<ActivityKind> {
        sqlx::query_as(
            "UPDATE activity_kinds SET name = COALESCE($2, name), \
             aliases = COALESCE($3, aliases), met = COALESCE($4, met), \
             note = COALESCE($5, note), updated_at = now() WHERE id = $1 RETURNING *",
        )
        .bind(id)
        .bind(p.name.map(|n| n.trim().to_lowercase()))
        .bind(p.aliases)
        .bind(p.met)
        .bind(p.note.map(|n| n.trim().to_string()))
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => DbError::NotFound,
            e => conflict_or(e, || "a sport kind with that name already exists".into()),
        })
    }

    pub async fn set_activity_kind_archived(
        &self,
        id: i32,
        archived: bool,
    ) -> DbResult<ActivityKind> {
        sqlx::query_as(
            "UPDATE activity_kinds SET archived_at = CASE WHEN $2 THEN COALESCE(archived_at, now()) ELSE NULL END, \
             updated_at = now() WHERE id = $1 RETURNING *",
        )
        .bind(id)
        .bind(archived)
        .fetch_one(&self.pool)
        .await
        .map_err(not_found_or)
    }

    pub async fn activity_day_totals(
        &self,
        user_id: i32,
        from: NaiveDate,
        to: NaiveDate,
    ) -> DbResult<Vec<ActivityDayTotals>> {
        Ok(sqlx::query_as(
            "SELECT day, sum(minutes)::int AS minutes, count(*)::int AS activities, \
             sum(kcal)::int AS kcal \
             FROM activities WHERE user_id = $1 AND day BETWEEN $2 AND $3 AND voided_at IS NULL \
             GROUP BY day ORDER BY day",
        )
        .bind(user_id)
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await?)
    }

    /// The earliest non-voided reading of each day in the range.
    pub async fn day_weights(
        &self,
        user_id: i32,
        from: NaiveDate,
        to: NaiveDate,
    ) -> DbResult<Vec<DayWeight>> {
        Ok(sqlx::query_as(
            "SELECT DISTINCT ON (day) day, weight_g FROM weights \
             WHERE user_id = $1 AND day BETWEEN $2 AND $3 AND voided_at IS NULL \
             ORDER BY day, measured_at, id",
        )
        .bind(user_id)
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await?)
    }

    /// The first day with any non-voided entry for a user, for "all time" ranges.
    pub async fn first_day(&self, user_id: i32) -> DbResult<Option<NaiveDate>> {
        let row: (Option<NaiveDate>,) = sqlx::query_as(
            "SELECT least( \
               (SELECT min(day) FROM meals WHERE user_id = $1 AND voided_at IS NULL), \
               (SELECT min(day) FROM weights WHERE user_id = $1 AND voided_at IS NULL), \
               (SELECT min(day) FROM activities WHERE user_id = $1 AND voided_at IS NULL))",
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    // ----- api tokens ------------------------------------------------------

    pub async fn create_token(
        &self,
        name: &str,
        token_hash: &str,
        scopes: &[String],
        user_id: Option<i32>,
        created_by: Option<i32>,
    ) -> DbResult<ApiToken> {
        sqlx::query_as(
            "INSERT INTO api_tokens (name, token_hash, scopes, user_id, created_by) \
             VALUES ($1, $2, $3, $4, $5) RETURNING *",
        )
        .bind(name.trim())
        .bind(token_hash)
        .bind(scopes)
        .bind(user_id)
        .bind(created_by)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            conflict_or(e, || {
                "a delegate token has no user; every other token needs one".into()
            })
        })
    }

    pub async fn list_tokens(&self) -> DbResult<Vec<ApiToken>> {
        Ok(sqlx::query_as("SELECT * FROM api_tokens ORDER BY id")
            .fetch_all(&self.pool)
            .await?)
    }

    pub async fn revoke_token(&self, id: i32) -> DbResult<()> {
        affected(
            sqlx::query(
                "UPDATE api_tokens SET revoked_at = now() WHERE id = $1 AND revoked_at IS NULL",
            )
            .bind(id)
            .execute(&self.pool)
            .await?,
        )
    }

    /// Active token by hash. Bumps `last_used_at` at most once a minute.
    pub async fn find_active_token(&self, token_hash: &str) -> DbResult<Option<ApiToken>> {
        let token: Option<ApiToken> =
            sqlx::query_as("SELECT * FROM api_tokens WHERE token_hash = $1 AND revoked_at IS NULL")
                .bind(token_hash)
                .fetch_optional(&self.pool)
                .await?;
        if let Some(t) = &token {
            let stale = t
                .last_used_at
                .map(|u| Utc::now() - u > chrono::Duration::minutes(1))
                .unwrap_or(true);
            if stale {
                sqlx::query("UPDATE api_tokens SET last_used_at = now() WHERE id = $1")
                    .bind(t.id)
                    .execute(&self.pool)
                    .await?;
            }
        }
        Ok(token)
    }
}
