import { expect, test } from "@playwright/test"
import type { Page } from "@playwright/test"

import {
    API,
    PASSWORD,
    createDraft,
    handle,
    logout,
    signup,
    signupAdmin,
    tokenFor,
} from "./support"

/** Signs up an admin and creates a draft tournament. Lands on its edit page. */
const draftAsAdmin = async (page: Page, name: string) => {
    const admin = await signupAdmin(page)
    return { admin, id: await createDraft(page, name) }
}

test("an admin creates a tournament, opens it, and the public page lists it", async ({
    page,
}) => {
    const name = `Cup ${handle("t")}`
    const { id } = await draftAsAdmin(page, name)

    await page.goto("/tournaments")
    await expect(page.getByText(name)).toHaveCount(0)

    await page.goto(`/admin/tournaments/${id}`)
    await page.getByRole("button", { name: "open registration" }).click()
    await expect(page.getByRole("status")).toContainText("status changed")
    await expect(page.getByRole("heading", { level: 1 })).toContainText("# open")

    // Other tournaments are open at the same time, so every assertion names the
    // card of this one.
    await page.goto("/tournaments")
    const card = page.locator(`[data-tournament="${id}"]`)
    await expect(
        card.getByRole("heading", { level: 3, name: new RegExp(name) }),
    ).toBeVisible()
    await expect(card).toContainText("REGISTRATION OPEN")
    await expect(card.getByRole("button", { name: "APPLY" })).toBeVisible()

    // The homepage carries the soonest open tournament, whichever it is.
    await page.goto("/")
    await expect(page.locator("#status-block")).toContainText(/Tournament/i)
    await expect(page.getByRole("link", { name: "APPLY" })).toBeVisible()
})

test("a player applies and retires while registration is open", async ({ page }) => {
    const name = `Cup ${handle("t")}`
    const { id } = await draftAsAdmin(page, name)
    await page.getByRole("button", { name: "open registration" }).click()
    await logout(page)

    await signup(page, handle("ply"))
    await page.goto("/tournaments")
    const card = page.locator(`[data-tournament="${id}"]`)
    await card.getByRole("button", { name: "APPLY" }).click()
    await expect(page).toHaveURL(/\/tournaments\?done=applied$/)
    await expect(page.getByRole("status")).toContainText("you are in")
    await expect(card.getByText("you are in.", { exact: true })).toBeVisible()

    const detail = await page.request.get(`/tournaments/${id}/detail.json`)
    expect(detail.ok()).toBe(true)
    expect((await detail.json()).tournament.entrantCount).toBe(1)

    await card.getByRole("button", { name: "RETIRE" }).click()
    await expect(page).toHaveURL(/\/tournaments\?done=retired$/)
    await expect(card.getByRole("button", { name: "APPLY" })).toBeVisible()
})

/** Signs players up through the API and adds them as entrants. Fast, for large fields. */
const enrol = async (page: Page, admin: string, id: string, names: string[]) => {
    const token = await tokenFor(page, admin)
    for (const name of names) {
        const created = await page.request.post(`${API}/api/auth/signup`, {
            data: { handle: name, password: PASSWORD },
        })
        const player =
            (await created.json()).player ??
            (await page.request.get(`${API}/api/players/${name}`).then(r => r.json()))
        const added = await page.request.post(
            `${API}/api/admin/tournaments/${id}/entrants`,
            {
                data: { playerId: player.id },
                headers: { authorization: `Bearer ${token}` },
            },
        )
        expect(added.status()).toBe(201)
    }
}

/** A side of a match that has no winner yet. Clicking it enters that result. */
const openSide = (page: Page) =>
    page
        .locator("[data-match]")
        .filter({ hasNot: page.getByText("win", { exact: true }) })
        .locator("button[data-pick]")
        .first()

const settled = async (page: Page) => {
    await expect(page.locator("[aria-busy=true]")).toHaveCount(0)
}

test("a bracket runs from generation to a champion, live on the kiosk", async ({
    page,
    context,
}) => {
    const players = [handle("one"), handle("two"), handle("tri")]
    for (const name of players) {
        await signup(page, name)
        await logout(page)
    }
    const name = `Cup ${handle("t")}`
    const { id } = await draftAsAdmin(page, name)

    for (const player of players) {
        await page.getByLabel("handle of the player to add").fill(player)
        await page.getByRole("button", { name: "add entrant" }).click()
        await expect(page.getByRole("status")).toContainText("entrant added")
    }
    await page.getByRole("button", { name: "go live" }).click()
    await expect(page.getByRole("heading", { level: 1 })).toContainText("# live")

    await page.getByRole("button", { name: "generate bracket" }).click()
    // Three entrants: two round-one matches, one of them a bye, plus the final.
    await expect(page.locator("[data-match]")).toHaveCount(3)
    await expect(page.getByText("bye")).toHaveCount(1)

    const kiosk = await context.newPage()
    await kiosk.goto(`/tournaments/${id}/kiosk`)
    await expect(kiosk.getByText("■ live")).toBeVisible()
    await expect(kiosk.getByText("champion")).toBeVisible()
    await expect(kiosk.getByText("tbd").first()).toBeVisible()

    // The one ready match of round one, then the final.
    await openSide(page).click()
    await settled(page)
    await expect(page.getByText("win", { exact: true })).toHaveCount(1)
    await openSide(page).click()
    await settled(page)
    await expect(page.getByText("win", { exact: true })).toHaveCount(2)
    await expect(page.locator("[data-champion]")).toHaveAttribute("data-champion", /.+/)

    page.once("dialog", dialog => dialog.accept())
    await page.getByRole("button", { name: "conclude" }).click()
    await expect(page.getByRole("heading", { level: 1 })).toContainText("# concluded")
    const winner = (await page.locator("[data-winner]").getAttribute("data-winner")) ?? ""
    expect(players).toContain(winner)

    // The kiosk polls the site and follows without a reload.
    await expect(kiosk.getByText("■ concluded")).toBeVisible({ timeout: 10_000 })
    await expect(kiosk.locator("[data-champion]").getByText(winner)).toBeVisible()
    await kiosk.close()

    await page.goto("/tournaments")
    const card = page.locator(`[data-tournament="${id}"]`)
    await expect(card).toContainText("CONCLUDED")
    await expect(card.getByRole("link", { name: winner })).toBeVisible()
    expect(await card.locator("svg rect").count()).toBeGreaterThan(0)

    await page.goto(`/tournaments/${id}/bracket`)
    await expect(page.getByText("champion")).toBeVisible()
    await expect(page.getByText(winner).first()).toBeVisible()
})

test("thirty players: seeds swap by drag, and the whole bracket plays out", async ({
    page,
}) => {
    const { admin, id } = await draftAsAdmin(page, `Big ${handle("t")}`)
    const names = Array.from({ length: 30 }, (_, i) => `${handle("p")}${i.toString(36)}`)
    await enrol(page, admin, id, names)

    await page.reload()
    await expect(page.getByText("entrants # 30")).toBeVisible()
    await page.getByRole("button", { name: "go live" }).click()
    await page.getByRole("button", { name: "generate bracket" }).click()

    const matches = page.locator("[data-match]")
    await expect(matches).toHaveCount(16 + 8 + 4 + 2 + 1)
    await expect(page.getByText("bye")).toHaveCount(2)
    await expect(page.getByText("quarter-finals")).toBeVisible()

    // Drag the second side of the first match onto the first side of the last
    // round-one match: the two trade seeds and appear in each other's box.
    const roundOne = page.locator("[data-match]").locator("nth=-1")
    const first = matches.nth(0).locator("[data-entrant]").nth(0)
    const target = matches.nth(15).locator("[data-entrant]").nth(1)
    const firstId = await first.getAttribute("data-entrant")
    const targetId = await target.getAttribute("data-entrant")
    expect(firstId && targetId && roundOne).toBeTruthy()
    // The island marks the sides draggable once it is hydrated.
    await expect(first).toHaveAttribute("draggable", "true")
    // A drag cannot scroll, so both boxes must be on screen: the sixteen
    // round-one matches need a tall window.
    await page.setViewportSize({ width: 1400, height: 2200 })
    await first.dragTo(target)
    await settled(page)
    await expect(matches.nth(0).locator("[data-entrant]").nth(0)).toHaveAttribute(
        "data-entrant",
        targetId ?? "",
    )
    await expect(matches.nth(15).locator("[data-entrant]").nth(1)).toHaveAttribute(
        "data-entrant",
        firstId ?? "",
    )

    // Play every match. Thirty entrants need twenty-nine results.
    for (let played = 0; played < 29; played++) {
        await openSide(page).click()
        await settled(page)
    }
    await expect(page.getByText("win", { exact: true })).toHaveCount(29)
    await expect(page.locator("[data-champion]")).toHaveAttribute("data-champion", /.+/)
    await expect(page.getByRole("button", { name: "regenerate" })).toBeDisabled()

    page.once("dialog", dialog => dialog.accept())
    await page.getByRole("button", { name: "conclude" }).click()
    await expect(page.getByRole("heading", { level: 1 })).toContainText("# concluded")
    const winner = (await page.locator("[data-winner]").getAttribute("data-winner")) ?? ""
    expect(names).toContain(winner)

    await page.goto(`/tournaments/${id}/bracket`)
    await expect(page.locator("[data-match]")).toHaveCount(31)
    await expect(page.locator("[data-champion]").getByText(winner)).toBeVisible()
})

test("an odd field gets byes, a result can be cleared, and the bracket regenerates", async ({
    page,
}) => {
    const { admin, id } = await draftAsAdmin(page, `Odd ${handle("t")}`)
    const names = Array.from({ length: 5 }, (_, i) => `${handle("o")}${i}`)
    await enrol(page, admin, id, names)
    await page.reload()
    await page.getByRole("button", { name: "go live" }).click()
    await page.getByRole("button", { name: "generate bracket" }).click()

    // Five entrants: eight slots, three byes, one real match in round one.
    await expect(page.locator("[data-match]")).toHaveCount(7)
    await expect(page.getByText("bye")).toHaveCount(3)
    // The round one match and the semi-final that two byes already filled.
    await expect(page.getByRole("button", { name: /wins$/ })).toHaveCount(4)

    await openSide(page).click()
    await settled(page)
    await expect(page.getByText("win", { exact: true })).toHaveCount(1)
    await expect(page.getByRole("button", { name: "regenerate" })).toBeDisabled()
    await expect(page.getByRole("button", { name: "remove bracket" })).toBeDisabled()

    // A click on the lit winner takes the result back.
    await page
        .locator("[data-match]")
        .filter({ hasText: "win" })
        .locator("button[data-pick]")
        .first()
        .click()
    await settled(page)
    await expect(page.getByText("win", { exact: true })).toHaveCount(0)
    await expect(page.getByRole("button", { name: "regenerate" })).toBeEnabled()

    await page.getByRole("button", { name: "regenerate" }).click()
    await expect(page.locator("[data-match]")).toHaveCount(7)
    await expect(page.getByText("bye")).toHaveCount(3)

    page.once("dialog", dialog => dialog.accept())
    await page.getByRole("button", { name: "remove bracket" }).click()
    await expect(page.locator("[data-match]")).toHaveCount(0)
    await expect(page.getByRole("button", { name: "generate bracket" })).toBeVisible()
})

test("only a round one side can be dragged or dropped on", async ({ page }) => {
    const { admin, id } = await draftAsAdmin(page, `Drag ${handle("t")}`)
    // Five entrants leave three byes, so two players reach round two with no
    // result behind them. Those sides take a click and must take no drop.
    await enrol(
        page,
        admin,
        id,
        Array.from({ length: 5 }, (_, i) => `${handle("d")}${i}`),
    )
    await page.reload()
    await page.getByRole("button", { name: "go live" }).click()
    await page.getByRole("button", { name: "generate bracket" }).click()
    await expect(page.locator("[data-match]")).toHaveCount(7)

    // Five entrants fill five sides of round one, and each one can be dragged.
    await expect(
        page.locator("[data-round='1'] [data-entrant][draggable=true]"),
    ).toHaveCount(5)

    // A later round holds sides that take a result. None of them may drag or
    // accept a drop: a swap must never ride on a click target.
    const later = page.locator("[data-round]:not([data-round='1'])")
    await expect(later.locator("button[data-pick]")).not.toHaveCount(0)
    await expect(later.locator("[draggable=true]")).toHaveCount(0)
})

test("a user cannot open the tournament backoffice", async ({ page }) => {
    await signup(page, handle("usr"))
    const hidden = await page.goto("/admin/tournaments")
    expect(hidden?.status()).toBe(404)
})

test("an open tournament past its deadline shows registration closed", async ({
    page,
}) => {
    const { admin, id } = await draftAsAdmin(page, `Late ${handle("t")}`)
    const token = await tokenFor(page, admin)
    const past = new Date(Date.now() - 3_600_000).toISOString()
    const patched = await page.request.patch(`${API}/api/admin/tournaments/${id}`, {
        data: { registrationClosesAt: past },
        headers: { authorization: `Bearer ${token}` },
    })
    expect(patched.ok()).toBe(true)
    await page.getByRole("button", { name: "open registration" }).click()

    await page.goto("/tournaments")
    const card = page.locator(`[data-tournament="${id}"]`)
    await expect(card).toContainText("REGISTRATION CLOSED")
    await expect(card.getByRole("button", { name: "APPLY" })).toHaveCount(0)
    await expect(card.getByRole("link", { name: "LOGIN TO APPLY" })).toHaveCount(0)
})
