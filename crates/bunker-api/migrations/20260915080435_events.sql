-- Events, the nights the bunker opens, and who came. Timestamps are unix
-- micros like every other table. An event has a start and an end, and the
-- check-in link works only between them.

create table events (
    id           text primary key check (length(id) = 36),
    name         text not null check (length(name) between 1 and 60),
    location     text not null check (length(location) <= 60),
    games        text not null check (length(games) <= 200),
    description  text not null check (length(description) <= 1000),
    -- The cover, as a file name under web/src/assets/images/events. The site
    -- owns the files, and a name it does not know renders no cover.
    image        text check (image is null or (length(image) between 1 and 80
                                                and image not glob '*[^A-Za-z0-9._-]*')),
    starts_at    integer not null check (starts_at > 0),
    ends_at      integer not null check (ends_at > starts_at),
    status       text not null default 'draft' check (status in ('draft', 'published')),
    -- The check-in link carries this code and not the id, so a guess opens no
    -- door. Lowercase letters and digits, so it survives a QR code and a phone.
    checkin_code text not null unique
                 check (length(checkin_code) = 12 and checkin_code not glob '*[^a-z0-9]*'),
    created_at   integer not null check (created_at > 0)
) strict;

-- One row per player per event: the primary key is the rule.
create table event_checkins (
    event_id      text not null references events(id) on delete cascade,
    player_id     text not null references players(id) on delete cascade,
    checked_in_at integer not null check (checked_in_at > 0),
    primary key (event_id, player_id)
) strict;

create index event_checkins_player on event_checkins (player_id);

-- The ledger gets the `checkin` kind and the event that paid it. SQLite
-- cannot alter a CHECK in place, so the table is built again and the rows are
-- copied. The view reads the table by name, so it goes first and comes back
-- last.
drop view player_standings;

create table point_entries_next (
    id            text primary key check (length(id) = 36),
    player_id     text not null references players(id) on delete cascade,
    amount        integer not null check (amount != 0 and abs(amount) <= 10000),
    kind          text not null
                  check (kind in ('checkin', 'tournament_entry', 'match_win', 'champion',
                                  'finalist', 'semifinalist', 'adjustment')),
    -- What paid, inside the kind: the event for a check-in, the tournament for
    -- an entry or a placement, the match for a win, the row itself for an
    -- adjustment. Never null, so the unique index below holds for every kind.
    source_ref    text not null check (length(source_ref) > 0),
    tournament_id text references tournaments(id) on delete cascade,
    event_id      text references events(id) on delete cascade,
    note          text check (note is null or length(note) between 1 and 200),
    created_by    text references players(id) on delete set null,
    created_at    integer not null check (created_at > 0),
    -- Only an admin takes cycles away, and only with a reason.
    check (kind = 'adjustment' or amount > 0),
    check (kind != 'adjustment' or note is not null),
    check (kind != 'checkin' or event_id is not null),
    -- A source pays one time however often the write is retried.
    unique (kind, player_id, source_ref)
) strict;

insert into point_entries_next
    (id, player_id, amount, kind, source_ref, tournament_id, note, created_by, created_at)
select id, player_id, amount, kind, source_ref, tournament_id, note, created_by, created_at
from point_entries;

drop table point_entries;
alter table point_entries_next rename to point_entries;

create index point_entries_player on point_entries (player_id, created_at desc);

-- The leaderboard. Equal totals share a place: two players at 300 are both
-- second, and the next one is fourth.
create view player_standings as
select p.id as player_id,
       coalesce(sum(e.amount), 0) as cycles,
       rank() over (order by coalesce(sum(e.amount), 0) desc) as place,
       count(*) over () as players
from players p
left join point_entries e on e.player_id = p.id
group by p.id;
