use axum::routing::get;
use axum::{Json, Router};
use bunker_models::{
    Account, Bracket, Description, Entrant, EntrantAdd, EntrantId, GameMode, GameName, Glyph,
    GlyphBits, GlyphColor, Handle, HandleChange, LoginRequest, Match, MatchId, MatchResult,
    NewTournament, Paginated, Password, PasswordChange, Player, PlayerId, Registrations, Role,
    RoleUpdate, SeedOrder, SignupRequest, StatusChange, TemporaryPassword, TokenResponse,
    Tournament, TournamentDetail, TournamentId, TournamentName, TournamentStatus, TournamentUpdate,
};
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};

use crate::internal::http::ApiErrorBody;
use crate::services::ErrorCode;

use super::{
    AppState, admin_router, admin_tournament_router, auth_router, health_router, player_router,
    tournament_router,
};

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
        player_router::change_handle,
        player_router::list_players,
        player_router::get_player,
        admin_router::list_players,
        admin_router::set_role,
        admin_router::rename_player,
        admin_router::reset_password,
        admin_router::delete_player,
        tournament_router::list_tournaments,
        tournament_router::get_tournament,
        tournament_router::registrations,
        tournament_router::register,
        tournament_router::retire,
        admin_tournament_router::list_tournaments,
        admin_tournament_router::create_tournament,
        admin_tournament_router::get_tournament,
        admin_tournament_router::update_tournament,
        admin_tournament_router::delete_tournament,
        admin_tournament_router::change_status,
        admin_tournament_router::add_entrant,
        admin_tournament_router::remove_entrant,
        admin_tournament_router::generate_bracket,
        admin_tournament_router::reorder_seeds,
        admin_tournament_router::delete_bracket,
        admin_tournament_router::report_result,
        admin_tournament_router::clear_result,
        health_router::live,
        health_router::ready,
    ),
    components(schemas(
        Account,
        ApiErrorBody,
        Bracket,
        Description,
        Entrant,
        EntrantAdd,
        EntrantId,
        GameMode,
        GameName,
        Match,
        MatchId,
        MatchResult,
        NewTournament,
        Registrations,
        SeedOrder,
        StatusChange,
        Tournament,
        TournamentDetail,
        TournamentId,
        TournamentName,
        TournamentStatus,
        TournamentUpdate,
        ErrorCode,
        Glyph,
        GlyphBits,
        GlyphColor,
        Handle,
        HandleChange,
        LoginRequest,
        Paginated<Player>,
        Paginated<Tournament>,
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
        (name = "tournaments", description = "Tournaments, entrants and brackets"),
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
