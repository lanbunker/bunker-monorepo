use bunker_models::{
    CHECKIN_CODE_LEN, CHECKIN_CYCLES, CheckinCode, CheckinGate, CheckinReceipt, CheckinWindow,
    Checkins, Event, EventDetail, EventFields, EventId, EventStatus, PageQuery, Paginated,
    PlayerId, checkin_award,
};
use rand::seq::IndexedRandom as _;
use time::OffsetDateTime;

use crate::storage::{CheckedIn, EventStorage, NewEventRow, PlayerStorage, StoredEvent};

use super::error::ServiceError;

/// What a scan at the door did. The router answers `201` for the first and
/// `200` for a repeat, with the same receipt.
#[derive(Debug, Clone)]
pub enum CheckinOutcome {
    First(CheckinReceipt),
    Repeat(CheckinReceipt),
}

/// The rules of an event: what an admin may change, who sees a draft, and when
/// the code at the door pays.
#[derive(Debug, Clone)]
pub struct EventService {
    storage: EventStorage,
    players: PlayerStorage,
}

impl EventService {
    pub const fn new(storage: EventStorage, players: PlayerStorage) -> Self {
        Self { storage, players }
    }

    /// A new event is a draft with a fresh check-in code.
    pub async fn create(&self, fields: EventFields) -> Result<Event, ServiceError> {
        fields
            .check_window()
            .map_err(|_| ServiceError::InvalidEventWindow)?;
        let id = EventId::generate();
        self.storage
            .create(&NewEventRow {
                id,
                fields,
                checkin_code: generate_code()?,
                created_at: OffsetDateTime::now_utc(),
            })
            .await?;

        Ok(self.load(id).await?.event)
    }

    /// The public list hides drafts. The admin list shows everything.
    pub async fn list(
        &self,
        query: PageQuery,
        include_drafts: bool,
    ) -> Result<Paginated<Event>, ServiceError> {
        Ok(self.storage.list(query, include_drafts).await?)
    }

    /// Everything the backoffice shows, the code for the QR included.
    pub async fn detail(&self, id: EventId) -> Result<EventDetail, ServiceError> {
        let stored = self.load(id).await?;
        let checkins = self.storage.checkins(id).await?;

        Ok(EventDetail {
            event: stored.event,
            checkin_code: stored.checkin_code,
            checkins,
        })
    }

    /// Replaces every field. The code and the status stay.
    pub async fn update(&self, id: EventId, fields: EventFields) -> Result<Event, ServiceError> {
        fields
            .check_window()
            .map_err(|_| ServiceError::InvalidEventWindow)?;
        if !self.storage.update(id, &fields).await? {
            return Err(ServiceError::EventNotFound(id));
        }

        Ok(self.load(id).await?.event)
    }

    /// Both ways. A draft hides the event and closes its door; the check-ins
    /// already made stay.
    pub async fn set_status(
        &self,
        id: EventId,
        status: EventStatus,
    ) -> Result<Event, ServiceError> {
        if !self.storage.set_status(id, status).await? {
            return Err(ServiceError::EventNotFound(id));
        }

        Ok(self.load(id).await?.event)
    }

    /// Idempotent. The check-ins and their cycles go with the event.
    pub async fn delete(&self, id: EventId) -> Result<(), ServiceError> {
        self.storage.delete(id).await?;

        Ok(())
    }

    /// What the door shows. A draft answers like an unknown code, so a leaked
    /// link says nothing about an event that is not out yet.
    pub async fn gate(&self, code: &CheckinCode) -> Result<CheckinGate, ServiceError> {
        let event = self.published_by_code(code).await?;
        let window = event.checkin_window(OffsetDateTime::now_utc());

        Ok(CheckinGate { event, window })
    }

    /// One check-in per player per event, and only while the doors are open. A
    /// second scan pays nothing and answers the first receipt.
    pub async fn check_in(
        &self,
        code: &CheckinCode,
        player: PlayerId,
    ) -> Result<CheckinOutcome, ServiceError> {
        let event = self.published_by_code(code).await?;
        let window = event.checkin_window(OffsetDateTime::now_utc());
        if window != CheckinWindow::Open {
            return Err(ServiceError::CheckinClosed(window));
        }

        self.record(event.id, player).await
    }

    /// An admin checks a player in, whatever the window and the status: a
    /// phone that did not scan, or a night from before the door existed. It
    /// pays like a scan, and a repeat pays nothing.
    pub async fn add_checkin(
        &self,
        id: EventId,
        player: PlayerId,
    ) -> Result<CheckinOutcome, ServiceError> {
        self.load(id).await?;
        if self.players.get_by_id(player).await?.is_none() {
            return Err(ServiceError::PlayerIdNotFound(player));
        }

        self.record(id, player).await
    }

    pub async fn checkins_of(&self, player: PlayerId) -> Result<Checkins, ServiceError> {
        Ok(Checkins {
            events: self.storage.events_of(player).await?,
        })
    }

    /// The check-in and its cycles, then the receipt. The count changed, so the
    /// receipt carries the event as it is after the write, and it says what
    /// this call paid: a repeat pays nothing.
    async fn record(&self, id: EventId, player: PlayerId) -> Result<CheckinOutcome, ServiceError> {
        let now = OffsetDateTime::now_utc();
        let award = checkin_award(id, player);
        let outcome = self.storage.check_in(id, player, now, &award).await?;
        let event = self.load(id).await?.event;
        let receipt = |checked_in_at, cycles| CheckinReceipt {
            event,
            checked_in_at,
            cycles,
        };

        Ok(match outcome {
            CheckedIn::New(at) => CheckinOutcome::First(receipt(at, CHECKIN_CYCLES)),
            CheckedIn::Already(first) => CheckinOutcome::Repeat(receipt(first, 0)),
        })
    }

    async fn load(&self, id: EventId) -> Result<StoredEvent, ServiceError> {
        self.storage
            .get(id)
            .await?
            .ok_or(ServiceError::EventNotFound(id))
    }

    async fn published_by_code(&self, code: &CheckinCode) -> Result<Event, ServiceError> {
        self.storage
            .get_by_code(code)
            .await?
            .filter(|event| event.status == EventStatus::Published)
            .ok_or(ServiceError::UnknownCheckinCode)
    }
}

/// Twelve characters from the alphabet of the code, so a guess has one chance
/// in 36^12 and the QR stays small.
fn generate_code() -> Result<CheckinCode, ServiceError> {
    let mut rng = rand::rng();
    let raw: String = (0..CHECKIN_CODE_LEN)
        .map(|_| char::from(*CheckinCode::ALPHABET.choose(&mut rng).unwrap_or(&b'a')))
        .collect();

    CheckinCode::try_new(raw).map_err(ServiceError::code_generation)
}
