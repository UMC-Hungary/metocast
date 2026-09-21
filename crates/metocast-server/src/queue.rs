//! Durable, SQS-style job queue backed by the `job_queue` table.
//!
//! Producers `enqueue`; a single worker `claim_batch`es with `FOR UPDATE SKIP
//! LOCKED`, retries failures with exponential backoff, and drops exhausted jobs
//! into the `dead` state (DLQ). Every mutation NOTIFYs `queue_changed`, which
//! both wakes the worker and feeds the Queues dashboard.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::time::Duration;
use uuid::Uuid;

use crate::connectors::{ConnectorStatus, youtube};
use crate::models::event::{Event, fetch_event};
use crate::server::AppState;
use crate::server::websocket::{broadcast_event_changed, write_youtube_result};

/// The only queue in use today; the table is queue-name generic.
pub const PLATFORM_SYNC: &str = "platform_sync";

const MAX_ATTEMPTS: i32 = 5;
const VISIBILITY_TIMEOUT_SECS: i64 = 300;
const TICK: Duration = Duration::from_secs(15);
const BATCH: i64 = 10;
const PRUNE_EVERY: Duration = Duration::from_secs(3600);
const RETAIN_SUCCEEDED_DAYS: i32 = 7;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: Uuid,
    pub queue: String,
    pub job_type: String,
    pub dedup_key: Option<String>,
    pub payload: Value,
    pub status: String,
    pub attempts: i32,
    pub max_attempts: i32,
    pub available_at: DateTime<Utc>,
    pub locked_at: Option<DateTime<Utc>>,
    pub locked_by: Option<String>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct QueueSummary {
    pub queue: String,
    pub pending: i64,
    pub processing: i64,
    pub succeeded: i64,
    pub dead: i64,
    pub oldest_available_at: Option<DateTime<Utc>>,
}

// ── Table helpers ─────────────────────────────────────────────────────────────

/// Insert a job. Jobs sharing a `dedup_key` coalesce while still pending, so a
/// burst of edits collapses into one run that reads the latest state.
pub async fn enqueue(
    pool: &PgPool,
    queue: &str,
    job_type: &str,
    dedup_key: Option<&str>,
    payload: Value,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"INSERT INTO job_queue (queue, job_type, dedup_key, payload, max_attempts)
           VALUES ($1, $2, $3, $4, $5)
           ON CONFLICT (queue, dedup_key) WHERE status = 'pending' AND dedup_key IS NOT NULL
           DO UPDATE SET payload      = EXCLUDED.payload,
                         job_type     = EXCLUDED.job_type,
                         available_at = now(),
                         updated_at   = now()"#,
    )
    .bind(queue)
    .bind(job_type)
    .bind(dedup_key)
    .bind(payload)
    .bind(MAX_ATTEMPTS)
    .execute(pool)
    .await?;
    Ok(())
}

/// Claim up to `limit` due jobs.
pub async fn claim_batch(
    pool: &PgPool,
    queue: &str,
    worker_id: &str,
    limit: i64,
) -> anyhow::Result<Vec<Job>> {
    let jobs = sqlx::query_as::<_, Job>(
        r#"UPDATE job_queue SET status = 'processing', locked_at = now(), locked_by = $2,
                                attempts = attempts + 1, updated_at = now()
           WHERE id IN (
               SELECT id FROM job_queue
               WHERE queue = $1 AND status = 'pending' AND available_at <= now()
               ORDER BY available_at
               FOR UPDATE SKIP LOCKED
               LIMIT $3
           )
           RETURNING *"#,
    )
    .bind(queue)
    .bind(worker_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(jobs)
}

pub async fn complete(pool: &PgPool, id: Uuid) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE job_queue SET status = 'succeeded', last_error = NULL, locked_at = NULL, \
         locked_by = NULL, updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Reschedule with backoff, or bury in the DLQ once attempts are exhausted.
pub async fn fail(pool: &PgPool, job: &Job, error: &str) -> anyhow::Result<()> {
    if job.attempts >= job.max_attempts {
        sqlx::query(
            "UPDATE job_queue SET status = 'dead', last_error = $2, locked_at = NULL, \
             locked_by = NULL, updated_at = now() WHERE id = $1",
        )
        .bind(job.id)
        .bind(error)
        .execute(pool)
        .await?;
    } else {
        // `dedup_key` is dropped on the way back to 'pending': a job that has
        // already been claimed leaves its coalescing group, so it cannot collide
        // with a newer pending job for the same key.
        sqlx::query(
            "UPDATE job_queue SET status = 'pending', last_error = $2, locked_at = NULL, \
             locked_by = NULL, dedup_key = NULL, \
             available_at = now() + make_interval(secs => $3), \
             updated_at = now() WHERE id = $1",
        )
        .bind(job.id)
        .bind(error)
        .bind(backoff_secs(job.attempts) as f64)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// Exponential backoff, 5s doubling up to a 1h cap.
/// ponytail: no jitter — one worker, so there is nothing to stampede.
fn backoff_secs(attempts: i32) -> i64 {
    let shift = attempts.clamp(1, 16) - 1;
    (5i64 << shift).min(3600)
}

/// SQS visibility timeout: hand back jobs whose worker died mid-flight.
pub async fn reclaim_stuck(pool: &PgPool, queue: &str) -> anyhow::Result<u64> {
    let res = sqlx::query(
        "UPDATE job_queue SET status = 'pending', locked_at = NULL, locked_by = NULL, \
         dedup_key = NULL, updated_at = now() \
         WHERE queue = $1 AND status = 'processing' \
           AND locked_at < now() - make_interval(secs => $2)",
    )
    .bind(queue)
    .bind(VISIBILITY_TIMEOUT_SECS as f64)
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

/// Drop succeeded jobs past their retention window. Without this the table
/// grows a row per sync forever, and `stats` scans all of it on every notify.
/// `dead` rows are kept: a buried job is something someone still has to look at.
pub async fn prune(pool: &PgPool) -> anyhow::Result<u64> {
    let res = sqlx::query(
        "DELETE FROM job_queue WHERE status = 'succeeded' \
         AND updated_at < now() - make_interval(days => $1)",
    )
    .bind(RETAIN_SUCCEEDED_DAYS)
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

pub async fn stats(pool: &PgPool) -> anyhow::Result<Vec<QueueSummary>> {
    let rows = sqlx::query_as::<_, QueueSummary>(
        r#"SELECT queue,
                  count(*) FILTER (WHERE status = 'pending')    AS pending,
                  count(*) FILTER (WHERE status = 'processing') AS processing,
                  count(*) FILTER (WHERE status = 'succeeded')  AS succeeded,
                  count(*) FILTER (WHERE status = 'dead')       AS dead,
                  min(available_at) FILTER (WHERE status = 'pending') AS oldest_available_at
           FROM job_queue GROUP BY queue ORDER BY queue"#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

// ── Worker ────────────────────────────────────────────────────────────────────

/// Runs the `platform_sync` queue until the process exits. Woken instantly by
/// `queue_changed` (via `state.queue_wake`), with a slow tick to pick up
/// backoff-due jobs and reclaim stuck ones.
pub fn spawn_worker(state: AppState) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(TICK);
        let mut last_prune = tokio::time::Instant::now();
        loop {
            run_cycle(&state).await;
            if last_prune.elapsed() >= PRUNE_EVERY {
                last_prune = tokio::time::Instant::now();
                match prune(&state.pool).await {
                    Ok(n) if n > 0 => tracing::info!("queue: pruned {n} succeeded jobs"),
                    Ok(_) => {}
                    Err(e) => tracing::error!("queue: prune failed: {e}"),
                }
            }
            tokio::select! {
                _ = state.queue_wake.notified() => {}
                _ = ticker.tick() => {}
            }
        }
    });
}

async fn run_cycle(state: &AppState) {
    if let Err(e) = reclaim_stuck(&state.pool, PLATFORM_SYNC).await {
        tracing::error!("queue: reclaim_stuck failed: {e}");
    }

    // Logged out of YouTube: leave everything sitting in the queue rather than
    // claiming jobs that cannot succeed. Nothing burns an attempt, nothing
    // reaches the dead-letter, and the next tick after the user logs back in
    // drains the backlog. Every job type in this queue targets YouTube today.
    if !youtube_ready(state).await {
        return;
    }

    loop {
        let jobs = match claim_batch(&state.pool, PLATFORM_SYNC, &state.server_id, BATCH).await {
            Ok(j) if j.is_empty() => return,
            Ok(j) => j,
            Err(e) => {
                tracing::error!("queue: claim_batch failed: {e}");
                return;
            }
        };
        for job in jobs {
            let outcome = handle_job(state, &job).await;
            let res = match outcome {
                Ok(()) => complete(&state.pool, job.id).await,
                Err(e) => {
                    tracing::error!("queue: job {} ({}) failed: {e}", job.id, job.job_type);
                    fail(&state.pool, &job, &e.to_string()).await
                }
            };
            if let Err(e) = res {
                tracing::error!("queue: could not record outcome for {}: {e}", job.id);
            }
        }
    }
}

/// Whether the queue can make progress at all. Checked before claiming so a
/// logged-out connector parks the queue instead of draining it into the DLQ.
async fn youtube_ready(state: &AppState) -> bool {
    matches!(
        state.youtube_connector.get_status().await,
        ConnectorStatus::Connected
    ) && youtube::load_tokens(&state.pool).await.is_some()
}

async fn handle_job(state: &AppState, job: &Job) -> anyhow::Result<()> {
    // Retryable: `youtube_ready` gates the claim, so this only trips when the
    // connector drops between the gate and here.
    let token = youtube::load_tokens(&state.pool)
        .await
        .ok_or_else(|| anyhow::anyhow!("YouTube token disappeared mid-cycle"))?
        .access_token;

    match job.job_type.as_str() {
        "youtube.upsert" => {
            let event_id: Uuid = serde_json::from_value(job.payload["event_id"].clone())?;
            let Some(event) = fetch_event(event_id, &state.pool).await? else {
                return Ok(()); // event deleted while queued — nothing to sync
            };
            if !should_sync(&event) {
                return Ok(());
            }
            let conn = event.connection("youtube");
            let result = youtube::schedule_event(
                &event.id.to_string(),
                event.published_title(),
                &event.date_time,
                &token,
                conn.and_then(|c| c.external_id.as_deref()),
                conn.and_then(|c| c.privacy_status.as_deref())
                    .unwrap_or("private"),
            )
            .await?;
            let updated = write_youtube_result(state, event.id, &result).await?;
            broadcast_event_changed(state, "UPDATE", &updated).await;
            Ok(())
        }
        "youtube.delete" => {
            let broadcast_id = job.payload["broadcast_id"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("missing broadcast_id"))?;
            youtube::delete_broadcast(broadcast_id, &token).await
        }
        other => Err(anyhow::anyhow!("unknown job_type '{other}'")),
    }
}

/// Create a broadcast for a future event that has none yet, or push edits to an
/// existing broadcast that has not gone live or ended.
fn should_sync(event: &Event) -> bool {
    let conn = event.connection("youtube");
    let status = conn
        .map(|c| c.schedule_status.as_str())
        .unwrap_or("not_scheduled");
    if conn.and_then(|c| c.external_id.as_deref()).is_some() {
        matches!(status, "scheduled" | "created" | "ready")
    } else {
        status != "scheduled" && event.date_time > Utc::now()
    }
}

// ── Producers ─────────────────────────────────────────────────────────────────

pub async fn enqueue_youtube_upsert(pool: &PgPool, event_id: Uuid) {
    if let Err(e) = enqueue(
        pool,
        PLATFORM_SYNC,
        "youtube.upsert",
        Some(&format!("youtube:{event_id}")),
        json!({ "event_id": event_id }),
    )
    .await
    {
        tracing::error!("queue: enqueue youtube.upsert for {event_id} failed: {e}");
    }
}

/// Enqueued *before* the event row disappears — the payload carries the
/// broadcast id rather than a foreign key so the job outlives the event.
pub async fn enqueue_youtube_delete(pool: &PgPool, event: &Event) {
    let Some(broadcast_id) = event
        .connection("youtube")
        .and_then(|c| c.external_id.as_deref())
    else {
        return;
    };
    if let Err(e) = enqueue(
        pool,
        PLATFORM_SYNC,
        "youtube.delete",
        None,
        json!({ "broadcast_id": broadcast_id }),
    )
    .await
    {
        tracing::error!("queue: enqueue youtube.delete for {} failed: {e}", event.id);
    }
}

// ── REST handlers ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct JobsQuery {
    status: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

pub async fn list_queues(State(state): State<AppState>) -> impl IntoResponse {
    match stats(&state.pool).await {
        Ok(rows) => Json(rows).into_response(),
        Err(e) => {
            tracing::error!("list_queues: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn list_jobs(
    State(state): State<AppState>,
    Path(queue): Path<String>,
    Query(q): Query<JobsQuery>,
) -> impl IntoResponse {
    let rows = sqlx::query_as::<_, Job>(
        "SELECT * FROM job_queue WHERE queue = $1 AND ($2::text IS NULL OR status = $2) \
         ORDER BY updated_at DESC LIMIT $3 OFFSET $4",
    )
    .bind(&queue)
    .bind(q.status.as_deref())
    .bind(q.limit.unwrap_or(100).clamp(1, 500))
    .bind(q.offset.unwrap_or(0).max(0))
    .fetch_all(&state.pool)
    .await;

    match rows {
        Ok(rows) => Json(rows).into_response(),
        Err(e) => {
            tracing::error!("list_jobs: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Redrive: put a dead (or stuck) job back on the queue with a clean slate.
pub async fn retry_job(State(state): State<AppState>, Path(id): Path<Uuid>) -> impl IntoResponse {
    let res = sqlx::query(
        "UPDATE job_queue SET status = 'pending', attempts = 0, available_at = now(), \
         last_error = NULL, locked_at = NULL, locked_by = NULL, dedup_key = NULL, \
         updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .execute(&state.pool)
    .await;
    affected(res)
}

pub async fn purge_job(State(state): State<AppState>, Path(id): Path<Uuid>) -> impl IntoResponse {
    let res = sqlx::query("DELETE FROM job_queue WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await;
    affected(res)
}

fn affected(res: Result<sqlx::postgres::PgQueryResult, sqlx::Error>) -> axum::response::Response {
    match res {
        Ok(r) if r.rows_affected() == 0 => StatusCode::NOT_FOUND.into_response(),
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            tracing::error!("job mutation: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::backoff_secs;

    #[test]
    fn backoff_doubles_then_caps() {
        assert_eq!(backoff_secs(1), 5);
        assert_eq!(backoff_secs(2), 10);
        assert_eq!(backoff_secs(3), 20);
        assert_eq!(backoff_secs(5), 80);
        assert_eq!(backoff_secs(100), 3600);
        assert_eq!(backoff_secs(0), 5);
    }
}
