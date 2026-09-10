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
```

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
make run
```

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
| GET | `/api/me` | bearer | the caller |
| GET | `/api/players` | none | roster, newest first |
| GET | `/api/players/{handle}` | none | one player |
| GET | `/health/live` | none | process is up, version |
| GET | `/health/ready` | none | database answers |

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

The site deploys to Cloudflare Workers with wrangler from GitHub Actions on each
push that touches `web/`. The API is a static binary that runs on a box in the
office behind a Cloudflare Tunnel. That pipeline is not written yet.
