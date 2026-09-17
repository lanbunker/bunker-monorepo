-- Every played match two times, one row per side. `won` is 1 for the side of
-- the row. A bye is not a match, because the join needs both entrants, and a
-- draft is not public.
--
-- This view is the one join point for a match log. A later casual match table
-- extends it with a `union all` branch of its own, and every reader follows.
create view player_matches as
select m.id         as match_id,
       m.tournament_id,
       t.date       as played_on,
       t.created_at as tournament_created_at,
       m.round,
       m.slot,
       a.player_id  as player_id,
       b.player_id  as opponent_id,
       (m.winner = m.entrant_a) as won
from matches m
join tournaments t         on t.id = m.tournament_id
join tournament_entrants a on a.tournament_id = m.tournament_id and a.id = m.entrant_a
join tournament_entrants b on b.tournament_id = m.tournament_id and b.id = m.entrant_b
where m.winner is not null and t.status in ('live', 'concluded')
union all
select m.id, m.tournament_id, t.date, t.created_at, m.round, m.slot,
       b.player_id, a.player_id, (m.winner = m.entrant_b)
from matches m
join tournaments t         on t.id = m.tournament_id
join tournament_entrants a on a.tournament_id = m.tournament_id and a.id = m.entrant_a
join tournament_entrants b on b.tournament_id = m.tournament_id and b.id = m.entrant_b
where m.winner is not null and t.status in ('live', 'concluded');
