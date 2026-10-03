//! Synthetic demo data. Every name, event and message here is invented.

use crate::db::{set_setting, ts};
use crate::security::Actor;
use crate::state::AppState;
use anyhow::Result;
use chrono::{Datelike, Duration};
use tendly_core::api::{GroupInput, MemberInput, TaskInput, UseTemplateInput};
use tendly_core::model::{Category, GroupKind, GroupMode, Priority};

pub async fn seed_demo(state: &AppState) -> Result<bool> {
    let existing: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM members").fetch_one(&state.db).await?;
    if existing > 0 {
        return Ok(false);
    }
    let tz = state.config.default_timezone.clone();
    let mk = |name: &str| MemberInput { display_name: name.into(), timezone: Some(tz.clone()) };
    let ctx = crate::security::RequestCtx { peer: [127, 0, 0, 1].into(), trusted_local: true, device_id: None, actor: None, admin: true };
    let alex = crate::routes::members::create(axum::extract::State(state.clone()), ctx.clone(), axum::Json(mk("Alex")))
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .0;
    let sam = crate::routes::members::create(axum::extract::State(state.clone()), ctx.clone(), axum::Json(mk("Sam")))
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .0;
    let robin = crate::routes::members::create(axum::extract::State(state.clone()), ctx, axum::Json(mk("Robin")))
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .0;
    let a = Actor { id: alex.id.clone(), name: alex.display_name.clone() };
    let home = crate::routes::groups::create(
        axum::extract::State(state.clone()),
        a.clone(),
        axum::Json(GroupInput {
            name: "Home".into(),
            kind: GroupKind::Family,
            mode: GroupMode::Household,
            description: Some("Everyday upkeep for our place.".into()),
            goal: None,
            start_date: None,
            end_date: None,
            member_ids: Some(vec![alex.id.clone(), sam.id.clone(), robin.id.clone()]),
        }),
    )
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))?
    .0;
    let today = state.now().with_timezone(&tz.parse::<chrono_tz::Tz>().unwrap_or(chrono_tz::Tz::UTC)).date_naive();
    let d = |n: i64| (today + Duration::days(n)).format("%Y-%m-%d").to_string();
    let rot = vec![alex.id.clone(), sam.id.clone(), robin.id.clone()];
    for (key, due, rotation) in [
        ("dishes", 0, true),
        ("waste", 1, true),
        ("laundry", 2, false),
        ("bathroom", -1, true),
        ("groceries", 3, false),
        ("bed_linen", 5, false),
    ] {
        let _ = crate::routes::misc::use_template(
            axum::extract::State(state.clone()),
            a.clone(),
            axum::extract::Path(key.to_string()),
            axum::Json(UseTemplateInput {
                group_id: Some(home.id.clone()),
                assignee_id: if rotation { None } else { Some(sam.id.clone()) },
                rotation: rotation.then(|| rot.clone()),
                due_date: Some(d(due)),
                timezone: Some(tz.clone()),
                title: None,
                recurrence: None,
            }),
        )
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    }
    let garden = crate::routes::groups::create(
        axum::extract::State(state.clone()),
        a.clone(),
        axum::Json(GroupInput {
            name: "Balcony garden".into(),
            kind: GroupKind::ProjectTeam,
            mode: GroupMode::Project,
            description: Some("Turn the balcony into a small herb garden.".into()),
            goal: Some("Herbs growing by spring".into()),
            start_date: Some(d(-7)),
            end_date: Some(d(60)),
            member_ids: Some(vec![alex.id.clone(), sam.id.clone()]),
        }),
    )
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))?
    .0;
    for (title, col, due, who, pr) in [
        ("Measure the balcony", "done", Some(-3), &alex.id, Priority::Normal),
        ("Pick planters", "in_progress", Some(4), &sam.id, Priority::High),
        ("Order soil and seeds", "planned", Some(10), &alex.id, Priority::Normal),
        ("Ask landlord about hooks", "blocked", Some(6), &sam.id, Priority::Normal),
        ("Build a watering schedule", "backlog", None, &alex.id, Priority::Low),
    ] {
        crate::routes::tasks::create_task(
            state,
            &a,
            TaskInput {
                title: title.into(),
                group_id: Some(garden.id.clone()),
                category: Some(Category::Home),
                priority: Some(pr),
                assignee_id: Some(who.clone()),
                due_date: due.map(d),
                column_key: Some(col.into()),
                timezone: Some(tz.clone()),
                ..TaskInput::default()
            },
            "demo",
        )
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    }
    for (title, cat, due, time, mins) in [
        ("Renew library card", Category::Errands, Some(1), None, Some(10)),
        ("Call grandma", Category::People, Some(0), Some("18:00"), Some(20)),
        ("Read chapter 4 for class", Category::School, Some(2), None, Some(45)),
        ("Send invoice to client", Category::Work, Some(0), Some("11:00"), Some(15)),
        ("Book a haircut", Category::Personal, None, None, Some(5)),
    ] {
        crate::routes::tasks::create_task(
            state,
            &a,
            TaskInput {
                title: title.into(),
                category: Some(cat),
                due_date: due.map(d),
                due_time: time.map(String::from),
                duration_minutes: mins,
                timezone: Some(tz.clone()),
                subtasks: (title == "Read chapter 4 for class")
                    .then(|| vec!["Skim headings".into(), "Read".into(), "Write 3 notes".into()]),
                ..TaskInput::default()
            },
            "demo",
        )
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    }
    // A local family calendar with a couple of synthetic events.
    let year = today.year();
    let ics = format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nX-WR-CALNAME:Family calendar\r\nX-WR-TIMEZONE:{tz}\r\nBEGIN:VEVENT\r\nUID:demo-swim@tendly.invalid\r\nDTSTART;TZID={tz}:{}T170000\r\nDTEND;TZID={tz}:{}T180000\r\nRRULE:FREQ=WEEKLY;COUNT=10\r\nSUMMARY:Swimming lessons\r\nLOCATION:Community pool\r\nCATEGORIES:People\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:demo-dentist@tendly.invalid\r\nDTSTART;TZID={tz}:{}T093000\r\nDTEND;TZID={tz}:{}T100000\r\nSUMMARY:Dentist check-up\r\nCATEGORIES:Personal\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:demo-trip@tendly.invalid\r\nDTSTART;VALUE=DATE:{}\r\nDTEND;VALUE=DATE:{}\r\nSUMMARY:Weekend trip\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
        (today + Duration::days(1)).format("%Y%m%d"),
        (today + Duration::days(1)).format("%Y%m%d"),
        (today + Duration::days(4)).format("%Y%m%d"),
        (today + Duration::days(4)).format("%Y%m%d"),
        (today + Duration::days(9)).format("%Y%m%d"),
        (today + Duration::days(11)).format("%Y%m%d"),
    );
    let _ = year;
    let parsed = tendly_core::ics::parse(&ics, &tendly_core::ics::Limits::default())?;
    let now = ts(state.now());
    let sid = crate::db::new_id();
    sqlx::query("INSERT INTO calendar_sources (id, name, kind, group_id, owner_id, is_private, last_status, created_at, updated_at) VALUES (?,?,'file',?,?,0,'ok',?,?)")
        .bind(&sid)
        .bind("Family calendar")
        .bind(&home.id)
        .bind(&alex.id)
        .bind(&now)
        .bind(&now)
        .execute(&state.db)
        .await?;
    let src = crate::calendar::load_source(state, &sid).await?.expect("just inserted");
    crate::calendar::apply_feed(state, &src, &parsed, Some(&a), "import", false).await?;
    let lid = crate::db::new_id();
    sqlx::query("INSERT INTO calendar_sources (id, name, kind, owner_id, is_private, last_status, created_at, updated_at) VALUES (?,?,'local',?,1,'ok',?,?)")
        .bind(&lid)
        .bind("Alex's calendar")
        .bind(&alex.id)
        .bind(&now)
        .bind(&now)
        .execute(&state.db)
        .await?;
    let _ = crate::routes::focus::create_countdown(
        axum::extract::State(state.clone()),
        a.clone(),
        axum::Json(tendly_core::api::CountdownInput {
            title: "Weekend trip".into(),
            date: d(9),
            time: None,
            timezone: Some(tz.clone()),
            category: Some(Category::People),
            group_id: Some(home.id.clone()),
        }),
    )
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    set_setting(&state.db, "demo_seeded", "true").await?;
    Ok(true)
}
