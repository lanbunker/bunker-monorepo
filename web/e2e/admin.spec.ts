import { expect, test } from "@playwright/test"

import { API, PASSWORD, createDraft, handle, signupAdmin, tokenFor } from "./support"

test("the overview counts the rows and reports the api as up", async ({ page }) => {
    await signupAdmin(page)
    await page.goto("/admin")
    await expect(page.getByRole("heading", { level: 1 })).toContainText(
        "root@bunker:~# status",
    )
    await expect(page.locator("[data-stat=api]")).toHaveText("up")
    // A count is a number. The page shows "?" only when the API is down. The
    // value sits on its own line in the markup, so the match allows the space.
    await expect(page.locator("[data-stat=players]")).toHaveText(/^\s*\d+\s*$/)
    await expect(page.locator("[data-stat=tournaments]")).toHaveText(/^\s*\d+\s*$/)
})

test("an admin edits the details of a tournament and the change survives a reload", async ({
    page,
}) => {
    await signupAdmin(page)
    const id = await createDraft(page, `Edit ${handle("t")}`)

    const renamed = `Renamed ${handle("t")}`
    await page.getByLabel("name").fill(renamed)
    await page.getByLabel("mode").fill("2v2 team")
    await page.getByLabel("description").fill("a longer description of the rules")
    await page.getByRole("button", { name: "save" }).click()

    await expect(page).toHaveURL(new RegExp(`/admin/tournaments/${id}\\?done=saved$`))
    await expect(page.getByRole("status")).toContainText("saved")

    await page.reload()
    await expect(page.getByLabel("name")).toHaveValue(renamed)
    await expect(page.getByLabel("mode")).toHaveValue("2v2 team")
    await expect(page.getByLabel("description")).toHaveValue(
        "a longer description of the rules",
    )
})

test("a refresh after a change does not repeat it", async ({ page }) => {
    await signupAdmin(page)
    const id = await createDraft(page, `Once ${handle("t")}`)

    await page.getByRole("button", { name: "open registration" }).click()
    await expect(page).toHaveURL(new RegExp(`/admin/tournaments/${id}\\?done=status$`))

    // The URL after the redirect is a GET, so a reload sends no action again.
    await page.reload()
    await expect(page.getByRole("heading", { level: 1 })).toContainText("# open")
    await expect(page.getByRole("alert")).toHaveCount(0)
})

test("an entrant is added, listed and removed", async ({ page }) => {
    const admin = await signupAdmin(page)
    await createDraft(page, `Roster ${handle("t")}`)
    const player = handle("ent")
    const signedUp = await page.request.post(`${API}/api/auth/signup`, {
        data: { handle: player, password: PASSWORD },
    })
    expect(signedUp.status()).toBe(201)

    await page.getByLabel("handle of the player to add").fill(player)
    await page.getByRole("button", { name: "add entrant" }).click()
    await expect(page.getByRole("status")).toContainText("entrant added")
    const entrants = page.getByRole("table", { name: "entrants" })
    await expect(entrants.getByRole("row", { name: new RegExp(player) })).toBeVisible()
    await expect(page.getByText("entrants # 1")).toBeVisible()

    page.once("dialog", dialog => dialog.accept())
    await entrants
        .getByRole("row", { name: new RegExp(player) })
        .getByRole("button", { name: "remove" })
        .click()
    await expect(page.getByRole("status")).toContainText("entrant removed")
    await expect(page.getByText("nobody yet.")).toBeVisible()

    // The same player is still in the roster: a removed entrant is not deleted.
    const still = await page.request.get(`${API}/api/players/${player}`)
    expect(still.ok()).toBe(true)
    expect(admin).not.toBe(player)
})

test("adding the same player twice leaves one entrant, not two", async ({ page }) => {
    await signupAdmin(page)
    await createDraft(page, `Dup ${handle("t")}`)
    const player = handle("twi")
    await page.request.post(`${API}/api/auth/signup`, {
        data: { handle: player, password: PASSWORD },
    })

    for (const attempt of ["first", "second"]) {
        await page.getByLabel("handle of the player to add").fill(player)
        await page.getByRole("button", { name: "add entrant" }).click()
        await expect(page.getByRole("status"), attempt).toContainText("entrant added")
    }

    // The API takes the second add without a complaint and keeps one row.
    await expect(page.getByText("entrants # 1")).toBeVisible()
    await expect(
        page.getByRole("table", { name: "entrants" }).getByRole("row"),
    ).toHaveCount(2)
    await expect(page.getByRole("alert")).toHaveCount(0)
})

test("a concluded tournament takes no more changes", async ({ page }) => {
    await signupAdmin(page)
    await createDraft(page, `Done ${handle("t")}`)

    page.once("dialog", dialog => dialog.accept())
    await page.getByRole("button", { name: "conclude" }).click()
    await expect(page.getByRole("heading", { level: 1 })).toContainText("# concluded")

    await expect(page.getByRole("button", { name: "add entrant" })).toHaveCount(0)
    await expect(page.getByRole("button", { name: "conclude" })).toHaveCount(0)
    await expect(page.getByRole("button", { name: "open registration" })).toHaveCount(0)
})

test("the tournament list pages and every row links to its editor", async ({ page }) => {
    const admin = await signupAdmin(page)
    const token = await tokenFor(page, admin)
    const made = Array.from({ length: 21 }, (_, i) => i)
    for (const index of made) {
        const response = await page.request.post(`${API}/api/admin/tournaments`, {
            data: {
                name: `Page ${handle("t")}${index}`,
                game: "COD MW2",
                mode: "1v1",
                description: "",
                date: "2030-03-03",
                registrationClosesAt: "2029-12-31T20:00:00Z",
            },
            headers: { authorization: `Bearer ${token}` },
        })
        expect(response.status()).toBe(201)
    }

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

test("a player cannot promote themself through a hand-made post", async ({ page }) => {
    const admin = await signupAdmin(page)
    const victim = handle("vic")
    await page.request.post(`${API}/api/auth/signup`, {
        data: { handle: victim, password: PASSWORD },
    })
    const token = await tokenFor(page, victim)

    const promotion = await page.request.patch(`${API}/api/admin/players/me`, {
        data: { role: "admin" },
        headers: { authorization: `Bearer ${token}` },
    })
    expect(promotion.status()).toBeGreaterThanOrEqual(400)

    // The admin sees them still as a user.
    await page.goto("/admin/players")
    await expect(page.getByRole("row", { name: new RegExp(victim) })).toContainText(
        "user",
    )
    expect(admin).not.toBe(victim)
})
