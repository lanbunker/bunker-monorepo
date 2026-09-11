import { execFileSync } from "node:child_process"

import { expect } from "@playwright/test"
import type { Page } from "@playwright/test"

export const PASSWORD = "correct-horse-battery"
export const handle = (prefix: string) => `${prefix}${Date.now().toString(36).slice(-5)}`

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

/** Promotes with SQL, the way `make admin` does on a real box. */
export const promote = (name: string) => {
    execFileSync("sqlite3", [
        "../.dev/e2e.db",
        `update players set role = 'admin' where handle = '${name}' collate nocase`,
    ])
}
