//! The rules, in the `automations` table, and the master switch, in `app_settings`.

use chrono::{DateTime, Utc};
use metocast_core::automations::{
    Automation, AutomationAction, AutomationDraft, AutomationTrigger, MAX_AUTOMATIONS,
};
use sqlx::PgPool;
use uuid::Uuid;

const ARMED_KEY: &str = "automations_enabled";

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// A limit the rule breaks, in words.
    #[error("{0}")]
    Invalid(String),
    #[error("not_found")]
    NotFound,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

#[derive(sqlx::FromRow)]
struct Row {
    id: Uuid,
    name: String,
    enabled: bool,
    triggers: serde_json::Value,
    actions: serde_json::Value,
    cooldown_seconds: f32,
    require_event: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

const COLUMNS: &str = "id, name, enabled, triggers, actions, cooldown_seconds, require_event, \
                       created_at, updated_at";

impl Row {
    /// A row whose JSON no longer fits the model (written by a later version, say) is not a
    /// reason to lose the rest.
    fn into_automation(self) -> Option<Automation> {
        let triggers = serde_json::from_value::<Vec<AutomationTrigger>>(self.triggers);
        let actions = serde_json::from_value::<Vec<AutomationAction>>(self.actions);
        match (triggers, actions) {
            (Ok(triggers), Ok(actions)) => Some(Automation {
                id: self.id,
                name: self.name,
                enabled: self.enabled,
                triggers,
                actions,
                cooldown_seconds: self.cooldown_seconds,
                require_event: self.require_event,
                created_at: self.created_at,
                updated_at: self.updated_at,
            }),
            (triggers, actions) => {
                tracing::warn!(
                    automation = %self.id,
                    name = %self.name,
                    triggers = ?triggers.err(),
                    actions = ?actions.err(),
                    "skipping an automation that no longer decodes"
                );
                None
            }
        }
    }
}

/// Every rule that still decodes, oldest first.
pub async fn list(pool: &PgPool) -> Result<Vec<Automation>, sqlx::Error> {
    let rows: Vec<Row> = sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM automations ORDER BY created_at, id"
    ))
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().filter_map(Row::into_automation).collect())
}

fn invalid(error: String) -> StoreError {
    StoreError::Invalid(error)
}

pub async fn create(pool: &PgPool, draft: &AutomationDraft) -> Result<Automation, StoreError> {
    draft.validate().map_err(invalid)?;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM automations")
        .fetch_one(pool)
        .await?;
    if usize::try_from(count).unwrap_or(usize::MAX) >= MAX_AUTOMATIONS {
        return Err(invalid(format!("at most {MAX_AUTOMATIONS} automations")));
    }
    let row: Row = sqlx::query_as(&format!(
        "INSERT INTO automations \
         (name, enabled, triggers, actions, cooldown_seconds, require_event) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING {COLUMNS}"
    ))
    .bind(draft.name.trim())
    .bind(draft.enabled)
    .bind(sqlx::types::Json(&draft.triggers))
    .bind(sqlx::types::Json(&draft.actions))
    .bind(draft.cooldown_seconds)
    .bind(draft.require_event)
    .fetch_one(pool)
    .await?;
    row.into_automation()
        .ok_or_else(|| invalid("the rule could not be read back".to_string()))
}

pub async fn replace(
    pool: &PgPool,
    id: Uuid,
    draft: &AutomationDraft,
) -> Result<Automation, StoreError> {
    draft.validate().map_err(invalid)?;
    let row: Option<Row> = sqlx::query_as(&format!(
        "UPDATE automations SET name = $2, enabled = $3, triggers = $4, actions = $5, \
         cooldown_seconds = $6, require_event = $7, updated_at = now() \
         WHERE id = $1 RETURNING {COLUMNS}"
    ))
    .bind(id)
    .bind(draft.name.trim())
    .bind(draft.enabled)
    .bind(sqlx::types::Json(&draft.triggers))
    .bind(sqlx::types::Json(&draft.actions))
    .bind(draft.cooldown_seconds)
    .bind(draft.require_event)
    .fetch_optional(pool)
    .await?;
    row.ok_or(StoreError::NotFound)?
        .into_automation()
        .ok_or_else(|| invalid("the rule could not be read back".to_string()))
}

pub async fn delete(pool: &PgPool, id: Uuid) -> Result<(), StoreError> {
    sqlx::query_scalar::<_, Uuid>("DELETE FROM automations WHERE id = $1 RETURNING id")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .map(drop)
        .ok_or(StoreError::NotFound)
}

/// Whether automations may fire. On unless someone turned it off, also when the row is gone.
pub async fn armed(pool: &PgPool) -> bool {
    sqlx::query_scalar::<_, String>("SELECT value FROM app_settings WHERE key = $1")
        .bind(ARMED_KEY)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .and_then(|value| value.parse().ok())
        .unwrap_or(true)
}

pub async fn set_armed(pool: &PgPool, armed: bool) -> anyhow::Result<()> {
    crate::database::settings::set_json(pool, ARMED_KEY, &armed).await
}
