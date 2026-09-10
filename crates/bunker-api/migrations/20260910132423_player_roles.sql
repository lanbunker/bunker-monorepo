-- Every player is a user. The crew promotes admins by hand, see `make admin`.
alter table players add column role text not null default 'user'
    check (role in ('user', 'admin'));
