## Core Rules

Critical rules for **ALL** packages. **Non-compliance will not be tolerated.**

- **ALWAYS** apply when writing code
- **ALWAYS** check when reviewing code changes

**Shell**

- **CRITICAL**: do not **EVER** run a `git` command that changes state. `git status`, `git diff` and `git log` are permitted

**Types & Imports:**

- **NEVER** use `any` or `as` casting
- Explicit type annotations **ONLY** when needed (inlined data, function params, etc)
- **NEVER** `is` type guards in `.filter` - narrowing is automatic
- **ALWAYS** `const` for functions, not `function`
- **NO** region-dividing comments unless explicitly requested
- Destructure arrays, **NOT** objects (`const [a, b] = arr` yes, `const { a } = obj` no)
- `import type` on separate lines, not inline `import { type X }`
- Prefix unused vars/params with `_`
- **ALWAYS** kebab-case for files

## Code Style

### Prefer declarative over imperative

- **Avoid `let` + loops**: Use `.find()`, `.map()`, `.filter()`, `.reduce()`, `.some()`, `.every()`, `.flatMap()` instead of `let` + `for` loops. If you need to find an item, use `.find()` - never declare a `let` variable and loop to assign it.
- **Default to `.reduce`** for accumulation, `for` **ONLY** when it clearly helps readability.
- **NEVER** `.forEach()` - use `for(const x of arr)` when a loop is needed.
- **Avoid manual array building**: Instead of `const arr = []; if (x) arr.push(...)`, use `[x ? mapX(x) : undefined, y ? mapY(y) : undefined].filter(defined)`.
- **Ternary over simple if/else**: For simple conditional assignments, prefer ternary operators. Avoid when they hurt readability (nested ternaries).
- **Prefer `const`**: Use `let` only when mutation is truly unavoidable. Build data with expressions (ternary, spread, array methods), not statements (if/push/reassign).

### Pattern matching with ts-pattern

- Use `.exhaustive()` by default - compile-time exhaustiveness checking catches missing cases.
- Use `.otherwise()` only when a genuine default/fallback is needed (e.g., unknown external input, catch-all logging). Don't use it to skip handling known cases.
- Don't overuse for trivial 2-branch conditions - a simple ternary or `if` is fine there.
- `.returnType<T>()` when: return type needs `as const`, or type already exists/is reusable.
- `.returnType` set -> no `as const` in branches.
- One-off return type -> `as const` in branches is fine, skip `.returnType`.

### Utilities: es-toolkit

- Use `es-toolkit` for utility operations: `chunk`, `groupBy`, `keyBy`, `sumBy`, `partition`, `mapValues`, `uniq`, `pick`, `omit`, etc.
- **ALWAYS** `identity` for pass-through functions.
- Prefer es-toolkit over hand-rolled loops or lodash.

### Modern JS built-ins

- Prefer `Object.groupBy()` / `Map.groupBy()` over manual grouping loops or `es-toolkit/groupBy` when the logic is simple.
- Prefer `Set` for uniqueness checks and set operations (`.union()`, `.intersection()`, `.difference()`).

## Checklist

**CRITICAL**: required before yielding control, from the repository root: `make web-check`

- It runs the format check and the production build. This is **THE ONLY** command you are to run before yielding control (unless **EXPLICITLY** told otherwise)

## React

- React Compiler enabled - **NEVER** disable
- No `useMemo`/`useCallback` - compiler handles memoization
- `useEffect`/`useLayoutEffect`: architect component tree to **avoid them**; deps **ONLY** primitives or stable external values
- **NEVER** destructure props - **EXCEPTION**: wrapper components, destructure **ONLY** non-child props
- **NEVER** `React.` namespace prefix - use hooks/components directly
- PascalCase filename = PascalCase component name
- Props type: `${ComponentName}Props`
- **NO** prop spreading (`{...props}`) - **EXCEPT** in `src/components/ui/` and tests. `Link` HTML attrs also exempt
- `button` must have explicit `type` attribute
- Every component file must only export components (react-refresh) or types

## UI & Styling

- Custom UI components: `src/components/ui/`
- Tailwind for styling (check `src/index.css` for existing utilities)
- Conditional classes: `cn` with objects: `{ "class1 class2": condition }`
- Rems for sizing, **NEVER** px
- Table headers: % widths summing to 100%
