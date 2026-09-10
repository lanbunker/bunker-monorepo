use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use bunker_models::{LoginRequest, SignupRequest, TokenResponse};

use crate::internal::http::{ApiError, ApiErrorBody, ValidJson};
use crate::services::AuthService;

use super::AppState;

pub fn auth_router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/signup", post(signup))
        .route("/api/auth/login", post(login))
}

#[utoipa::path(
    post,
    path = "/api/auth/signup",
    tag = "auth",
    request_body = SignupRequest,
    responses(
        (status = 201, body = TokenResponse),
        (status = 409, body = ApiErrorBody),
        (status = 422, body = ApiErrorBody),
    )
)]
pub(super) async fn signup(
    State(auth): State<AuthService>,
    ValidJson(request): ValidJson<SignupRequest>,
) -> Result<(StatusCode, Json<TokenResponse>), ApiError> {
    let token = auth.signup(request).await?;

    Ok((StatusCode::CREATED, Json(token)))
}

#[utoipa::path(
    post,
    path = "/api/auth/login",
    tag = "auth",
    request_body = LoginRequest,
    responses(
        (status = 200, body = TokenResponse),
        (status = 401, body = ApiErrorBody),
        (status = 422, body = ApiErrorBody),
    )
)]
pub(super) async fn login(
    State(auth): State<AuthService>,
    ValidJson(request): ValidJson<LoginRequest>,
) -> Result<Json<TokenResponse>, ApiError> {
    Ok(Json(auth.login(request).await?))
}
