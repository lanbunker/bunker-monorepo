//! Domain types shared by the API, the cabinet daemon and every future Rust
//! client. No I/O, no HTTP, no SQL.
//!
//! `nutype` declares each constraint one time and makes the constructor, the
//! error type and the serde code. A value that crosses a boundary one time is
//! valid everywhere below.

mod auth;
mod bracket;
mod event;
mod glyph;
mod handle;
mod id;
mod pagination;
mod player;
mod points;
mod rivalry;
mod role;
mod schema;
mod tournament;

pub use auth::{
    LoginRequest, PASSWORD_MAX_LEN, PASSWORD_MIN_LEN, Password, PasswordChange, PasswordError,
    SignupRequest, TemporaryPassword, TokenResponse,
};
pub use bracket::{Bracket, BracketError, MAX_ENTRANTS, Match, MatchId};
pub use event::{
    CHECKIN_CODE_LEN, Checkin, CheckinAdd, CheckinCode, CheckinCodeError, CheckinGate,
    CheckinReceipt, CheckinWindow, Checkins, EVENT_NAME_MAX_LEN, Event, EventDetail, EventFields,
    EventId, EventName, EventNameError, EventStatus, EventStatusChange, EventWindow,
    EventWindowError, GAMES_MAX_LEN, Games, GamesError, IMAGE_NAME_MAX_LEN, ImageName,
    ImageNameError, LOCATION_MAX_LEN, Location, LocationError, UnknownEventStatus,
};
pub use glyph::{
    GLYPH_CELLS, GLYPH_COLORS, GLYPH_MASK, GLYPH_SIZE, Glyph, GlyphBits, GlyphBitsError,
    GlyphColor, generate_glyph,
};
pub use handle::{HANDLE_MAX_LEN, HANDLE_MIN_LEN, Handle, HandleChange, HandleError};
pub use pagination::{
    PAGE_SIZE_MAX, PageNumber, PageNumberError, PageQuery, PageSize, PageSizeError, Paginated,
    RosterQuery, SearchTerm, SearchTermError,
};
pub use player::{Account, Player, PlayerId};
pub use points::{
    ADJUSTMENT_MAX, Adjustment, Amount, AmountError, Award, AwardRule, CHECKIN_CYCLES, CyclesLog,
    CyclesRules, ENTRY_CYCLES, FieldTier, KindTotal, MATCH_WIN_CYCLES, NOTE_MAX_LEN, NextRank,
    Note, NoteError, PointEntry, PointEntryId, PointKind, Rank, RankRule, Standing, TierRule,
    UnknownPointKind, checkin_award, tournament_awards,
};
pub use rivalry::{MatchLog, MatchRecord, NEMESIS_MIN_LOSSES, PlayedMatch, Rivalry};
pub use role::{Role, RoleUpdate, UnknownRole};
pub use tournament::{
    DESCRIPTION_MAX_LEN, Description, DescriptionError, Entrant, EntrantAdd, EntrantId,
    GAME_MODE_MAX_LEN, GAME_NAME_MAX_LEN, GameMode, GameModeError, GameName, GameNameError,
    MatchResult, NewTournament, RegistrationRequest, Registrations, SKILL_LEVEL_MAX,
    SKILL_LEVEL_MIN, SeedOrder, SkillLevel, SkillLevelError, StatusChange, TOURNAMENT_NAME_MAX_LEN,
    Tournament, TournamentDetail, TournamentId, TournamentName, TournamentNameError,
    TournamentStatus, TournamentUpdate, UnknownStatus,
};
