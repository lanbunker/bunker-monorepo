## Core Rules

Critical rules for **ALL** packages. **Non-compliance will not be tolerated.**

- **ALWAYS** apply when writing code
- **ALWAYS** check when reviewing code changes

**Shell**

- **CRITICAL**: do not **EVER** run a `git` command that changes state. `git status`, `git diff` and `git log` are permitted

**Types & Imports:**

- **NEVER** use `any`, and **NEVER** write a type assertion (`value as Type`, `value!`, `<Type>value`). `as const` and `satisfies` are allowed: they narrow, they do not lie
- To turn an unknown value into a typed one, write a type guard or parse it with zod. `oxlint` denies the assertion forms, so there is no escape hatch to reach for
- Explicit type annotations **ONLY** when needed (inlined data, function params, return types that act as a contract check)
- **NEVER** `is` type guards in `.filter` - narrowing is automatic
- **ALWAYS** `const` for functions, not `function`
- **NO** region-dividing comments unless explicitly requested
- Destructure arrays, **NOT** objects (`const [a, b] = arr` yes, `const { a } = obj` no)
- `import type` on separate lines, not inline `import { type X }`
- Prefix unused vars/params with `_`
- **ALWAYS** kebab-case for files
- `tsconfig.json` runs with `noUncheckedIndexedAccess`. An index access answers `T | undefined`. Handle it with `?.` and `??`, never with `!`

## Code Style

### Prefer declarative over imperative

- **Avoid `let` + loops**: Use `.find()`, `.map()`, `.filter()`, `.reduce()`, `.some()`, `.every()`, `.flatMap()` instead of `let` + `for` loops. If you need to find an item, use `.find()` - never declare a `let` variable and loop to assign it.
- **Default to `.reduce`** for accumulation, `for` **ONLY** when it clearly helps readability.
- **NEVER** `.forEach()` - use `for(const x of arr)` when a loop is needed.
- **NEVER** `.sort()` - it mutates. Use `.toSorted()`.
- **Avoid manual array building**: Instead of `const arr = []; if (x) arr.push(...)`, use `[x ? mapX(x) : undefined, y ? mapY(y) : undefined].filter(defined)`.
- **Ternary over simple if/else**: For simple conditional assignments, prefer ternary operators. Avoid when they hurt readability (nested ternaries).
- **Prefer `const`**: Use `let` only when mutation is truly unavoidable. Build data with expressions (ternary, spread, array methods), not statements (if/push/reassign).
- **`ts-pattern` for a decision on a union**. `match(value).with(...).exhaustive()` on a status, a code or a tagged union, so a new variant fails to compile. A `Record<Union, T>` lookup is fine when every arm is a plain value.

## Checklist

**CRITICAL**: required before yielding control, from the repository root: `make web-check`

- It **formats**, then type checks, lints, runs `astro check` and builds. This is **THE ONLY** command you are to run before yielding control (unless **EXPLICITLY** told otherwise)
- Formatting is a write, not a check: the checklist leaves the tree formatted, so a save in the editor changes nothing afterwards. CI runs `pnpm fmt:check` instead, so a commit that skipped the checklist fails there
- A change to signup, login, profile, players, tournaments or admin also needs `make web-e2e`

### Tools

- **Two formatters, one style.** `oxfmt` owns every file type it can parse and reads `.oxfmtrc.json`. `prettier` owns `.astro`, which oxfmt cannot parse, and reads `.prettierrc.json`. Their settings match: no semicolons, double quotes, trailing commas, width 90. Astro keeps a two space indent, everything else four
- `pnpm fmt` runs both. Never hand-format around either, and never format an `.astro` file with oxfmt or a `.ts` file with prettier
- `oxlint` lints. Its settings live in `.oxlintrc.json`. **NEVER** silence a rule with an inline comment. Either correct the code or change the rule in the config with a reason
- Two type checkers run, and both must pass. `pnpm typecheck` is TypeScript 7, the native compiler: it reads every `.ts` and `.tsx` in under a second. `pnpm check` is `astro check`, which needs TypeScript 6 and is the only one that reads `.astro`. TypeScript 7 is stricter in places, so a generic that satisfies one must satisfy the other
- The `typescript` dependency is pinned to 6.x because `astro check` refuses 7.x. The `tsgo` alias holds 7.x. Do not swap them

## API

- **NEVER** call the API with a hand-written `fetch`, and **NEVER** call `apiClient` on its own. Every call goes through `call` or `callEmpty` in `src/lib/api.ts`
- `call` answers an `ApiResult`: `{ ok: true, data }` or `{ ok: false, failure }`. It never throws, so an outage is a value a page renders, not a 500
- In a page: `dataOf(result)` for the value, `failureOf(result)` for the failure, `isMissing(failure)` to decide a 404, `messageOf(failure, fallback)` for the sentence
- In an action: `unwrap(result)` from `src/lib/action.ts`. It throws the matching `ActionError`, so no handler builds an error response of its own
- **`call` for a route with a body, `callEmpty` for a route that answers 204.** The compiler does not catch a mix-up: `call` on a 204 route infers `never` and compiles. At runtime a 2xx with no body counts as an outage, so the mistake shows in the log with the URL, and the test for that route fails. Check `openapi.json` when you add a call
- **NEVER** edit `src/lib/api-types.d.ts`. It is generated by `make api-types` from the Rust annotations
- A body that arrives over the wire without `call` (the kiosk polls its own site route) is parsed with zod in `src/lib/detail.ts`. Its return type is the contract check
- The session cookie is HttpOnly. Only Astro actions in `src/actions/` set or clear it, and only server code reads it through `Astro.locals`
- Read the session with `sessionOf` or `adminSessionOf` from `src/lib/session.ts`, never from `Astro.locals` by hand. They answer one value that holds both the player and a token that is not optional
- A page that needs a login redirects to `/login` when `sessionOf` answers nothing. An admin page rewrites to `/404` when `adminSessionOf` answers nothing
- The backoffice uses `AdminLayout`, never `Layout`. It must stay visibly different from the site

## Errors

A failure the visitor caused must reach the page as **one sentence they can act on**.

- Astro reports a refused input as an `ActionInputError` whose `message` is the issue list as JSON. **NEVER** render `error.message` by hand. Use the `Alert` component, or `errorMessage` from `src/lib/form.ts`, which reads the field messages back
- Every form failure renders through `<Alert error={result?.error} />`. Every other message renders through `<Alert message="..." />`
- A zod rule in `src/lib/schemas.ts` carries its own message, written as a sentence for the player. The API validates again and is the authority; these rules exist so a typing mistake costs no round trip
- A page with several forms picks its failure with `firstError(...)`
- After a success a page redirects, so a refresh repeats nothing. The redirect carries a **known key** in `?done=`, never a ready-made sentence: the query string is anyone's to write. `noticeFrom(url, notices)` maps the key back
- `e2e/errors.spec.ts` asserts that no message is a JSON blob. Add to it when you add a form

## Astro

- Pages are server rendered. Data comes from `call` in the frontmatter, never from a client fetch
- Forms post to actions in `src/actions/`. A page redirects after a successful action. The one exception is a value that must show once, such as a temporary password
- A `<script>` in a page runs once per page, so it selects every matching element and never assumes one. It can `import` from `src/lib/`, so a helper is written one time
- Text that comes from data goes in `data-*` attributes, never in an inline handler string
- `button` elements carry an explicit `type`
- PascalCase filename for a component, kebab-case for everything else
- `astro.config.mjs` sets `compressHTML: true`. Astro 7 defaults to JSX whitespace rules, which glue two inline elements that only a line break separates

## Configuration

- `wrangler.jsonc` holds `API_URL`. **A Worker var beats the shell in local mode**, so a local run selects an environment: `CLOUDFLARE_ENV=dev` for `make web-dev`, `CLOUDFLARE_ENV=e2e` for Playwright. The top level is production, and a deploy uses it
- The e2e API port must match `env.e2e.vars.API_URL` in `wrangler.jsonc`

## React islands

- Interactivity lives in a React component under `src/components/*.tsx`, mounted with `client:load`. Everything else is an Astro component with no script
- An island receives its first state as props from the page and never fetches on mount. It calls Astro actions from `astro:actions` for writes and a site route for reads, never the API
- An island keeps its buttons disabled until it is mounted. Use `useHydrated` from `src/lib/use-hydrated.ts`, never a `setState` in an effect
- The API is the source of truth: an island replaces its state with what an action answers, and computes nothing that the API computes
- Props are read as `props.x`, never destructured. The props type is `${Name}Props`
- A component that both Astro and React render, such as the glyph, is written one time in React. The Astro wrapper renders it with no client directive, so it ships no script

## UI & Styling

- Custom UI components: `src/components/ui/`
- Tailwind for styling. Tokens live in `src/styles/global.css` under `@theme`, including the backoffice palette `admin-*`
- A class string that two files share lives in `src/lib/styles.ts`
- Conditional classes: `class:list` with arrays and objects
- Rems for sizing, **NEVER** px
- Table headers: % widths summing to 100%
- A control that takes a gesture is a `<button type="button">`, never a `div` with a role. `oxlint` denies the second form
- A control that only a mouse can work, such as a draggable bracket side, is **not** a button. A button announces itself to a screen reader and takes a tab stop, so a key press on it must do something
- Attach a handler only where the gesture belongs. A drag handler on every side of a bracket makes every side a drop target, including the ones no hint offers

## Tests

- Playwright only, under `e2e/`. Shared helpers live in `e2e/support.ts`
- `signup`, `logout`, `signupAdmin` and `tokenFor` do the setup. Bulk setup goes straight to the API through `tokenFor`, because the UI is slow for thirty rows
- Write a test that can fail. An assertion that still passes after you delete the feature is worse than no test
- Target a control by its role and name. Use a `data-*` attribute only when a role cannot say which element is meant, as `data-pick` does for a bracket side that takes a result
- The suite shares one database, so every handle comes from `handle(prefix)`
