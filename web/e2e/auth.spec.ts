import { expect, test } from "@playwright/test"

import {
    FROM_SITE,
    PASSWORD,
    clearSession,
    forcedPlayer,
    handle,
    login,
    logout,
    newAdmin,
    newPlayer,
    readableFailure,
    signup,
    signupMany,
} from "./support"

/** Signup, login, logout, the password and the handle: the flows of an account. */

test("a signup shows the profile with a glyph, and the handle logs in in any case", async ({
    page,
}) => {
    const name = handle("dav")
    await signup(page, name)

    await expect(page.getByRole("heading", { level: 1, name, exact: true })).toBeVisible()
    await expect(page.getByText("you", { exact: true })).toBeVisible()
    expect(await page.locator("main svg rect").count()).toBeGreaterThanOrEqual(7)

    await logout(page)
    await login(page, name.toUpperCase())
    await expect(page.getByRole("heading", { level: 1, name, exact: true })).toBeVisible()
})

test("a refused signup names the fault, keeps the handle and gives no password back", async ({
    page,
    request,
}) => {
    const taken = handle("tkn")
    await signupMany(request, taken, 1)

    // Two passwords that differ: the message names the field, not the schema.
    await page.goto("/signup")
    await page.getByLabel("handle:").fill(`${taken}0`)
    await page.getByLabel("password:", { exact: true }).fill(PASSWORD)
    await page.getByLabel("repeat:").fill("something-else-entirely")
    await page.getByRole("button", { name: "ENLIST" }).click()

    await expect(page).toHaveURL(/\/signup(\?|$)/)
    await readableFailure(page, "The two passwords differ.")
    await expect(page.getByLabel("handle:")).toHaveValue(`${taken}0`)
    // A password never comes back: it would then sit in the HTML of the answer.
    await expect(page.getByLabel("password:", { exact: true })).toHaveValue("")
    await expect(page.getByLabel("repeat:")).toHaveValue("")
    expect(await page.content()).not.toContain(PASSWORD)

    // The handle is the one taken above, so the second try hits the API rule.
    await page.getByLabel("password:", { exact: true }).fill(PASSWORD)
    await page.getByLabel("repeat:").fill(PASSWORD)
    await page.getByRole("button", { name: "ENLIST" }).click()
    await expect(page).toHaveURL(/\/signup(\?|$)/)
    await readableFailure(page, /already taken/)

    // A free handle goes through, and the form carried it all along.
    await page.getByLabel("handle:").fill(handle("fre"))
    await page.getByLabel("password:", { exact: true }).fill(PASSWORD)
    await page.getByLabel("repeat:").fill(PASSWORD)
    await page.getByRole("button", { name: "ENLIST" }).click()
    await expect(page).toHaveURL(/\/profile(\?|$)/)
})

test("the server refuses a short password and a bad handle with its own sentence", async ({
    page,
}) => {
    // The browser blocks both through `minlength` and `pattern`, so the posts go
    // straight to the action to prove the server answers a readable message too.
    const short = await page.request.post("/signup?_action=signup", {
        form: { handle: handle("shr"), password: "short", confirmPassword: "short" },
        headers: FROM_SITE,
    })
    expect(short.status()).toBe(400)
    expect(await short.text()).toContain("A password is 8 to 128 characters.")

    const bad = await page.request.post("/signup?_action=signup", {
        form: { handle: "no spaces!", password: PASSWORD, confirmPassword: PASSWORD },
        headers: FROM_SITE,
    })
    expect(bad.status()).toBe(400)
    const body = await bad.text()
    expect(body).toContain("A handle is 3 to 20 characters")
    expect(body).not.toContain("Failed to validate")
})

test("a wrong login reads as a sentence and stays on the page", async ({
    page,
    context,
}) => {
    const player = await newPlayer(context, "wrg")
    await clearSession(context)

    await page.goto("/login")
    await page.getByLabel("login:").fill(player.name)
    await page.getByLabel("password:").fill("not-the-password")
    await page.getByRole("button", { name: "LOGIN" }).click()

    await expect(page).toHaveURL(/\/login(\?|$)/)
    await readableFailure(page, /handle or the password is wrong/)
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

test("a player changes their password, and a wrong or mismatched one is refused", async ({
    page,
    context,
}) => {
    const player = await newPlayer(context, "pwd")
    const next = "another-long-passphrase"

    await page.goto("/password")
    await page.getByLabel("current:").fill("not-the-password")
    await page.getByLabel("new:", { exact: true }).fill(next)
    await page.getByLabel("repeat new:").fill(next)
    await page.getByRole("button", { name: "CHANGE PASSWORD" }).click()
    await readableFailure(page, /current password is wrong/)

    await page.getByLabel("current:").fill(PASSWORD)
    await page.getByLabel("new:", { exact: true }).fill(next)
    await page.getByLabel("repeat new:").fill("a-different-passphrase")
    await page.getByRole("button", { name: "CHANGE PASSWORD" }).click()
    await readableFailure(page, /differ/)

    await page.getByLabel("current:").fill(PASSWORD)
    await page.getByLabel("new:", { exact: true }).fill(next)
    await page.getByLabel("repeat new:").fill(next)
    await page.getByRole("button", { name: "CHANGE PASSWORD" }).click()
    await expect(page).toHaveURL(/\/profile(\?|$)/)
    await expect(page.getByRole("status")).toContainText("password changed")

    await logout(page)
    await login(page, player.name, next)
})

test("an admin reset forces the player to set a new password before anything else", async ({
    page,
    context,
}) => {
    const user = await newPlayer(context, "rst")
    await newAdmin(context)

    // The roster pages at twenty rows and other workers are enlisting, so the
    // search is the only way to be sure this row is on the page.
    await page.goto(`/admin/players?q=${user.name}`)
    page.once("dialog", dialog => dialog.accept())
    await page
        .getByRole("row", { name: new RegExp(user.name) })
        .getByRole("button", { name: "reset password" })
        .click()
    const notice = page.getByRole("status")
    await expect(notice).toContainText("temporary password")
    const temporary = (await notice.locator("p").nth(1).textContent())?.trim() ?? ""
    expect(temporary.length).toBeGreaterThanOrEqual(16)

    await clearSession(context)
    await page.goto("/login")
    await page.getByLabel("login:").fill(user.name)
    await page.getByLabel("password:").fill(temporary)
    await page.getByRole("button", { name: "LOGIN" }).click()
    await expect(page).toHaveURL(/\/password$/)

    // Nothing else opens until the password is theirs.
    await page.goto("/players")
    await expect(page).toHaveURL(/\/password$/)
    await page.goto("/handle")
    await expect(page).toHaveURL(/\/password$/)

    await page.getByLabel("current:").fill(temporary)
    await page.getByLabel("new:", { exact: true }).fill("my-own-long-passphrase")
    await page.getByLabel("repeat new:").fill("my-own-long-passphrase")
    await page.getByRole("button", { name: "CHANGE PASSWORD" }).click()
    await expect(page).toHaveURL(/\/profile(\?|$)/)

    await page.goto("/players")
    await expect(page).toHaveURL(/\/players$/)
})

test("an action name opens no page and runs no action past the password gate", async ({
    page,
    context,
}) => {
    await forcedPlayer(context)
    const held = async (path: string) => {
        const response = await page.request.post(path, {
            form: { code: "abcdefghijkl" },
            headers: FROM_SITE,
            maxRedirects: 0,
        })
        expect(response.status(), path).toBe(302)
        expect(response.headers().location, path).toBe("/password")
    }

    // A form action renders the page it posts to, so an allowed name opens
    // nothing outside the gate.
    await page.goto("/profile?_action=logout")
    await expect(page).toHaveURL(/\/password$/)
    await held("/profile?_action=logout")
    // An island call names its action in the path. A second name in the query
    // does not replace it.
    await held("/_actions/checkIn?_action=logout")
    // The password page runs only its own two actions.
    await held("/password?_action=checkIn")

    // The logout stays open, so a player on a shared screen can leave.
    await page.goto("/password")
    await page.getByRole("button", { name: "LOGOUT" }).click()
    await expect(page).toHaveURL(/\/login(\?|$)/)
    await page.goto("/profile")
    await expect(page).toHaveURL(/\/login(\?|$)/)
})

test("a player renames themself, keeps the glyph, and cannot take a used name", async ({
    page,
    context,
}) => {
    const taken = await newPlayer(context, "tkn")
    // The second signup replaces the session cookie, so the page is this player.
    const player = await newPlayer(context, "old")
    const renamed = handle("new")

    await page.goto("/profile")
    const glyph = await page.locator("[data-glyph-bits]").textContent()

    await page.getByRole("link", { name: "CHANGE HANDLE" }).click()
    await page.getByLabel("new handle:").fill(taken.name.toUpperCase())
    await page.getByRole("button", { name: "RENAME" }).click()
    await expect(page).toHaveURL(/\/handle(\?|$)/)
    await readableFailure(page, /taken/i)

    await page.getByLabel("new handle:").fill(renamed)
    await page.getByRole("button", { name: "RENAME" }).click()
    await expect(page).toHaveURL(/\/profile\?done=handle$/)
    await expect(page.getByRole("status")).toContainText("handle changed")
    await expect(
        page.getByRole("heading", { level: 1, name: renamed, exact: true }),
    ).toBeVisible()
    await expect(page.locator("[data-glyph-bits]")).toHaveText(glyph ?? "")

    expect((await page.goto(`/players/${player.name}`))?.status()).toBe(404)
    await page.goto(`/players/${renamed}`)
    await expect(
        page.getByRole("heading", { level: 1, name: renamed, exact: true }),
    ).toBeVisible()
})

test("a next link that leaves the site is dropped, on login and on signup", async ({
    page,
    context,
}) => {
    await newPlayer(context, "nxt")
    // A logged-in player is sent straight to `next`. These shapes read as
    // another origin to a browser, so every one must fall back to the profile.
    // A control character in a `Location` header fails the response, so it
    // falls back the same way.
    for (const next of [
        "//evil.example",
        "/%5Cevil.example",
        "/x%5Cevil.example",
        "/%00",
        "/x%0D%0Aset-cookie:x=1",
        "/%7F",
    ]) {
        await page.goto(`/login?next=${next}`)
        await expect(page).toHaveURL(/\/profile(\?|$)/)
        await page.goto(`/signup?next=${next}`)
        await expect(page).toHaveURL(/\/profile(\?|$)/)
    }
    // A path of this site passes.
    await page.goto("/login?next=%2Fevents")
    await expect(page).toHaveURL(/\/events$/)
})

test("the profile needs a login, and the session cookie is closed to scripts", async ({
    page,
    context,
}) => {
    await page.goto("/profile")
    await expect(page).toHaveURL(/\/login(\?|$)/)
    // A refused `next` never reaches a page as a failure either.
    expect((await page.goto("/login?next=/%00"))?.status()).toBe(200)

    await signup(page, handle("csr"))
    const cookie = (await context.cookies()).find(c => c.name === "bunker_session")
    expect(cookie?.httpOnly).toBe(true)
    expect(cookie?.sameSite).toBe("Lax")

    // A page with a player in it never sits in a shared cache, and no page can
    // be framed or sniffed.
    const profile = await page.request.get("/profile")
    expect(profile.headers()["cache-control"]).toBe("private, no-store")
    expect(profile.headers()["x-content-type-options"]).toBe("nosniff")
    expect(profile.headers()["x-frame-options"]).toBe("DENY")
    expect(profile.headers()["content-security-policy"]).toBe("frame-ancestors 'none'")

    // A user posting an admin action is refused, whatever the form holds, and
    // the backoffice page it posted to answers the same 404 as a dead link.
    const response = await page.request.post("/admin/players?_action=setRole", {
        form: { id: "00000000-0000-0000-0000-000000000001", role: "admin" },
        headers: FROM_SITE,
    })
    expect(response.status()).toBe(404)
})
