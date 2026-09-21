use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceListener {
    pub id: Uuid,
    pub connector_type: String,
    pub category: String,
    pub device_item_value: String,
    pub device_item_name: String,
    pub friendly_name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
