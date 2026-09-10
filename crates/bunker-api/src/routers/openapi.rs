use axum::routing::get;
use axum::{Json, Router};
use bunker_models::{
    Account, Glyph, GlyphBits, GlyphColor, Handle, LoginRequest, Paginated, Password,
    PasswordChange, Player, PlayerId, Role, RoleUpdate, SignupRequest, TemporaryPassword,
    TokenResponse,
};
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};

use crate::internal::http::ApiErrorBody;
use crate::services::ErrorCode;

use super::{AppState, admin_router, auth_router, health_router, player_router};

/// The whole contract. `make openapi` writes it to `openapi.json`, and the site
/// generates its TypeScript types from that file.
#[derive(OpenApi)]
#[openapi(
    info(title = "LAN BUNKER API", version = env!("CARGO_PKG_VERSION")),
    paths(
        auth_router::signup,
        auth_router::login,
        player_router::me,
        player_router::change_password,
        player_router::list_players,
        player_router::get_player,
        admin_router::list_players,
        admin_router::set_role,
        admin_router::reset_password,
        admin_router::delete_player,
        health_router::live,
        health_router::ready,
    ),
    components(schemas(
        Account,
        ApiErrorBody,
        ErrorCode,
        Glyph,
        GlyphBits,
        GlyphColor,
        Handle,
        LoginRequest,
        Paginated<Player>,
        Password,
        PasswordChange,
        Player,
        PlayerId,
        Role,
        RoleUpdate,
        SignupRequest,
        TemporaryPassword,
        TokenResponse,
    )),
    modifiers(&BearerAuth),
    tags(
        (name = "auth", description = "Signup and login"),
        (name = "players", description = "Public player data"),
        (name = "admin", description = "Backoffice, admins only"),
        (name = "health", description = "Liveness and readiness"),
    )
)]
#[derive(Debug)]
pub struct ApiDoc;

struct BearerAuth;

impl Modify for BearerAuth {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "bearer",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .build(),
            ),
        );
    }
}

pub fn openapi_router() -> Router<AppState> {
    Router::new().route("/api/openapi.json", get(document))
}

async fn document() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}
