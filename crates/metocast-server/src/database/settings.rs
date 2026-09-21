//! JSON-valued rows in `app_settings` — the core's own config storage, so
//! connector settings are readable and writable without Tauri.

use anyhow::Result;
use serde::{Serialize, de::DeserializeOwned};
use sqlx::PgPool;

/// Returns the stored value, or `T::default()` when the key is unset or the
/// stored JSON no longer matches `T`.
pub async fn get_json<T: DeserializeOwned + Default>(pool: &PgPool, key: &str) -> T {
    sqlx::query_scalar::<_, String>("SELECT value FROM app_settings WHERE key = $1")
        .bind(key)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub async fn set_json<T: Serialize>(pool: &PgPool, key: &str, value: &T) -> Result<()> {
    sqlx::query(
        "INSERT INTO app_settings (key, value) VALUES ($1, $2) \
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()",
    )
    .bind(key)
    .bind(serde_json::to_string(value)?)
    .execute(pool)
    .await?;
    Ok(())
}

/// One-time import of a value that used to live in the desktop Tauri store.
/// Existing rows win, so this is a no-op after the first run.
pub async fn seed_json(pool: &PgPool, key: &str, value: &serde_json::Value) -> Result<()> {
    sqlx::query(
        "INSERT INTO app_settings (key, value) VALUES ($1, $2) ON CONFLICT (key) DO NOTHING",
    )
    .bind(key)
    .bind(serde_json::to_string(value)?)
    .execute(pool)
    .await?;
    Ok(())
}
