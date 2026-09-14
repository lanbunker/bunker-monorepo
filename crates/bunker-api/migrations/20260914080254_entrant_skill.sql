-- The level a player gives themself when they apply, 1 to 5. Null for an
-- entrant an admin added without one. The bracket is seeded from it.
alter table tournament_entrants
    add column skill integer check (skill is null or skill between 1 and 5);
