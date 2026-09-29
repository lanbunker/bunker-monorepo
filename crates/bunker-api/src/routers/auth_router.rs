use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use bunker_models::{LoginRequest, SignupRequest, TokenResponse};

use crate::internal::http::{ApiError, ApiErrorBody, ValidJson, no_store};
use crate::services::AuthService;

use super::AppState;
use super::responses::BodyErrors;

pub fn auth_router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/signup", post(signup))
        .route("/api/auth/login", post(login))
        .route_layer(no_store())
}

#[utoipa::path(
    post,
    path = "/api/auth/signup",
    tag = "auth",
    request_body = SignupRequest,
    responses(
        BodyErrors,
        (status = 201, body = TokenResponse, description = "Logged in. Sent with `Cache-Control: no-store`"),
        (status = 409, body = ApiErrorBody),
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
        BodyErrors,
        (status = 200, body = TokenResponse, description = "Sent with `Cache-Control: no-store`"),
        (status = 401, body = ApiErrorBody),
    )
)]
pub(super) async fn login(
    State(auth): State<AuthService>,
    ValidJson(request): ValidJson<LoginRequest>,
) -> Result<Json<TokenResponse>, ApiError> {
    Ok(Json(auth.login(request).await?))
}
