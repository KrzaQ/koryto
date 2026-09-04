use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;

use super::*;
use crate::app::time as apptime;
use crate::db_or_skip;

fn utc(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}
fn d(s: &str) -> NaiveDate {
    s.parse().unwrap()
}
fn dec(s: &str) -> Decimal {
    s.parse().unwrap()
}

struct Home {
    household: Household,
    alice: User,
    bob: User,
}

async fn seed_home(db: &Db) -> Home {
    let household = db.create_household("home").await.unwrap();
    let alice = db
        .upsert_user(
            "sub-alice",
            Some("alice@example.com"),
            Some("Alice"),
            "Europe/Warsaw",
        )
        .await
        .unwrap();
    let bob = db
        .upsert_user(
            "sub-bob",
            Some("bob@example.com"),
            Some("Bob"),
            "Europe/Warsaw",
        )
        .await
        .unwrap();
    let alice = db
        .set_user_household(alice.id, Some(household.id))
        .await
        .unwrap();
    let bob = db
        .set_user_household(bob.id, Some(household.id))
        .await
        .unwrap();
    Home {
        household,
        alice,
        bob,
    }
}

async fn meal_at(db: &Db, user: &User, instant: &str, kcal: i32, protein: Option<i32>) -> Meal {
    let r = apptime::resolve(db, user, Some(utc(instant)), None, None)
        .await
        .unwrap();
    db.insert_meal(NewMeal {
        user_id: user.id,
        eaten_at: r.instant,
        timezone: r.timezone,
        day: r.day,
        day_override: r.day_override,
        description: format!("meal at {instant}"),
        kcal,
        protein_g: protein,
        source: SOURCE_ESTIMATE.into(),
        food_id: None,
        portions: None,
        created_by: user.id,
        created_via: VIA_MCP.into(),
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn migrations_apply_and_report_status() {
    let t = db_or_skip!();
    let status = t.db.migration_status().await.unwrap();
    assert!(!status.is_empty());
    assert!(status.iter().all(|(_, _, applied)| *applied));
    t.finish().await;
}

#[tokio::test]
async fn households_users_and_origin_location() {
    let t = db_or_skip!();
    let db = &t.db;
    let home = seed_home(db).await;

    // Login again: same row, refreshed, still one origin location.
    let again = db
        .upsert_user("sub-alice", None, None, "America/New_York")
        .await
        .unwrap();
    assert_eq!(again.id, home.alice.id);
    assert_eq!(again.email.as_deref(), Some("alice@example.com"));
    let locs = db.list_locations(home.alice.id).await.unwrap();
    assert_eq!(locs.len(), 1);
    assert!(locs[0].is_origin());
    assert_eq!(locs[0].timezone, "Europe/Warsaw");

    let members = db.household_members(home.household.id).await.unwrap();
    assert_eq!(
        members.iter().map(|u| u.id).collect::<Vec<_>>(),
        [home.alice.id, home.bob.id]
    );
    assert_eq!(
        db.find_household("HOME").await.unwrap().id,
        home.household.id
    );
    assert!(matches!(
        db.find_household("nope").await,
        Err(DbError::NotFound)
    ));

    let carol = db
        .upsert_user(
            "sub-carol",
            Some("Carol@Example.com"),
            None,
            "Europe/Warsaw",
        )
        .await
        .unwrap();
    assert!(carol.household_id.is_none());
    assert_eq!(
        db.find_user_by_email("carol@example.com")
            .await
            .unwrap()
            .unwrap()
            .id,
        carol.id
    );
    assert_eq!(carol.display(), "Carol@Example.com");

    let profiled = db
        .update_profile(
            home.alice.id,
            ProfilePatch {
                height_mm: Some(Some(1700)),
                sex: Some(Some("female".into())),
                day_boundary_minutes: Some(180),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(profiled.height_mm, Some(1700));
    assert_eq!(profiled.day_boundary_minutes, 180);
    let cleared = db
        .update_profile(
            home.alice.id,
            ProfilePatch {
                height_mm: Some(None),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(cleared.height_mm, None);
    assert_eq!(cleared.sex.as_deref(), Some("female"));
    assert!(matches!(
        db.update_profile(
            home.alice.id,
            ProfilePatch {
                sex: Some(Some("yes".into())),
                ..Default::default()
            }
        )
        .await,
        Err(DbError::Conflict(_))
    ));
    t.finish().await;
}

#[tokio::test]
async fn locations_are_effective_dated_and_origin_is_fixed() {
    let t = db_or_skip!();
    let db = &t.db;
    let home = seed_home(db).await;
    let a = &home.alice;

    let ny = db
        .insert_location(a.id, utc("2026-09-10T12:00:00Z"), "America/New_York")
        .await
        .unwrap();
    assert!(matches!(
        db.insert_location(a.id, utc("2026-09-10T12:00:00Z"), "Asia/Tokyo")
            .await,
        Err(DbError::Conflict(_))
    ));
    assert_eq!(
        db.zone_at(a.id, utc("2026-09-01T00:00:00Z")).await.unwrap(),
        "Europe/Warsaw"
    );
    assert_eq!(
        db.zone_at(a.id, utc("2026-09-10T12:00:00Z")).await.unwrap(),
        "America/New_York"
    );
    // Bob is unaffected.
    assert_eq!(
        db.zone_at(home.bob.id, utc("2026-09-20T00:00:00Z"))
            .await
            .unwrap(),
        "Europe/Warsaw"
    );

    let origin_row = db.list_locations(a.id).await.unwrap().remove(0);
    assert!(matches!(
        db.delete_location(origin_row.id).await,
        Err(DbError::Conflict(_))
    ));
    assert!(matches!(
        db.update_location(origin_row.id, Some(utc("2020-01-01T00:00:00Z")), None)
            .await,
        Err(DbError::Conflict(_))
    ));
    let moved = db
        .update_location(origin_row.id, None, Some("Europe/Berlin"))
        .await
        .unwrap();
    assert_eq!(moved.timezone, "Europe/Berlin");
    db.delete_location(ny.id).await.unwrap();
    assert!(matches!(
        db.delete_location(ny.id).await,
        Err(DbError::NotFound)
    ));
    t.finish().await;
}

#[tokio::test]
async fn entries_get_days_from_zone_and_boundary_and_recompute() {
    let t = db_or_skip!();
    let db = &t.db;
    let home = seed_home(db).await;
    let a = &home.alice;

    // Warsaw, default 04:00 boundary: 01:30 local on the 5th is the 4th.
    let late = meal_at(db, a, "2026-09-04T23:30:00Z", 600, Some(30)).await;
    assert_eq!(late.timezone, "Europe/Warsaw");
    assert_eq!(late.day, d("2026-09-04"));
    let morning = meal_at(db, a, "2026-09-05T06:00:00Z", 400, None).await;
    assert_eq!(morning.day, d("2026-09-05"));

    // Fly to New York on the 10th: a 21:00 dinner there is still the 12th
    // on her clock even though Warsaw is already the 13th.
    db.insert_location(a.id, utc("2026-09-10T12:00:00Z"), "America/New_York")
        .await
        .unwrap();
    let dinner = meal_at(db, a, "2026-09-13T01:00:00Z", 900, Some(40)).await;
    assert_eq!(dinner.timezone, "America/New_York");
    assert_eq!(dinner.day, d("2026-09-12"));

    // An explicit day override sticks.
    let r = apptime::resolve(
        db,
        a,
        Some(utc("2026-09-13T02:00:00Z")),
        None,
        Some(d("2026-09-13")),
    )
    .await
    .unwrap();
    assert!(r.day_override);
    let overridden = db
        .insert_meal(NewMeal {
            user_id: a.id,
            eaten_at: r.instant,
            timezone: r.timezone,
            day: r.day,
            day_override: r.day_override,
            description: "counted for tomorrow".into(),
            kcal: 100,
            protein_g: None,
            source: SOURCE_MANUAL.into(),
            food_id: None,
            portions: None,
            created_by: a.id,
            created_via: VIA_WEB.into(),
        })
        .await
        .unwrap();
    assert_eq!(overridden.day, d("2026-09-13"));

    // The flight was actually on the 4th: move the location back and
    // recompute. The late meal on the 4th evening was 17:30 in New York and
    // so stays on the 4th; the morning meal at 08:00 Warsaw is 02:00 in
    // New York and moves to the 4th as well. The override does not move.
    let ny = db.list_locations(a.id).await.unwrap().remove(1);
    db.update_location(ny.id, Some(utc("2026-09-04T00:00:00Z")), None)
        .await
        .unwrap();
    let changed = apptime::recompute_days(db, a).await.unwrap();
    assert_eq!(changed, 2);
    assert_eq!(
        db.get_meal(late.id).await.unwrap().timezone,
        "America/New_York"
    );
    assert_eq!(db.get_meal(late.id).await.unwrap().day, d("2026-09-04"));
    assert_eq!(db.get_meal(morning.id).await.unwrap().day, d("2026-09-04"));
    assert_eq!(
        db.get_meal(overridden.id).await.unwrap().day,
        d("2026-09-13")
    );
    assert_eq!(
        db.get_meal(overridden.id).await.unwrap().timezone,
        "America/New_York"
    );
    // Idempotent.
    assert_eq!(apptime::recompute_days(db, a).await.unwrap(), 0);

    // Bob's entries are his own; a household check is by owner.
    let bobs = meal_at(db, &home.bob, "2026-09-05T10:00:00Z", 500, None).await;
    assert_eq!(
        db.entry_owner(EntryKind::Meal, bobs.id).await.unwrap(),
        home.bob.id
    );
    assert!(matches!(
        db.entry_owner(EntryKind::Meal, 9999).await,
        Err(DbError::NotFound)
    ));
    assert_eq!(
        db.list_meals(a.id, d("2026-09-01"), d("2026-09-30"), false)
            .await
            .unwrap()
            .len(),
        4
    );
    assert_eq!(
        db.list_meals(home.bob.id, d("2026-09-01"), d("2026-09-30"), false)
            .await
            .unwrap()
            .len(),
        1
    );
    t.finish().await;
}

#[tokio::test]
async fn totals_skip_voided_and_take_the_first_weight() {
    let t = db_or_skip!();
    let db = &t.db;
    let home = seed_home(db).await;
    let a = &home.alice;

    let m1 = meal_at(db, a, "2026-09-04T07:00:00Z", 400, Some(20)).await;
    meal_at(db, a, "2026-09-04T11:00:00Z", 700, None).await;
    let m3 = meal_at(db, a, "2026-09-04T17:00:00Z", 300, Some(15)).await;
    db.void_entry(EntryKind::Meal, m3.id).await.unwrap();
    assert!(matches!(
        db.void_entry(EntryKind::Meal, m3.id).await,
        Err(DbError::NotFound)
    ));
    assert!(db.get_meal(m3.id).await.unwrap().voided_at.is_some());

    let totals = db
        .meal_day_totals(a.id, d("2026-09-01"), d("2026-09-30"))
        .await
        .unwrap();
    assert_eq!(
        totals,
        vec![MealDayTotals {
            day: d("2026-09-04"),
            kcal: 1100,
            protein_g: Some(20),
            meals: 2,
            meals_without_protein: 1,
        }]
    );
    assert_eq!(
        db.list_meals(a.id, d("2026-09-04"), d("2026-09-04"), false)
            .await
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        db.list_meals(a.id, d("2026-09-04"), d("2026-09-04"), true)
            .await
            .unwrap()
            .len(),
        3
    );
    db.unvoid_entry(EntryKind::Meal, m3.id).await.unwrap();
    assert_eq!(
        db.meal_day_totals(a.id, d("2026-09-04"), d("2026-09-04"))
            .await
            .unwrap()[0]
            .kcal,
        1400
    );

    // A patch that clears protein and one that leaves it alone.
    let patched = db
        .update_meal(
            m1.id,
            MealPatch {
                kcal: Some(450),
                protein_g: Some(None),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!((patched.kcal, patched.protein_g), (450, None));
    let patched = db
        .update_meal(
            m1.id,
            MealPatch {
                description: Some("eggs".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(
        (patched.description.as_str(), patched.protein_g),
        ("eggs", None)
    );

    // Weights: two readings on one day, the earliest is the day's weight.
    for (when, grams) in [
        ("2026-09-04T09:00:00Z", 82500),
        ("2026-09-04T05:30:00Z", 82100),
        ("2026-09-06T05:30:00Z", 81900),
    ] {
        let r = apptime::resolve(db, a, Some(utc(when)), None, None)
            .await
            .unwrap();
        db.insert_weight(NewWeight {
            user_id: a.id,
            measured_at: r.instant,
            timezone: r.timezone,
            day: r.day,
            day_override: false,
            weight_g: grams,
            created_by: a.id,
            created_via: VIA_MCP.into(),
        })
        .await
        .unwrap();
    }
    let dw = db
        .day_weights(a.id, d("2026-09-01"), d("2026-09-30"))
        .await
        .unwrap();
    assert_eq!(
        dw,
        vec![
            DayWeight {
                day: d("2026-09-04"),
                weight_g: 82100
            },
            DayWeight {
                day: d("2026-09-06"),
                weight_g: 81900
            },
        ]
    );
    assert!(matches!(
        db.insert_weight(NewWeight {
            user_id: a.id,
            measured_at: utc("2026-09-04T09:00:00Z"),
            timezone: "Europe/Warsaw".into(),
            day: d("2026-09-04"),
            day_override: false,
            weight_g: 5000,
            created_by: a.id,
            created_via: VIA_MCP.into(),
        })
        .await,
        Err(DbError::Conflict(_))
    ));

    // Activities: minutes per day; kcal never enters the meal totals.
    let r = apptime::resolve(db, a, Some(utc("2026-09-04T16:00:00Z")), None, None)
        .await
        .unwrap();
    let run = db
        .insert_activity(NewActivity {
            user_id: a.id,
            started_at: r.instant,
            timezone: r.timezone,
            day: r.day,
            day_override: false,
            kind: " Run ".into(),
            minutes: 45,
            kcal: Some(400),
            note: String::new(),
            created_by: a.id,
            created_via: VIA_MCP.into(),
        })
        .await
        .unwrap();
    assert_eq!(run.kind, "run");
    let at = db
        .activity_day_totals(a.id, d("2026-09-01"), d("2026-09-30"))
        .await
        .unwrap();
    assert_eq!(
        at,
        vec![ActivityDayTotals {
            day: d("2026-09-04"),
            minutes: 45,
            activities: 1
        }]
    );
    assert_eq!(db.first_day(a.id).await.unwrap(), Some(d("2026-09-04")));
    assert_eq!(db.first_day(home.bob.id).await.unwrap(), None);
    t.finish().await;
}

#[tokio::test]
async fn foods_are_unique_per_household_until_archived() {
    let t = db_or_skip!();
    let db = &t.db;
    let home = seed_home(db).await;
    let other = db.create_household("other").await.unwrap();

    let curry = db
        .insert_food(NewFood {
            household_id: home.household.id,
            name: "Lentil curry".into(),
            aliases: vec!["dal".into()],
            portion: "1 bowl (350 g)".into(),
            kcal: 520,
            protein_g: Some(24),
            created_by: home.alice.id,
        })
        .await
        .unwrap();
    assert!(matches!(
        db.insert_food(NewFood {
            household_id: home.household.id,
            name: " lentil CURRY ".into(),
            aliases: vec![],
            portion: "1 bowl".into(),
            kcal: 500,
            protein_g: None,
            created_by: home.bob.id,
        })
        .await,
        Err(DbError::Conflict(_))
    ));
    // Same name in another household is fine.
    db.insert_food(NewFood {
        household_id: other.id,
        name: "Lentil curry".into(),
        aliases: vec![],
        portion: "1 bowl".into(),
        kcal: 500,
        protein_g: None,
        created_by: home.alice.id,
    })
    .await
    .unwrap();

    assert_eq!(
        db.find_food(home.household.id, "DAL")
            .await
            .unwrap()
            .unwrap()
            .id,
        curry.id
    );
    assert!(
        db.find_food(home.household.id, "curry")
            .await
            .unwrap()
            .is_none()
    );
    let hits = db
        .search_foods(home.household.id, "curr", false)
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].food.id, curry.id);
    assert_eq!(hits[0].uses, 0);

    // Log a meal against it and see the usage count.
    let r = apptime::resolve(db, &home.alice, None, None, None)
        .await
        .unwrap();
    db.insert_meal(NewMeal {
        user_id: home.alice.id,
        eaten_at: r.instant,
        timezone: r.timezone,
        day: r.day,
        day_override: false,
        description: "Lentil curry".into(),
        kcal: 780,
        protein_g: Some(36),
        source: SOURCE_FOOD.into(),
        food_id: Some(curry.id),
        portions: Some(dec("1.5")),
        created_by: home.alice.id,
        created_via: VIA_MCP.into(),
    })
    .await
    .unwrap();
    let all = db.search_foods(home.household.id, "", false).await.unwrap();
    assert_eq!(all[0].uses, 1);

    // Archive: hidden, name becomes free, unarchive refused while taken.
    db.set_food_archived(curry.id, true).await.unwrap();
    assert!(
        db.find_food(home.household.id, "dal")
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        db.search_foods(home.household.id, "", false)
            .await
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        db.search_foods(home.household.id, "", true)
            .await
            .unwrap()
            .len(),
        1
    );
    let replacement = db
        .insert_food(NewFood {
            household_id: home.household.id,
            name: "Lentil curry".into(),
            aliases: vec![],
            portion: "1 bowl".into(),
            kcal: 480,
            protein_g: None,
            created_by: home.bob.id,
        })
        .await
        .unwrap();
    assert!(matches!(
        db.set_food_archived(curry.id, false).await,
        Err(DbError::Conflict(_))
    ));
    let renamed = db
        .update_food(
            replacement.id,
            FoodPatch {
                name: Some("Dal".into()),
                protein_g: Some(Some(22)),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(
        (renamed.name.as_str(), renamed.protein_g),
        ("Dal", Some(22))
    );
    db.set_food_archived(curry.id, false).await.unwrap();
    t.finish().await;
}

#[tokio::test]
async fn targets_are_effective_dated() {
    let t = db_or_skip!();
    let db = &t.db;
    let home = seed_home(db).await;
    let a = &home.alice;
    assert!(
        db.target_for(a.id, d("2026-09-01"))
            .await
            .unwrap()
            .is_none()
    );
    db.insert_target(NewTarget {
        user_id: a.id,
        valid_from: d("2026-09-01"),
        kcal: 1800,
        protein_g: Some(120),
        weight_g: Some(70000),
    })
    .await
    .unwrap();
    let second = db
        .insert_target(NewTarget {
            user_id: a.id,
            valid_from: d("2026-10-01"),
            kcal: 2000,
            protein_g: None,
            weight_g: None,
        })
        .await
        .unwrap();
    assert!(matches!(
        db.insert_target(NewTarget {
            user_id: a.id,
            valid_from: d("2026-10-01"),
            kcal: 1,
            protein_g: None,
            weight_g: None,
        })
        .await,
        Err(DbError::Conflict(_))
    ));
    assert!(
        db.target_for(a.id, d("2026-08-31"))
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        db.target_for(a.id, d("2026-09-15"))
            .await
            .unwrap()
            .unwrap()
            .kcal,
        1800
    );
    assert_eq!(
        db.target_for(a.id, d("2026-10-01"))
            .await
            .unwrap()
            .unwrap()
            .kcal,
        2000
    );
    assert!(
        db.target_for(home.bob.id, d("2026-10-01"))
            .await
            .unwrap()
            .is_none()
    );
    let patched = db
        .update_target(
            second.id,
            TargetPatch {
                protein_g: Some(Some(130)),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(patched.protein_g, Some(130));
    db.delete_target(second.id).await.unwrap();
    assert_eq!(db.list_targets(a.id).await.unwrap().len(), 1);
    t.finish().await;
}

#[tokio::test]
async fn tokens_are_personal_or_delegate() {
    let t = db_or_skip!();
    let db = &t.db;
    let home = seed_home(db).await;
    let personal = db
        .create_token(
            "claude-code",
            &"a".repeat(64),
            &["read".into(), "write".into()],
            Some(home.alice.id),
            Some(home.alice.id),
        )
        .await
        .unwrap();
    let delegate = db
        .create_token(
            "openwebui",
            &"b".repeat(64),
            &["read".into(), "write".into(), "delegate".into()],
            None,
            None,
        )
        .await
        .unwrap();
    assert!(matches!(
        db.create_token("bad", &"c".repeat(64), &["read".into()], None, None)
            .await,
        Err(DbError::Conflict(_))
    ));
    assert!(matches!(
        db.create_token(
            "bad",
            &"d".repeat(64),
            &["read".into(), "delegate".into()],
            Some(home.bob.id),
            Some(home.alice.id)
        )
        .await,
        Err(DbError::Conflict(_))
    ));
    assert_eq!(
        db.find_active_token(&"a".repeat(64))
            .await
            .unwrap()
            .unwrap()
            .id,
        personal.id
    );
    db.revoke_token(personal.id).await.unwrap();
    assert!(
        db.find_active_token(&"a".repeat(64))
            .await
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        db.revoke_token(personal.id).await,
        Err(DbError::NotFound)
    ));
    assert_eq!(db.list_tokens().await.unwrap().len(), 2);
    assert_eq!(delegate.user_id, None);
    t.finish().await;
}
