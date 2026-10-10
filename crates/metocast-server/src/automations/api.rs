//! The REST surface of automations. The same data goes out over the WebSocket as
//! `automations.changed` and `automation.run`.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use metocast_core::automations::AutomationDraft;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use super::store::StoreError;
use crate::server::AppState;

fn refused(error: StoreError) -> Response {
    match error {
        StoreError::Invalid(message) => {
            (StatusCode::BAD_REQUEST, Json(json!({ "error": message }))).into_response()
        }
        StoreError::NotFound => StatusCode::NOT_FOUND.into_response(),
        StoreError::Database(error) => {
            tracing::error!("automations: {error}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// `GET /api/automations`: the rules and the master switch.
pub async fn list(State(state): State<AppState>) -> impl IntoResponse {
    Json(state.automations.snapshot().await)
}

/// `POST /api/automations`
pub async fn create(State(state): State<AppState>, Json(draft): Json<AutomationDraft>) -> Response {
    match state.automations.create(&draft).await {
        Ok(created) => (StatusCode::CREATED, Json(created)).into_response(),
        Err(error) => refused(error),
    }
}

/// `PUT /api/automations/{id}`: replaces the whole rule.
pub async fn replace(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(draft): Json<AutomationDraft>,
) -> Response {
    match state.automations.replace(id, &draft).await {
        Ok(replaced) => Json(replaced).into_response(),
        Err(error) => refused(error),
    }
}

/// `DELETE /api/automations/{id}`
pub async fn delete(State(state): State<AppState>, Path(id): Path<Uuid>) -> Response {
    match state.automations.delete(id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => refused(error),
    }
}

/// `POST /api/automations/{id}/run`: runs the rule's actions now, whatever its cooldown, its
/// event check and the master switch say, and answers with the run.
pub async fn run(State(state): State<AppState>, Path(id): Path<Uuid>) -> Response {
    match state.automations.run_now(id).await {
        Some(finished) => Json(finished).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// `GET /api/automations/runs`: the last 50 runs, newest first.
pub async fn runs(State(state): State<AppState>) -> impl IntoResponse {
    Json(state.automations.runs())
}

#[derive(Deserialize)]
pub struct Armed {
    armed: bool,
}

/// `PUT /api/automations/armed`: the master switch.
pub async fn set_armed(State(state): State<AppState>, Json(body): Json<Armed>) -> Response {
    match state.automations.set_armed(body.armed).await {
        Ok(()) => Json(json!({ "armed": body.armed })).into_response(),
        Err(error) => {
            tracing::error!("automations: cannot save the master switch: {error}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
