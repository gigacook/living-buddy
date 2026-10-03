//! Review inbox: suggestions extracted from connected mailboxes or pasted
//! text. Nothing becomes a task or event until a person accepts it here.

use crate::calendar::load_source;
use crate::db::{get_setting, new_id, parse_ts, parse_ts_opt, ts};
use crate::error::{AppError, AppResult};
use crate::routes::tasks::create_task;
use crate::security::{Actor, RequestCtx};
use crate::state::AppState;
use crate::validate;
use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use tendly_core::api::{AcceptSuggestionInput, CalendarEventInput, ConnectorSummary, IntakeInput, IntakeResult, Suggestion, TaskInput};
use tendly_core::extraction::{heuristic_extract, minimize, SuggestionDraft, SuggestionKind};

#[derive(sqlx::FromRow)]
struct SuggestionRow {
    id: String,
    connector_id: Option<String>,
    connector_name: String,
    subject: String,
    received_at: String,
    draft: String,
    extractor: String,
    status: String,
    created_at: String,
    decided_at: Option<String>,
    decided_by: Option<String>,
    result_type: Option<String>,
    result_id: Option<String>,
}

const SELECT: &str = "SELECT id, connector_id, connector_name, subject, received_at, draft, extractor, status, created_at, decided_at, decided_by, result_type, result_id FROM suggestions";

impl TryFrom<SuggestionRow> for Suggestion {
    type Error = serde_json::Error;
    fn try_from(r: SuggestionRow) -> Result<Self, Self::Error> {
        Ok(Suggestion {
            draft: serde_json::from_str(&r.draft)?,
            id: r.id,
            connector_id: r.connector_id,
            connector_name: r.connector_name,
            subject: r.subject,
            received_at: parse_ts(&r.received_at),
            extractor: r.extractor,
            status: r.status,
            created_at: parse_ts(&r.created_at),
            decided_at: parse_ts_opt(r.decided_at),
            decided_by: r.decided_by,
            result_type: r.result_type,
            result_id: r.result_id,
        })
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct ListQuery {
    status: Option<String>,
}

pub async fn list(State(state): State<AppState>, actor: Actor, Query(q): Query<ListQuery>) -> AppResult<Json<Vec<Suggestion>>> {
    let status = q.status.unwrap_or_else(|| "pending".into());
    if !["pending", "accepted", "dismissed", "all"].contains(&status.as_str()) {
        return Err(AppError::field("status", "Unknown status."));
    }
    let rows = sqlx::query_as::<_, SuggestionRow>(&format!(
        "{SELECT} WHERE member_id = ? AND (? = 'all' OR status = ?) ORDER BY created_at DESC LIMIT 200"
    ))
    .bind(&actor.id)
    .bind(&status)
    .bind(&status)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows.into_iter().filter_map(|r| Suggestion::try_from(r).ok()).collect()))
}

async fn load_for(state: &AppState, actor: &Actor, id: &str) -> AppResult<Suggestion> {
    let row = sqlx::query_as::<_, SuggestionRow>(&format!("{SELECT} WHERE id = ? AND member_id = ?"))
        .bind(id)
        .bind(&actor.id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound("suggestion"))?;
    Suggestion::try_from(row).map_err(|e| AppError::Internal(e.into()))
}

/// Paste an email or message to get suggestions. Uses local rules unless the
/// administrator allowed AI for pasted text and a provider is configured.
pub async fn intake(
    State(state): State<AppState>,
    ctx: RequestCtx,
    actor: Actor,
    Json(i): Json<IntakeInput>,
) -> AppResult<Json<IntakeResult>> {
    if !state.limiter.check("intake", ctx.peer, 60, 3600, state.now()) {
        return Err(AppError::RateLimited);
    }
    if i.text.trim().is_empty() {
        return Err(AppError::field("text", "Paste some text first."));
    }
    if i.text.len() > 50_000 {
        return Err(AppError::field("text", "That's a lot of text. Paste just the relevant part (up to about 50,000 characters)."));
    }
    let max_chars: usize = get_setting(&state.db, "ai_max_excerpt_chars").await?.and_then(|v| v.parse().ok()).unwrap_or(2000);
    let msg = minimize(i.subject.as_deref().unwrap_or(""), &i.text, state.now(), max_chars);
    let mut notice = None;
    let provider = crate::ai::current_provider(&state).await?;
    let ai_allowed = get_setting(&state.db, "ai_allow_paste").await?.as_deref() == Some("true");
    let (drafts, extractor): (Vec<SuggestionDraft>, String) = if i.use_ai == Some(true) {
        if !ai_allowed || !provider.is_configured() {
            notice = Some("AI extraction is not enabled on this server, so local rules were used.".to_string());
            (heuristic_extract(&msg), "rules".into())
        } else {
            match crate::ai::extract(&state, &provider, &msg).await {
                Ok(d) => (d, provider.label()),
                Err(e) => {
                    notice = Some(format!(
                        "The AI provider was not available ({}); local rules were used instead.",
                        tendly_core::redact::redact(&e.to_string())
                    ));
                    (heuristic_extract(&msg), "rules (AI unavailable)".into())
                }
            }
        }
    } else {
        (heuristic_extract(&msg), "rules".into())
    };
    if drafts.is_empty() && notice.is_none() {
        notice = Some("Nothing to track was found. You can still add a task by hand.".into());
    }
    let now = state.now();
    let purge = ts(now + chrono::Duration::days(30));
    let mut out = Vec::new();
    for d in drafts {
        let id = new_id();
        sqlx::query("INSERT INTO suggestions (id, item_id, connector_id, connector_name, member_id, subject, received_at, draft, extractor, status, created_at, purge_after) VALUES (?,NULL,NULL,'Pasted text',?,?,?,?,?,'pending',?,?)")
            .bind(&id)
            .bind(&actor.id)
            .bind(if msg.subject.is_empty() { "(no subject)".to_string() } else { msg.subject.clone() })
            .bind(ts(now))
            .bind(serde_json::to_string(&d)?)
            .bind(&extractor)
            .bind(ts(now))
            .bind(&purge)
            .execute(&state.db)
            .await?;
        out.push(load_for(&state, &actor, &id).await?);
    }
    Ok(Json(IntakeResult { suggestions: out, extractor, notice }))
}

async fn default_local_source(state: &AppState, actor: &Actor) -> AppResult<String> {
    let existing: Option<String> =
        sqlx::query_scalar("SELECT id FROM calendar_sources WHERE kind = 'local' AND owner_id = ? ORDER BY created_at LIMIT 1")
            .bind(&actor.id)
            .fetch_optional(&state.db)
            .await?;
    if let Some(id) = existing {
        return Ok(id);
    }
    let id = new_id();
    let now = ts(state.now());
    sqlx::query("INSERT INTO calendar_sources (id, name, kind, owner_id, is_private, last_status, created_at, updated_at) VALUES (?,?,'local',?,1,'ok',?,?)")
        .bind(&id)
        .bind(format!("{}'s calendar", actor.name))
        .bind(&actor.id)
        .bind(&now)
        .bind(&now)
        .execute(&state.db)
        .await?;
    Ok(id)
}

/// The explicit confirmation step: creates exactly one task or event from
/// the (possibly edited) suggestion. No other side effects.
pub async fn accept(
    State(state): State<AppState>,
    actor: Actor,
    Path(id): Path<String>,
    Json(i): Json<AcceptSuggestionInput>,
) -> AppResult<Json<Suggestion>> {
    let s = load_for(&state, &actor, &id).await?;
    if s.status != "pending" {
        return Err(AppError::bad("This suggestion was already handled."));
    }
    let (result_type, result_id) = match i.create_as.as_str() {
        "task" => {
            let notes = validate::opt_text(i.notes.as_deref(), 4000, true)
                .or_else(|| s.draft.evidence.clone().map(|e| format!("From “{}”: {e}", s.subject)));
            let t = create_task(
                &state,
                &actor,
                TaskInput {
                    title: i.title.clone(),
                    group_id: i.group_id.clone(),
                    notes,
                    category: i.category.or(s.draft.category),
                    assignee_id: i.assignee_id.clone(),
                    due_date: i.date.clone(),
                    due_time: i.time.clone(),
                    timezone: i.timezone.clone(),
                    tags: Some(vec!["inbox".into()]),
                    ..TaskInput::default()
                },
                "inbox",
            )
            .await?;
            ("task", t.id)
        }
        "event" => {
            let date = i.date.clone().ok_or_else(|| AppError::field("date", "Events need a date."))?;
            let source_id = match &i.source_id {
                Some(sid) => {
                    let src = load_source(&state, sid).await?.ok_or(AppError::NotFound("calendar"))?;
                    if src.kind != "local" {
                        return Err(AppError::field("sourceId", "Pick one of your own calendars."));
                    }
                    sid.clone()
                }
                None => default_local_source(&state, &actor).await?,
            };
            let ev = crate::routes::calendar::create_event(
                State(state.clone()),
                actor.clone(),
                Json(CalendarEventInput {
                    source_id,
                    title: i.title.clone(),
                    description: validate::opt_text(i.notes.as_deref(), 4000, true),
                    location: s.draft.location.clone(),
                    all_day: i.time.is_none(),
                    start_date: date,
                    start_time: i.time.clone(),
                    end_date: None,
                    end_time: None,
                    timezone: i.timezone.clone(),
                    recurrence: None,
                    category: i.category.or(s.draft.category),
                    group_id: i.group_id.clone(),
                }),
            )
            .await?;
            ("event", ev.0.id)
        }
        _ => return Err(AppError::field("createAs", "Choose task or event.")),
    };
    sqlx::query("UPDATE suggestions SET status = 'accepted', decided_at = ?, decided_by = ?, result_type = ?, result_id = ? WHERE id = ?")
        .bind(ts(state.now()))
        .bind(&actor.id)
        .bind(result_type)
        .bind(&result_id)
        .bind(&id)
        .execute(&state.db)
        .await?;
    Ok(Json(load_for(&state, &actor, &id).await?))
}

pub async fn dismiss(State(state): State<AppState>, actor: Actor, Path(id): Path<String>) -> AppResult<Json<Suggestion>> {
    load_for(&state, &actor, &id).await?;
    sqlx::query("UPDATE suggestions SET status = 'dismissed', decided_at = ?, decided_by = ? WHERE id = ? AND status = 'pending'")
        .bind(ts(state.now()))
        .bind(&actor.id)
        .bind(&id)
        .execute(&state.db)
        .await?;
    Ok(Json(load_for(&state, &actor, &id).await?))
}

pub async fn connectors(State(state): State<AppState>, actor: Actor) -> AppResult<Json<Vec<ConnectorSummary>>> {
    let rows: Vec<(String, String, String, i64, String, Option<String>)> = sqlx::query_as(
        "SELECT id, provider, display_name, enabled, status, last_run_at FROM connectors WHERE owner_member_id = ? ORDER BY created_at",
    )
    .bind(&actor.id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|(id, provider, display_name, enabled, status, last)| ConnectorSummary {
                id,
                provider,
                display_name,
                enabled: enabled != 0,
                status,
                last_run_at: parse_ts_opt(last),
            })
            .collect(),
    ))
}

pub fn kind_label(k: SuggestionKind) -> &'static str {
    match k {
        SuggestionKind::Task => "task",
        SuggestionKind::Appointment => "appointment",
        SuggestionKind::Deadline => "deadline",
        SuggestionKind::FollowUp => "follow-up",
    }
}

pub async fn counts(State(state): State<AppState>, actor: Actor) -> AppResult<Json<Value>> {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM suggestions WHERE member_id = ? AND status = 'pending'")
        .bind(&actor.id)
        .fetch_one(&state.db)
        .await?;
    Ok(Json(json!({"pending": n})))
}
