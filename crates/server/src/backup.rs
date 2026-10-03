//! Backup, restore and JSON export.

use crate::state::AppState;
use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};
use sqlx::{Column, Row};
use std::path::Path;

/// Consistent online snapshot using SQLite's `VACUUM INTO`.
pub async fn backup_to(state: &AppState, out: &Path) -> Result<()> {
    if out.exists() {
        bail!("{} already exists; choose a new file name", out.display());
    }
    let path = out.to_str().context("backup path must be valid UTF-8")?;
    sqlx::query("VACUUM INTO ?").bind(path).execute(&state.db).await?;
    Ok(())
}

/// Replaces the database file with a backup. Run while the server is stopped.
pub async fn restore_from(backup: &Path, database: &Path) -> Result<()> {
    let opts = sqlx::sqlite::SqliteConnectOptions::new().filename(backup).read_only(true);
    let pool = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect_with(opts).await.context("opening backup")?;
    let check: String = sqlx::query_scalar("PRAGMA integrity_check").fetch_one(&pool).await?;
    if check != "ok" {
        bail!("backup failed integrity check: {check}");
    }
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations").fetch_one(&pool).await.context("not a Tendly database")?;
    if n == 0 {
        bail!("not a Tendly database");
    }
    pool.close().await;
    if database.exists() {
        let aside = database.with_extension("db.before-restore");
        std::fs::rename(database, &aside).context("moving current database aside")?;
        for suffix in ["-wal", "-shm"] {
            let p = format!("{}{suffix}", database.display());
            let _ = std::fs::remove_file(p);
        }
    }
    std::fs::copy(backup, database).context("copying backup into place")?;
    Ok(())
}

/// Tables and the columns that must never leave the server in an export.
const EXPORT_TABLES: &[(&str, &[&str])] = &[
    ("members", &[]),
    ("tgroups", &[]),
    ("group_members", &[]),
    ("milestones", &[]),
    ("tasks", &[]),
    ("task_completions", &[]),
    ("routine_templates", &[]),
    ("countdowns", &[]),
    ("alarms", &[]),
    ("usage_tracker", &[]),
    ("calendar_sources", &["url_ciphertext", "etag", "last_modified_header"]),
    ("calendar_events", &[]),
    ("share_links", &["token_hash"]),
    ("activity", &[]),
    ("notifications", &[]),
    ("suggestions", &[]),
];

pub async fn export_json(state: &AppState) -> Result<Value> {
    let mut out = Map::new();
    out.insert("format".into(), json!("tendly-export/1"));
    out.insert("exportedAt".into(), json!(state.now()));
    out.insert("note".into(), json!("Secrets (tokens, keys, subscription URLs, share-link hashes, connector credentials) are excluded."));
    for (table, skip) in EXPORT_TABLES {
        let rows = sqlx::query(&format!("SELECT * FROM {table}")).fetch_all(&state.db).await?;
        let mut list = Vec::new();
        for r in rows {
            let mut obj = Map::new();
            for (i, col) in r.columns().iter().enumerate() {
                let name = col.name();
                if skip.contains(&name) {
                    continue;
                }
                let v: Value = if let Ok(v) = r.try_get::<Option<i64>, _>(i) {
                    json!(v)
                } else if let Ok(v) = r.try_get::<Option<f64>, _>(i) {
                    json!(v)
                } else if let Ok(v) = r.try_get::<Option<String>, _>(i) {
                    json!(v)
                } else {
                    Value::Null
                };
                obj.insert(name.to_string(), v);
            }
            list.push(Value::Object(obj));
        }
        out.insert(table.to_string(), Value::Array(list));
    }
    Ok(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn backup_restore_and_export_without_secrets() {
        let dir = tempfile::tempdir().unwrap();
        let state = crate::test_state(dir.path()).await;
        let now = crate::db::ts(state.now());
        sqlx::query("INSERT INTO members (id, display_name, color, prefs, created_at, updated_at) VALUES ('m1','Robin','mint','{}',?,?)").bind(&now).bind(&now).execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO calendar_sources (id, name, kind, url_ciphertext, created_at, updated_at) VALUES ('s1','Feed','url','v1:SECRET',?,?)").bind(&now).bind(&now).execute(&state.db).await.unwrap();
        let export = export_json(&state).await.unwrap();
        let text = export.to_string();
        assert!(text.contains("Robin"));
        assert!(!text.contains("v1:SECRET"));

        let backup = dir.path().join("backup.db");
        backup_to(&state, &backup).await.unwrap();
        assert!(backup_to(&state, &backup).await.is_err());
        let target = dir.path().join("restored.db");
        restore_from(&backup, &target).await.unwrap();
        let pool = crate::db::connect(&target).await.unwrap();
        let name: String = sqlx::query_scalar("SELECT display_name FROM members WHERE id = 'm1'").fetch_one(&pool).await.unwrap();
        assert_eq!(name, "Robin");
    }
}
