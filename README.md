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
| GET | `/api/players` | none | the leaderboard, first place first. `?page=1&pageSize=20`, pageSize up to 100 |
| GET | `/api/players/{handle}` | none | one player, with cycles, rank and place |
| GET | `/api/players/{handle}/cycles` | none | the cycles log, newest first, with the totals by kind |
| GET | `/api/players/{handle}/matches` | none | the match log, newest first, with the record and the nemesis |
| GET | `/api/cycles/rules` | none | how cycles are earned, and the ladder |
| GET | `/api/admin/players` | admin | every player, newest first, same paging as `/api/players` |
| POST | `/api/admin/players/{id}/cycles` | admin | add or take cycles, with a note |
| PATCH | `/api/admin/players/{id}` | admin | set the role |
| PUT | `/api/admin/players/{id}/handle` | admin | rename a player |
| POST | `/api/admin/players/{id}/password-reset` | admin | temporary password, forces a change at login |
| DELETE | `/api/admin/players/{id}` | admin | remove a player |
| GET | `/api/tournaments` | none | tournaments, newest event first, drafts hidden. Same paging |
| GET | `/api/tournaments/{id}` | none | one tournament with entrants and bracket |
| GET | `/api/me/registrations` | bearer | the tournaments the caller entered |
| POST | `/api/tournaments/{id}/registration` | bearer | apply with a level from 1 to 5. A second call changes the level |
| DELETE | `/api/tournaments/{id}/registration` | bearer | retire. Idempotent |
| GET, POST | `/api/admin/tournaments` | admin | every tournament, create a draft |
| GET, PATCH, DELETE | `/api/admin/tournaments/{id}` | admin | detail, edit fields, delete with entrants and matches |
| POST | `/api/admin/tournaments/{id}/status` | admin | move the status, name the winner |
| POST | `/api/admin/tournaments/{id}/entrants` | admin | add a player, with an optional level |
| DELETE | `/api/admin/tournaments/{id}/entrants/{entrantId}` | admin | remove an entrant |
| POST, DELETE | `/api/admin/tournaments/{id}/bracket` | admin | generate a bracket seeded by level, remove it |
| PUT | `/api/admin/tournaments/{id}/seeds` | admin | rebuild the bracket in a given seed order |
| PUT, DELETE | `/api/admin/tournaments/{id}/matches/{matchId}/result` | admin | enter a result, clear it |
| GET | `/api/events` | none | published events, the latest night first. Same paging |
| GET | `/api/checkin/{code}` | none | the event behind a check-in code, and whether its doors are open |
| POST | `/api/checkin/{code}` | bearer | check in. 201 pays the cycles, 200 on a repeat, which pays nothing |
| GET | `/api/me/checkins` | bearer | the events the caller checked in to |
| GET, POST | `/api/admin/events` | admin | every event, create a draft |
| GET, PUT, DELETE | `/api/admin/events/{id}` | admin | detail with the code and the check-ins, replace the fields, delete with check-ins and cycles |
| POST | `/api/admin/events/{id}/status` | admin | publish, or back to draft |
| POST | `/api/admin/events/{id}/checkins` | admin | check a player in by hand, any status, any time. Pays like a scan |
| GET | `/api/openapi.json` | none | the contract |
| GET | `/health/live` | none | process is up, version |
| GET | `/health/ready` | none | database answers |

## Events

An event is one night: a name, a place, the games, a cover and a window from
the doors to the last game. A new event is a draft that only admins see. A
published event is on the site, the latest night first, and past nights stay
as the archive.

Every event has a check-in code, twelve lowercase letters and digits, made when
the event is created and never sent to the public. The backoffice shows the
link, `/checkin/{code}` on the site, and renders it as a QR poster at
`/admin/events/{id}/qr.svg`: black on white, with CHECK-IN and the name of the
night under the code, as an SVG to print or as a PNG the browser draws from
it. The link carries the code and not the id, so a guess opens no door. A
draft answers a 404 to its own code, so a leaked link says nothing before the
night is announced.

A player scans the code, and the page shows the event and one of three states:
the doors are not open yet, the night is over, or the door is open. At an open
door a visitor without a session gets two buttons, enlist and login, and both
carry the door in `?next=` so the player lands back on it, logged in. A signup
logs the player in at once. A logged-in player sees their glyph and handle over
one button, and a second link logs them out for a friend's turn on the same
phone. The check-in is one row per player per event, the primary key of
`event_checkins`, and it pays 100 cycles in the same transaction. A second
scan pays nothing and answers the first receipt. The server decides the
window, so no page reads a clock to open the door. An admin can check a player
in by hand from the backoffice, at any time and in any status, for a phone that
did not scan or for a night from before the door existed. It pays the same, one
time per player.

The nights that happened before the events table existed are not in a
migration. An admin creates them in the backoffice, and `make seed` creates
them for local work.

The covers are files under `web/src/assets/images/events`. The API holds a file
name, the backoffice offers the names it finds, and a name the site does not
know renders no cover. A new cover is a commit and a deploy.

## Tournaments

A tournament moves through four statuses. `draft` is visible to admins only.
`open` takes registrations until `registrationClosesAt`. `live` freezes the
entrants and opens the bracket work. `concluded` is final: nothing changes after
it. A draft can go live at once, which is how an old tournament is backfilled.
`live` can go back to `open` only while no bracket exists.

A player applies with a level from 1 to 5: how good they say they are at the
game. The site asks for it on its own page, with the player's glyph and handle
above the confirm button. To change the level, the player retires and applies
again. An admin can add a player with a level or without one. An admin add with
a level corrects the level of a player who is already in.

The bracket is single elimination and optional. The admin generates it seeded by
level: the strongest is seed 1, and entrants of one level are shuffled first, so
a regeneration gives a new draw among them. An entrant without a level counts as
a 3. Round one pairs neighbours in seed order, seed 1 against seed 2, seed 3
against seed 4, and so on, so a beginner plays a beginner and a strong player
plays a strong player. When the field is not a power of two, the top seeds get
the byes, one per match at most. The two halves of the bracket meet only in the
final. The admin moves seeds by hand, and enters one result per match. A winner
moves into the next round. A result can change until the next match is decided.
A bye is not a result. While no result exists, the bracket can be regenerated or
removed, as long as the tournament is not concluded. With a bracket, the final
decides the winner. Without one, the admin names the winner among the entrants,
or nobody.

Matches point at entrants and not at players, so a team can enter one day.

### Nemesis

The public profile and the private profile show a match log, and name the
opponent who beats the player the most. A view, `player_matches`, holds every
played match two times, one row per side, with the day of its tournament. A bye
is not a match, and only a `live` or a `concluded` tournament is in the view.
Every read goes to the bracket rows, so a corrected result changes the answer at
once.

One opponent must win at least two matches against the player before the site
names them. A tie on the count goes to the opponent whose last win is the more
recent. Recency is the day of the tournament, and then its creation instant for
two tournaments on one day. The handle breaks a full tie, so two reads answer
the same name. An opponent who deleted their account stays in the log without a
link, and never holds the title: the next opponent takes it.

`player_matches` is the join point a casual match extends, with a table of its
own and a `union all` branch in the view. The record and the nemesis read the
view alone and need no other change. The log also joins `tournaments` for the
name and the game, and counts the rounds of the bracket, so a match outside a
tournament needs those two columns to come from the branch itself.

## Cycles

Cycles are the points. The ledger is one table, `point_entries`: every gain and
every loss is a signed row with a kind, and nothing else is stored. The total
of a player is a sum, the rank is a threshold on the total, and the place is the
position among all players. A view, `player_standings`, computes the three on
every read, so they are never stale. A row is never updated or deleted. A
correction is a new row with the opposite sign and a note.

Three sources write the ledger today. The door of an event pays 100 cycles to
a player who checks in, one time per event, in the transaction that writes the
check-in. A concluded tournament pays every entrant
for the entry, every won match, and the placements: champion, finalist and the
two semifinalists. The entry and a win pay the same in every field. A placement
follows the size of the field, every entrant counted: small below 8, medium
below 16, large from 16 on, and the large tier is a cap. Without a bracket only
the entry and the named winner pay.
The rows are written in the transaction that concludes the tournament. Every
row names its source, the event, the tournament or the match, and a unique index on the
kind, the player and the source makes a second payment a failure and not a
duplicate. An admin adds or
takes cycles by hand, with a note that everyone reads.
A deleted tournament takes its cycles with it, and so do a deleted event and a
deleted player.

The amounts and the ladder live in one file, `crates/bunker-models/src/points.rs`.
`GET /api/cycles/rules` serves them, and the site renders its legend from that
call, so the page can never disagree with the ledger. The ranks, bottom first:
zombie, guest at 80, user at 600, sudoer at 1500, daemon at 3000, kernel at 6000.

A future source, such as an arcade score, is one variant in `PointKind` with its amount, one writer that names its source, and
one migration that recreates the table with the new kind in the CHECK list,
because SQLite cannot alter a CHECK in place.

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

`make checklist` runs the Rust suite. `make web-check` checks the format, runs
both type checkers, lints and builds the site. `make web-e2e` runs Playwright
against a fresh API on `.dev/e2e.db` and the production build of the site, served
by the Workers runtime as on Cloudflare: signup, login, the roster,
the profile, every public page, 404s, the backoffice, tournaments and brackets.
It also asserts that every refusal reaches the page as one readable sentence.

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

The site names the API in `API_URL`, which lives in `web/wrangler.jsonc`. A
Worker var beats the shell in local mode, so a local run selects an environment
instead of exporting a variable: `CLOUDFLARE_ENV=dev` for `make web-dev` and
`CLOUDFLARE_ENV=e2e` for Playwright. The top level is production. `web/README.md`
gives the detail.

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
