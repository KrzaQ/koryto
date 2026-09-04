//! Who may see and touch whose data: everyone in a household, nobody else.

use crate::db::{Db, EntryKind, Food, User};

use super::{AppError, AppResult};

/// The acting user's household, or 403 for someone not placed in one yet.
pub fn household_of(actor: &User) -> AppResult<i32> {
    actor
        .household_id
        .ok_or_else(|| AppError::Forbidden("you are in no household; log in again".into()))
}

/// The person a request concerns: `user_id`, defaulting to the actor, who
/// must be in the actor's household.
pub async fn member(db: &Db, actor: &User, user_id: Option<i32>) -> AppResult<User> {
    let household = household_of(actor)?;
    let Some(id) = user_id.filter(|id| *id != actor.id) else {
        return Ok(actor.clone());
    };
    let user = db.get_user(id).await.map_err(|_| AppError::NotFound)?;
    if user.household_id != Some(household) {
        return Err(AppError::Forbidden(format!(
            "{} is not in your household",
            user.display()
        )));
    }
    Ok(user)
}

/// Resolve a household member by id, email or a fragment of their name.
pub async fn member_named(db: &Db, actor: &User, who: &str) -> AppResult<User> {
    let household = household_of(actor)?;
    let who = who.trim();
    if let Ok(id) = who.parse::<i32>() {
        return member(db, actor, Some(id)).await;
    }
    let members = db.household_members(household).await?;
    let lower = who.to_lowercase();
    if lower == "me" || lower == "myself" {
        return Ok(actor.clone());
    }
    let mut hits: Vec<&User> = members
        .iter()
        .filter(|u| {
            u.email
                .as_deref()
                .is_some_and(|e| e.eq_ignore_ascii_case(who))
        })
        .collect();
    if hits.is_empty() {
        hits = members
            .iter()
            .filter(|u| u.display().to_lowercase().contains(&lower))
            .collect();
    }
    match hits.len() {
        1 => Ok(hits[0].clone()),
        0 => Err(AppError::NotFound),
        _ => Err(AppError::Conflict(format!(
            "{who:?} matches several people: {}",
            hits.iter()
                .map(|u| u.display())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

/// An entry the actor may touch: it belongs to someone in the household.
pub async fn owned_entry(db: &Db, actor: &User, kind: EntryKind, id: i32) -> AppResult<User> {
    let owner = db.entry_owner(kind, id).await?;
    member(db, actor, Some(owner)).await
}

/// A food of the actor's household.
pub async fn food(db: &Db, actor: &User, id: i32) -> AppResult<Food> {
    let household = household_of(actor)?;
    let f = db.get_food(id).await?;
    if f.household_id != household {
        return Err(AppError::NotFound);
    }
    Ok(f)
}
