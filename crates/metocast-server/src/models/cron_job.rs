use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

// ── Internal DB row (cron_jobs table only) ────────────────────────────────────

#[derive(sqlx::FromRow)]
struct CronJobRow {
    pub id: Uuid,
    pub name: String,
    pub cron_expression: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ── Public API type (joins features table) ────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CronJob {
    pub id: Uuid,
    pub name: String,
    pub cron_expression: String,
    pub enabled: bool,
    pub pull_youtube: bool,
    pub auto_upload: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ── Request bodies ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCronJob {
    pub name: String,
    pub cron_expression: String,
    pub enabled: bool,
    pub pull_youtube: bool,
    pub auto_upload: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCronJob {
    pub name: String,
    pub cron_expression: String,
    pub enabled: bool,
    pub pull_youtube: bool,
    pub auto_upload: bool,
}

// ── DB helpers ────────────────────────────────────────────────────────────────

/// Load all cron jobs, joining with the features table to populate boolean flags.
pub async fn list_all(pool: &PgPool) -> anyhow::Result<Vec<CronJob>> {
    let rows = sqlx::query_as::<_, CronJobRow>(
        "SELECT id, name, cron_expression, enabled, created_at, updated_at \
         FROM cron_jobs ORDER BY created_at",
    )
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        return Ok(vec![]);
    }

    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();

    #[derive(sqlx::FromRow)]
    struct FeatureRow {
        cron_job_id: Uuid,
        feature: String,
    }

    let features = sqlx::query_as::<_, FeatureRow>(
        "SELECT cron_job_id, feature FROM cron_job_features WHERE cron_job_id = ANY($1)",
    )
    .bind(&ids)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let pull_youtube = features
                .iter()
                .any(|f| f.cron_job_id == r.id && f.feature == "pull_youtube");
            let auto_upload = features
                .iter()
                .any(|f| f.cron_job_id == r.id && f.feature == "auto_upload");
            CronJob {
                id: r.id,
                name: r.name,
                cron_expression: r.cron_expression,
                enabled: r.enabled,
                pull_youtube,
                auto_upload,
                created_at: r.created_at,
                updated_at: r.updated_at,
            }
        })
        .collect())
}

/// Upsert the feature rows for a job based on the boolean flags.
pub async fn sync_features(
    executor: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    job_id: Uuid,
    pull_youtube: bool,
    auto_upload: bool,
) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM cron_job_features WHERE cron_job_id = $1")
        .bind(job_id)
        .execute(&mut **executor)
        .await?;

    if pull_youtube {
        sqlx::query(
            "INSERT INTO cron_job_features (cron_job_id, feature) VALUES ($1, 'pull_youtube')",
        )
        .bind(job_id)
        .execute(&mut **executor)
        .await?;
    }

    if auto_upload {
        sqlx::query(
            "INSERT INTO cron_job_features (cron_job_id, feature) VALUES ($1, 'auto_upload')",
        )
        .bind(job_id)
        .execute(&mut **executor)
        .await?;
    }

    Ok(())
}
