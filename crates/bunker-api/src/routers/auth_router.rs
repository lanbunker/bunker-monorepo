use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use bunker_models::{LoginRequest, SignupRequest, TokenResponse};

use crate::internal::http::{ApiError, ValidJson};
use crate::services::AuthService;

use super::AppState;

pub fn auth_router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/signup", post(signup))
        .route("/api/auth/login", post(login))
}

async fn signup(
    State(auth): State<AuthService>,
    ValidJson(request): ValidJson<SignupRequest>,
) -> Result<(StatusCode, Json<TokenResponse>), ApiError> {
    let token = auth.signup(request).await?;

    Ok((StatusCode::CREATED, Json(token)))
}

async fn login(
    State(auth): State<AuthService>,
    ValidJson(request): ValidJson<LoginRequest>,
) -> Result<Json<TokenResponse>, ApiError> {
    Ok(Json(auth.login(request).await?))
}
