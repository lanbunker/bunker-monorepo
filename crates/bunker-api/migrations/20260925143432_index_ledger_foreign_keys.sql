-- `player_standings` sums the ledger on every read of a player. With the amount
-- in the index, the sum reads the index alone and never the table.
create index point_entries_player_amount on point_entries (player_id, amount);

-- A delete of a tournament, an event or a player finds the rows that point at
-- it through these. Without them every such delete scans the whole ledger. The
-- rows without a reference are the most, so the indexes leave them out.
create index point_entries_tournament on point_entries (tournament_id)
    where tournament_id is not null;
create index point_entries_event on point_entries (event_id)
    where event_id is not null;
create index point_entries_created_by on point_entries (created_by)
    where created_by is not null;
create index tournaments_winner on tournaments (winner_entrant_id)
    where winner_entrant_id is not null;
