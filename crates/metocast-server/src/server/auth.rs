use axum::{
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::Next,
    response::Response,
};

use crate::server::AppState;

pub async fn auth_middleware(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string());

    let Some(provided) = token else {
        return Err(StatusCode::UNAUTHORIZED);
    };

    let current = state.auth_token.read().await;
    if provided != *current {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(next.run(req).await)
}
