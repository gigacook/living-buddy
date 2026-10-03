use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;
use tendly_core::templates::TEMPLATES;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

pub async fn connect(path: &Path) -> Result<SqlitePool> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("creating data directory")?;
    }
    let opts = SqliteConnectOptions::from_str(&format!("sqlite://{}", path.display()))?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePoolOptions::new().max_connections(8).connect_with(opts).await?;
    MIGRATOR.run(&pool).await.context("running database migrations")?;
    seed_templates(&pool).await?;
    Ok(pool)
}

async fn seed_templates(pool: &SqlitePool) -> Result<()> {
    let now = ts(Utc::now());
    for t in TEMPLATES {
        sqlx::query(
            "INSERT OR IGNORE INTO routine_templates (key, title, category, recurrence, repeat_mode, duration_minutes, checklist, tip, updated_at) VALUES (?,?,?,?,?,?,?,?,?)",
        )
        .bind(t.key)
        .bind(t.title)
        .bind(t.category.as_str())
        .bind(t.rrule)
        .bind(t.repeat_mode.as_str())
        .bind(t.duration_minutes as i64)
        .bind(serde_json::to_string(t.checklist)?)
        .bind(t.tip)
        .bind(&now)
        .execute(pool)
        .await?;
    }
    Ok(())
}

pub fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

pub fn ts(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub fn parse_ts(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).map(|d| d.with_timezone(&Utc)).unwrap_or_else(|_| DateTime::<Utc>::MIN_UTC)
}

pub fn parse_ts_opt(s: Option<String>) -> Option<DateTime<Utc>> {
    s.as_deref().and_then(|s| DateTime::parse_from_rfc3339(s).ok()).map(|d| d.with_timezone(&Utc))
}

pub async fn get_setting(pool: &SqlitePool, key: &str) -> Result<Option<String>> {
    Ok(sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key = ?").bind(key).fetch_optional(pool).await?)
}

pub async fn set_setting(pool: &SqlitePool, key: &str, value: &str) -> Result<()> {
    sqlx::query("INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value")
        .bind(key)
        .bind(value)
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn migrations_apply_and_are_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        let pool = connect(&path).await.unwrap();
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM routine_templates").fetch_one(&pool).await.unwrap();
        assert_eq!(n, 12);
        pool.close().await;
        // Re-opening runs no migrations twice and keeps templates unique.
        let pool = connect(&path).await.unwrap();
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM routine_templates").fetch_one(&pool).await.unwrap();
        assert_eq!(n, 12);
        let applied: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations").fetch_one(&pool).await.unwrap();
        assert_eq!(applied as usize, MIGRATOR.iter().count());
        let fk: i64 = sqlx::query_scalar("PRAGMA foreign_keys").fetch_one(&pool).await.unwrap();
        assert_eq!(fk, 1);
    }
}
