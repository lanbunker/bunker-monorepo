# lanbunker

This monorepo contains all software for LAN BUNKER live multiplayer events.

- Main website (Astro)
- Backend API (Rust)
- Arcade cabinate software (Rust)

```
web/                   Astro site, server rendered on Cloudflare Workers
crates/bunker-models   Domain types shared by every Rust crate
crates/bunker-api      axum + sqlx + SQLite backend
crates/bunker-cabd     Cabinet daemon for RetroPie boxes (empty for now)
deploy/                Container bootstrap, systemd units, one-time setup guide
```

The site has signup and login, a public roster and player pages, a profile page,
and a hidden `/admin` backoffice for admins. Players get a generated glyph and a
color from their handle, stored at signup.

The Rust side is one Cargo workspace. The site is a pnpm project inside `web/`
with its own README. They meet over HTTP: the site calls the API server side, and
the browser never talks to the API.

## Try it

You need Rust, `sqlite3` and the sqlx CLI. The CLI is necessary only to create
the database and run migrations.

```bash
cp .env.template .env
make install-cli
make db
make dev          # the API on :3000, restarts on save
make web-dev      # the site on :4321, talks to :3000
make admin handle=dave   # promote a player after signup
```

An admin can reset a password from the backoffice. The player logs in with the
temporary password and must choose a new one before any other page opens.

The database is `.dev/bunker.db`, ignored by git and kept between runs. Delete it
with `make db-reset`.

```bash
curl -s localhost:3000/api/auth/signup -H 'content-type: application/json' \
  -d '{"handle":"dave","password":"correct-horse-battery"}'
```

```bash
curl -s localhost:3000/api/players/dave
```

`make dev` rebuilds and restarts the API after each save. `make web-dev` starts
the site. `make checklist` runs format, clippy, every test and the offline query
check, which is what CI runs.

## Why the backend is built this way

- **An invalid value cannot exist.** `nutype` declares each constraint in
  `bunker-models` and makes the constructor, the error type and the serde code.
  No code can build a `Handle` that skips validation, and a request body cannot
  do it either.
- **An error is a value.** Each layer owns its error enum. Two exhaustive matches
  make the responses. A new variant does not compile until you classify it.
- **The compiler checks each query.** `sqlx::query!` compares each query with the
  local database at build time. CI builds that database from the migrations with
  `sqlite3` in milliseconds.
- **The structure is readable in one file.** `crates/bunker-api/schema.sql` is
  generated from the migrated database and a test keeps it in step. Read it, not
  the migrations, to learn the current shape.
- **The tests bring their own database.** Each test creates a SQLite file in a
  temporary directory and migrates it. No Docker, no setup, no shared state.
- **Passwords never travel.** Argon2id hashes on the blocking pool, the type
  redacts itself, and login answers the same for an unknown handle and a wrong
  password.
- **A token is a signed claim.** HS256 with one secret from the environment. The
  check reads no database. Production refuses to start without a real secret.

## Data flow

```
Request
  -> ValidJson / ValidPath / Authenticated    (rejected at the boundary)
    -> routers/    handler
      -> services/ business rules, ServiceError
        -> storage/ typed queries, StorageError
          -> SQLite
```

A failure goes up as a typed error and becomes an `ApiError` one time:

```json
{ "code": "HandleTaken", "message": "handle `dave` is already taken", "status": 409 }
```

A `cause` field with the chain of causes appears only when `APP_ENV` is `local`
or `test`.

## Routes

| Method | Path | Auth | Answer |
| --- | --- | --- | --- |
| POST | `/api/auth/signup` | none | 201, token |
| POST | `/api/auth/login` | none | 200, token |
| GET | `/api/me` | bearer | the caller and whether a password change is due |
| POST | `/api/me/password` | bearer | change the password, current one required. Answers a fresh token, every older token dies |
| PUT | `/api/me/handle` | bearer | change the handle. The glyph stays |
| GET | `/api/players` | none | roster, newest first. `?page=1&pageSize=20`, pageSize up to 100 |
| GET | `/api/players/{handle}` | none | one player |
| GET | `/api/admin/players` | admin | every player, same paging as `/api/players` |
| PATCH | `/api/admin/players/{id}` | admin | set the role |
| PUT | `/api/admin/players/{id}/handle` | admin | rename a player |
| POST | `/api/admin/players/{id}/password-reset` | admin | temporary password, forces a change at login |
| DELETE | `/api/admin/players/{id}` | admin | remove a player |
| GET | `/api/tournaments` | none | tournaments, newest event first, drafts hidden. Same paging |
| GET | `/api/tournaments/{id}` | none | one tournament with entrants and bracket |
| GET | `/api/me/registrations` | bearer | the tournaments the caller entered |
| POST | `/api/tournaments/{id}/registration` | bearer | apply. Idempotent |
| DELETE | `/api/tournaments/{id}/registration` | bearer | retire. Idempotent |
| GET, POST | `/api/admin/tournaments` | admin | every tournament, create a draft |
| GET, PATCH, DELETE | `/api/admin/tournaments/{id}` | admin | detail, edit fields, delete with entrants and matches |
| POST | `/api/admin/tournaments/{id}/status` | admin | move the status, name the winner |
| POST | `/api/admin/tournaments/{id}/entrants` | admin | add a player |
| DELETE | `/api/admin/tournaments/{id}/entrants/{entrantId}` | admin | remove an entrant |
| POST, DELETE | `/api/admin/tournaments/{id}/bracket` | admin | generate a random bracket, remove it |
| PUT | `/api/admin/tournaments/{id}/seeds` | admin | rebuild the bracket in a given seed order |
| PUT, DELETE | `/api/admin/tournaments/{id}/matches/{matchId}/result` | admin | enter a result, clear it |
| GET | `/api/openapi.json` | none | the contract |
| GET | `/health/live` | none | process is up, version |
| GET | `/health/ready` | none | database answers |

## Tournaments

A tournament moves through four statuses. `draft` is visible to admins only.
`open` takes registrations until `registrationClosesAt`. `live` freezes the
entrants and opens the bracket work. `concluded` is final: nothing changes after
it. A draft can go live at once, which is how an old tournament is backfilled.
`live` can go back to `open` only while no bracket exists.

The bracket is single elimination and optional. The admin generates it from a
random order of the entrants, moves seeds by hand, and enters one result per
match. A winner moves into the next round. A result can change until the next
match is decided. A bye is not a result. While no result exists, the bracket can
be regenerated or removed, as long as the tournament is not concluded. With a bracket, the final decides the winner. Without
one, the admin names the winner among the entrants, or nobody.

Matches point at entrants and not at players, so a team can enter one day. The
placements a bracket implies, second for the loser of the final and so on, are
the input of a future score system. Nothing is stored for that yet.

The bracket rules live in `crates/bunker-models/src/bracket.rs` as pure data with
their own tests. The site shows a bracket on `/tournaments/{id}/bracket`, and
`/tournaments/{id}/kiosk` is the same view in a bare layout that polls
`/tournaments/{id}/detail.json` every three seconds for a screen at the event.

## End to end types

The API describes itself with utoipa. `make api-types` writes
`crates/bunker-api/openapi.json` from the route annotations and generates
`web/src/lib/api-types.d.ts` from it. The site calls the API through
`openapi-fetch`, so every path, body and response is typed from the same source.
A Rust test fails when the committed document is stale, and CI fails when the
generated types are stale.

## Tests

`make checklist` runs the Rust suite. `make web-check` type checks and builds the
site. `make web-e2e` runs Playwright against a fresh API on `.dev/e2e.db` and the
dev site: signup, login, roster, profile, 404 and the backoffice.

## Change the schema

The migrations under `crates/bunker-api/migrations/` say how the database gets
to its shape. `crates/bunker-api/schema.sql` says what the shape is, and a test
keeps the two in step. `CLAUDE.md` gives the procedure and the rules.

## Configuration

| Variable | Default | Meaning |
| --- | --- | --- |
| `APP_ENV` | `local` | `test`, `local` or `prod`. Selects every capability. |
| `BIND_ADDRESS` | `127.0.0.1` | Listen address. `0.0.0.0` when the LAN needs the API directly |
| `PORT` | `3000` | Listen port |
| `DATABASE_URL` | `sqlite://.dev/bunker.db?mode=rwc` | SQLite file. `mode=rwc` creates it |
| `DB_MAX_CONNECTIONS` | `8` | Pool size |
| `JWT_SECRET` | dev value | 32 characters or more. Required when `APP_ENV=prod` |
| `RUST_LOG` | from `APP_ENV` | Replaces the log filter |

The sqlx macros compile each query against `DATABASE_URL` from `.env`, which
must point at the migrated local database from `make db`. A build against an
empty or unmigrated file fails with `no such table`.

## Deploy

Two pipelines, both from GitHub Actions on a push to `main`. The `dev` branch
runs CI and deploys nothing.

| What | Where it runs | How |
| --- | --- | --- |
| Site | Cloudflare Workers | `deploy.yml`: `pnpm build`, then wrangler. The Worker var `API_URL` points at the API name. |
| API | A Debian 12 LXC container on the Proxmox box in the office | `deploy-api` job in `ci.yml`, after `rust`, `web` and `e2e` are green |

The API job builds a static musl binary, opens SSH through a Cloudflare Tunnel
with `cloudflared` and a key made for CI, uploads the binary, runs the root
script `bunker-deploy` on the box, and checks `/health/ready` on
`api.lanbunker.eu`. Migrations run at startup, so a deploy is one binary swap.
The box keeps no open port: `cloudflared` inside the container dials out, and
Cloudflare routes `api.lanbunker.eu` to port 3000 and `ssh.lanbunker.eu` to
port 22.

GitHub Actions needs one secret, `DEPLOY_SSH_KEY`, and two variables,
`API_HOST` and `SSH_HOST`. `deploy/README.md` holds the one-time setup of the
container and the tunnel, the bootstrap script, and the day-to-day commands:
logs, backups, making an admin, rotating the key.
