//! Durable job queue and scheduler for connector syncs, calendar refreshes and
//! retention. Jobs live in SQLite with leases, idempotency keys, exponential
//! backoff and a retry limit, so a crash or a second worker cannot double-run them.
//!
//! Run exactly one scheduling mode: the embedded worker inside `tendly serve`
//! (default), `tendly worker` as its own service, or `tendly worker --once`
//! from cron / a systemd timer.

use crate::connectors::ConnectorError;
use crate::db::{new_id, parse_ts, ts};
use crate::state::AppState;
use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use rand::Rng;
use serde_json::{json, Value};

pub const LEASE_SECS: i64 = 300;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Job {
    pub id: String,
    pub kind: String,
    pub payload: String,
    pub attempts: i64,
    pub max_attempts: i64,
}

pub async fn enqueue(state: &AppState, kind: &str, payload: Value, idempotency_key: Option<&str>, run_after: DateTime<Utc>) -> Result<bool> {
    let now = ts(state.now());
    let res = sqlx::query("INSERT OR IGNORE INTO jobs (id, kind, payload, status, attempts, max_attempts, run_after, idempotency_key, created_at, updated_at) VALUES (?,?,?,'queued',0,5,?,?,?,?)")
        .bind(new_id())
        .bind(kind)
        .bind(payload.to_string())
        .bind(ts(run_after))
        .bind(idempotency_key)
        .bind(&now)
        .bind(&now)
        .execute(&state.db)
        .await?;
    Ok(res.rows_affected() == 1)
}

/// Atomically claims the next ready job (queued, or running with an expired lease).
pub async fn claim(state: &AppState) -> Result<Option<Job>> {
    let now = state.now();
    let job = sqlx::query_as::<_, Job>(
        "UPDATE jobs SET status = 'running', attempts = attempts + 1, locked_until = ?, updated_at = ?
         WHERE id = (SELECT id FROM jobs WHERE (status = 'queued' AND run_after <= ?) OR (status = 'running' AND locked_until < ?) ORDER BY run_after LIMIT 1)
         RETURNING id, kind, payload, attempts, max_attempts",
    )
    .bind(ts(now + Duration::seconds(LEASE_SECS)))
    .bind(ts(now))
    .bind(ts(now))
    .bind(ts(now))
    .fetch_optional(&state.db)
    .await?;
    Ok(job)
}

pub fn backoff(attempts: i64, retry_after: Option<u64>) -> Duration {
    if let Some(s) = retry_after {
        return Duration::seconds(s.clamp(1, 6 * 3600) as i64);
    }
    let base = 30i64 * 2i64.pow(attempts.clamp(0, 10) as u32);
    let jitter = rand::thread_rng().gen_range(0..=base / 4 + 1);
    Duration::seconds((base + jitter).min(6 * 3600))
}

async fn finish(state: &AppState, job: &Job) -> Result<()> {
    sqlx::query("UPDATE jobs SET status = 'done', locked_until = NULL, last_error = NULL, updated_at = ? WHERE id = ?")
        .bind(ts(state.now()))
        .bind(&job.id)
        .execute(&state.db)
        .await?;
    Ok(())
}

async fn fail(state: &AppState, job: &Job, err: &str, retryable: bool, retry_after: Option<u64>) -> Result<()> {
    let now = state.now();
    let dead = !retryable || job.attempts >= job.max_attempts;
    sqlx::query("UPDATE jobs SET status = ?, locked_until = NULL, last_error = ?, run_after = ?, updated_at = ? WHERE id = ?")
        .bind(if dead { "dead" } else { "queued" })
        .bind(tendly_core::redact::redact(err))
        .bind(ts(now + backoff(job.attempts, retry_after)))
        .bind(ts(now))
        .bind(&job.id)
        .execute(&state.db)
        .await?;
    Ok(())
}

pub async fn run_job(state: &AppState, job: &Job) -> Result<()> {
    let payload: Value = serde_json::from_str(&job.payload).unwrap_or(json!({}));
    match job.kind.as_str() {
        "connector_sync" => {
            let id = payload.get("connectorId").and_then(|v| v.as_str()).unwrap_or_default();
            match crate::connectors::run_sync(state, id).await {
                Ok(()) => finish(state, job).await,
                Err(ConnectorError::RateLimited(after)) => fail(state, job, "rate limited", true, after).await,
                Err(ConnectorError::Transient(m)) => fail(state, job, &m, true, None).await,
                // Auth and permanent problems wait for a person; retrying would only spam the provider.
                Err(e) => fail(state, job, &e.to_string(), false, None).await,
            }
        }
        "calendar_refresh" => {
            let id = payload.get("sourceId").and_then(|v| v.as_str()).unwrap_or_default();
            let Some(source) = crate::calendar::load_source(state, id).await? else { return finish(state, job).await };
            match crate::calendar::refresh_url_source(state, &source).await {
                Ok(stats) => {
                    if stats.inserted + stats.updated + stats.cancelled > 0 {
                        let mut conn = state.db.acquire().await?;
                        crate::activity::record(&mut conn, &ts(state.now()), crate::activity::NewActivity {
                            actor: None, source: &format!("poll:{}", source.name), entity_type: "calendar_source", entity_id: &source.id,
                            group_id: source.group_id.as_deref(), op: "poll",
                            summary: format!("Scheduled refresh of “{}”: {} new, {} changed, {} removed", source.name, stats.inserted, stats.updated, stats.cancelled),
                            revision: 0, before: None, after: None,
                        }).await?;
                    }
                    finish(state, job).await
                }
                Err(e) => fail(state, job, &e.to_string(), true, None).await,
            }
        }
        "retention" => {
            crate::connectors::apply_retention(state).await?;
            sqlx::query("DELETE FROM jobs WHERE status IN ('done','dead') AND updated_at < ?").bind(ts(state.now() - Duration::days(14))).execute(&state.db).await?;
            sqlx::query("DELETE FROM oauth_states WHERE created_at < ?").bind(ts(state.now() - Duration::hours(1))).execute(&state.db).await?;
            finish(state, job).await
        }
        other => fail(state, job, &format!("unknown job kind {other}"), false, None).await,
    }
}

/// Enqueues work that is due. Time-bucketed idempotency keys mean that
/// running the scheduler twice in the same window does not duplicate jobs.
pub async fn schedule_due(state: &AppState) -> Result<u32> {
    let now = state.now();
    let mut n = 0;
    let connectors: Vec<(String, Option<String>, i64, i64, String)> =
        sqlx::query_as("SELECT id, last_run_at, poll_minutes, consecutive_failures, status FROM connectors WHERE enabled = 1").fetch_all(&state.db).await?;
    for (id, last, poll, failures, status) in connectors {
        if status == "needs_auth" || status == "error" {
            continue;
        }
        let poll = poll.clamp(5, 24 * 60);
        // Back off polling after repeated failures.
        let factor = 2i64.pow(failures.clamp(0, 5) as u32);
        let due = last.map(|l| parse_ts(&l) + Duration::minutes(poll * factor) <= now).unwrap_or(true);
        if due {
            let bucket = now.timestamp() / (poll * 60);
            if enqueue(state, "connector_sync", json!({"connectorId": id}), Some(&format!("sync:{id}:{bucket}")), now).await? {
                n += 1;
            }
        }
    }
    let sources: Vec<(String, Option<String>, i64)> =
        sqlx::query_as("SELECT id, last_fetched_at, refresh_minutes FROM calendar_sources WHERE kind = 'url' AND enabled = 1").fetch_all(&state.db).await?;
    for (id, last, every) in sources {
        let every = every.clamp(15, 24 * 60);
        let due = last.map(|l| parse_ts(&l) + Duration::minutes(every) <= now).unwrap_or(true);
        if due {
            let bucket = now.timestamp() / (every * 60);
            if enqueue(state, "calendar_refresh", json!({"sourceId": id}), Some(&format!("cal:{id}:{bucket}")), now).await? {
                n += 1;
            }
        }
    }
    if enqueue(state, "retention", json!({}), Some(&format!("retention:{}", now.format("%Y-%m-%d"))), now).await? {
        n += 1;
    }
    Ok(n)
}

/// Schedules due work, then drains up to `max_jobs` ready jobs.
pub async fn run_once(state: &AppState, max_jobs: usize) -> Result<usize> {
    schedule_due(state).await?;
    let mut done = 0;
    while done < max_jobs {
        let Some(job) = claim(state).await? else { break };
        if let Err(e) = run_job(state, &job).await {
            tracing::warn!(job = %job.id, kind = %job.kind, error = %tendly_core::redact::redact(&format!("{e:#}")), "job crashed");
            let _ = fail(state, &job, &e.to_string(), true, None).await;
        }
        done += 1;
    }
    Ok(done)
}

pub async fn run_forever(state: AppState, interval: std::time::Duration) {
    loop {
        match run_once(&state, 50).await {
            Ok(n) if n > 0 => tracing::info!(jobs = n, "worker processed jobs"),
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %tendly_core::redact::redact(&format!("{e:#}")), "worker cycle failed"),
        }
        tokio::time::sleep(interval).await;
    }
}
