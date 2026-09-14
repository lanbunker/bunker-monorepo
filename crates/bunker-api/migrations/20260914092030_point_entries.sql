-- The cycles ledger. Every gain and every loss is one row, and a total is a
-- sum. A row is never updated or deleted: a correction is a new row with the
-- opposite sign and a note. The rows of a tournament go with the tournament,
-- and the rows of a player go with the player.
--
-- A new kind is one migration that recreates this table with the new CHECK
-- list: SQLite cannot alter a CHECK in place.
create table point_entries (
    id            text primary key check (length(id) = 36),
    player_id     text not null references players(id) on delete cascade,
    amount        integer not null check (amount != 0 and abs(amount) <= 10000),
    kind          text not null
                  check (kind in ('tournament_entry', 'match_win', 'champion', 'finalist',
                                  'semifinalist', 'adjustment')),
    -- What paid, inside the kind: the tournament for an entry or a placement,
    -- the match for a win, the row itself for an adjustment. Never null, so the
    -- unique index below holds for every kind, including the ones to come.
    source_ref    text not null check (length(source_ref) > 0),
    tournament_id text references tournaments(id) on delete cascade,
    note          text check (note is null or length(note) between 1 and 200),
    created_by    text references players(id) on delete set null,
    created_at    integer not null check (created_at > 0),
    -- Only an admin takes cycles away, and only with a reason.
    check (kind = 'adjustment' or amount > 0),
    check (kind != 'adjustment' or note is not null),
    -- A source pays one time however often the write is retried.
    unique (kind, player_id, source_ref)
) strict;

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
