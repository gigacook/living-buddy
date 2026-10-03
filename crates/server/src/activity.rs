//! Append-only activity history with attribution.

use crate::db::{new_id, parse_ts};
use crate::security::Actor;
use anyhow::Result;
use serde_json::Value;
use sqlx::{Sqlite, SqliteConnection};
use tendly_core::api::Activity;

/// Used when an upstream feed changes something and does not say who did it.
pub const UNKNOWN_ACTOR: &str = "external source / unknown actor";

pub struct NewActivity<'a> {
    pub actor: Option<&'a Actor>,
    pub source: &'a str,
    pub entity_type: &'a str,
    pub entity_id: &'a str,
    pub group_id: Option<&'a str>,
    pub op: &'a str,
    pub summary: String,
    pub revision: u32,
    pub before: Option<Value>,
    pub after: Option<Value>,
}

pub async fn record(conn: &mut SqliteConnection, at: &str, a: NewActivity<'_>) -> Result<()> {
    let (actor_id, actor_name) = match a.actor {
        Some(actor) => (Some(actor.id.clone()), actor.name.clone()),
        None => (None, if a.source == "system" { "Tendly".to_string() } else { UNKNOWN_ACTOR.to_string() }),
    };
    sqlx::query::<Sqlite>(
        "INSERT INTO activity (id, at, actor_id, actor_name, source, entity_type, entity_id, group_id, op, summary, revision, before_json, after_json) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(new_id())
    .bind(at)
    .bind(actor_id)
    .bind(actor_name)
    .bind(a.source)
    .bind(a.entity_type)
    .bind(a.entity_id)
    .bind(a.group_id)
    .bind(a.op)
    .bind(tendly_core::model::clean_text(&a.summary, 500, false))
    .bind(a.revision as i64)
    .bind(a.before.map(|v| v.to_string()))
    .bind(a.after.map(|v| v.to_string()))
    .execute(conn)
    .await?;
    Ok(())
}

#[derive(sqlx::FromRow)]
pub struct ActivityRow {
    id: String,
    at: String,
    actor_id: Option<String>,
    actor_name: String,
    source: String,
    entity_type: String,
    entity_id: String,
    group_id: Option<String>,
    op: String,
    summary: String,
    revision: i64,
    before_json: Option<String>,
    after_json: Option<String>,
}

impl From<ActivityRow> for Activity {
    fn from(r: ActivityRow) -> Self {
        Activity {
            id: r.id,
            at: parse_ts(&r.at),
            actor_id: r.actor_id,
            actor_name: r.actor_name,
            source: r.source,
            entity_type: r.entity_type,
            entity_id: r.entity_id,
            group_id: r.group_id,
            op: r.op,
            summary: r.summary,
            revision: r.revision as u32,
            before: r.before_json.and_then(|s| serde_json::from_str(&s).ok()),
            after: r.after_json.and_then(|s| serde_json::from_str(&s).ok()),
        }
    }
}

pub const SELECT: &str = "SELECT id, at, actor_id, actor_name, source, entity_type, entity_id, group_id, op, summary, revision, before_json, after_json FROM activity";
