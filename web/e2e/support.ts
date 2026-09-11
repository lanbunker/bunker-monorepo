import { execFileSync } from "node:child_process"

import { expect } from "@playwright/test"
import type { Locator, Page } from "@playwright/test"

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
    await page.getByLabel("name").fill(name)
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
    const login = await page.request.post(`${API}/api/auth/login`, {
        data: { handle: name, password: PASSWORD },
    })
    expect(login.ok()).toBe(true)
    const body: unknown = await login.json()
    const token =
        typeof body === "object" && body !== null && "token" in body ? body.token : ""
    expect(typeof token).toBe("string")
    return String(token)
}
