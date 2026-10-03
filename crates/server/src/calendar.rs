//! Calendar storage, feed application (import / poll), occurrence queries
//! and ICS/JSON export. Shared by authenticated routes and share links.

use crate::activity::{record, NewActivity};
use crate::db::{new_id, parse_ts, parse_ts_opt, ts};
use crate::security::Actor;
use crate::state::AppState;
use anyhow::Result;
use chrono::{DateTime, Duration, NaiveDate, NaiveTime, Utc};
use chrono_tz::Tz;
use serde_json::json;
use std::collections::{BTreeSet, HashMap};
use tendly_core::api::{CalendarEvent, CalendarSource, EventOccurrence, LocalOverride};
use tendly_core::ics::{expand_occurrences, EventStatus, IcsEvent, IcsTime, OutEvent, ParsedCalendar};
use tendly_core::merge::{decide_upsert, dedupe, MergeCandidate, StoredVersion, UpsertDecision};
use tendly_core::model::Category;
use tendly_core::share::{allows_event, allows_task, EventFacts, ShareDetail, ShareScope, TaskFacts};

#[derive(sqlx::FromRow, Clone, Debug)]
pub struct SourceRow {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub url_ciphertext: Option<String>,
    pub url_display: Option<String>,
    pub group_id: Option<String>,
    pub owner_id: Option<String>,
    pub is_private: i64,
    pub priority: i64,
    pub enabled: i64,
    pub refresh_minutes: i64,
    pub etag: Option<String>,
    pub last_modified_header: Option<String>,
    pub last_fetched_at: Option<String>,
    pub last_status: String,
    pub last_error: Option<String>,
    pub warnings: String,
    pub default_tz: Option<String>,
    pub created_at: String,
}

pub const SOURCE_SELECT: &str = "SELECT id, name, kind, url_ciphertext, url_display, group_id, owner_id, is_private, priority, enabled, refresh_minutes, etag, last_modified_header, last_fetched_at, last_status, last_error, warnings, default_tz, created_at FROM calendar_sources";

/// Sources a person can see: their own, their groups', and shared ungrouped ones.
pub const SOURCE_VISIBLE: &str = "(owner_id = ?1 OR group_id IN (SELECT group_id FROM group_members WHERE member_id = ?1) OR (is_private = 0 AND group_id IS NULL))";

impl SourceRow {
    pub fn tz(&self, fallback: &str) -> Tz {
        self.default_tz
            .as_deref()
            .and_then(tendly_core::ics::resolve_tzid)
            .unwrap_or_else(|| fallback.parse().unwrap_or(Tz::UTC))
    }

    pub async fn to_api(&self, state: &AppState) -> Result<CalendarSource> {
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM calendar_events WHERE source_id = ? AND removed_upstream = 0")
            .bind(&self.id)
            .fetch_one(&state.db)
            .await?;
        Ok(CalendarSource {
            id: self.id.clone(),
            name: self.name.clone(),
            kind: self.kind.clone(),
            url_display: self.url_display.clone(),
            group_id: self.group_id.clone(),
            owner_id: self.owner_id.clone(),
            is_private: self.is_private != 0,
            priority: self.priority as i32,
            enabled: self.enabled != 0,
            refresh_minutes: self.refresh_minutes as u32,
            last_fetched_at: parse_ts_opt(self.last_fetched_at.clone()),
            last_status: self.last_status.clone(),
            last_error: self.last_error.clone(),
            event_count: n as u32,
            warnings: serde_json::from_str(&self.warnings).unwrap_or_default(),
            created_at: parse_ts(&self.created_at),
        })
    }
}

pub async fn load_source(state: &AppState, id: &str) -> Result<Option<SourceRow>> {
    Ok(sqlx::query_as::<_, SourceRow>(&format!("{SOURCE_SELECT} WHERE id = ?")).bind(id).fetch_optional(&state.db).await?)
}

pub async fn visible_sources(state: &AppState, actor: &Actor) -> Result<Vec<SourceRow>> {
    Ok(sqlx::query_as::<_, SourceRow>(&format!("{SOURCE_SELECT} WHERE {SOURCE_VISIBLE} ORDER BY priority, created_at"))
        .bind(&actor.id)
        .fetch_all(&state.db)
        .await?)
}

#[derive(sqlx::FromRow, Clone, Debug)]
pub struct EventRow {
    pub id: String,
    pub source_id: String,
    pub uid: String,
    pub instance_key: String,
    pub title: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub start_json: String,
    pub end_json: String,
    pub rrule: Option<String>,
    pub exdates: String,
    pub rdates: String,
    pub status: String,
    pub sequence: i64,
    pub dtstamp: Option<String>,
    pub last_modified: Option<String>,
    pub categories: String,
    pub content_hash: String,
    pub category: Option<String>,
    pub group_id: Option<String>,
    pub local_override: Option<String>,
    pub removed_upstream: i64,
    pub revision: i64,
    pub start_utc: String,
    pub updated_at: String,
}

pub const EVENT_SELECT: &str = "SELECT id, source_id, uid, instance_key, title, description, location, start_json, end_json, rrule, exdates, rdates, status, sequence, dtstamp, last_modified, categories, content_hash, category, group_id, local_override, removed_upstream, revision, start_utc, updated_at FROM calendar_events";

impl EventRow {
    pub fn start(&self) -> IcsTime {
        serde_json::from_str(&self.start_json).unwrap_or(IcsTime::Utc { at: parse_ts(&self.start_utc) })
    }
    pub fn end(&self) -> IcsTime {
        serde_json::from_str(&self.end_json).unwrap_or_else(|_| self.start())
    }
    pub fn exdates(&self) -> Vec<IcsTime> {
        serde_json::from_str(&self.exdates).unwrap_or_default()
    }
    pub fn rdates(&self) -> Vec<IcsTime> {
        serde_json::from_str(&self.rdates).unwrap_or_default()
    }
    pub fn override_(&self) -> LocalOverride {
        self.local_override.as_deref().and_then(|s| serde_json::from_str(s).ok()).unwrap_or_default()
    }
    pub fn effective_category(&self) -> Option<Category> {
        self.override_().category.or_else(|| self.category.as_deref().and_then(Category::parse))
    }

    pub fn to_api(&self, source: &SourceRow, fallback_tz: &str) -> CalendarEvent {
        let tz = source.tz(fallback_tz);
        let start = self.start();
        let end = self.end();
        let (sd, st) = split_local(&start);
        let (mut ed, et) = split_local(&end);
        if end.is_date() {
            // DTEND is exclusive for all-day events; the API shows the last day.
            if let Ok(d) = NaiveDate::parse_from_str(&ed, "%Y-%m-%d") {
                ed = (d - Duration::days(1)).max(NaiveDate::parse_from_str(&sd, "%Y-%m-%d").unwrap_or(d)).format("%Y-%m-%d").to_string();
            }
        }
        CalendarEvent {
            id: self.id.clone(),
            source_id: self.source_id.clone(),
            uid: self.uid.clone(),
            title: self.title.clone(),
            description: self.description.clone(),
            location: self.location.clone(),
            all_day: start.is_date(),
            start_date: sd,
            start_time: st,
            end_date: ed,
            end_time: et,
            timezone: start.zone(tz).name().to_string(),
            recurrence: self.rrule.clone(),
            status: self.status.clone(),
            sequence: self.sequence as i32,
            category: self.effective_category(),
            group_id: self.group_id.clone().or(source.group_id.clone()),
            revision: self.revision as u32,
            local_override: self.local_override.as_deref().and_then(|s| serde_json::from_str(s).ok()),
            editable: source.kind == "local",
            updated_at: parse_ts(&self.updated_at),
        }
    }
}

fn split_local(t: &IcsTime) -> (String, Option<String>) {
    let l = t.local();
    match t {
        IcsTime::Date { date } => (date.format("%Y-%m-%d").to_string(), None),
        _ => (l.format("%Y-%m-%d").to_string(), Some(l.format("%H:%M").to_string())),
    }
}

fn category_from_ics(cats: &[String]) -> Option<String> {
    cats.iter().find_map(|c| Category::parse(&c.to_lowercase())).map(|c| c.as_str().to_string())
}

#[derive(Default, Debug, Clone, Copy)]
pub struct ApplyStats {
    pub inserted: u32,
    pub updated: u32,
    pub unchanged: u32,
    pub cancelled: u32,
}

const MAX_ACTIVITY_PER_RUN: u32 = 100;

/// Applies a parsed feed to one source. When `snapshot` is true, events that
/// are no longer present are marked as removed upstream — for this source only.
pub async fn apply_feed(
    state: &AppState,
    source: &SourceRow,
    cal: &ParsedCalendar,
    actor: Option<&Actor>,
    origin: &str,
    snapshot: bool,
) -> Result<ApplyStats> {
    let tz = cal
        .default_tz
        .as_deref()
        .and_then(tendly_core::ics::resolve_tzid)
        .unwrap_or_else(|| source.tz(&state.config.default_timezone));
    let now = ts(state.now());
    let activity_source = format!("{origin}:{}", source.name);
    let mut stats = ApplyStats::default();
    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
    let mut logged = 0u32;
    let mut tx = state.db.begin().await?;
    sqlx::query("UPDATE calendar_sources SET default_tz = ? WHERE id = ?").bind(tz.name()).bind(&source.id).execute(&mut *tx).await?;

    for e in &cal.events {
        let instance = e.recurrence_id.as_ref().map(|r| r.instance_key(tz)).unwrap_or_default();
        seen.insert((e.uid.clone(), instance.clone()));
        let existing: Option<(String, i64, Option<String>, String, String, i64, i64)> = sqlx::query_as(
            "SELECT id, sequence, last_modified, content_hash, status, revision, removed_upstream FROM calendar_events WHERE source_id = ? AND uid = ? AND instance_key = ?",
        )
        .bind(&source.id)
        .bind(&e.uid)
        .bind(&instance)
        .fetch_optional(&mut *tx)
        .await?;
        let incoming = StoredVersion {
            sequence: e.sequence,
            last_modified: e.last_modified,
            content_hash: e.content_hash(),
            cancelled: e.status == EventStatus::Cancelled,
        };
        let stored = existing.as_ref().map(|(_, seq, lm, hash, status, _, _)| StoredVersion {
            sequence: *seq,
            last_modified: parse_ts_opt(lm.clone()),
            content_hash: hash.clone(),
            cancelled: status == "cancelled",
        });
        let mut decision = decide_upsert(stored.as_ref(), &incoming);
        if let (Some((_, _, _, _, _, _, removed)), UpsertDecision::Skip { .. }) = (&existing, &decision) {
            if *removed != 0 {
                decision = UpsertDecision::Update { reason: "reappeared upstream" };
            }
        }
        let end = e.effective_end();
        let start_utc = ts(e.start.to_utc(tz));
        match decision {
            UpsertDecision::Insert => {
                let id = new_id();
                insert_event(&mut tx, &id, &source.id, e, &instance, &end, &start_utc, &now).await?;
                stats.inserted += 1;
                if logged < MAX_ACTIVITY_PER_RUN {
                    logged += 1;
                    record(&mut tx, &now, NewActivity {
                        actor, source: &activity_source, entity_type: "calendar_event", entity_id: &id, group_id: source.group_id.as_deref(),
                        op: "create", summary: format!("“{}” appeared in {}", e.summary, source.name), revision: 1, before: None,
                        after: Some(json!({"title": e.summary, "start": e.start, "sequence": e.sequence})),
                    }).await?;
                }
            }
            UpsertDecision::Update { reason } => {
                let (id, _, _, _, old_status, revision, _) = existing.clone().expect("update implies existing");
                let old: Option<(String, String)> = sqlx::query_as("SELECT title, start_json FROM calendar_events WHERE id = ?").bind(&id).fetch_optional(&mut *tx).await?;
                sqlx::query(
                    "UPDATE calendar_events SET title=?, description=?, location=?, start_json=?, end_json=?, rrule=?, exdates=?, rdates=?, status=?, sequence=?, dtstamp=?, last_modified=?, categories=?, content_hash=?, category=?, removed_upstream=0, revision=revision+1, start_utc=?, updated_at=? WHERE id=?",
                )
                .bind(&e.summary)
                .bind(&e.description)
                .bind(&e.location)
                .bind(serde_json::to_string(&e.start)?)
                .bind(serde_json::to_string(&end)?)
                .bind(&e.rrule)
                .bind(serde_json::to_string(&e.exdates)?)
                .bind(serde_json::to_string(&e.rdates)?)
                .bind(e.status.as_str())
                .bind(e.sequence)
                .bind(e.dtstamp.map(ts))
                .bind(e.last_modified.map(ts))
                .bind(serde_json::to_string(&e.categories)?)
                .bind(&incoming.content_hash)
                .bind(category_from_ics(&e.categories))
                .bind(&start_utc)
                .bind(&now)
                .bind(&id)
                .execute(&mut *tx)
                .await?;
                let cancelled_now = e.status == EventStatus::Cancelled && old_status != "cancelled";
                if cancelled_now {
                    stats.cancelled += 1;
                } else {
                    stats.updated += 1;
                }
                if logged < MAX_ACTIVITY_PER_RUN {
                    logged += 1;
                    record(&mut tx, &now, NewActivity {
                        actor, source: &activity_source, entity_type: "calendar_event", entity_id: &id, group_id: source.group_id.as_deref(),
                        op: if cancelled_now { "cancel" } else { "update" },
                        summary: format!("“{}” {} in {} ({reason})", e.summary, if cancelled_now { "was cancelled" } else { "changed" }, source.name),
                        revision: revision as u32 + 1,
                        before: old.map(|(t, s)| json!({"title": t, "start": serde_json::from_str::<serde_json::Value>(&s).unwrap_or_default()})),
                        after: Some(json!({"title": e.summary, "start": e.start, "sequence": e.sequence, "status": e.status.as_str()})),
                    }).await?;
                }
            }
            UpsertDecision::Skip { .. } => stats.unchanged += 1,
        }
    }

    if snapshot {
        let stored: Vec<(String, String, String, String, i64)> = sqlx::query_as(
            "SELECT id, uid, instance_key, title, revision FROM calendar_events WHERE source_id = ? AND removed_upstream = 0",
        )
        .bind(&source.id)
        .fetch_all(&mut *tx)
        .await?;
        for (id, uid, inst, title, revision) in stored {
            if !seen.contains(&(uid, inst)) {
                sqlx::query("UPDATE calendar_events SET removed_upstream = 1, revision = revision + 1, updated_at = ? WHERE id = ?")
                    .bind(&now)
                    .bind(&id)
                    .execute(&mut *tx)
                    .await?;
                stats.cancelled += 1;
                if logged < MAX_ACTIVITY_PER_RUN {
                    logged += 1;
                    record(&mut tx, &now, NewActivity {
                        actor, source: &activity_source, entity_type: "calendar_event", entity_id: &id, group_id: source.group_id.as_deref(),
                        op: "removed_upstream", summary: format!("“{title}” is no longer in {}", source.name), revision: revision as u32 + 1,
                        before: None, after: None,
                    }).await?;
                }
            }
        }
    }
    let total = stats.inserted + stats.updated + stats.cancelled;
    if logged >= MAX_ACTIVITY_PER_RUN && total > logged {
        record(&mut tx, &now, NewActivity {
            actor, source: &activity_source, entity_type: "calendar_source", entity_id: &source.id, group_id: source.group_id.as_deref(),
            op: origin, summary: format!("{} more changes in {} were applied", total - logged, source.name), revision: 0, before: None, after: None,
        }).await?;
    }
    sqlx::query("UPDATE calendar_sources SET warnings = ?, updated_at = ? WHERE id = ?")
        .bind(serde_json::to_string(&cal.warnings.iter().take(20).collect::<Vec<_>>())?)
        .bind(&now)
        .bind(&source.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(stats)
}

#[allow(clippy::too_many_arguments)]
async fn insert_event(
    conn: &mut sqlx::SqliteConnection,
    id: &str,
    source_id: &str,
    e: &IcsEvent,
    instance: &str,
    end: &IcsTime,
    start_utc: &str,
    now: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO calendar_events (id, source_id, uid, instance_key, title, description, location, start_json, end_json, rrule, exdates, rdates, status, sequence, dtstamp, last_modified, categories, content_hash, category, revision, start_utc, created_at, updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,1,?,?,?)",
    )
    .bind(id)
    .bind(source_id)
    .bind(&e.uid)
    .bind(instance)
    .bind(&e.summary)
    .bind(&e.description)
    .bind(&e.location)
    .bind(serde_json::to_string(&e.start)?)
    .bind(serde_json::to_string(end)?)
    .bind(&e.rrule)
    .bind(serde_json::to_string(&e.exdates)?)
    .bind(serde_json::to_string(&e.rdates)?)
    .bind(e.status.as_str())
    .bind(e.sequence)
    .bind(e.dtstamp.map(ts))
    .bind(e.last_modified.map(ts))
    .bind(serde_json::to_string(&e.categories)?)
    .bind(e.content_hash())
    .bind(category_from_ics(&e.categories))
    .bind(start_utc)
    .bind(now)
    .bind(now)
    .execute(conn)
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct OccQuery {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub group_ids: Vec<String>,
    pub member_id: Option<String>,
    pub categories: Vec<Category>,
    pub source_ids: Vec<String>,
    pub include_tasks: bool,
    pub include_events: bool,
}

pub enum Viewer<'a> {
    Actor(&'a Actor),
    Share(&'a ShareScope),
}

struct Candidate {
    occ: EventOccurrence,
    merge: Option<MergeCandidate>,
}

async fn member_group_ids(state: &AppState, member: &str) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar("SELECT group_id FROM group_members WHERE member_id = ?").bind(member).fetch_all(&state.db).await?)
}

/// Visible, filtered event rows plus their sources (masters and overrides).
async fn visible_events(state: &AppState, viewer: &Viewer<'_>, q: &OccQuery) -> Result<Vec<(EventRow, SourceRow)>> {
    let sources: Vec<SourceRow> = match viewer {
        Viewer::Actor(a) => visible_sources(state, a).await?,
        Viewer::Share(_) => sqlx::query_as::<_, SourceRow>(&format!("{SOURCE_SELECT} ORDER BY priority, created_at")).fetch_all(&state.db).await?,
    };
    let member_groups = match &q.member_id {
        Some(m) => Some(member_group_ids(state, m).await?),
        None => None,
    };
    let mut out = Vec::new();
    for s in sources.into_iter().filter(|s| s.enabled != 0) {
        if !q.source_ids.is_empty() && !q.source_ids.contains(&s.id) {
            continue;
        }
        let rows = sqlx::query_as::<_, EventRow>(&format!(
            "{EVENT_SELECT} WHERE source_id = ? AND removed_upstream = 0 AND (rrule IS NOT NULL OR rdates != '[]' OR (start_utc < ? AND start_utc > ?))"
        ))
        .bind(&s.id)
        .bind(ts(q.to))
        .bind(ts(q.from - Duration::days(400)))
        .fetch_all(&state.db)
        .await?;
        for r in rows {
            let group = r.group_id.clone().or(s.group_id.clone());
            let category = r.effective_category();
            if r.override_().hidden == Some(true) {
                continue;
            }
            if !q.categories.is_empty() && !category.map(|c| q.categories.contains(&c)).unwrap_or(false) {
                continue;
            }
            if !q.group_ids.is_empty() && !group.as_ref().map(|g| q.group_ids.contains(g)).unwrap_or(false) {
                continue;
            }
            if let Some(mg) = &member_groups {
                let owned = s.owner_id.as_deref() == q.member_id.as_deref();
                let in_group = group.as_ref().map(|g| mg.contains(g)).unwrap_or(false);
                if !owned && !in_group {
                    continue;
                }
            }
            if let Viewer::Share(scope) = viewer {
                let facts = EventFacts { source_id: &s.id, source_private: s.is_private != 0, group_id: group.as_deref(), category };
                if !allows_event(scope, &facts) {
                    continue;
                }
            }
            out.push((r, s.clone()));
        }
    }
    Ok(out)
}

pub async fn occurrences(state: &AppState, viewer: Viewer<'_>, q: &OccQuery) -> Result<Vec<EventOccurrence>> {
    let mut candidates: Vec<Candidate> = Vec::new();
    if q.include_events {
        let rows = visible_events(state, &viewer, q).await?;
        // Overrides replace generated instances of their master.
        let overrides: HashMap<(String, String, String), &EventRow> = rows
            .iter()
            .filter(|(r, _)| !r.instance_key.is_empty())
            .map(|(r, _)| ((r.source_id.clone(), r.uid.clone(), r.instance_key.clone()), r))
            .collect();
        for (r, s) in &rows {
            let tz = s.tz(&state.config.default_timezone);
            let (occs, _warning) = expand_occurrences(&r.start(), &r.end(), r.rrule.as_deref(), &r.exdates(), &r.rdates(), tz, q.from, q.to, 2000);
            for o in occs {
                let is_master = r.instance_key.is_empty();
                if is_master && overrides.contains_key(&(r.source_id.clone(), r.uid.clone(), o.instance_key.clone())) {
                    continue;
                }
                let key = if is_master { o.instance_key.clone() } else { r.instance_key.clone() };
                if r.status == "cancelled" {
                    continue;
                }
                let all_day = o.start.is_date();
                let (start, end) = (o.start.to_utc(tz), o.end.to_utc(tz));
                let ov = r.override_();
                candidates.push(Candidate {
                    merge: (s.kind != "local").then(|| MergeCandidate {
                        id: format!("{}|{}", r.id, key),
                        source_id: s.id.clone(),
                        source_priority: s.priority,
                        uid: r.uid.clone(),
                        instance_key: key.clone(),
                        sequence: r.sequence,
                        last_modified: parse_ts_opt(r.last_modified.clone()),
                    }),
                    occ: EventOccurrence {
                        kind: "event".into(),
                        event_id: r.id.clone(),
                        source_id: Some(s.id.clone()),
                        source_name: Some(s.name.clone()),
                        instance_key: key,
                        uid: r.uid.clone(),
                        title: r.title.clone(),
                        description: match (&r.description, &ov.note) {
                            (Some(d), Some(n)) => Some(format!("{d}\n\nNote: {n}")),
                            (None, Some(n)) => Some(format!("Note: {n}")),
                            (d, None) => d.clone(),
                        },
                        location: r.location.clone(),
                        start,
                        end,
                        all_day,
                        start_date: all_day.then(|| o.start.local().format("%Y-%m-%d").to_string()),
                        end_date: all_day.then(|| {
                            let last = (o.end.local() - Duration::days(1)).max(o.start.local());
                            last.format("%Y-%m-%d").to_string()
                        }),
                        status: r.status.clone(),
                        recurring: r.rrule.is_some() || !r.instance_key.is_empty(),
                        category: r.effective_category(),
                        group_id: r.group_id.clone().or(s.group_id.clone()),
                        assignee_id: None,
                        also_in: vec![],
                        has_local_override: r.local_override.is_some(),
                    },
                });
            }
        }
    }
    // Deterministic de-duplication across feed sources.
    let merge_inputs: Vec<MergeCandidate> = candidates.iter().filter_map(|c| c.merge.clone()).collect();
    let picks = dedupe(&merge_inputs);
    let winners: HashMap<String, Vec<String>> = picks.into_iter().map(|p| (p.winner_id, p.also_in)).collect();
    let mut out: Vec<EventOccurrence> = Vec::new();
    for c in candidates {
        match &c.merge {
            None => out.push(c.occ),
            Some(m) => {
                if let Some(also) = winners.get(&m.id) {
                    let mut occ = c.occ;
                    occ.also_in = also.clone();
                    out.push(occ);
                }
            }
        }
    }
    if q.include_tasks {
        out.extend(task_occurrences(state, &viewer, q).await?);
    }
    out.sort_by(|a, b| a.start.cmp(&b.start).then(a.title.cmp(&b.title)));
    if let Viewer::Share(scope) = viewer {
        for o in &mut out {
            apply_detail(o, scope.detail);
        }
    }
    Ok(out)
}

pub fn apply_detail(o: &mut EventOccurrence, detail: ShareDetail) {
    match detail {
        ShareDetail::Full => {}
        ShareDetail::TitlesOnly => {
            o.description = None;
            o.location = None;
            o.assignee_id = None;
        }
        ShareDetail::BusyOnly => {
            o.title = "Busy".into();
            o.description = None;
            o.location = None;
            o.category = None;
            o.assignee_id = None;
            o.group_id = None;
            o.source_name = None;
            o.source_id = None;
            o.uid = format!("busy-{}@tendly", &tendly_core::share::hash_token(&o.uid)[..16]);
            o.event_id = String::new();
        }
    }
}

#[derive(sqlx::FromRow)]
struct TaskLite {
    id: String,
    group_id: Option<String>,
    title: String,
    notes: Option<String>,
    category: String,
    owner_id: String,
    assignee_id: Option<String>,
    due_date: String,
    due_time: Option<String>,
    duration_minutes: Option<i64>,
    timezone: String,
    recurrence: Option<String>,
    version: i64,
    updated_at: String,
}

async fn task_occurrences(state: &AppState, viewer: &Viewer<'_>, q: &OccQuery) -> Result<Vec<EventOccurrence>> {
    let from_d = (q.from - Duration::days(1)).date_naive().format("%Y-%m-%d").to_string();
    let to_d = (q.to + Duration::days(1)).date_naive().format("%Y-%m-%d").to_string();
    let base = "SELECT id, group_id, title, notes, category, owner_id, assignee_id, due_date, due_time, duration_minutes, timezone, recurrence, version, updated_at FROM tasks WHERE deleted_at IS NULL AND completed_at IS NULL AND due_date IS NOT NULL AND due_date >= ? AND due_date <= ?";
    let rows: Vec<TaskLite> = match viewer {
        Viewer::Actor(a) => sqlx::query_as::<_, TaskLite>(&format!(
            "{base} AND ((group_id IS NULL AND (owner_id = ? OR assignee_id = ?)) OR group_id IN (SELECT group_id FROM group_members WHERE member_id = ?))"
        ))
        .bind(&from_d)
        .bind(&to_d)
        .bind(&a.id)
        .bind(&a.id)
        .bind(&a.id)
        .fetch_all(&state.db)
        .await?,
        Viewer::Share(_) => sqlx::query_as::<_, TaskLite>(base).bind(&from_d).bind(&to_d).fetch_all(&state.db).await?,
    };
    let mut out = Vec::new();
    for t in rows {
        let category = Category::parse(&t.category);
        if !q.categories.is_empty() && !category.map(|c| q.categories.contains(&c)).unwrap_or(false) {
            continue;
        }
        if !q.group_ids.is_empty() && !t.group_id.as_ref().map(|g| q.group_ids.contains(g)).unwrap_or(false) {
            continue;
        }
        if let Some(m) = &q.member_id {
            if t.assignee_id.as_deref() != Some(m) && !(t.group_id.is_none() && &t.owner_id == m) {
                continue;
            }
        }
        if let Viewer::Share(scope) = viewer {
            let facts = TaskFacts { group_id: t.group_id.as_deref(), owner_id: &t.owner_id, assignee_id: t.assignee_id.as_deref(), category };
            if !allows_task(scope, &facts) {
                continue;
            }
        }
        let Ok(date) = NaiveDate::parse_from_str(&t.due_date, "%Y-%m-%d") else { continue };
        let tz: Tz = t.timezone.parse().unwrap_or(Tz::UTC);
        let (start, end, all_day) = match t.due_time.as_deref().and_then(|x| NaiveTime::parse_from_str(x, "%H:%M").ok()) {
            Some(time) => {
                let s = tendly_core::recurrence::resolve_local(tz, date.and_time(time));
                (s, s + Duration::minutes(t.duration_minutes.unwrap_or(30)), false)
            }
            None => {
                let s = tendly_core::recurrence::resolve_local(tz, date.and_time(NaiveTime::MIN));
                (s, s + Duration::days(1), true)
            }
        };
        if !(start < q.to && end > q.from) {
            continue;
        }
        let _ = (&t.version, &t.updated_at);
        out.push(EventOccurrence {
            kind: "task".into(),
            event_id: t.id.clone(),
            source_id: None,
            source_name: None,
            instance_key: t.due_date.replace('-', ""),
            uid: format!("task-{}@tendly", t.id),
            title: t.title,
            description: t.notes,
            location: None,
            start,
            end,
            all_day,
            start_date: all_day.then(|| t.due_date.clone()),
            end_date: all_day.then(|| t.due_date.clone()),
            status: "confirmed".into(),
            recurring: t.recurrence.is_some(),
            category,
            group_id: t.group_id,
            assignee_id: t.assignee_id,
            also_in: vec![],
            has_local_override: false,
        });
    }
    Ok(out)
}

/// Builds an ICS document. Recurring events are exported as masters with
/// RRULE/EXDATE (plus overrides), so subscribers expand them natively.
pub async fn export_ics(state: &AppState, viewer: Viewer<'_>, q: &OccQuery, name: &str) -> Result<String> {
    let detail = match &viewer {
        Viewer::Share(s) => s.detail,
        Viewer::Actor(_) => ShareDetail::Full,
    };
    let occs = occurrences(state, viewer, q).await?;
    // Collect which rows have at least one surviving occurrence in range.
    let mut wanted_events: BTreeSet<String> = BTreeSet::new();
    let mut tasks: Vec<&EventOccurrence> = Vec::new();
    for o in &occs {
        if o.kind == "task" {
            tasks.push(o);
        } else if !o.event_id.is_empty() {
            wanted_events.insert(o.event_id.clone());
        }
    }
    let mut out_events: Vec<OutEvent> = Vec::new();
    let mut seen_uid_instance: BTreeSet<(String, String)> = BTreeSet::new();
    if detail == ShareDetail::BusyOnly {
        // Busy-only feeds export flattened instances with opaque UIDs.
        for o in &occs {
            out_events.push(OutEvent {
                uid: o.uid.clone(),
                summary: "Busy".into(),
                description: None,
                location: None,
                start: if o.all_day { IcsTime::Date { date: NaiveDate::parse_from_str(o.start_date.as_deref().unwrap_or(""), "%Y-%m-%d").unwrap_or(o.start.date_naive()) } } else { IcsTime::Utc { at: o.start } },
                end: Some(if o.all_day {
                    IcsTime::Date { date: NaiveDate::parse_from_str(o.end_date.as_deref().unwrap_or(""), "%Y-%m-%d").map(|d| d + Duration::days(1)).unwrap_or(o.end.date_naive()) }
                } else {
                    IcsTime::Utc { at: o.end }
                }),
                rrule: None,
                exdates: vec![],
                recurrence_id: None,
                status: EventStatus::Confirmed,
                sequence: 0,
                dtstamp: state.now(),
                last_modified: None,
                categories: vec![],
                source_label: None,
            });
        }
        return Ok(tendly_core::ics::write_calendar(name, &out_events, state.now()));
    }
    for id in wanted_events {
        let Some(r) = sqlx::query_as::<_, EventRow>(&format!("{EVENT_SELECT} WHERE id = ?")).bind(&id).fetch_optional(&state.db).await? else { continue };
        if !seen_uid_instance.insert((r.uid.clone(), r.instance_key.clone())) {
            continue;
        }
        let source = load_source(state, &r.source_id).await?;
        let full = detail == ShareDetail::Full;
        out_events.push(OutEvent {
            uid: r.uid.clone(),
            summary: r.title.clone(),
            description: if full { r.description.clone() } else { None },
            location: if full { r.location.clone() } else { None },
            start: r.start(),
            end: Some(r.end()),
            rrule: if r.instance_key.is_empty() { r.rrule.clone() } else { None },
            exdates: r.exdates(),
            recurrence_id: if r.instance_key.is_empty() { None } else { recurrence_id_from_key(&r.instance_key) },
            status: EventStatus::parse(&r.status),
            sequence: r.sequence,
            dtstamp: parse_ts(&r.updated_at),
            last_modified: Some(parse_ts(&r.updated_at)),
            categories: r.effective_category().map(|c| vec![c.as_str().to_string()]).unwrap_or_default(),
            source_label: if full { source.map(|s| s.name) } else { None },
        });
        // Include overrides for exported masters.
        if r.instance_key.is_empty() && r.rrule.is_some() {
            let ovs = sqlx::query_as::<_, EventRow>(&format!("{EVENT_SELECT} WHERE source_id = ? AND uid = ? AND instance_key != '' AND removed_upstream = 0"))
                .bind(&r.source_id)
                .bind(&r.uid)
                .fetch_all(&state.db)
                .await?;
            for ov in ovs {
                if !seen_uid_instance.insert((ov.uid.clone(), ov.instance_key.clone())) {
                    continue;
                }
                out_events.push(OutEvent {
                    uid: ov.uid.clone(),
                    summary: ov.title.clone(),
                    description: if full { ov.description.clone() } else { None },
                    location: if full { ov.location.clone() } else { None },
                    start: ov.start(),
                    end: Some(ov.end()),
                    rrule: None,
                    exdates: vec![],
                    recurrence_id: recurrence_id_from_key(&ov.instance_key),
                    status: EventStatus::parse(&ov.status),
                    sequence: ov.sequence,
                    dtstamp: parse_ts(&ov.updated_at),
                    last_modified: Some(parse_ts(&ov.updated_at)),
                    categories: vec![],
                    source_label: None,
                });
            }
        }
    }
    for t in tasks {
        let tr: Option<(i64, String)> = sqlx::query_as("SELECT version, updated_at FROM tasks WHERE id = ?").bind(&t.event_id).fetch_optional(&state.db).await?;
        let (version, updated) = tr.unwrap_or((1, ts(state.now())));
        out_events.push(OutEvent {
            uid: t.uid.clone(),
            summary: t.title.clone(),
            description: if detail == ShareDetail::Full { t.description.clone() } else { None },
            location: None,
            start: if t.all_day {
                IcsTime::Date { date: NaiveDate::parse_from_str(t.start_date.as_deref().unwrap_or(""), "%Y-%m-%d").unwrap_or(t.start.date_naive()) }
            } else {
                IcsTime::Utc { at: t.start }
            },
            end: Some(if t.all_day {
                IcsTime::Date { date: NaiveDate::parse_from_str(t.start_date.as_deref().unwrap_or(""), "%Y-%m-%d").map(|d| d + Duration::days(1)).unwrap_or(t.end.date_naive()) }
            } else {
                IcsTime::Utc { at: t.end }
            }),
            rrule: None,
            exdates: vec![],
            recurrence_id: None,
            status: EventStatus::Confirmed,
            sequence: version,
            dtstamp: parse_ts(&updated),
            last_modified: Some(parse_ts(&updated)),
            categories: t.category.map(|c| vec![c.as_str().to_string()]).unwrap_or_default(),
            source_label: Some("Tendly tasks".into()),
        });
    }
    Ok(tendly_core::ics::write_calendar(name, &out_events, state.now()))
}

fn recurrence_id_from_key(key: &str) -> Option<IcsTime> {
    if key.len() == 8 {
        return NaiveDate::parse_from_str(key, "%Y%m%d").ok().map(|date| IcsTime::Date { date });
    }
    chrono::NaiveDateTime::parse_from_str(key.trim_end_matches('Z'), "%Y%m%dT%H%M%S")
        .ok()
        .map(|n| IcsTime::Utc { at: chrono::TimeZone::from_utc_datetime(&Utc, &n) })
}

/// Fetches and applies a URL source; records status and a change-history entry.
pub async fn refresh_url_source(state: &AppState, source: &SourceRow) -> Result<ApplyStats> {
    let now = ts(state.now());
    let url = match &source.url_ciphertext {
        Some(c) => state.cipher.decrypt(c)?,
        None => anyhow::bail!("source has no URL"),
    };
    let policy = crate::fetch::FetchPolicy {
        trusted: state.config.trusted_fetch_networks.clone(),
        use_proxy: state.config.fetch_via_proxy,
        ..Default::default()
    };
    let result = crate::fetch::fetch_calendar(&url, &policy, source.etag.as_deref(), source.last_modified_header.as_deref()).await;
    let fetched = match result {
        Ok(f) => f,
        Err(e) => {
            sqlx::query("UPDATE calendar_sources SET last_status = 'error', last_error = ?, last_fetched_at = ? WHERE id = ?")
                .bind(e.to_string())
                .bind(&now)
                .bind(&source.id)
                .execute(&state.db)
                .await?;
            anyhow::bail!(e.to_string());
        }
    };
    if fetched.not_modified {
        sqlx::query("UPDATE calendar_sources SET last_status = 'ok', last_error = NULL, last_fetched_at = ? WHERE id = ?")
            .bind(&now)
            .bind(&source.id)
            .execute(&state.db)
            .await?;
        return Ok(ApplyStats::default());
    }
    let body = fetched.body.unwrap_or_default();
    let parsed = match tendly_core::ics::parse(&body, &tendly_core::ics::Limits::default()) {
        Ok(p) => p,
        Err(e) => {
            sqlx::query("UPDATE calendar_sources SET last_status = 'error', last_error = ?, last_fetched_at = ? WHERE id = ?")
                .bind(e.to_string())
                .bind(&now)
                .bind(&source.id)
                .execute(&state.db)
                .await?;
            anyhow::bail!(e.to_string());
        }
    };
    let stats = apply_feed(state, source, &parsed, None, "poll", true).await?;
    sqlx::query("UPDATE calendar_sources SET last_status = 'ok', last_error = NULL, last_fetched_at = ?, etag = ?, last_modified_header = ? WHERE id = ?")
        .bind(&now)
        .bind(&fetched.etag)
        .bind(&fetched.last_modified)
        .bind(&source.id)
        .execute(&state.db)
        .await?;
    Ok(stats)
}
