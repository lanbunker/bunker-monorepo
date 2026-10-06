use bunker_models::{
    Account, AdminRosterQuery, Handle, Paginated, Player, PlayerId, Role, RosterQuery,
};

use crate::storage::{ListOrder, PlayerStorage, Removal, Renamed, RoleChanged};

use super::error::ServiceError;

#[derive(Debug, Clone)]
pub struct PlayerService {
    storage: PlayerStorage,
}

impl PlayerService {
    pub const fn new(storage: PlayerStorage) -> Self {
        Self { storage }
    }

    /// The public page of a player.
    pub async fn get_by_handle(&self, handle: &Handle) -> Result<Player, ServiceError> {
        public_by_handle(&self.storage, handle).await
    }

    /// What an admin finds behind a handle they typed, active or not.
    pub async fn lookup(&self, handle: &Handle) -> Result<Player, ServiceError> {
        self.storage
            .get_by_handle(handle)
            .await?
            .ok_or_else(|| ServiceError::PlayerNotFound(handle.clone()))
    }

    /// The caller's own account, with what only the owner sees.
    pub async fn account(&self, id: PlayerId) -> Result<Account, ServiceError> {
        self.storage
            .account(id)
            .await?
            .ok_or(ServiceError::PlayerIdNotFound(id))
    }

    /// The public leaderboard: active players only, first place first.
    pub async fn leaderboard(
        &self,
        query: &RosterQuery,
    ) -> Result<Paginated<Player>, ServiceError> {
        Ok(self
            .storage
            .list(query.page(), query.term(), Some(true), ListOrder::Standing)
            .await?)
    }

    /// The backoffice roster: the last signup first.
    pub async fn roster(
        &self,
        query: &AdminRosterQuery,
    ) -> Result<Paginated<Player>, ServiceError> {
        Ok(self
            .storage
            .list(query.page(), query.term(), query.active, ListOrder::Newest)
            .await?)
    }

    /// `actor` is the admin doing it. Nobody changes their own role, and the
    /// crew keeps at least one admin, so nobody locks the crew out.
    pub async fn set_role(
        &self,
        actor: PlayerId,
        id: PlayerId,
        role: Role,
    ) -> Result<Player, ServiceError> {
        if actor == id {
            return Err(ServiceError::SelfAction);
        }
        match self.storage.set_role(id, role).await? {
            RoleChanged::Player(player) => Ok(player),
            RoleChanged::NotFound => Err(ServiceError::PlayerIdNotFound(id)),
            RoleChanged::LastAdmin => Err(ServiceError::LastAdmin),
        }
    }

    /// The glyph stays. A handle that another player holds, in any letter case, is refused.
    pub async fn rename(&self, id: PlayerId, handle: Handle) -> Result<Player, ServiceError> {
        match self.storage.rename(id, &handle).await? {
            Renamed::Player(player) => Ok(player),
            Renamed::HandleTaken => Err(ServiceError::HandleTaken(handle)),
            Renamed::NotFound => Err(ServiceError::PlayerIdNotFound(id)),
        }
    }

    /// Idempotent. A delete of a player that is already absent is a success.
    pub async fn delete(&self, actor: PlayerId, id: PlayerId) -> Result<(), ServiceError> {
        if actor == id {
            return Err(ServiceError::SelfAction);
        }
        match self.storage.delete(id).await? {
            Removal::Removed | Removal::Absent => Ok(()),
            Removal::LastAdmin => Err(ServiceError::LastAdmin),
        }
    }

    pub async fn check_ready(&self) -> Result<(), ServiceError> {
        self.storage.check_reachable().await?;

        Ok(())
    }
}

/// A player anyone may look up. An inactive player answers like an unknown
/// handle, so a signup that never came to an event has no public page. The
/// handle can still show where the player took part, such as an entrant list.
pub(super) async fn public_by_handle(
    storage: &PlayerStorage,
    handle: &Handle,
) -> Result<Player, ServiceError> {
    storage
        .get_by_handle(handle)
        .await?
        .filter(|player| player.active)
        .ok_or_else(|| ServiceError::PlayerNotFound(handle.clone()))
}
