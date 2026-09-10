import { execFileSync } from "node:child_process"

import { expect, test } from "@playwright/test"
import type { Page } from "@playwright/test"

const PASSWORD = "correct-horse-battery"
const handle = (prefix: string) => `${prefix}${Date.now().toString(36).slice(-5)}`

/** The API keeps sessions in a cookie set by the site, so a signup logs in. */
const signup = async (page: Page, name: string) => {
    await page.goto("/signup")
    await page.getByLabel("handle:").fill(name)
    await page.getByLabel("password:", { exact: true }).fill(PASSWORD)
    await page.getByLabel("repeat:").fill(PASSWORD)
    await page.getByRole("button", { name: "ENLIST" }).click()
    await expect(page).toHaveURL(/\/profile(\?|$)/)
}

const logout = async (page: Page) => {
    await page.goto("/profile")
    await page.getByRole("button", { name: "LOGOUT" }).click()
    await expect(page).toHaveURL(/\/login(\?|$)/)
}

/** Promotes with SQL, the way `make admin` does on a real box. */
const promote = (name: string) => {
    execFileSync("sqlite3", [
        "../.dev/e2e.db",
        `update players set role = 'admin' where handle = '${name}' collate nocase`,
    ])
}

test("signup shows the profile with a generated glyph, then logout and login again", async ({
    page,
}) => {
    const name = handle("dave")
    await signup(page, name)

    await expect(page.getByRole("heading", { level: 1, name, exact: true })).toBeVisible()
    await expect(page.getByText("you", { exact: true })).toBeVisible()
    const glyphCells = page.locator("main svg rect")
    expect(await glyphCells.count()).toBeGreaterThanOrEqual(7)

    await logout(page)

    await page.getByLabel("login:").fill(name.toUpperCase())
    await page.getByLabel("password:").fill(PASSWORD)
    await page.getByRole("button", { name: "LOGIN" }).click()
    await expect(page).toHaveURL(/\/profile(\?|$)/)
    await expect(page.getByRole("heading", { level: 1, name, exact: true })).toBeVisible()
})

test("a wrong password stays on the login page with the api message", async ({
    page,
}) => {
    const name = handle("zio")
    await signup(page, name)
    await logout(page)

    await page.getByLabel("login:").fill(name)
    await page.getByLabel("password:").fill("not-the-password")
    await page.getByRole("button", { name: "LOGIN" }).click()

    await expect(page).toHaveURL(/\/login(\?|$)/)
    await expect(page.getByRole("alert")).toContainText("handle or the password is wrong")
})

test("a taken handle is refused at signup", async ({ page }) => {
    const name = handle("dup")
    await signup(page, name)
    await logout(page)

    await page.goto("/signup")
    await page.getByLabel("handle:").fill(name)
    await page.getByLabel("password:", { exact: true }).fill(PASSWORD)
    await page.getByLabel("repeat:").fill(PASSWORD)
    await page.getByRole("button", { name: "ENLIST" }).click()

    await expect(page).toHaveURL(/\/signup(\?|$)/)
    await expect(page.getByRole("alert")).toContainText("already taken")
})

test("a signup with two different passwords is refused", async ({ page }) => {
    await page.goto("/signup")
    await page.getByLabel("handle:").fill(handle("mis"))
    await page.getByLabel("password:", { exact: true }).fill(PASSWORD)
    await page.getByLabel("repeat:").fill("something-else-entirely")
    await page.getByRole("button", { name: "ENLIST" }).click()

    await expect(page).toHaveURL(/\/signup(\?|$)/)
    await expect(page.getByRole("alert")).toContainText("differ")
})

test("typing a digit in a field does not switch section", async ({ page }) => {
    await page.goto("/login")
    await page.getByLabel("login:").click()
    await page.keyboard.type("dave2")
    await expect(page).toHaveURL(/\/login$/)
    await expect(page.getByLabel("login:")).toHaveValue("dave2")

    await page.getByLabel("password:").click()
    await page.keyboard.type("1234567")
    await expect(page).toHaveURL(/\/login$/)
    await expect(page.getByLabel("password:")).toHaveValue("1234567")

    await page.getByRole("button", { name: "LOGIN" }).focus()
    await page.keyboard.press("3")
    await expect(page).toHaveURL(/\/login$/)

    await page.evaluate(() => {
        const focused = document.activeElement
        if (focused instanceof HTMLElement) focused.blur()
    })
    await page.keyboard.press("2")
    await expect(page).toHaveURL(/\/events$/)
})

test("the roster, the old scores link and the public player page show the player", async ({
    page,
}) => {
    const name = handle("pub")
    await signup(page, name)
    await logout(page)

    await page.goto("/players")
    await expect(page.getByRole("link", { name })).toBeVisible()

    await page.goto("/scores")
    await expect(page).toHaveURL(/\/players$/)
    await page.getByRole("link", { name }).click()
    await expect(page).toHaveURL(new RegExp(`/players/${name}$`))
    await expect(page.getByRole("heading", { level: 1, name, exact: true })).toBeVisible()
    await expect(page.getByText("you", { exact: true })).toHaveCount(0)
})

test("an unknown player is a 404", async ({ page }) => {
    const response = await page.goto("/players/nobody-here")
    expect(response?.status()).toBe(404)
})

test("the profile page needs a login", async ({ page }) => {
    await page.goto("/profile")
    await expect(page).toHaveURL(/\/login(\?|$)/)
})

test("the backoffice is hidden from users and open to admins", async ({ page }) => {
    const user = handle("usr")
    await signup(page, user)
    const hidden = await page.goto("/admin")
    expect(hidden?.status()).toBe(404)
    await logout(page)

    const admin = handle("adm")
    await signup(page, admin)
    promote(admin)

    await page.goto("/admin")
    await expect(page.getByRole("link", { name: "BACKOFFICE root@bunker" })).toBeVisible()
    await expect(page.getByRole("heading", { level: 1 })).toContainText(
        "root@bunker:~# status",
    )

    await page.goto("/admin/players")
    const row = page.getByRole("row", { name: new RegExp(user) })
    await expect(row).toContainText("user")
    await row.getByRole("button", { name: "promote" }).click()
    await expect(page.getByRole("status")).toContainText(`${user} is now admin`)
    await expect(page.getByRole("row", { name: new RegExp(user) })).toContainText("admin")

    page.once("dialog", dialog => dialog.accept())
    await page
        .getByRole("row", { name: new RegExp(user) })
        .getByRole("button", { name: "delete" })
        .click()
    await expect(page.getByRole("status")).toContainText("player deleted")
    await expect(page.getByRole("row", { name: new RegExp(user) })).toHaveCount(0)
})

test("a player changes their password and logs in with the new one", async ({ page }) => {
    const name = handle("pwd")
    await signup(page, name)

    await page.goto("/password")
    await page.getByLabel("current:").fill(PASSWORD)
    await page.getByLabel("new:", { exact: true }).fill("another-long-passphrase")
    await page.getByLabel("repeat new:").fill("another-long-passphrase")
    await page.getByRole("button", { name: "CHANGE PASSWORD" }).click()
    await expect(page).toHaveURL(/\/profile(\?|$)/)
    await expect(page.getByRole("status")).toContainText("password changed")

    await logout(page)
    await page.getByLabel("login:").fill(name)
    await page.getByLabel("password:").fill("another-long-passphrase")
    await page.getByRole("button", { name: "LOGIN" }).click()
    await expect(page).toHaveURL(/\/profile(\?|$)/)
})

test("a wrong current password and a mismatched repeat are refused", async ({ page }) => {
    const name = handle("pwx")
    await signup(page, name)

    await page.goto("/password")
    await page.getByLabel("current:").fill("not-the-password")
    await page.getByLabel("new:", { exact: true }).fill("another-long-passphrase")
    await page.getByLabel("repeat new:").fill("another-long-passphrase")
    await page.getByRole("button", { name: "CHANGE PASSWORD" }).click()
    await expect(page.getByRole("alert")).toContainText("current password is wrong")

    await page.getByLabel("current:").fill(PASSWORD)
    await page.getByLabel("new:", { exact: true }).fill("another-long-passphrase")
    await page.getByLabel("repeat new:").fill("a-different-passphrase")
    await page.getByRole("button", { name: "CHANGE PASSWORD" }).click()
    await expect(page.getByRole("alert")).toContainText("differ")
})

test("an admin reset forces the player to set a new password before anything else", async ({
    page,
}) => {
    const user = handle("rst")
    await signup(page, user)
    await logout(page)

    const admin = handle("adm")
    await signup(page, admin)
    promote(admin)

    await page.goto("/admin/players")
    page.once("dialog", dialog => dialog.accept())
    await page
        .getByRole("row", { name: new RegExp(user) })
        .getByRole("button", { name: "reset password" })
        .click()
    const notice = page.getByRole("status")
    await expect(notice).toContainText("temporary password")
    const temporary = (await notice.locator("p").nth(1).textContent())?.trim() ?? ""
    expect(temporary.length).toBeGreaterThanOrEqual(16)
    await logout(page)

    await page.getByLabel("login:").fill(user)
    await page.getByLabel("password:").fill(temporary)
    await page.getByRole("button", { name: "LOGIN" }).click()
    await expect(page).toHaveURL(/\/password$/)

    await page.goto("/players")
    await expect(page).toHaveURL(/\/password$/)

    await page.getByLabel("current:").fill(temporary)
    await page.getByLabel("new:", { exact: true }).fill("my-own-long-passphrase")
    await page.getByLabel("repeat new:").fill("my-own-long-passphrase")
    await page.getByRole("button", { name: "CHANGE PASSWORD" }).click()
    await expect(page).toHaveURL(/\/profile(\?|$)/)

    await page.goto("/players")
    await expect(page).toHaveURL(/\/players$/)
})

test("the roster paginates at twenty players", async ({ page, request }) => {
    const prefix = handle("pg")
    for (let index = 0; index < 22; index += 1) {
        const response = await request.post("http://127.0.0.1:3999/api/auth/signup", {
            data: { handle: `${prefix}${index}`, password: PASSWORD },
        })
        expect(response.status()).toBe(201)
    }

    await page.goto("/players")
    const pager = page.getByRole("navigation", { name: "Pages" })
    await expect(pager).toContainText("page 1 of")
    await expect(page.getByRole("row")).toHaveCount(21)
    await expect(pager.locator("[aria-disabled='true']")).toContainText("PREV")

    await pager.getByRole("link", { name: "NEXT →" }).click()
    await expect(page).toHaveURL(/\/players\?page=2$/)
    await expect(page.getByRole("navigation", { name: "Pages" })).toContainText(
        "page 2 of",
    )
    await expect(page.locator("tbody tr").first()).toContainText("21")

    await page.goto("/players?page=banana")
    await expect(page.getByRole("navigation", { name: "Pages" })).toContainText(
        "page 1 of",
    )
})

test("the session cookie is http only and a user cannot post an admin action", async ({
    page,
    context,
}) => {
    await signup(page, handle("csr"))

    const cookie = (await context.cookies()).find(c => c.name === "bunker_session")
    expect(cookie?.httpOnly).toBe(true)
    expect(cookie?.sameSite).toBe("Lax")

    const response = await page.request.post("/admin/players?_action=setRole", {
        form: { id: "00000000-0000-0000-0000-000000000001", role: "admin" },
        headers: { origin: "http://127.0.0.1:4399" },
    })
    expect(response.status()).toBeGreaterThanOrEqual(400)
})

test("a page past the end goes to the last page", async ({ page }) => {
    await page.goto("/players?page=999")
    await expect(page).toHaveURL(/\/players\?page=\d+$/)
    await expect(page).not.toHaveURL(/page=999/)
})
