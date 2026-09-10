-- Set when an admin issued a temporary password. The player must replace it at
-- the next login before anything else.
alter table players add column must_change_password integer not null default 0
    check (must_change_password in (0, 1));
