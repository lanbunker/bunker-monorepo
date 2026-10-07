# lanbunker

All the software for LAN BUNKER, a recurring LAN party: the website, the API
and the arcade cabinet daemon.

```
web/                   Astro site on Cloudflare Workers. See web/README.md
crates/bunker-models   Domain types shared by every Rust crate
crates/bunker-api      axum + sqlx + SQLite API
crates/bunker-cabd     Cabinet daemon for RetroPie boxes (prints a glyph, nothing more yet)
deploy/                The API box: bootstrap, systemd units, backups. See deploy/README.md
```

The site calls the API on the server side. The browser never talks to the API.
The session is an HttpOnly cookie that holds the API bearer token.

## Run it locally

You need Rust, `sqlite3`, pnpm and the sqlx CLI.

```bash
cp .env.template .env
make install-cli         # the sqlx CLI, one time
make db                  # .dev/bunker.db, migrated
make dev                 # the API on :3000, restarts on save
make web-dev             # the site on :4321
make seed                # 30 players, a tournament, an event with open doors
make admin handle=dave   # promote a player
```

`make db-reset` deletes the local database and creates it again.

## Checks

| Command | What it does |
| --- | --- |
| `make checklist` | Rust: format, clippy with warnings as errors, every test |
| `make web-check` | Site: format, both type checkers, lint, build |
| `make web-e2e` | Playwright against a fresh API and the production build of the site |
| `make api-types` | Writes `openapi.json` and `web/src/lib/api-types.d.ts` from the route annotations |

CI runs all of them. A test fails when `openapi.json` or `schema.sql` is stale.
`CLAUDE.md` holds the rules for the code.

## How the API is built

```
Request
  -> ValidJson / ValidQuery / ValidPath / Authenticated / AdminOnly   (the boundary)
    -> routers/   handler
      -> services/  business rules, ServiceError
        -> storage/   sqlx queries, StorageError
          -> SQLite
```

- **An invalid value cannot exist.** Each field with rules is a `nutype` type in
  `bunker-models`. No code and no request body can build one that skips the
  check.
- **The compiler checks each query.** `sqlx::query!` compiles against the
  migrated local database. `schema.sql` shows the current shape in one file.
- **An error is a value.** Each layer owns its error enum, and exhaustive
  matches give the code and the status. A client sees one fixed sentence, for
  example `{ "code": "HandleTaken", "message": "That handle is already taken", "status": 409 }`.
  The cause chain shows only when `APP_ENV` is `local` or `test`.
- **A concurrent change never lands half.** A write carries the state that its
  request read, and storage checks that state in the same statement. A lost
  race answers `409`. Every write transaction starts with `BEGIN IMMEDIATE`.
- **Passwords and tokens.** Argon2id, at most four hashes at once. A token is
  HS256 with one secret. Each request reads one row of `players`, so a deleted
  player, a password change, a demotion and a password reset take effect at
  once. After a reset, the API answers every route except `/api/me` and
  `/api/me/password` with `403 PasswordChangeRequired`.
- **Only our clients reach `/api`.** The site, and later the cabinets, send one
  shared key in `X-Api-Key`. The key lives in GitHub, and each deploy carries it
  to the box and to the Worker. `/health` needs no key. When `API_KEY` is not
  set, as in local work, `/api` needs no key either.
- **Signup has a bot check.** The site checks a Cloudflare Turnstile token
  before it calls the API. The API does not check the token.
- **Each test brings its own database.** No Docker and no shared state.

The full contract is `crates/bunker-api/openapi.json`, also served at
`/api/openapi.json`. The routes, by area:

| Area | Routes |
| --- | --- |
| Account | `/api/auth/signup`, `/api/auth/login`, `/api/me`, `/api/me/password`, `/api/me/handle`, `/api/me/registrations`, `/api/me/checkins` |
| Players | `/api/players`, `/api/players/{handle}`, `.../cycles`, `.../matches`, `/api/cycles/rules` |
| Tournaments | `/api/tournaments`, `/api/tournaments/{id}`, `.../registration` |
| Events | `/api/events`, `/api/checkin/{code}` |
| Backoffice | everything under `/api/admin/`: players, cycles, tournaments, brackets, results, events, manual check-ins |
| Health | `/health/live` (version and commit), `/health/ready` (the database answers) |

## The rules of the game

**Events.** An event is one night with a window from the doors to the last
game. A draft is visible to admins only. Each event has a secret check-in code
of 12 characters, printed as a QR poster from the backoffice. A player scans
it at the door and gets 100 cycles, one time per event. The server decides the
window. An admin can check a player in by hand at any time, and take a
check-in back with the cycles it paid. Event covers are
files under `web/src/assets/images/events`.

**Tournaments.** The statuses are `draft`, `open`, `live` and `concluded`.
`open` takes registrations until `registrationClosesAt`. `live` freezes the
entrants. `concluded` is final, and the API refuses each change after it. A
player applies with a level from 1 to 5, and a second application changes the
level. A tournament takes at most 256 entrants.

**Brackets.** Single elimination, and optional. Seeds follow the level, and
entrants of one level are shuffled. Round one pairs neighbors in seed order,
and the top seeds get the byes. The admin can move seeds and enters one result
per match. A result can change until the next match is decided. Without a
bracket, the admin names the winner. The rules are pure code in
`crates/bunker-models/src/bracket.rs`. `/tournaments/{id}/kiosk` shows a live
bracket for a screen at the event.

**Cycles.** Cycles are the points. The ledger is one table, `point_entries`,
with one signed row for each gain and each loss. No row is ever changed: a
correction is a new row. The view `player_standings` computes the total, the
rank and the place on each read. A check-in pays, and a concluded tournament
pays the entry, each win and the top four places. A unique index stops a
second payment. The amounts and the ranks live in
`crates/bunker-models/src/points.rs`, and `GET /api/cycles/rules` serves them
to the site.

**Nemesis.** A profile names the opponent who beat the player most often, with
at least two wins. A tie goes to the most recent win. The view `player_matches`
holds each played match one time for each side.

## Configuration

The API reads its configuration from the environment. Locally it also reads
`.env`, except when `APP_ENV=prod`.

| Variable | Default | Meaning |
| --- | --- | --- |
| `APP_ENV` | `local` in a debug build | `test`, `local` or `prod`. A release build refuses to start without it |
| `BIND_ADDRESS` | `127.0.0.1` | Listen address |
| `PORT` | `3000` | Listen port |
| `DATABASE_URL` | `sqlite://.dev/bunker.db?mode=rwc` | `mode=rwc` creates the file. Production uses `mode=rw` |
| `DB_MAX_CONNECTIONS` | `8` | Pool size |
| `JWT_SECRET` | dev value | 32 characters or more. Required in `prod`, and the dev value is refused there |
| `API_KEY` | none | The shared key every caller of `/api` sends in `X-Api-Key`. 32 characters or more. Required in `prod` and `test`. In `local`, `/api` is open without it |
| `RUST_LOG` | from `APP_ENV` | Log filter. An invalid filter stops the start |

The site reads `API_URL` from `web/wrangler.jsonc`. In production, `API_KEY`,
`TURNSTILE_SITE_KEY` and `TURNSTILE_SECRET_KEY` are Worker secrets that CI
uploads. See `web/README.md`.

## Deploy

A push to `main` runs CI, then deploys the API, then the site. A push to `dev`
and a pull request run CI only.

- **API:** a static binary in a Debian container on the office Proxmox box,
  behind a Cloudflare Tunnel. The deploy rolls back when the new binary is not
  healthy. Litestream sends each database change to Backblaze B2.
- **Site:** Cloudflare Workers, through wrangler. The deploy also sets the
  Worker secrets from GitHub.

`deploy/README.md` gives the setup and the day-to-day commands. Before the
first push that adds `API_KEY`, upgrade the box one time, see "Upgrade the box
to the API key" there.
