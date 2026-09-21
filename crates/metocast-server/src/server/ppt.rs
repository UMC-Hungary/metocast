use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::server::{AppState, websocket};

// ── Folder management ────────────────────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct PptFolder {
    pub id: Uuid,
    pub path: String,
    pub name: String,
    pub sort_order: i32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddFolderBody {
    pub path: String,
    pub name: String,
}

pub async fn list_folders(State(state): State<AppState>) -> impl IntoResponse {
    match sqlx::query_as::<_, PptFolder>(
        "SELECT id, path, name, sort_order FROM ppt_folders ORDER BY sort_order, name",
    )
    .fetch_all(&state.pool)
    .await
    {
        Ok(folders) => (
            StatusCode::OK,
            Json(json!({ "success": true, "data": folders })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "success": false, "error": e.to_string() })),
        ),
    }
}

pub async fn add_folder(
    State(state): State<AppState>,
    Json(body): Json<AddFolderBody>,
) -> impl IntoResponse {
    let path = body.path.trim();
    if !std::fs::metadata(path).is_ok_and(|m| m.is_dir()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "success": false,
                "error": format!("No such folder on the server: {path}")
            })),
        );
    }

    let result = sqlx::query_as::<_, PptFolder>(
        "INSERT INTO ppt_folders (path, name) VALUES ($1, $2) \
         ON CONFLICT (path) DO UPDATE SET name = EXCLUDED.name \
         RETURNING id, path, name, sort_order",
    )
    .bind(path)
    .bind(&body.name)
    .fetch_one(&state.pool)
    .await;

    match result {
        Ok(folder) => {
            websocket::broadcast_ppt_folders_changed(&state.ws_clients).await;
            (
                StatusCode::CREATED,
                Json(json!({ "success": true, "data": folder })),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "success": false, "error": e.to_string() })),
        ),
    }
}

pub async fn remove_folder(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    match sqlx::query("DELETE FROM ppt_folders WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await
    {
        Ok(_) => {
            websocket::broadcast_ppt_folders_changed(&state.ws_clients).await;
            (StatusCode::OK, Json(json!({ "success": true })))
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "success": false, "error": e.to_string() })),
        ),
    }
}

// ── File search ───────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PptFile {
    pub id: String,
    pub name: String,
    pub path: String,
    pub folder_id: String,
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub filter: Option<String>,
}

/// Internal search helper used by both the HTTP handler and WS command handler.
pub async fn search_files_internal(pool: &sqlx::PgPool, filter: &str) -> Vec<PptFile> {
    let folders = match sqlx::query_as::<_, PptFolder>(
        "SELECT id, path, name, sort_order FROM ppt_folders ORDER BY sort_order, name",
    )
    .fetch_all(pool)
    .await
    {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };

    let filter_lower = filter.to_lowercase();
    let mut scored: Vec<(i32, PptFile)> = Vec::new();

    for folder in &folders {
        let dir = match std::fs::read_dir(&folder.path) {
            Ok(d) => d,
            Err(_) => continue,
        };

        for entry in dir.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase())
                .unwrap_or_default();
            if ext != "ppt" && ext != "pptx" {
                continue;
            }

            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_lowercase();

            let score = if filter_lower.is_empty() {
                1
            } else if stem.starts_with(&filter_lower) {
                2
            } else if stem.contains(&filter_lower) {
                1
            } else {
                continue;
            };

            let file_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            let file_path = path.to_string_lossy().to_string();

            scored.push((
                score,
                PptFile {
                    id: file_path.clone(),
                    name: file_name,
                    path: file_path,
                    folder_id: folder.id.to_string(),
                },
            ));
        }
    }

    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
    scored.into_iter().take(5).map(|(_, f)| f).collect()
}

pub async fn search_files(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> impl IntoResponse {
    let filter = query.filter.as_deref().unwrap_or("");
    let files = search_files_internal(&state.pool, filter).await;
    (
        StatusCode::OK,
        Json(json!({ "success": true, "data": files })),
    )
}

// ── Keynote control (macOS only) ─────────────────────────────────────────────

#[cfg(target_os = "macos")]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenBody {
    pub file_path: String,
}

#[cfg(target_os = "macos")]
#[derive(Debug, Deserialize)]
pub struct GotoBody {
    pub slide: u32,
}

#[cfg(target_os = "macos")]
pub async fn keynote_status(State(state): State<AppState>) -> impl IntoResponse {
    let status = state.keynote_connector.get_status().await;
    (
        StatusCode::OK,
        Json(json!({ "success": true, "data": status })),
    )
}

#[cfg(target_os = "macos")]
pub async fn keynote_open(
    State(state): State<AppState>,
    Json(body): Json<OpenBody>,
) -> impl IntoResponse {
    match state.keynote_connector.open_file(&body.file_path).await {
        Ok(()) => (StatusCode::OK, Json(json!({ "success": true }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "success": false, "error": e })),
        ),
    }
}

#[cfg(target_os = "macos")]
pub async fn keynote_next(State(state): State<AppState>) -> impl IntoResponse {
    match state.keynote_connector.next().await {
        Ok(()) => (StatusCode::OK, Json(json!({ "success": true }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "success": false, "error": e })),
        ),
    }
}

#[cfg(target_os = "macos")]
pub async fn keynote_prev(State(state): State<AppState>) -> impl IntoResponse {
    match state.keynote_connector.prev().await {
        Ok(()) => (StatusCode::OK, Json(json!({ "success": true }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "success": false, "error": e })),
        ),
    }
}

#[cfg(target_os = "macos")]
pub async fn keynote_first(State(state): State<AppState>) -> impl IntoResponse {
    match state.keynote_connector.first().await {
        Ok(()) => (StatusCode::OK, Json(json!({ "success": true }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "success": false, "error": e })),
        ),
    }
}

#[cfg(target_os = "macos")]
pub async fn keynote_last(State(state): State<AppState>) -> impl IntoResponse {
    match state.keynote_connector.last().await {
        Ok(()) => (StatusCode::OK, Json(json!({ "success": true }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "success": false, "error": e })),
        ),
    }
}

#[cfg(target_os = "macos")]
pub async fn keynote_goto(
    State(state): State<AppState>,
    Json(body): Json<GotoBody>,
) -> impl IntoResponse {
    match state.keynote_connector.goto(body.slide).await {
        Ok(()) => (StatusCode::OK, Json(json!({ "success": true }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "success": false, "error": e })),
        ),
    }
}

#[cfg(target_os = "macos")]
pub async fn keynote_start(State(state): State<AppState>) -> impl IntoResponse {
    match state.keynote_connector.start_slideshow().await {
        Ok(()) => (StatusCode::OK, Json(json!({ "success": true }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "success": false, "error": e })),
        ),
    }
}

#[cfg(target_os = "macos")]
pub async fn keynote_stop(State(state): State<AppState>) -> impl IntoResponse {
    match state.keynote_connector.stop_slideshow().await {
        Ok(()) => (StatusCode::OK, Json(json!({ "success": true }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "success": false, "error": e })),
        ),
    }
}

#[cfg(target_os = "macos")]
pub async fn keynote_close_all(State(state): State<AppState>) -> impl IntoResponse {
    match state.keynote_connector.close_all().await {
        Ok(()) => (StatusCode::OK, Json(json!({ "success": true }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "success": false, "error": e })),
        ),
    }
}

#[cfg(not(target_os = "macos"))]
pub async fn keynote_not_implemented() -> impl IntoResponse {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(json!({ "success": false, "error": "Keynote is only available on macOS" })),
    )
}
