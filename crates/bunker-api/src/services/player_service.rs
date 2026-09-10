use bunker_models::{Handle, PageQuery, Paginated, Player, PlayerId, Role};

use crate::storage::{PlayerStorage, Renamed};

use super::error::ServiceError;

#[derive(Debug, Clone)]
pub struct PlayerService {
    storage: PlayerStorage,
}

impl PlayerService {
    pub const fn new(storage: PlayerStorage) -> Self {
        Self { storage }
    }

    pub async fn get_by_handle(&self, handle: &Handle) -> Result<Player, ServiceError> {
        self.storage
            .get_by_handle(handle)
            .await?
            .ok_or_else(|| ServiceError::PlayerNotFound(handle.clone()))
    }

    pub async fn list(&self, query: PageQuery) -> Result<Paginated<Player>, ServiceError> {
        Ok(self.storage.list(query).await?)
    }

    /// `actor` is the admin doing it. Nobody changes their own role, so the last
    /// admin cannot lock the crew out by accident.
    pub async fn set_role(
        &self,
        actor: PlayerId,
        id: PlayerId,
        role: Role,
    ) -> Result<Player, ServiceError> {
        if actor == id {
            return Err(ServiceError::SelfAction);
        }
        self.storage
            .set_role(id, role)
            .await?
            .ok_or(ServiceError::PlayerIdNotFound(id))
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
        self.storage.delete(id).await?;

        Ok(())
    }

    pub async fn check_ready(&self) -> Result<(), ServiceError> {
        self.storage.check_reachable().await?;

        Ok(())
    }
}
