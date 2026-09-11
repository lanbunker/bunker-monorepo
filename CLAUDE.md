# CLAUDE.md for LAN BUNKER

A monorepo for a LAN party community: a public website, a Rust backend and a
cabinet daemon. `README.md` gives the structure and the reasons. This file gives
the rules.

## The project

LAN BUNKER is a recurring local multiplayer event. The site shows events,
tournaments and media in a terminal style. The backend holds players, their
generated glyphs, tournaments with entrants and brackets, and, later, scores and
cycles (the points). The
daemon will run on RetroPie arcade cabinets and post scores to the backend.

```
web/                  Astro site on Cloudflare Workers. Its own rules: web/CLAUDE.md
crates/bunker-models  Domain types shared by every Rust crate. No I/O.
crates/bunker-api     axum + sqlx + SQLite backend. Layers below.
crates/bunker-cabd    The cabinet daemon. Empty for now.
deploy/               What runs on the API box: bootstrap script, systemd units.
```

The Rust side is a Cargo workspace. The site is a pnpm project inside `web/`.
The two meet over HTTP: the site calls the API server side through a client
generated from `crates/bunker-api/openapi.json`. The browser never talks to the
API. The session is a cookie the site sets, holding the API bearer token.

## Critical rules

**You must obey each rule.**

1. **NEVER** run a `git` command that changes state. `git status`, `git diff` and
   `git log` are always permitted. Use `git add` and `git commit` only when the
   user asks for a commit. Each command that moves a reference or discards work
   (checkout, reset, rebase, merge, push, `branch -D`, `stash drop`) is forbidden.
2. **NEVER** panic in a `src/` directory. Clippy denies `unwrap`, `expect`,
   `panic!`, `todo!`, `dbg!`, `unreachable!`, slice indexing and lossy casts, and
   the workspace forbids `unsafe`. Each fallible function returns `Result`. If a case cannot happen, change the
   types until no code can express it. A test can unwrap.
3. **ALWAYS** write or update a test for a change in behaviour, then run
   `make checklist` before you report that a Rust task is complete. Write each
   test from the contract (the route, the model, the error code), and not from the
   implementation, so that the test can find a bug in the implementation.
4. **ALWAYS** grep for each caller and each reference before you change an item.
   Do the full audit on the first pass.
5. **ALWAYS** connect a feature from end to end: route, handler, service, storage,
   migration, `schema.sql`, `openapi.json`, `web/src/lib/api-types.d.ts`. Never
   filter, sort or paginate in a handler if the query can do it.
6. **NEVER** edit a migration that is applied, and never edit `schema.sql` by
   hand. See *Change the schema*.
7. **NEVER** write a query outside `storage/`, and never build SQL from a value a
   caller sent. Each query is a `sqlx::query!` or `sqlx::query_as!` macro with
   bound parameters, so the compiler checks it.
8. **NEVER** log or return a password, a password hash or a JWT secret. The types
   redact themselves. Keep it that way.
9. **NEVER** use an em dash. Use a comma, a colon or two sentences.

## Checklist

Run **exactly** this command before you return control on a Rust task:

```
make checklist
```

It runs `cargo fmt`, clippy with warnings as errors, and every test. Add no flag,
no pipe, no redirection and no `timeout` wrapper. If it fails, correct the cause.
Never report a complete task with a broken build.

The `sqlx::query!` macros compile against the local database, so the checklist
creates it with `make db` when it is missing. The tests need no Docker and no
service. Each test creates its own SQLite file in a temporary directory and
migrates it.

For a change under `web/`, follow `web/CLAUDE.md` and run `make web-check`. A
change to a user flow also needs `make web-e2e`, which runs Playwright against a
fresh API.

## Layers in `bunker-api`

Each arrow points down. A module never imports a module above it.

```
server    composition root: the wiring and the status table  -> everything
routers   HTTP: extract, call, return a typed response       -> services, internal::http
services  business rules, owns ServiceError and ErrorCode    -> storage
storage   queries and rows, owns StorageError                -> config
config    environment parsing and capabilities               -> nothing
internal  HTTP support code and telemetry                    -> services (codes), config
```

Domain types come from `bunker-models` and are visible to every layer. They hold
no I/O, no HTTP and no SQL.

These violations must never occur:

- A router that uses `storage` or `sqlx`. It goes around the business rules.
- A router that imports `server`. `AppState` is in `src/routers.rs` for this
  reason: the arrow must not exist.
- An `AppState` field that is not a service.
- An `axum` type or a `sqlx` type in a `bunker-models` signature or a `services`
  signature.
- Business logic in `storage`. Storage translates rows, and does nothing else.
- A domain type that maps to a row directly. A row is a separate struct
  (`PlayerRow`) that becomes a domain value through `TryFrom`.
- A `ServiceError` inspected in `internal::http`. That module maps only boundary
  rejections (body, path, header, timeout, 404, 405) and renders what it gets.

## Errors

Each decision has one place.

| Decision | Place |
| --- | --- |
| Which codes exist | `ErrorCode` in `src/services/error.rs` |
| The code and the client message of a failure | `ServiceError::public`, same file |
| Temporary failure or bug | `From<StorageError> for ServiceError`, same file |
| The status of a code | `From<ErrorCode> for StatusCode` in `src/server.rs` |
| The response body | `src/internal/http/api_error.rs`, which decides nothing |

Those three matches are exhaustive and take no wildcard arm, so a new variant does
not compile until you classify it.

- A handler returns `Result<_, ApiError>` and uses `?`. Never build an error
  response by hand, and never match a `ServiceError` in a handler.
- `From<StorageError> for ServiceError` is written by hand, and not derived with
  `#[from]`, so `?` cannot report a lost database as a bug.
- Never write `#[error(transparent)]` on a variant that holds the error of another
  layer. The wrapped message would disappear from the cause chain.
- An error keeps its context and its source. Never put a cause into a string if
  you can keep the original.
- **There are two registers for a message.** An internal error is lowercase and
  has no full stop. A client message is a sentence with a capital letter, and
  gives only what the client can act on.
- Login failures share one code and one message for an unknown handle and a
  wrong password. Do not split them.
- Use `anyhow` only in `server::run` and `main`, where the caller only reports.

## Rust standards

- **Constants first, then public items, then private helpers.**
- **One `thiserror` type per layer.** Never one global error type.
- **Parse, do not validate.** A field with rules gets a `nutype` newtype in
  `bunker-models`, and not a check in a handler. Never add a method like
  `is_valid`.
- **A fallible constructor returns `Result`.** Use `try_new`, and never a `new`
  that panics.
- **Use `&str` and `&[T]`** in a signature that only reads. Store owned values.
- **No `mod.rs` in `src/`.** A module is `foo.rs` plus a `foo/` directory. The
  module root exports the public items of its children again, so a consumer
  imports one level deep. `tests/support/mod.rs` is the one exception, because
  Cargo compiles each `tests/*.rs` as its own binary but ignores subdirectories.
- **A file name is snake_case** and matches the module.
- **All external dependencies live in the root `Cargo.toml`** under
  `[workspace.dependencies]`, one version each. A crate pulls what it needs with
  `dep.workspace = true`.
- **Keep the crate graph shallow and arrows down.** `bunker-models` depends on
  nothing internal. `bunker-api` and `bunker-cabd` depend on it and not on each
  other.
- **Write a thing one time.** The reason a maintainer needs is in the code, at
  the smallest scope that owns the decision. The reason a reader of the repo
  needs is in `README.md`.

## Comments

- **Write no comment by default.** Use names and structure. Never restate the
  code.
- **Give the reason**, and not the action: a constraint that is not obvious, a
  dependency on order, an external problem, or a cost that the call does not show.
- **A comment has no time.** It describes the code as it is now. Never write
  history ("changed to", "previously", "now uses"). Git holds the history.
- Put `///` on a public item that needs a doc. Put it on a private item only when
  the logic is not obvious.
- Write no banner comment and no section divider.

## Add an endpoint

1. **Model.** Put the request type and the response type in `bunker-models`. Use a
   `nutype` newtype for each field that has rules. Each wire type has
   `#[serde(rename_all = "camelCase")]`, and each request type also has
   `deny_unknown_fields`. Export the types from `lib.rs`.
2. **Storage.** Write the query with `sqlx::query!` and bound parameters. Return
   `Option`, or an enum such as `Created`, for a result that is not a failure. The
   service decides what an absent row means. Give a limit to each query that has
   no window.
3. **Service.** Write the business rules and return `ServiceError`. A storage
   result with a meaning, such as a unique violation, becomes a domain variant.
4. **Errors.** A new `ServiceError` variant needs an arm in `ServiceError::public`.
   A new `ErrorCode` needs a status in `From<ErrorCode> for StatusCode`. The
   compiler asks for both.
5. **Router.** Write a handler that takes `ValidJson`, `ValidQuery`, `ValidPath` or
   `Authenticated`. Never use the axum `Query` or `Path`, because `clippy.toml`
   rejects them. Ask for the service through `State<…>`.
6. **Document.** Put `#[utoipa::path]` on the handler and list it in
   `routers/openapi.rs`. A new wire type derives `ToSchema`, or gets an impl in
   `bunker-models/src/schema.rs` when it is a `nutype` newtype.
7. **Tests.** Put a boundary rejection in `tests/http_contract_test.rs`, and put
   behaviour in the `tests/<feature>_api_test.rs` file. Use `support::TestApi`
   for both, and assert the status and the `code` together with
   `support::assert_error`.
8. **Generate.** Run `make api-types` and commit `openapi.json` and
   `api-types.d.ts`. A test and CI fail when either is stale.

## Configuration

- `src/config/env.rs` is the one place that reads the environment. Never call
  `std::env::var` anywhere else.
- Each capability comes from `AppEnv`. A new capability is a field on `AppConfig`
  and a value for each variant in `resolve_app_config`. Never add an environment
  variable for one feature.

## Change the schema

`README.md` gives the reason for each step. The procedure:

1. Run `make migration name=what_it_does`. It writes an empty file under
   `crates/bunker-api/migrations/`.
2. Write the SQL. Use `strict` tables. Mirror each domain invariant as a `CHECK`.
   A row that the model refuses fails each read of the full table, and not only
   its own read.
3. Run `make migrate`. It applies the SQL to the local database and writes
   `crates/bunker-api/schema.sql` again. That file is how you read the current
   structure without reading every migration. A test fails when it is stale.

A new `not null` column needs a default if the table holds rows. A drop or a
rename of a column destroys data. Ask the user first. There is no down
migration: a mistake is corrected by a new forward migration.

## Tests

- `TestApi::without_database()` for a request that the boundary rejects. Its pool
  points at a file that cannot be opened. `TestApi::with_database()` for
  behaviour. It creates and migrates one SQLite file per test.
- Tests share nothing, so they need no unique names and no locks. Use plain
  handles such as `dave`.
- **A test that skips must never report success.** Mark it `#[ignore]` with a
  reason, so the summary counts it. Never return early from the body of a test.
- Write a test that can fail. An assertion that still passes after you delete the
  feature is worse than no test.
- The glyph algorithm is shared with `web/src/lib/glyph.ts`. The vectors in
  `crates/bunker-models/tests/glyph_test.rs` come from that file. A change to one
  side must update both and the vectors.
