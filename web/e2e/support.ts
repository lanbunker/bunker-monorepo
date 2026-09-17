import { execFileSync } from "node:child_process"

import { expect } from "@playwright/test"
import type {
    APIRequestContext,
    APIResponse,
    BrowserContext,
    Locator,
    Page,
} from "@playwright/test"
import { z } from "zod"

export const PASSWORD = "correct-horse-battery"

/** The API of the e2e servers. Setup goes straight there, because it is fast. */
export const API = "http://127.0.0.1:3999"

/** The site under test. A cookie and an action post both need the origin. */
export const SITE = "http://127.0.0.1:4399"

/** Astro refuses an action post from another origin. */
export const FROM_SITE = { origin: SITE }

/** The cookie that carries the bearer token. Only the server reads it. */
const SESSION_COOKIE = "bunker_session"

const tokenSchema = z.object({ token: z.string() })
const idSchema = z.object({ id: z.string() })

/** The part of the site's own detail route that a test reads back. */
export const detailSchema = z.object({
    tournament: z.object({ entrantCount: z.number() }),
    entrants: z.array(z.object({ skill: z.number().nullable() })),
})

/** The standing of a player, as a test reads it back from the API. */
const standingSchema = z.object({
    standing: z.object({
        cycles: z.number(),
        place: z.number(),
        players: z.number(),
        rank: z.string(),
    }),
})

/** The rules of the ledger, as a test reads them from the API. */
const rulesSchema = z.object({
    tiers: z.array(z.object({ tier: z.string(), minEntrants: z.number() })),
    awards: z.array(z.object({ kind: z.string(), cycles: z.array(z.number()) })),
    ranks: z.array(z.object({ rank: z.string(), floor: z.number() })),
})

let made = 0

/**
 * A handle no other test holds. Workers are separate processes and one process
 * can ask twice inside a millisecond, so the name carries the process, a
 * counter and a random tail. It adds about ten characters, so a prefix of four
 * leaves room for the index a caller appends inside the 20 a handle allows.
 */
export const handle = (prefix: string) => {
    made += 1
    const process36 = process.pid.toString(36).slice(-4)
    const random = Math.random().toString(36).slice(2, 6)
    return `${prefix}${process36}${made.toString(36)}${random}`
}

/**
 * A response body parsed with a schema, so a test never reads an `any`. A body
 * of another shape fails here, with the schema's message, and not deep inside
 * an assertion.
 */
export const jsonOf = async <T>(
    response: APIResponse,
    schema: z.ZodType<T>,
): Promise<T> => {
    expect(response.ok()).toBe(true)
    const body: unknown = await response.json()
    return schema.parse(body)
}

export const bearer = (token: string) => ({ authorization: `Bearer ${token}` })

/** Signs a player up through the API. Answers the bearer token. */
export const apiSignup = async (
    request: APIRequestContext,
    name: string,
): Promise<string> => {
    const response = await request.post(`${API}/api/auth/signup`, {
        data: { handle: name, password: PASSWORD },
    })
    expect(response.status(), name).toBe(201)
    return (await jsonOf(response, tokenSchema)).token
}

/** A bearer token for a player that already exists. */
export const apiLogin = async (
    request: APIRequestContext,
    name: string,
    password = PASSWORD,
): Promise<string> => {
    const response = await request.post(`${API}/api/auth/login`, {
        data: { handle: name, password },
    })
    return (await jsonOf(response, tokenSchema)).token
}

/** Puts a token in the session cookie. The next page load is logged in. */
export const setSession = (context: BrowserContext, token: string) =>
    context.addCookies([{ name: SESSION_COOKIE, value: token, url: SITE }])

/** Drops the session. The next page load is anonymous. */
export const clearSession = (context: BrowserContext) => context.clearCookies()

/** A new player, already logged in, and not one page load spent on it. */
export const newPlayer = async (context: BrowserContext, prefix = "ply") => {
    const name = handle(prefix)
    const token = await apiSignup(context.request, name)
    await setSession(context, token)
    return { name, token }
}

/** The same with the admin role. The role comes from the row, not the token. */
export const newAdmin = (context: BrowserContext, prefix = "adm") =>
    adminNamed(context, handle(prefix))

/**
 * An admin under a name the test chose. Use it when a second account must share
 * the prefix, so one search shows both rows.
 */
export const adminNamed = async (context: BrowserContext, name: string) => {
    const token = await apiSignup(context.request, name)
    promote(name)
    await setSession(context, token)
    return { name, token }
}

/**
 * Promotes with SQL, the way `make admin` does on a real box. The timeout lets
 * the command wait for the write lock of the API instead of failing at once.
 */
export const promote = (name: string) => {
    execFileSync("sqlite3", [
        "-cmd",
        ".timeout 5000",
        "../.dev/e2e.db",
        `update players set role = 'admin' where handle = '${name}' collate nocase`,
    ])
}

/** The API keeps sessions in a cookie set by the site, so a signup logs in. */
export const signup = async (page: Page, name: string) => {
    await page.goto("/signup")
    await page.getByLabel("handle:").fill(name)
    await page.getByLabel("password:", { exact: true }).fill(PASSWORD)
    await page.getByLabel("repeat:").fill(PASSWORD)
    await page.getByRole("button", { name: "ENLIST" }).click()
    await expect(page).toHaveURL(/\/profile(\?|$)/)
}

export const login = async (page: Page, name: string, password = PASSWORD) => {
    await page.goto("/login")
    await page.getByLabel("login:").fill(name)
    await page.getByLabel("password:", { exact: true }).fill(password)
    await page.getByRole("button", { name: "LOGIN" }).click()
    await expect(page).toHaveURL(/\/profile(\?|$)/)
}

export const logout = async (page: Page) => {
    await page.goto("/profile")
    await page.getByRole("button", { name: "LOGOUT" }).click()
    await expect(page).toHaveURL(/\/login(\?|$)/)
}

/** The sentence a failed form shows. */
export const alertText = (page: Page): Locator => page.getByRole("alert").first()

/** A message a player can act on: one sentence, no JSON and no stack. */
export const readableFailure = async (page: Page, contains: RegExp | string) => {
    const alert = alertText(page)
    await expect(alert).toBeVisible()
    await expect(alert).toContainText(contains)
    const text = (await alert.textContent())?.trim() ?? ""
    expect(text).not.toContain("Failed to validate")
    expect(text).not.toMatch(/[[{]"/)
    expect(text.length).toBeLessThan(200)
}

/** Signs up `count` players named `prefix0`, `prefix1`, ... at the same time. */
export const signupMany = (request: APIRequestContext, prefix: string, count: number) =>
    Promise.all(
        Array.from({ length: count }, (_, index) =>
            apiSignup(request, `${prefix}${index}`),
        ),
    )

const pad = (n: number) => String(n).padStart(2, "0")

/** A day that registration can still be open on, in the two shapes forms want. */
export const tomorrow = () => {
    const d = new Date(Date.now() + 86_400_000)
    const day = `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`
    return { day, deadline: `${day}T20:00` }
}

/** Creates a draft through the backoffice form and lands on its edit page. */
export const createDraft = async (page: Page, name: string): Promise<string> => {
    await page.goto("/admin/tournaments")
    await page.getByLabel("name", { exact: true }).fill(name)
    await page.getByLabel("game").fill("COD MW2")
    await page.getByLabel("mode").fill("1v1 sniper only")
    await page.getByLabel("date").fill(tomorrow().day)
    await page
        .getByLabel("registration closes (your local time)")
        .fill(tomorrow().deadline)
    await page.getByRole("button", { name: "CREATE DRAFT" }).click()
    await expect(page).toHaveURL(/\/admin\/tournaments\/[0-9a-f-]{36}$/)
    return page.url().split("/").at(-1) ?? ""
}

type DraftFields = {
    name?: string
    date?: string
    registrationClosesAt?: string
}

/** A draft tournament straight through the API. Answers its id. */
export const apiDraft = async (
    request: APIRequestContext,
    token: string,
    fields: DraftFields = {},
): Promise<string> => {
    const response = await request.post(`${API}/api/admin/tournaments`, {
        data: {
            name: fields.name ?? `Cup ${handle("t")}`,
            game: "COD MW2",
            mode: "1v1",
            description: "",
            date: fields.date ?? "2030-01-01",
            registrationClosesAt: fields.registrationClosesAt ?? "2029-12-31T20:00:00Z",
        },
        headers: bearer(token),
    })
    expect(response.status()).toBe(201)
    return (await jsonOf(response, idSchema)).id
}

export const setTournamentStatus = async (
    request: APIRequestContext,
    token: string,
    id: string,
    status: string,
) => {
    const response = await request.post(`${API}/api/admin/tournaments/${id}/status`, {
        data: { status },
        headers: bearer(token),
    })
    expect(response.ok(), status).toBe(true)
}

/**
 * Adds players that already have an account as entrants, all at the same time.
 * `levels` gives each name its level by index, and a missing one is unrated.
 */
const addEntrants = async (
    request: APIRequestContext,
    token: string,
    id: string,
    names: string[],
    levels: (number | undefined)[] = [],
) =>
    Promise.all(
        names.map(async (name, index) => {
            const found = await jsonOf(
                await request.get(`${API}/api/players/${name}`),
                idSchema,
            )
            const added = await request.post(
                `${API}/api/admin/tournaments/${id}/entrants`,
                {
                    data: { playerId: found.id, skill: levels[index] ?? null },
                    headers: bearer(token),
                },
            )
            expect(added.status(), name).toBe(201)
        }),
    )

/**
 * Signs the players up and adds them as entrants, all at the same time. A tie
 * on level is drawn at random, so the order of the adds decides nothing.
 * `levels` gives each name its level by index, and a missing one is unrated.
 */
export const enrol = async (
    request: APIRequestContext,
    token: string,
    id: string,
    names: string[],
    levels: (number | undefined)[] = [],
) => {
    await Promise.all(names.map(name => apiSignup(request, name)))
    await addEntrants(request, token, id, names, levels)
}

const bracketSchema = z.object({
    rounds: z.array(z.array(z.object({ id: z.string() }))),
})

const adminDetailSchema = z.object({
    entrants: z.array(
        z.object({
            id: z.string(),
            player: z.object({ handle: z.string() }).nullable(),
        }),
    ),
})

/**
 * One live tournament on `date` where `winner` beats `loser`, through the API.
 * Both accounts must exist. Two entrants make one match, so the result needs no
 * walk of the bracket. Answers the tournament id.
 */
export const apiDuel = async (
    request: APIRequestContext,
    token: string,
    winner: string,
    loser: string,
    date: string,
): Promise<string> => {
    const id = await apiDraft(request, token, { date })
    await addEntrants(request, token, id, [winner, loser])
    await setTournamentStatus(request, token, id, "live")

    const generated = await request.post(`${API}/api/admin/tournaments/${id}/bracket`, {
        data: {},
        headers: bearer(token),
    })
    const bracket = await jsonOf(generated, bracketSchema)
    const match = bracket.rounds[0]?.[0]
    const detail = await jsonOf(
        await request.get(`${API}/api/admin/tournaments/${id}`, {
            headers: bearer(token),
        }),
        adminDetailSchema,
    )
    const side = detail.entrants.find(entrant => entrant.player?.handle === winner)
    if (!match || !side) throw new Error(`no duel between ${winner} and ${loser}`)

    const reported = await request.put(
        `${API}/api/admin/tournaments/${id}/matches/${match.id}/result`,
        { data: { winner: side.id }, headers: bearer(token) },
    )
    expect(reported.ok(), `${winner} wins`).toBe(true)

    return id
}

/** The id of a player, read back by handle. */
export const playerId = async (request: APIRequestContext, name: string) =>
    (await jsonOf(await request.get(`${API}/api/players/${name}`), idSchema)).id

export const adjustCycles = async (
    request: APIRequestContext,
    token: string,
    id: string,
    amount: number,
    note: string,
) => {
    const response = await request.post(`${API}/api/admin/players/${id}/cycles`, {
        data: { amount, note },
        headers: bearer(token),
    })
    expect(response.status(), note).toBe(201)
}

/** The cycles a kind pays in a field of `entrants`, from the rules. */
export const cyclesOf = (
    rules: z.infer<typeof rulesSchema>,
    kind: string,
    entrants: number,
): number => {
    const tier = rules.tiers.findLastIndex(t => entrants >= t.minEntrants)
    return (
        rules.awards.find(award => award.kind === kind)?.cycles[Math.max(tier, 0)] ??
        Number.NaN
    )
}

/** The rules of the ledger, read once per test that needs them. */
export const cyclesRules = async (request: APIRequestContext) =>
    jsonOf(await request.get(`${API}/api/cycles/rules`), rulesSchema)

/** The standing of a player, read back from the API. */
export const standingOf = async (request: APIRequestContext, name: string) =>
    (await jsonOf(await request.get(`${API}/api/players/${name}`), standingSchema))
        .standing
