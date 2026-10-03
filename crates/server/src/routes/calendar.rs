//! Calendar sources, imports, events, occurrences and exports.

use crate::activity::{record, ActivityRow, NewActivity, SELECT as ACTIVITY_SELECT};
use crate::calendar::{self, load_source, OccQuery, SourceRow, Viewer, EVENT_SELECT};
use crate::db::{new_id, ts};
use crate::error::{AppError, AppResult};
use crate::routes::groups::ensure_member;
use crate::security::Actor;
use crate::state::AppState;
use crate::validate;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue};
use axum::response::IntoResponse;
use axum::Json;
use chrono::{DateTime, Duration, NaiveDate, NaiveTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use tendly_core::api::{
    Activity, CalendarEvent, CalendarEventInput, CalendarEventPatch, CalendarSource, CalendarSourceInput, CalendarSourcePatch, EventOccurrence, ImportResult,
};
use tendly_core::ics::{EventStatus, IcsEvent, IcsTime, Limits};
use tendly_core::model::Category;

async fn source_for_actor(state: &AppState, actor: &Actor, id: &str) -> AppResult<SourceRow> {
    let s = load_source(state, id).await?.ok_or(AppError::NotFound("calendar"))?;
    let visible = s.owner_id.as_deref() == Some(&actor.id)
        || (s.is_private == 0 && s.group_id.is_none())
        || match &s.group_id {
            Some(g) => ensure_member(state, g, actor).await.is_ok(),
            None => false,
        };
    if !visible {
        return Err(AppError::Forbidden("This calendar is private to someone else."));
    }
    Ok(s)
}

pub async fn sources(State(state): State<AppState>, actor: Actor) -> AppResult<Json<Vec<CalendarSource>>> {
    let mut out = Vec::new();
    for s in calendar::visible_sources(&state, &actor).await? {
        out.push(s.to_api(&state).await?);
    }
    Ok(Json(out))
}

pub async fn create_source(State(state): State<AppState>, actor: Actor, Json(i): Json<CalendarSourceInput>) -> AppResult<Json<CalendarSource>> {
    let name = validate::title("name", &i.name, 80)?;
    if let Some(g) = &i.group_id {
        ensure_member(&state, g, &actor).await?;
    }
    let refresh = i.refresh_minutes.unwrap_or(60).clamp(15, 24 * 60);
    let (cipher, display) = match i.kind.as_str() {
        "local" => (None, None),
        "url" => {
            let raw = i.url.as_deref().ok_or_else(|| AppError::field("url", "Paste the calendar's subscription link."))?;
            let url = crate::fetch::validate_url(raw).map_err(|e| AppError::field("url", e.to_string()))?;
            (Some(state.cipher.encrypt(url.as_str())?), Some(tendly_core::redact::url_for_display(url.as_str())))
        }
        _ => return Err(AppError::field("kind", "Choose a local calendar or a subscription link.")),
    };
    let id = new_id();
    let now = ts(state.now());
    let mut tx = state.db.begin().await?;
    sqlx::query("INSERT INTO calendar_sources (id, name, kind, url_ciphertext, url_display, group_id, owner_id, is_private, refresh_minutes, created_at, updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?)")
        .bind(&id)
        .bind(&name)
        .bind(&i.kind)
        .bind(&cipher)
        .bind(&display)
        .bind(&i.group_id)
        .bind(&actor.id)
        .bind(i.is_private.unwrap_or(i.group_id.is_none()) as i64)
        .bind(refresh as i64)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    record(&mut tx, &now, NewActivity {
        actor: Some(&actor), source: "app", entity_type: "calendar_source", entity_id: &id, group_id: i.group_id.as_deref(), op: "create",
        summary: if i.kind == "url" { format!("{} subscribed to “{name}”", actor.name) } else { format!("{} created the calendar “{name}”", actor.name) },
        revision: 1, before: None, after: Some(json!({"kind": i.kind, "host": display})),
    }).await?;
    tx.commit().await?;
    let source = load_source(&state, &id).await?.ok_or(AppError::NotFound("calendar"))?;
    if source.kind == "url" {
        // First fetch right away so the person sees results; failures are recorded on the source.
        let _ = calendar::refresh_url_source(&state, &source).await;
    }
    let source = load_source(&state, &id).await?.ok_or(AppError::NotFound("calendar"))?;
    Ok(Json(source.to_api(&state).await?))
}

pub async fn update_source(State(state): State<AppState>, actor: Actor, Path(id): Path<String>, Json(p): Json<CalendarSourcePatch>) -> AppResult<Json<CalendarSource>> {
    let s = source_for_actor(&state, &actor, &id).await?;
    let mut tx = state.db.begin().await?;
    if let Some(n) = &p.name {
        sqlx::query("UPDATE calendar_sources SET name = ? WHERE id = ?").bind(validate::title("name", n, 80)?).bind(&id).execute(&mut *tx).await?;
    }
    if let Some(g) = &p.group_id {
        if let Some(g) = g {
            ensure_member(&state, g, &actor).await?;
        }
        sqlx::query("UPDATE calendar_sources SET group_id = ? WHERE id = ?").bind(g).bind(&id).execute(&mut *tx).await?;
    }
    if let Some(v) = p.is_private {
        sqlx::query("UPDATE calendar_sources SET is_private = ? WHERE id = ?").bind(v as i64).bind(&id).execute(&mut *tx).await?;
    }
    if let Some(v) = p.enabled {
        sqlx::query("UPDATE calendar_sources SET enabled = ? WHERE id = ?").bind(v as i64).bind(&id).execute(&mut *tx).await?;
    }
    if let Some(v) = p.priority {
        sqlx::query("UPDATE calendar_sources SET priority = ? WHERE id = ?").bind(v.clamp(0, 1000) as i64).bind(&id).execute(&mut *tx).await?;
    }
    if let Some(v) = p.refresh_minutes {
        sqlx::query("UPDATE calendar_sources SET refresh_minutes = ? WHERE id = ?").bind(v.clamp(15, 1440) as i64).bind(&id).execute(&mut *tx).await?;
    }
    let now = ts(state.now());
    record(&mut tx, &now, NewActivity {
        actor: Some(&actor), source: "app", entity_type: "calendar_source", entity_id: &id, group_id: s.group_id.as_deref(), op: "update",
        summary: format!("{} changed settings of “{}”", actor.name, s.name), revision: 0, before: None, after: Some(serde_json::to_value(&p)?),
    }).await?;
    tx.commit().await?;
    let s = load_source(&state, &id).await?.ok_or(AppError::NotFound("calendar"))?;
    Ok(Json(s.to_api(&state).await?))
}

pub async fn delete_source(State(state): State<AppState>, actor: Actor, Path(id): Path<String>) -> AppResult<Json<Value>> {
    let s = source_for_actor(&state, &actor, &id).await?;
    if s.owner_id.as_deref() != Some(&actor.id) && s.group_id.is_none() {
        return Err(AppError::Forbidden("Only the person who added this calendar can remove it."));
    }
    let now = ts(state.now());
    let mut tx = state.db.begin().await?;
    // Deleting a source removes only its own events (ON DELETE CASCADE).
    sqlx::query("DELETE FROM calendar_sources WHERE id = ?").bind(&id).execute(&mut *tx).await?;
    record(&mut tx, &now, NewActivity {
        actor: Some(&actor), source: "app", entity_type: "calendar_source", entity_id: &id, group_id: s.group_id.as_deref(), op: "delete",
        summary: format!("{} removed the calendar “{}” and its events", actor.name, s.name), revision: 0, before: None, after: None,
    }).await?;
    tx.commit().await?;
    Ok(Json(json!({"deleted": true})))
}

pub async fn refresh_source(State(state): State<AppState>, ctx: crate::security::RequestCtx, actor: Actor, Path(id): Path<String>) -> AppResult<Json<ImportResult>> {
    let s = source_for_actor(&state, &actor, &id).await?;
    if s.kind != "url" {
        return Err(AppError::bad("Only subscribed calendars can be refreshed. Re-import the file instead."));
    }
    if !state.limiter.check("refresh", ctx.peer, 30, 60, state.now()) {
        return Err(AppError::RateLimited);
    }
    let stats = calendar::refresh_url_source(&state, &s).await.map_err(|e| AppError::Upstream(e.to_string()))?;
    let now = ts(state.now());
    let mut conn = state.db.acquire().await?;
    record(&mut conn, &now, NewActivity {
        actor: Some(&actor), source: &format!("poll:{}", s.name), entity_type: "calendar_source", entity_id: &id, group_id: s.group_id.as_deref(), op: "poll",
        summary: format!("{} refreshed “{}”: {} new, {} changed, {} removed", actor.name, s.name, stats.inserted, stats.updated, stats.cancelled),
        revision: 0, before: None, after: None,
    }).await?;
    let s = load_source(&state, &id).await?.ok_or(AppError::NotFound("calendar"))?;
    Ok(Json(ImportResult {
        warnings: serde_json::from_str(&s.warnings).unwrap_or_default(),
        source: s.to_api(&state).await?,
        inserted: stats.inserted,
        updated: stats.updated,
        unchanged: stats.unchanged,
        cancelled: stats.cancelled,
    }))
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ImportQuery {
    source_id: Option<String>,
    name: Option<String>,
    group_id: Option<String>,
    is_private: Option<bool>,
}

/// Imports an .ics file (sent as the raw request body). Re-importing into the
/// same source updates events in place instead of duplicating them.
pub async fn import(State(state): State<AppState>, actor: Actor, Query(q): Query<ImportQuery>, body: Bytes) -> AppResult<Json<ImportResult>> {
    if body.len() > Limits::default().max_bytes {
        return Err(AppError::TooLarge);
    }
    let text = String::from_utf8(body.to_vec()).map_err(|_| AppError::bad("The file is not valid UTF-8 text."))?;
    let parsed = tendly_core::ics::parse(&text, &Limits::default()).map_err(|e| AppError::bad(e.to_string()))?;
    let source = match &q.source_id {
        Some(id) => {
            let s = source_for_actor(&state, &actor, id).await?;
            if s.kind != "file" {
                return Err(AppError::bad("Files can only be re-imported into an imported calendar."));
            }
            s
        }
        None => {
            if let Some(g) = &q.group_id {
                ensure_member(&state, g, &actor).await?;
            }
            let name = q.name.clone().or(parsed.name.clone()).unwrap_or_else(|| "Imported calendar".into());
            let name = validate::title("name", &name, 80)?;
            let id = new_id();
            let now = ts(state.now());
            sqlx::query("INSERT INTO calendar_sources (id, name, kind, group_id, owner_id, is_private, last_status, last_fetched_at, created_at, updated_at) VALUES (?,?,'file',?,?,?,'ok',?,?,?)")
                .bind(&id)
                .bind(&name)
                .bind(&q.group_id)
                .bind(&actor.id)
                .bind(q.is_private.unwrap_or(q.group_id.is_none()) as i64)
                .bind(&now)
                .bind(&now)
                .bind(&now)
                .execute(&state.db)
                .await?;
            load_source(&state, &id).await?.ok_or(AppError::NotFound("calendar"))?
        }
    };
    let stats = calendar::apply_feed(&state, &source, &parsed, Some(&actor), "import", q.source_id.is_some()).await?;
    let now = ts(state.now());
    sqlx::query("UPDATE calendar_sources SET last_fetched_at = ?, last_status = 'ok' WHERE id = ?").bind(&now).bind(&source.id).execute(&state.db).await?;
    let mut conn = state.db.acquire().await?;
    record(&mut conn, &now, NewActivity {
        actor: Some(&actor), source: "import:file", entity_type: "calendar_source", entity_id: &source.id, group_id: source.group_id.as_deref(), op: "import",
        summary: format!("{} imported a file into “{}”: {} new, {} changed, {} removed", actor.name, source.name, stats.inserted, stats.updated, stats.cancelled),
        revision: 0, before: None, after: None,
    }).await?;
    let s = load_source(&state, &source.id).await?.ok_or(AppError::NotFound("calendar"))?;
    Ok(Json(ImportResult {
        warnings: parsed.warnings,
        source: s.to_api(&state).await?,
        inserted: stats.inserted,
        updated: stats.updated,
        unchanged: stats.unchanged,
        cancelled: stats.cancelled,
    }))
}

#[derive(Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct RangeQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    pub group_id: Option<String>,
    pub member_id: Option<String>,
    pub category: Option<String>,
    pub source_id: Option<String>,
    pub include_tasks: Option<bool>,
    pub include_events: Option<bool>,
    pub name: Option<String>,
}

fn parse_bound(s: Option<&str>, default: DateTime<Utc>) -> AppResult<DateTime<Utc>> {
    match s {
        None => Ok(default),
        Some(v) => {
            if let Ok(d) = DateTime::parse_from_rfc3339(v) {
                return Ok(d.with_timezone(&Utc));
            }
            NaiveDate::parse_from_str(v, "%Y-%m-%d")
                .map(|d| d.and_time(NaiveTime::MIN).and_utc())
                .map_err(|_| AppError::field("from", "Use a date like 2026-10-01."))
        }
    }
}

pub fn build_query(state: &AppState, q: &RangeQuery) -> AppResult<OccQuery> {
    let now = state.now();
    let from = parse_bound(q.from.as_deref(), now - Duration::days(7))?;
    let to = parse_bound(q.to.as_deref(), now + Duration::days(60))?;
    if to <= from || to - from > Duration::days(800) {
        return Err(AppError::field("to", "Choose a range of up to about two years."));
    }
    let split = |s: &Option<String>| -> Vec<String> {
        s.as_deref().map(|v| v.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).take(50).collect()).unwrap_or_default()
    };
    let mut categories = Vec::new();
    for c in split(&q.category) {
        categories.push(Category::parse(&c).ok_or_else(|| AppError::field("category", "Unknown category."))?);
    }
    Ok(OccQuery {
        from,
        to,
        group_ids: split(&q.group_id),
        member_id: q.member_id.clone(),
        categories,
        source_ids: split(&q.source_id),
        include_tasks: q.include_tasks.unwrap_or(true),
        include_events: q.include_events.unwrap_or(true),
    })
}

pub async fn occurrences(State(state): State<AppState>, actor: Actor, Query(q): Query<RangeQuery>) -> AppResult<Json<Vec<EventOccurrence>>> {
    let oq = build_query(&state, &q)?;
    for g in &oq.group_ids {
        ensure_member(&state, g, &actor).await?;
    }
    Ok(Json(calendar::occurrences(&state, Viewer::Actor(&actor), &oq).await?))
}

pub async fn export_ics(State(state): State<AppState>, actor: Actor, Query(q): Query<RangeQuery>) -> AppResult<impl IntoResponse> {
    let oq = build_query(&state, &q)?;
    for g in &oq.group_ids {
        ensure_member(&state, g, &actor).await?;
    }
    let name = q.name.clone().unwrap_or_else(|| "Tendly".into());
    let body = calendar::export_ics(&state, Viewer::Actor(&actor), &oq, &tendly_core::model::clean_text(&name, 80, false)).await?;
    let mut h = HeaderMap::new();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/calendar; charset=utf-8"));
    h.insert(header::CONTENT_DISPOSITION, HeaderValue::from_static("attachment; filename=\"tendly.ics\""));
    Ok((h, body))
}

pub async fn export_json(State(state): State<AppState>, actor: Actor, Query(q): Query<RangeQuery>) -> AppResult<impl IntoResponse> {
    let oq = build_query(&state, &q)?;
    for g in &oq.group_ids {
        ensure_member(&state, g, &actor).await?;
    }
    let occ = calendar::occurrences(&state, Viewer::Actor(&actor), &oq).await?;
    let mut h = HeaderMap::new();
    h.insert(header::CONTENT_DISPOSITION, HeaderValue::from_static("attachment; filename=\"tendly-calendar.json\""));
    Ok((h, Json(json!({"generatedAt": state.now(), "from": oq.from, "to": oq.to, "items": occ}))))
}

async fn load_event(state: &AppState, actor: &Actor, id: &str) -> AppResult<(calendar::EventRow, SourceRow)> {
    let r = sqlx::query_as::<_, calendar::EventRow>(&format!("{EVENT_SELECT} WHERE id = ?"))
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound("event"))?;
    let s = source_for_actor(state, actor, &r.source_id).await?;
    Ok((r, s))
}

pub async fn get_event(State(state): State<AppState>, actor: Actor, Path(id): Path<String>) -> AppResult<Json<CalendarEvent>> {
    let (r, s) = load_event(&state, &actor, &id).await?;
    Ok(Json(r.to_api(&s, &state.config.default_timezone)))
}

fn input_to_ics(i: &CalendarEventInput, tz_default: &str, uid: &str, sequence: i64) -> AppResult<IcsEvent> {
    let title = validate::title("title", &i.title, 200)?;
    let tz = validate::timezone(i.timezone.as_deref(), tz_default)?;
    let sd = validate::date("startDate", Some(&i.start_date))?.ok_or_else(|| AppError::field("startDate", "Pick a start date."))?;
    let sd = NaiveDate::parse_from_str(&sd, "%Y-%m-%d").map_err(|_| AppError::field("startDate", "Invalid date."))?;
    let ed = match validate::date("endDate", i.end_date.as_deref())? {
        Some(d) => NaiveDate::parse_from_str(&d, "%Y-%m-%d").map_err(|_| AppError::field("endDate", "Invalid date."))?,
        None => sd,
    };
    if ed < sd {
        return Err(AppError::field("endDate", "The event ends before it starts."));
    }
    let (start, end) = if i.all_day {
        (IcsTime::Date { date: sd }, IcsTime::Date { date: ed + Duration::days(1) })
    } else {
        let st = validate::time("startTime", i.start_time.as_deref())?.ok_or_else(|| AppError::field("startTime", "Pick a start time or make it all-day."))?;
        let st = NaiveTime::parse_from_str(&st, "%H:%M").expect("validated");
        let et = match validate::time("endTime", i.end_time.as_deref())? {
            Some(t) => NaiveTime::parse_from_str(&t, "%H:%M").expect("validated"),
            None => st + Duration::hours(1),
        };
        let s = sd.and_time(st);
        let mut e = ed.and_time(et);
        if e <= s {
            if i.end_date.is_none() {
                e = s + Duration::hours(1);
            } else {
                return Err(AppError::field("endTime", "The event ends before it starts."));
            }
        }
        (IcsTime::Zoned { local: s, tzid: tz.clone() }, IcsTime::Zoned { local: e, tzid: tz })
    };
    Ok(IcsEvent {
        uid: uid.to_string(),
        uid_generated: false,
        summary: title,
        description: validate::opt_text(i.description.as_deref(), 10_000, true),
        location: validate::opt_text(i.location.as_deref(), 500, false),
        start,
        end: Some(end),
        duration_secs: None,
        rrule: validate::recurrence(i.recurrence.as_deref())?,
        exdates: vec![],
        rdates: vec![],
        recurrence_id: None,
        status: EventStatus::Confirmed,
        sequence,
        dtstamp: None,
        last_modified: None,
        categories: i.category.map(|c| vec![c.as_str().to_string()]).unwrap_or_default(),
    })
}

pub async fn create_event(State(state): State<AppState>, actor: Actor, Json(i): Json<CalendarEventInput>) -> AppResult<Json<CalendarEvent>> {
    let s = source_for_actor(&state, &actor, &i.source_id).await?;
    if s.kind != "local" {
        return Err(AppError::bad("New events go into one of your own calendars, not a subscription."));
    }
    if let Some(g) = &i.group_id {
        ensure_member(&state, g, &actor).await?;
    }
    let id = new_id();
    let uid = format!("{id}@tendly");
    let e = input_to_ics(&i, &state.config.default_timezone, &uid, 0)?;
    let now = ts(state.now());
    let tz = s.tz(&state.config.default_timezone);
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "INSERT INTO calendar_events (id, source_id, uid, instance_key, title, description, location, start_json, end_json, rrule, status, sequence, last_modified, dtstamp, categories, content_hash, category, group_id, revision, start_utc, created_at, updated_at) VALUES (?,?,?,'',?,?,?,?,?,?,'confirmed',0,?,?,?,?,?,?,1,?,?,?)",
    )
    .bind(&id)
    .bind(&s.id)
    .bind(&uid)
    .bind(&e.summary)
    .bind(&e.description)
    .bind(&e.location)
    .bind(serde_json::to_string(&e.start)?)
    .bind(serde_json::to_string(&e.effective_end())?)
    .bind(&e.rrule)
    .bind(&now)
    .bind(&now)
    .bind(serde_json::to_string(&e.categories)?)
    .bind(e.content_hash())
    .bind(i.category.map(|c| c.as_str()))
    .bind(&i.group_id)
    .bind(ts(e.start.to_utc(tz)))
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    record(&mut tx, &now, NewActivity {
        actor: Some(&actor), source: "app", entity_type: "calendar_event", entity_id: &id, group_id: i.group_id.as_deref().or(s.group_id.as_deref()),
        op: "create", summary: format!("{} added “{}” to {}", actor.name, e.summary, s.name), revision: 1, before: None,
        after: Some(json!({"start": e.start, "title": e.summary})),
    }).await?;
    tx.commit().await?;
    let (r, s) = load_event(&state, &actor, &id).await?;
    Ok(Json(r.to_api(&s, &state.config.default_timezone)))
}

/// Local events are fully editable. Events from subscriptions or imports only
/// accept local overrides (category, note, hidden): Tendly does not write back
/// to external providers.
pub async fn update_event(State(state): State<AppState>, actor: Actor, Path(id): Path<String>, Json(p): Json<CalendarEventPatch>) -> AppResult<Json<CalendarEvent>> {
    let (r, s) = load_event(&state, &actor, &id).await?;
    if r.revision as u32 != p.expected_revision {
        return Err(AppError::Conflict {
            message: "This event changed in the meantime (possibly from a feed refresh). Review and try again.".into(),
            current: Some(serde_json::to_value(r.to_api(&s, &state.config.default_timezone))?),
        });
    }
    let now = ts(state.now());
    let mut tx = state.db.begin().await?;
    let mut summary = String::new();
    let before = json!({"title": r.title, "start": r.start(), "status": r.status, "override": r.override_()});
    if let Some(input) = &p.event {
        if s.kind != "local" {
            return Err(AppError::bad("This event comes from a calendar Tendly reads but cannot edit. You can add a note, change its category or hide it here."));
        }
        let e = input_to_ics(input, &state.config.default_timezone, &r.uid, r.sequence + 1)?;
        let tz = s.tz(&state.config.default_timezone);
        sqlx::query("UPDATE calendar_events SET title=?, description=?, location=?, start_json=?, end_json=?, rrule=?, sequence=sequence+1, last_modified=?, categories=?, content_hash=?, category=?, group_id=?, start_utc=?, revision=revision+1, updated_at=? WHERE id=?")
            .bind(&e.summary)
            .bind(&e.description)
            .bind(&e.location)
            .bind(serde_json::to_string(&e.start)?)
            .bind(serde_json::to_string(&e.effective_end())?)
            .bind(&e.rrule)
            .bind(&now)
            .bind(serde_json::to_string(&e.categories)?)
            .bind(e.content_hash())
            .bind(input.category.map(|c| c.as_str()))
            .bind(&input.group_id)
            .bind(ts(e.start.to_utc(tz)))
            .bind(&now)
            .bind(&id)
            .execute(&mut *tx)
            .await?;
        summary = format!("{} edited “{}”", actor.name, e.summary);
    }
    if let Some(ov) = &p.local_override {
        let cleaned = tendly_core::api::LocalOverride {
            category: ov.category,
            hidden: ov.hidden,
            note: validate::opt_text(ov.note.as_deref(), 1000, true),
        };
        let stored = if cleaned == Default::default() { None } else { Some(serde_json::to_string(&cleaned)?) };
        sqlx::query("UPDATE calendar_events SET local_override = ?, revision = revision + 1, updated_at = ? WHERE id = ?")
            .bind(stored)
            .bind(&now)
            .bind(&id)
            .execute(&mut *tx)
            .await?;
        if summary.is_empty() {
            summary = format!("{} adjusted “{}” locally", actor.name, r.title);
        }
    }
    if p.cancel == Some(true) {
        if s.kind != "local" {
            return Err(AppError::bad("Hide events from subscriptions instead; Tendly cannot cancel them at the source."));
        }
        sqlx::query("UPDATE calendar_events SET status = 'cancelled', sequence = sequence + 1, revision = revision + 1, last_modified = ?, updated_at = ? WHERE id = ?")
            .bind(&now)
            .bind(&now)
            .bind(&id)
            .execute(&mut *tx)
            .await?;
        summary = format!("{} cancelled “{}”", actor.name, r.title);
    }
    if summary.is_empty() {
        return Err(AppError::bad("Nothing to change."));
    }
    let after_row = sqlx::query_as::<_, calendar::EventRow>(&format!("{EVENT_SELECT} WHERE id = ?")).bind(&id).fetch_one(&mut *tx).await?;
    record(&mut tx, &now, NewActivity {
        actor: Some(&actor), source: "app", entity_type: "calendar_event", entity_id: &id, group_id: after_row.group_id.as_deref().or(s.group_id.as_deref()),
        op: if p.cancel == Some(true) { "cancel" } else if p.event.is_some() { "update" } else { "override" },
        summary, revision: after_row.revision as u32, before: Some(before),
        after: Some(json!({"title": after_row.title, "start": after_row.start(), "status": after_row.status, "override": after_row.override_()})),
    }).await?;
    tx.commit().await?;
    Ok(Json(after_row.to_api(&s, &state.config.default_timezone)))
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ChangesQuery {
    source_id: Option<String>,
    limit: Option<u32>,
}

/// Calendar change history: who (or "external source / unknown actor"), when, what.
pub async fn changes(State(state): State<AppState>, actor: Actor, Query(q): Query<ChangesQuery>) -> AppResult<Json<Vec<Activity>>> {
    let limit = q.limit.unwrap_or(100).clamp(1, 500);
    let visible: Vec<String> = calendar::visible_sources(&state, &actor).await?.into_iter().map(|s| s.id).collect();
    let rows = sqlx::query_as::<_, ActivityRow>(&format!(
        "{ACTIVITY_SELECT} WHERE entity_type IN ('calendar_source','calendar_event','share') ORDER BY at DESC LIMIT 2000"
    ))
    .fetch_all(&state.db)
    .await?;
    let mut out = Vec::new();
    for r in rows {
        let a: Activity = r.into();
        let source_of_event = if a.entity_type == "calendar_event" {
            sqlx::query_scalar::<_, String>("SELECT source_id FROM calendar_events WHERE id = ?").bind(&a.entity_id).fetch_optional(&state.db).await?
        } else {
            None
        };
        let relevant_source = if a.entity_type == "calendar_source" { Some(a.entity_id.clone()) } else { source_of_event };
        let ok = match (&relevant_source, a.entity_type.as_str()) {
            (_, "share") => a.actor_id.as_deref() == Some(&actor.id),
            (Some(src), _) => visible.contains(src) && q.source_id.as_ref().map(|f| f == src).unwrap_or(true),
            (None, _) => a.actor_id.as_deref() == Some(&actor.id) && q.source_id.is_none(),
        };
        if ok {
            out.push(a);
            if out.len() >= limit as usize {
                break;
            }
        }
    }
    Ok(Json(out))
}
