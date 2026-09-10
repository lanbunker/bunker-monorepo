-- Players and their login. The glyph is generated once at signup and stored.
-- Each domain invariant is also a CHECK: a row the model refuses would fail every
-- read of the table, not only its own.
create table players (
    id            text primary key
                  check (length(id) = 36),
    handle        text not null unique collate nocase
                  check (length(handle) between 3 and 20
                         and handle not glob '*[^A-Za-z0-9_.-]*'),
    password_hash text not null,
    glyph_bits    integer not null
                  check (glyph_bits between 0 and 33554431),
    glyph_color   text not null
                  check (glyph_color in (
                      '#ffb000', '#4fd1e0', '#b48cff', '#ff6b57',
                      '#9dff57', '#ff5cc8', '#cfe7ff', '#ffd75c'
                  )),
    -- Unix time in microseconds. An integer sorts correctly, a formatted text
    -- with a trimmed fraction does not.
    created_at    integer not null
                  check (created_at > 0)
) strict;
