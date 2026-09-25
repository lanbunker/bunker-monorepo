use bunker_models::{
    Award, Checkin, CheckinCode, Description, Event, EventFields, EventId, EventName, EventStatus,
    EventWindow, Games, ImageName, Location, PageQuery, Paginated, PlayerId,
};
use time::OffsetDateTime;

use super::db::{DbPool, begin_write};
use super::error::StorageError;
use super::point_storage::{AwardSource, insert_awards};
use super::row::{PlayerRow, from_micros, parse_uuid, to_micros};

const EVENTS: &str = "events";
const CHECKINS: &str = "event_checkins";

/// The primary key of `event_checkins`, as SQLite names it.
const CHECKIN_CONSTRAINT: &str = "event_checkins.event_id, event_checkins.player_id";

/// The most check-ins one read returns. A night in the bunker has tens of
/// players, so this bound is never a page, only a cap.
const CHECKINS_MAX: i64 = 1000;

#[derive(Debug, Clone)]
pub struct NewEventRow {
    pub id: EventId,
    pub fields: EventFields,
    pub checkin_code: CheckinCode,
    pub created_at: OffsetDateTime,
}

/// An event with the secret only an admin reads.
#[derive(Debug, Clone)]
pub struct StoredEvent {
    pub event: Event,
    pub checkin_code: CheckinCode,
}

/// What an insert of a check-in did, with the time as the row holds it: the
/// column keeps microseconds, and a clock can give more. A second scan is a
/// result, not a failure, and it carries the time of the first one.
#[derive(Debug, Clone, Copy)]
pub enum CheckedIn {
    New(OffsetDateTime),
    Already(OffsetDateTime),
}

#[derive(Debug, Clone)]
pub struct EventStorage {
    pool: DbPool,
}

impl EventStorage {
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, row: &NewEventRow) -> Result<(), StorageError> {
        let id = row.id.into_inner().to_string();
        let name = row.fields.name.as_ref();
        let location = row.fields.location.as_ref();
        let games = row.fields.games.as_ref();
        let description = row.fields.description.as_ref();
        let image = row.fields.image.as_ref().map(AsRef::<str>::as_ref);
        let starts_at = to_micros(EVENTS, row.fields.window.starts_at())?;
        let ends_at = to_micros(EVENTS, row.fields.window.ends_at())?;
        let status = EventStatus::Draft.as_str();
        let code = row.checkin_code.as_ref();
        let created_at = to_micros(EVENTS, row.created_at)?;

        sqlx::query!(
            "insert into events (id, name, location, games, description, image, starts_at, ends_at,
                                 status, checkin_code, created_at)
             values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            id,
            name,
            location,
            games,
            description,
            image,
            starts_at,
            ends_at,
            status,
            code,
            created_at,
        )
        .execute(&self.pool)
        .await
        .map_err(StorageError::from_query)?;

        Ok(())
    }

    /// `None` when no row has the id.
    pub async fn get(&self, id: EventId) -> Result<Option<StoredEvent>, StorageError> {
        let id = id.into_inner().to_string();
        let row = sqlx::query_as!(
            EventRow,
            r#"select e.id, e.name, e.location, e.games, e.description, e.image, e.starts_at,
                      e.ends_at, e.status, e.checkin_code, e.created_at,
                      (select count(*) from event_checkins c where c.event_id = e.id) as "checkin_count!: i64"
               from events e where e.id = ?1"#,
            id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(StorageError::from_query)?;

        row.map(TryInto::try_into).transpose()
    }

    /// The event behind a check-in link, whatever its status. `None` when no
    /// row has the code.
    pub async fn get_by_code(&self, code: &CheckinCode) -> Result<Option<Event>, StorageError> {
        let code = code.as_ref();
        let row = sqlx::query_as!(
            EventRow,
            r#"select e.id, e.name, e.location, e.games, e.description, e.image, e.starts_at,
                      e.ends_at, e.status, e.checkin_code, e.created_at,
                      (select count(*) from event_checkins c where c.event_id = e.id) as "checkin_count!: i64"
               from events e where e.checkin_code = ?1"#,
            code,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(StorageError::from_query)?;

        row.map(|row| StoredEvent::try_from(row).map(|stored| stored.event))
            .transpose()
    }

    /// The latest night first. `include_drafts` is the admin view; the public
    /// list never shows a draft.
    pub async fn list(
        &self,
        query: PageQuery,
        include_drafts: bool,
    ) -> Result<Paginated<Event>, StorageError> {
        let limit = query.limit();
        let offset = query.offset();
        let drafts = i64::from(include_drafts);
        let draft = EventStatus::Draft.as_str();

        let mut tx = self.pool.begin().await.map_err(StorageError::from_query)?;
        let total = sqlx::query_scalar!(
            "select count(*) from events where ?1 = 1 or status != ?2",
            drafts,
            draft,
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(StorageError::from_query)?;
        let rows = sqlx::query_as!(
            EventRow,
            r#"select e.id, e.name, e.location, e.games, e.description, e.image, e.starts_at,
                      e.ends_at, e.status, e.checkin_code, e.created_at,
                      (select count(*) from event_checkins c where c.event_id = e.id) as "checkin_count!: i64"
               from events e
               where ?1 = 1 or e.status != ?2
               order by e.starts_at desc, e.id asc
               limit ?3 offset ?4"#,
            drafts,
            draft,
            limit,
            offset,
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(StorageError::from_query)?;
        tx.commit().await.map_err(StorageError::from_query)?;

        let items = rows
            .into_iter()
            .map(|row| StoredEvent::try_from(row).map(|stored| stored.event))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Paginated::new(
            items,
            u64::try_from(total).unwrap_or_default(),
            query,
        ))
    }

    /// Replaces every field. `false` when no row has the id.
    pub async fn update(&self, id: EventId, fields: &EventFields) -> Result<bool, StorageError> {
        let id = id.into_inner().to_string();
        let name = fields.name.as_ref();
        let location = fields.location.as_ref();
        let games = fields.games.as_ref();
        let description = fields.description.as_ref();
        let image = fields.image.as_ref().map(AsRef::<str>::as_ref);
        let starts_at = to_micros(EVENTS, fields.window.starts_at())?;
        let ends_at = to_micros(EVENTS, fields.window.ends_at())?;

        let result = sqlx::query!(
            "update events
             set name = ?1, location = ?2, games = ?3, description = ?4, image = ?5,
                 starts_at = ?6, ends_at = ?7
             where id = ?8",
            name,
            location,
            games,
            description,
            image,
            starts_at,
            ends_at,
            id,
        )
        .execute(&self.pool)
        .await
        .map_err(StorageError::from_query)?;

        Ok(result.rows_affected() > 0)
    }

    /// `false` when no row has the id.
    pub async fn set_status(&self, id: EventId, status: EventStatus) -> Result<bool, StorageError> {
        let id = id.into_inner().to_string();
        let status = status.as_str();
        let result = sqlx::query!("update events set status = ?1 where id = ?2", status, id)
            .execute(&self.pool)
            .await
            .map_err(StorageError::from_query)?;

        Ok(result.rows_affected() > 0)
    }

    /// The check-ins and their cycles go with it. `true` when a row was removed.
    pub async fn delete(&self, id: EventId) -> Result<bool, StorageError> {
        let id = id.into_inner().to_string();
        let result = sqlx::query!("delete from events where id = ?1", id)
            .execute(&self.pool)
            .await
            .map_err(StorageError::from_query)?;

        Ok(result.rows_affected() > 0)
    }

    /// First at the door first, up to [`CHECKINS_MAX`].
    pub async fn checkins(&self, event: EventId) -> Result<Vec<Checkin>, StorageError> {
        let event = event.into_inner().to_string();
        let rows = sqlx::query_as!(
            CheckinRow,
            r#"select c.checked_in_at,
                      p.id, p.handle, p.glyph_bits, p.glyph_color, p.role, p.created_at,
                      s.cycles as "cycles!: i64", s.place as "place!: i64", s.players as "players!: i64"
               from event_checkins c
               join players p on p.id = c.player_id
               join player_standings s on s.player_id = p.id
               where c.event_id = ?1
               order by c.checked_in_at asc, p.id asc
               limit ?2"#,
            event,
            CHECKINS_MAX,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StorageError::from_query)?;

        rows.into_iter().map(TryInto::try_into).collect()
    }

    /// The check-in and its cycles, in one transaction: a player at the door is
    /// never half paid. A second scan writes nothing and answers the first
    /// time.
    pub async fn check_in(
        &self,
        event: EventId,
        player: PlayerId,
        at: OffsetDateTime,
        award: &Award,
    ) -> Result<CheckedIn, StorageError> {
        let event_id = event.into_inner().to_string();
        let player_id = player.into_inner().to_string();
        let checked_in_at = to_micros(CHECKINS, at)?;

        let mut tx = begin_write(&self.pool).await?;
        let inserted = sqlx::query!(
            "insert into event_checkins (event_id, player_id, checked_in_at) values (?1, ?2, ?3)",
            event_id,
            player_id,
            checked_in_at,
        )
        .execute(&mut *tx)
        .await;
        match inserted {
            Ok(_) => {}
            Err(error) => match StorageError::from_query(error) {
                StorageError::UniqueViolation { constraint, .. }
                    if constraint == CHECKIN_CONSTRAINT =>
                {
                    let first = sqlx::query_scalar!(
                        "select checked_in_at from event_checkins where event_id = ?1 and player_id = ?2",
                        event_id,
                        player_id,
                    )
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(StorageError::from_query)?;
                    tx.commit().await.map_err(StorageError::from_query)?;

                    return Ok(CheckedIn::Already(from_micros(CHECKINS, first)?));
                }
                other => return Err(other),
            },
        }
        insert_awards(
            &mut tx,
            AwardSource::Event(event),
            std::slice::from_ref(award),
            at,
        )
        .await?;
        tx.commit().await.map_err(StorageError::from_query)?;

        Ok(CheckedIn::New(from_micros(CHECKINS, checked_in_at)?))
    }

    /// The events the player checked in to, the latest check-in first, up to
    /// [`CHECKINS_MAX`], so a cut drops the oldest.
    pub async fn events_of(&self, player: PlayerId) -> Result<Vec<EventId>, StorageError> {
        let player = player.into_inner().to_string();
        let rows = sqlx::query_scalar!(
            "select event_id from event_checkins where player_id = ?1
             order by checked_in_at desc limit ?2",
            player,
            CHECKINS_MAX,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StorageError::from_query)?;

        rows.into_iter()
            .map(|raw| parse_uuid(CHECKINS, &raw).map(EventId::new))
            .collect()
    }
}

#[derive(Debug)]
struct EventRow {
    id: String,
    name: String,
    location: String,
    games: String,
    description: String,
    image: Option<String>,
    starts_at: i64,
    ends_at: i64,
    status: String,
    checkin_code: String,
    created_at: i64,
    checkin_count: i64,
}

impl TryFrom<EventRow> for StoredEvent {
    type Error = StorageError;

    fn try_from(row: EventRow) -> Result<Self, Self::Error> {
        let event = Event {
            id: EventId::new(parse_uuid(EVENTS, &row.id)?),
            name: EventName::try_new(row.name)
                .map_err(|e| StorageError::malformed_row(EVENTS, e))?,
            location: Location::try_new(row.location)
                .map_err(|e| StorageError::malformed_row(EVENTS, e))?,
            games: Games::try_new(row.games).map_err(|e| StorageError::malformed_row(EVENTS, e))?,
            description: Description::try_new(row.description)
                .map_err(|e| StorageError::malformed_row(EVENTS, e))?,
            image: row
                .image
                .map(ImageName::try_new)
                .transpose()
                .map_err(|e| StorageError::malformed_row(EVENTS, e))?,
            window: EventWindow::try_new(
                from_micros(EVENTS, row.starts_at)?,
                from_micros(EVENTS, row.ends_at)?,
            )
            .map_err(|e| StorageError::malformed_row(EVENTS, e))?,
            status: row
                .status
                .parse()
                .map_err(|e| StorageError::malformed_row(EVENTS, e))?,
            checkin_count: u32::try_from(row.checkin_count)
                .map_err(|e| StorageError::malformed_row(EVENTS, e))?,
            created_at: from_micros(EVENTS, row.created_at)?,
        };

        Ok(Self {
            event,
            checkin_code: CheckinCode::try_new(row.checkin_code)
                .map_err(|e| StorageError::malformed_row(EVENTS, e))?,
        })
    }
}

#[derive(Debug)]
struct CheckinRow {
    checked_in_at: i64,
    id: String,
    handle: String,
    glyph_bits: i64,
    glyph_color: String,
    role: String,
    created_at: i64,
    cycles: i64,
    place: i64,
    players: i64,
}

impl TryFrom<CheckinRow> for Checkin {
    type Error = StorageError;

    fn try_from(row: CheckinRow) -> Result<Self, Self::Error> {
        Ok(Self {
            checked_in_at: from_micros(CHECKINS, row.checked_in_at)?,
            player: PlayerRow {
                id: row.id,
                handle: row.handle,
                glyph_bits: row.glyph_bits,
                glyph_color: row.glyph_color,
                role: row.role,
                created_at: row.created_at,
                cycles: row.cycles,
                place: row.place,
                players: row.players,
            }
            .try_into()?,
        })
    }
}
