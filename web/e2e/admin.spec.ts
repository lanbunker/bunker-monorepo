import { expect, test } from "@playwright/test"

import {
    API,
    apiDraft,
    apiSignup,
    bearer,
    createDraft,
    handle,
    newAdmin,
    newPlayer,
    signupMany,
} from "./support"

/** The backoffice: who reaches it, what it lists, and what it changes. */

/** Every change redirects to the whole roster, so each step searches again. */
const adminRoster = (name: string) => `/admin/players?q=${name}`

test("the backoffice is closed to a user and open to an admin", async ({
    page,
    context,
}) => {
    await newPlayer(context, "usr")
    expect((await page.goto("/admin"))?.status()).toBe(404)
    expect((await page.goto("/admin/tournaments"))?.status()).toBe(404)

    await newAdmin(context)
    await page.goto("/admin")
    await expect(page.getByRole("link", { name: "BACKOFFICE root@bunker" })).toBeVisible()
    await expect(page.getByRole("heading", { level: 1 })).toContainText(
        "root@bunker:~# status",
    )
    await expect(page.locator("[data-stat=api]")).toHaveText("up")
    // A count is a number. The page shows "?" only when the API is down. The
    // value sits on its own line in the markup, so the match allows the space.
    await expect(page.locator("[data-stat=players]")).toHaveText(/^\s*\d+\s*$/)
    await expect(page.locator("[data-stat=tournaments]")).toHaveText(/^\s*\d+\s*$/)
})

test("an admin promotes, renames and deletes a player from the roster", async ({
    page,
    context,
}) => {
    const user = await newPlayer(context, "usr")
    await newAdmin(context)
    await page.goto(adminRoster(user.name))
    const row = (name: string) => page.getByRole("row", { name: new RegExp(name) })
    await expect(row(user.name)).toContainText("user")
    await row(user.name).getByRole("button", { name: "promote" }).click()
    await expect(page.getByRole("status")).toContainText(`${user.name} is now admin`)

    await page.goto(adminRoster(user.name))
    await expect(row(user.name)).toContainText("admin")
    const renamed = handle("ren")
    await row(user.name).getByLabel(`new handle for ${user.name}`).fill(renamed)
    await row(user.name).getByRole("button", { name: "rename" }).click()
    await expect(page.getByRole("status")).toContainText(`renamed to ${renamed}`)

    await page.goto(adminRoster(renamed))
    await expect(row(renamed)).toBeVisible()
    await expect(row(user.name)).toHaveCount(0)
    page.once("dialog", dialog => dialog.accept())
    await row(renamed).getByRole("button", { name: "delete" }).click()
    await expect(page.getByRole("status")).toContainText("player deleted")

    await page.goto(adminRoster(renamed))
    await expect(page.getByRole("search")).toContainText("0 matches")
})

test("a player cannot promote themself through a hand-made post", async ({
    page,
    context,
}) => {
    const victim = await newPlayer(context, "vic")
    const promotion = await page.request.patch(`${API}/api/admin/players/me`, {
        data: { role: "admin" },
        headers: bearer(victim.token),
    })
    expect(promotion.status()).toBeGreaterThanOrEqual(400)

    // The admin sees them still as a user.
    await newAdmin(context)
    await page.goto(`/admin/players?q=${victim.name}`)
    await expect(page.getByRole("row", { name: new RegExp(victim.name) })).toContainText(
        "user",
    )
})

test("the roster search scopes the rows, counts them, pages them and clears", async ({
    page,
    context,
}) => {
    await newAdmin(context)
    const prefix = handle("apq")
    await signupMany(context.request, prefix, 21)

    await page.goto("/admin/players")
    await expect(page.getByRole("heading", { level: 1 })).toContainText(
        "root@bunker:~# players",
    )
    const field = page.getByLabel("search players by name")
    await field.fill(`${prefix.toUpperCase()}0`)
    await field.press("Enter")
    await expect(page).toHaveURL(/\/admin\/players\?q=/)
    await expect(page.getByRole("row", { name: new RegExp(`${prefix}0`) })).toBeVisible()
    await expect(page.getByRole("search")).toContainText("1 match")

    // The pager keeps the term, and the last page holds the rest of the field.
    await page.goto(`/admin/players?q=${prefix}`)
    const pager = page.getByRole("navigation", { name: "Pages" })
    await expect(pager).toContainText("page 1 of 2")
    await expect(page.locator("tbody tr")).toHaveCount(20)
    await pager.getByRole("link", { name: "NEXT →" }).click()
    await expect(page).toHaveURL(new RegExp(`/admin/players\\?q=${prefix}&page=2$`))
    await expect(page.locator("tbody tr")).toHaveCount(1)

    await page.getByRole("link", { name: "CLEAR" }).click()
    await expect(page).toHaveURL(/\/admin\/players$/)
    await expect(page.getByRole("search")).toContainText("rows")

    await page.goto("/admin/players?q=nobody-has-this")
    await expect(page.getByRole("search")).toContainText("0 matches")
    await expect(page.locator("tbody tr")).toHaveCount(0)
})

test("an admin edits the details of a tournament and the change survives a reload", async ({
    page,
    context,
}) => {
    await newAdmin(context)
    const id = await createDraft(page, `Edit ${handle("t")}`)

    const renamed = `Renamed ${handle("t")}`
    await page.getByLabel("name", { exact: true }).fill(renamed)
    await page.getByLabel("mode").fill("2v2 team")
    await page.getByLabel("description").fill("a longer description of the rules")
    await page.getByRole("button", { name: "save" }).click()

    await expect(page).toHaveURL(new RegExp(`/admin/tournaments/${id}\\?done=saved$`))
    await expect(page.getByRole("status")).toContainText("saved")

    await page.reload()
    await expect(page.getByLabel("name", { exact: true })).toHaveValue(renamed)
    await expect(page.getByLabel("mode")).toHaveValue("2v2 team")
    await expect(page.getByLabel("description")).toHaveValue(
        "a longer description of the rules",
    )
})

test("a refresh after a change does not repeat it", async ({ page, context }) => {
    await newAdmin(context)
    const id = await createDraft(page, `Once ${handle("t")}`)

    await page.getByRole("button", { name: "open registration" }).click()
    await expect(page).toHaveURL(new RegExp(`/admin/tournaments/${id}\\?done=status$`))

    // The URL after the redirect is a GET, so a reload sends no action again.
    await page.reload()
    await expect(page.getByRole("heading", { level: 1 })).toContainText("# open")
    await expect(page.getByRole("alert")).toHaveCount(0)
})

test("an entrant is added once however often, listed, and removed", async ({
    page,
    context,
}) => {
    const admin = await newAdmin(context)
    const id = await apiDraft(context.request, admin.token)
    const player = handle("ent")
    await apiSignup(context.request, player)

    await page.goto(`/admin/tournaments/${id}`)
    // The API takes a second add without a complaint and keeps one row.
    for (const attempt of ["first", "second"]) {
        await page.getByLabel("handle of the player to add").fill(player)
        await page.getByRole("button", { name: "add entrant" }).click()
        await expect(page.getByRole("status"), attempt).toContainText("entrant added")
    }
    const entrants = page.getByRole("table", { name: "entrants" })
    await expect(entrants.getByRole("row", { name: new RegExp(player) })).toBeVisible()
    await expect(entrants.getByRole("row")).toHaveCount(2)
    await expect(page.getByText("entrants # 1")).toBeVisible()
    await expect(page.getByRole("alert")).toHaveCount(0)

    page.once("dialog", dialog => dialog.accept())
    await entrants
        .getByRole("row", { name: new RegExp(player) })
        .getByRole("button", { name: "remove" })
        .click()
    await expect(page.getByRole("status")).toContainText("entrant removed")
    await expect(page.getByText("nobody yet.")).toBeVisible()

    // The same player is still in the roster: a removed entrant is not deleted.
    expect((await page.request.get(`${API}/api/players/${player}`)).ok()).toBe(true)
})

test("a concluded tournament takes no more changes", async ({ page, context }) => {
    const admin = await newAdmin(context)
    const id = await apiDraft(context.request, admin.token)

    await page.goto(`/admin/tournaments/${id}`)
    page.once("dialog", dialog => dialog.accept())
    await page.getByRole("button", { name: "conclude" }).click()
    await expect(page.getByRole("heading", { level: 1 })).toContainText("# concluded")

    await expect(page.getByRole("button", { name: "add entrant" })).toHaveCount(0)
    await expect(page.getByRole("button", { name: "conclude" })).toHaveCount(0)
    await expect(page.getByRole("button", { name: "open registration" })).toHaveCount(0)
})

test("the tournament list pages and every row links to its editor", async ({
    page,
    context,
}) => {
    const admin = await newAdmin(context)
    await Promise.all(
        Array.from({ length: 21 }, () => apiDraft(context.request, admin.token)),
    )

    await page.goto("/admin/tournaments")
    const pager = page.getByRole("navigation", { name: "Pages" })
    await expect(pager).toContainText("page 1 of")
    await expect(page.locator("tbody tr")).toHaveCount(20)
    await pager.getByRole("link", { name: "NEXT →" }).click()
    await expect(page).toHaveURL(/\/admin\/tournaments\?page=2$/)

    await page.locator("tbody tr a").first().click()
    await expect(page).toHaveURL(/\/admin\/tournaments\/[0-9a-f-]{36}$/)
    await expect(page.getByRole("heading", { level: 1 })).toContainText("root@bunker")
})
