-- A player is active after a first check-in, and an admin always is. Only an
-- active player has a place, and only active players count in `players`, so a
-- signup that never came to an event moves nobody on the board.
drop view player_standings;

create view player_standings as
select player_id,
       cycles,
       active,
       case when active then rank() over (partition by active order by cycles desc) end as place,
       sum(active) over () as players
from (
    select p.id as player_id,
           coalesce((select sum(e.amount) from point_entries e where e.player_id = p.id), 0)
               as cycles,
           (p.role = 'admin'
            or exists (select 1 from event_checkins c where c.player_id = p.id)) as active
    from players p
);
