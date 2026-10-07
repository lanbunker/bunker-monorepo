use bunker_models::{
    Adjustment, CyclesLog, Handle, PageQuery, PlayerId, PointEntry, PointEntryId, PointKind,
};
use time::OffsetDateTime;

use crate::storage::{NewAdjustment, PlayerStorage, PointStorage, StorageError};

use super::error::ServiceError;
use super::player_service::public_by_handle;

/// The rules of the ledger: who may write to it by hand. The tournament awards
/// live in `TournamentService`, because they are part of concluding a
/// tournament.
#[derive(Debug, Clone)]
pub struct PointsService {
    storage: PointStorage,
    players: PlayerStorage,
}

impl PointsService {
    pub const fn new(storage: PointStorage, players: PlayerStorage) -> Self {
        Self { storage, players }
    }

    /// An admin gives or takes cycles, with a reason, to any player, themself
    /// included. The entry names the admin, so the ledger shows who wrote it.
    pub async fn adjust(
        &self,
        admin: PlayerId,
        player: PlayerId,
        adjustment: Adjustment,
    ) -> Result<PointEntry, ServiceError> {
        if !self.players.exists(player).await? {
            return Err(ServiceError::PlayerIdNotFound(player));
        }

        let written = self
            .storage
            .add_adjustment(&NewAdjustment {
                id: PointEntryId::generate(),
                player,
                amount: adjustment.amount,
                note: adjustment.note,
                created_by: admin,
                created_at: OffsetDateTime::now_utc(),
            })
            .await;
        match written {
            Ok(entry) => Ok(entry),
            // The player was deleted after the check above.
            Err(StorageError::ForeignKeyViolation(_)) => {
                Err(ServiceError::PlayerIdNotFound(player))
            }
            Err(error) => Err(error.into()),
        }
    }

    /// The history of a player, newest first, with the totals by kind. The
    /// note of an adjustment is public: an admin writes it for everyone.
    pub async fn history(
        &self,
        handle: &Handle,
        query: PageQuery,
    ) -> Result<CyclesLog, ServiceError> {
        let player = public_by_handle(&self.players, handle).await?;

        self.history_of(player.id, query).await
    }

    /// The same history for the caller's own account, active or not.
    pub async fn history_of(
        &self,
        player: PlayerId,
        query: PageQuery,
    ) -> Result<CyclesLog, ServiceError> {
        let entries = self.storage.history(player, query).await?;
        let mut totals = self.storage.totals(player).await?;
        // The legend lists the kinds in this order, so the breakdown does too.
        totals.sort_by_key(|total| PointKind::ALL.iter().position(|k| *k == total.kind));

        Ok(CyclesLog { entries, totals })
    }
}
