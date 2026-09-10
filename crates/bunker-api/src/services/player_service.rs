use bunker_models::{Handle, Player, PlayerId};

use crate::storage::PlayerStorage;

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

    pub async fn get_by_id(&self, id: PlayerId) -> Result<Player, ServiceError> {
        self.storage
            .get_by_id(id)
            .await?
            .ok_or(ServiceError::PlayerIdNotFound(id))
    }

    pub async fn list(&self) -> Result<Vec<Player>, ServiceError> {
        Ok(self.storage.list().await?)
    }

    pub async fn check_ready(&self) -> Result<(), ServiceError> {
        self.storage.check_reachable().await?;

        Ok(())
    }
}
