import { execFileSync } from "node:child_process"

import { expect } from "@playwright/test"
import type { APIResponse, Locator, Page } from "@playwright/test"
import { z } from "zod"

export const PASSWORD = "correct-horse-battery"
export const handle = (prefix: string) => `${prefix}${Date.now().toString(36).slice(-5)}`

/** The API port of the e2e servers. Bulk setup goes straight there. */
export const API = "http://127.0.0.1:3999"

/** The API keeps sessions in a cookie set by the site, so a signup logs in. */
export const signup = async (page: Page, name: string) => {
    await page.goto("/signup")
    await page.getByLabel("handle:").fill(name)
    await page.getByLabel("password:", { exact: true }).fill(PASSWORD)
    await page.getByLabel("repeat:").fill(PASSWORD)
    await page.getByRole("button", { name: "ENLIST" }).click()
    await expect(page).toHaveURL(/\/profile(\?|$)/)
}

export const login = async (page: Page, name: string) => {
    await page.goto("/login")
    await page.getByLabel("login:").fill(name)
    await page.getByLabel("password:", { exact: true }).fill(PASSWORD)
    await page.getByRole("button", { name: "LOGIN" }).click()
    await expect(page).toHaveURL(/\/profile(\?|$)/)
}

export const logout = async (page: Page) => {
    await page.goto("/profile")
    await page.getByRole("button", { name: "LOGOUT" }).click()
    await expect(page).toHaveURL(/\/login(\?|$)/)
}

/** Signs up an admin through the site and promotes them. Leaves them logged in. */
export const signupAdmin = async (page: Page, prefix = "adm") => {
    const name = handle(prefix)
    await signup(page, name)
    promote(name)
    return name
}

/** Promotes with SQL, the way `make admin` does on a real box. */
export const promote = (name: string) => {
    execFileSync("sqlite3", [
        "../.dev/e2e.db",
        `update players set role = 'admin' where handle = '${name}' collate nocase`,
    ])
}

/**
 * The sentence a failed form shows.
 *
 * Astro writes a refused input into the message as JSON. A page must never show
 * that, so every check of a failure goes through here and the shape is asserted
 * once, in `readableFailure`.
 */
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

/** A bearer token for direct API calls, for setup that the UI would make slow. */
export const tokenFor = async (page: Page, name: string): Promise<string> => {
    const response = await page.request.post(`${API}/api/auth/login`, {
        data: { handle: name, password: PASSWORD },
    })
    expect(response.ok()).toBe(true)
    const body: unknown = await response.json()
    const token =
        typeof body === "object" && body !== null && "token" in body ? body.token : ""
    expect(typeof token).toBe("string")
    return String(token)
}

/** The standing of a player, as a test reads it back from the API. */
export const standingSchema = z.object({
    standing: z.object({
        cycles: z.number(),
        place: z.number(),
        players: z.number(),
        rank: z.string(),
    }),
})

/** The rules of the ledger, as a test reads them from the API. */
export const rulesSchema = z.object({
    tiers: z.array(z.object({ tier: z.string(), minEntrants: z.number() })),
    awards: z.array(z.object({ kind: z.string(), cycles: z.array(z.number()) })),
    ranks: z.array(z.object({ rank: z.string(), floor: z.number() })),
})

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
