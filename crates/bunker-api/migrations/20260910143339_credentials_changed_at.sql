-- Unix seconds of the last password change or reset. A token issued before this
-- moment is refused, so a reset evicts every older session.
alter table players add column credentials_changed_at integer not null default 0
    check (credentials_changed_at >= 0);
