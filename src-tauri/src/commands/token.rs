use std::sync::Arc;
use tauri::State;
use tauri_plugin_store::StoreExt;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::AppRuntime;

#[tauri::command]
pub async fn get_token(runtime: State<'_, Arc<RwLock<AppRuntime>>>) -> Result<String, String> {
    let auth_token = {
        let rt = runtime.read().await;
        rt.auth_token.clone()
    };
    let token = auth_token.read().await.clone();
    Ok(token)
}

#[tauri::command]
pub async fn refresh_token(
    runtime: State<'_, Arc<RwLock<AppRuntime>>>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let new_token = Uuid::new_v4().to_string();

    {
        let rt = runtime.read().await;
        let mut token = rt.auth_token.write().await;
        *token = new_token.clone();
    }

    let store = app.store("app-settings.json").map_err(|e| e.to_string())?;
    store.set("auth_token", serde_json::Value::String(new_token.clone()));
    store.save().map_err(|e| e.to_string())?;

    Ok(new_token)
}
