import { expect, test } from "@playwright/test"

import {
    API,
    PASSWORD,
    handle,
    logout,
    readableFailure,
    signup,
    signupAdmin,
    tokenFor,
} from "./support"

/** A well-formed UUID that matches no row. A nil-shaped id is not a valid UUID. */
const MISSING_ID = "3f2504e0-4f89-41d3-9a0c-0305e82c3301"

/**
 * Every failure a player can cause must reach the page as one sentence they can
 * act on. Astro reports a refused input as JSON inside the message, and the API
 * reports its own refusals in a body. Both go through the same helper on the
 * site, so both are checked here.
 */

test("a signup with two different passwords names the field, not the schema", async ({
    page,
}) => {
    await page.goto("/signup")
    await page.getByLabel("handle:").fill(handle("mix"))
    await page.getByLabel("password:", { exact: true }).fill(PASSWORD)
    await page.getByLabel("repeat:").fill("something-else-entirely")
    await page.getByRole("button", { name: "ENLIST" }).click()

    await expect(page).toHaveURL(/\/signup(\?|$)/)
    await readableFailure(page, "The two passwords differ.")
})

test("a short password and a bad handle each get their own sentence", async ({
    page,
}) => {
    // The browser blocks both through `minlength` and `pattern`, so the posts go
    // straight to the action to prove the server answers a readable message too.
    const short = await page.request.post("/signup?_action=signup", {
        form: { handle: handle("shr"), password: "short", confirmPassword: "short" },
        headers: { origin: "http://127.0.0.1:4399" },
    })
    expect(short.status()).toBe(400)
    expect(await short.text()).toContain("A password is 8 to 128 characters.")

    const bad = await page.request.post("/signup?_action=signup", {
        form: { handle: "no spaces!", password: PASSWORD, confirmPassword: PASSWORD },
        headers: { origin: "http://127.0.0.1:4399" },
    })
    expect(bad.status()).toBe(400)
    const body = await bad.text()
    expect(body).toContain("A handle is 3 to 20 characters")
    expect(body).not.toContain("Failed to validate")
})

test("a taken handle shows the message the api wrote", async ({ page }) => {
    const name = handle("tkn")
    await signup(page, name)
    await logout(page)

    await page.goto("/signup")
    await page.getByLabel("handle:").fill(name)
    await page.getByLabel("password:", { exact: true }).fill(PASSWORD)
    await page.getByLabel("repeat:").fill(PASSWORD)
    await page.getByRole("button", { name: "ENLIST" }).click()

    await expect(page).toHaveURL(/\/signup(\?|$)/)
    await readableFailure(page, /already taken/)
})

test("a wrong login and a wrong current password each read as a sentence", async ({
    page,
}) => {
    const name = handle("wrg")
    await signup(page, name)
    await logout(page)

    await page.getByLabel("login:").fill(name)
    await page.getByLabel("password:").fill("not-the-password")
    await page.getByRole("button", { name: "LOGIN" }).click()
    await readableFailure(page, /handle or the password is wrong/)

    await page.getByLabel("login:").fill(name)
    await page.getByLabel("password:").fill(PASSWORD)
    await page.getByRole("button", { name: "LOGIN" }).click()
    await expect(page).toHaveURL(/\/profile(\?|$)/)

    await page.goto("/password")
    await page.getByLabel("current:").fill("not-the-password")
    await page.getByLabel("new:", { exact: true }).fill("another-long-passphrase")
    await page.getByLabel("repeat new:").fill("another-long-passphrase")
    await page.getByRole("button", { name: "CHANGE PASSWORD" }).click()
    await readableFailure(page, /current password is wrong/)
})

test("an admin adding an unknown handle is told, and the page still works", async ({
    page,
}) => {
    const admin = await signupAdmin(page)
    const token = await tokenFor(page, admin)
    const created = await page.request.post(`${API}/api/admin/tournaments`, {
        data: {
            name: `Cup ${handle("e")}`,
            game: "COD MW2",
            mode: "1v1",
            description: "",
            date: "2030-01-01",
            registrationClosesAt: "2029-12-31T20:00:00Z",
        },
        headers: { authorization: `Bearer ${token}` },
    })
    expect(created.status()).toBe(201)
    const body: Record<string, unknown> = await created.json()
    const id = String(body.id)

    await page.goto(`/admin/tournaments/${id}`)
    await page.getByLabel("handle of the player to add").fill("nobody-at-all")
    await page.getByRole("button", { name: "add entrant" }).click()
    await readableFailure(page, /not found|no player/i)
    // The failure is a message, not a broken page: the form is still there.
    await expect(page.getByLabel("handle of the player to add")).toBeVisible()
})

test("a tournament form with no date is refused with a sentence", async ({ page }) => {
    await signupAdmin(page)
    const refused = await page.request.post(
        "/admin/tournaments?_action=createTournament",
        {
            form: {
                name: "No date cup",
                game: "COD MW2",
                mode: "1v1",
                description: "",
                date: "",
                registrationClosesAt: "",
            },
            headers: { origin: "http://127.0.0.1:4399" },
        },
    )
    expect(refused.status()).toBe(400)
    const body = await refused.text()
    expect(body).toMatch(/Pick a date\.|Pick the moment registration closes\./)
    expect(body).not.toContain("Failed to validate")
})

test("a logged out visitor cannot apply, and a user cannot run an admin action", async ({
    page,
}) => {
    const applyAnonymously = await page.request.post(
        "/tournaments?_action=applyToTournament",
        {
            form: { id: MISSING_ID },
            headers: { origin: "http://127.0.0.1:4399" },
        },
    )
    expect(applyAnonymously.status()).toBe(401)

    // The action refuses a user, and the backoffice page it posts to rewrites
    // to 404 for the same user. The answer is 404: the site never tells a
    // stranger that the page exists.
    await signup(page, handle("usr"))
    const asUser = await page.request.post(
        "/admin/tournaments?_action=deleteTournament",
        {
            form: { id: MISSING_ID },
            headers: { origin: "http://127.0.0.1:4399" },
        },
    )
    expect(asUser.status()).toBe(404)
    expect(await asUser.text()).not.toContain("root@bunker")
})

test("a refused form keeps what was typed, and never a password", async ({ page }) => {
    const name = handle("kep")
    await page.goto("/signup")
    await page.getByLabel("handle:").fill(name)
    await page.getByLabel("password:", { exact: true }).fill(PASSWORD)
    await page.getByLabel("repeat:").fill("a-different-passphrase")
    await page.getByRole("button", { name: "ENLIST" }).click()

    await readableFailure(page, "The two passwords differ.")
    await expect(page.getByLabel("handle:")).toHaveValue(name)
    // A password never comes back: it would then sit in the HTML of the answer.
    await expect(page.getByLabel("password:", { exact: true })).toHaveValue("")
    await expect(page.getByLabel("repeat:")).toHaveValue("")
    expect(await page.content()).not.toContain(PASSWORD)

    // The second try goes through with the handle already in place.
    await page.getByLabel("password:", { exact: true }).fill(PASSWORD)
    await page.getByLabel("repeat:").fill(PASSWORD)
    await page.getByRole("button", { name: "ENLIST" }).click()
    await expect(page).toHaveURL(/\/profile(\?|$)/)
})

test("a refused draft keeps every field of the tournament form", async ({ page }) => {
    await signupAdmin(page)
    await page.goto("/admin/tournaments")
    const name = `Keep ${handle("t")}`
    await page.getByLabel("name").fill(name)
    await page.getByLabel("game").fill("COD MW2")
    await page.getByLabel("description").fill("the rules of the cup")
    await page.getByLabel("date").fill("2030-05-05")
    await page
        .getByLabel("registration closes (your local time)")
        .fill("2030-05-04T20:00")
    // A mode of spaces alone passes `required` in the browser and fails the
    // rule on the server, so the post reaches the action.
    await page.getByLabel("mode").fill("   ")
    await page.getByRole("button", { name: "CREATE DRAFT" }).click()

    await readableFailure(page, "A tournament needs a mode.")
    await expect(page.getByLabel("name")).toHaveValue(name)
    await expect(page.getByLabel("game")).toHaveValue("COD MW2")
    await expect(page.getByLabel("description")).toHaveValue("the rules of the cup")
    await expect(page.getByLabel("date")).toHaveValue("2030-05-05")
})

test("a malformed id is refused before it reaches the api", async ({ page }) => {
    await signup(page, handle("mal"))
    const refused = await page.request.post("/tournaments?_action=applyToTournament", {
        form: { id: "not-a-uuid" },
        headers: { origin: "http://127.0.0.1:4399" },
    })
    expect(refused.status()).toBe(400)
    expect(await refused.text()).not.toContain("Failed to validate")
})
