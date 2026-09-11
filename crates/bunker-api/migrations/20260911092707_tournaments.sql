-- Tournaments, their entrants and the matches of a single elimination bracket.
-- Timestamps are unix micros like players.created_at. The day of the event is
-- ISO text, because a day has no timezone.

create table tournaments (
    id                     text primary key check (length(id) = 36),
    name                   text not null check (length(name) between 1 and 60),
    game                   text not null check (length(game) between 1 and 40),
    mode                   text not null check (length(mode) between 1 and 30),
    description            text not null check (length(description) <= 1000),
    date                   text not null
                           check (date glob '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    registration_closes_at integer not null check (registration_closes_at > 0),
    status                 text not null default 'draft'
                           check (status in ('draft', 'open', 'live', 'concluded')),
    -- Only a concluded tournament has a winner. That the entrant belongs to this
    -- tournament is checked by the service: a CHECK cannot read another row.
    winner_entrant_id      text references tournament_entrants(id) on delete set null,
    created_at             integer not null check (created_at > 0),
    check (status = 'concluded' or winner_entrant_id is null)
) strict;

create table tournament_entrants (
    id            text primary key check (length(id) = 36),
    tournament_id text not null references tournaments(id) on delete cascade,
    -- Null once the player deleted their account, so an old bracket keeps its
    -- shape. A team column can sit next to this one later.
    player_id     text references players(id) on delete set null,
    seed          integer check (seed is null or seed >= 1),
    registered_at integer not null check (registered_at > 0),
    unique (tournament_id, player_id),
    -- Lets a match reference an entrant of its own tournament only.
    unique (tournament_id, id)
) strict;

create index tournament_entrants_player on tournament_entrants (player_id);

create table matches (
    id            text primary key check (length(id) = 36),
    tournament_id text not null references tournaments(id) on delete cascade,
    round         integer not null check (round >= 1),
    slot          integer not null check (slot >= 0),
    entrant_a     text,
    entrant_b     text,
    winner        text,
    unique (tournament_id, round, slot),
    check (winner is null or winner = entrant_a or winner = entrant_b),
    foreign key (tournament_id, entrant_a) references tournament_entrants (tournament_id, id),
    foreign key (tournament_id, entrant_b) references tournament_entrants (tournament_id, id),
    foreign key (tournament_id, winner) references tournament_entrants (tournament_id, id)
) strict;
