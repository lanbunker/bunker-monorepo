use bunker_models::{Handle, MatchLog, PageQuery, PlayerId};

use crate::storage::{MatchStorage, PlayerStorage};

use super::error::ServiceError;
use super::player_service::public_by_handle;

/// What the bracket results say about one player. The results themselves are
/// written by `TournamentService`, which owns the bracket.
#[derive(Debug, Clone)]
pub struct MatchService {
    storage: MatchStorage,
    players: PlayerStorage,
}

impl MatchService {
    pub const fn new(storage: MatchStorage, players: PlayerStorage) -> Self {
        Self { storage, players }
    }

    /// The match log of a player, newest first, with the overall record and the
    /// nemesis. Every read goes to the bracket rows, so a corrected result
    /// changes the answer at once.
    pub async fn log(&self, handle: &Handle, query: PageQuery) -> Result<MatchLog, ServiceError> {
        let player = public_by_handle(&self.players, handle).await?;

        self.log_of(player.id, query).await
    }

    /// The same log for the caller's own account, active or not.
    pub async fn log_of(
        &self,
        player: PlayerId,
        query: PageQuery,
    ) -> Result<MatchLog, ServiceError> {
        Ok(MatchLog {
            record: self.storage.record(player).await?,
            nemesis: self.storage.nemesis(player).await?,
            matches: self.storage.history(player, query).await?,
        })
    }
}
