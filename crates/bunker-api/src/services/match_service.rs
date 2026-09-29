use bunker_models::{Handle, MatchLog, PageQuery};

use crate::storage::{MatchStorage, PlayerStorage};

use super::error::ServiceError;

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
        let player = self
            .players
            .get_by_handle(handle)
            .await?
            .ok_or_else(|| ServiceError::PlayerNotFound(handle.clone()))?;

        Ok(MatchLog {
            record: self.storage.record(player.id).await?,
            nemesis: self.storage.nemesis(player.id).await?,
            matches: self.storage.history(player.id, query).await?,
        })
    }
}
