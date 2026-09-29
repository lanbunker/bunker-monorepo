import { expect, test } from "@playwright/test"
import type { Page } from "@playwright/test"

import {
    apiDraft,
    apiSignup,
    clearSession,
    createDraft,
    cyclesOf,
    cyclesRules,
    detailSchema,
    enrol,
    handle,
    jsonOf,
    newAdmin,
    newPlayer,
    setTournamentStatus,
    standingOf,
} from "./support"

/** A side of a match that has no winner yet. Clicking it enters that result. */
const openSide = (page: Page) =>
    page
        .locator("[data-match]")
        .filter({ hasNot: page.getByText("win", { exact: true }) })
        .locator("button[data-pick]")
        .first()

/** The island answers every click, so the next one waits for the last to land. */
const settled = async (page: Page) => {
    await expect(page.locator("[aria-busy=true]")).toHaveCount(0)
}

test("an admin creates a tournament, opens it, and the public page lists it", async ({
    page,
    context,
}) => {
    await newAdmin(context)
    const name = `Cup ${handle("t")}`
    const id = await createDraft(page, name)

    // A draft is not on the site.
    await page.goto("/tournaments")
    await expect(page.locator(`[data-tournament="${id}"]`)).toHaveCount(0)

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
    await expect(card.getByRole("link", { name: "APPLY" })).toBeVisible()

    // The homepage carries the soonest open tournament of the whole database,
    // and another worker can own that one. What holds whichever it is: this
    // test keeps at least one tournament open whose deadline has not passed, so
    // the block names a tournament, and the way in shows without the note that
    // registration is closed.
    await page.goto("/")
    const block = page.locator("#status-block")
    await expect(block).toContainText(/Tournament/i)
    await expect(block).not.toContainText("registration closed")
    await expect(page.getByRole("link", { name: "APPLY" })).toBeVisible()
})

test("a player applies and retires while registration is open", async ({
    page,
    context,
}) => {
    const admin = await newAdmin(context)
    const id = await apiDraft(context.request, admin.token)
    await setTournamentStatus(context.request, admin.token, id, "open")
    const player = await newPlayer(context, "ply")

    await page.goto("/tournaments")
    const card = page.locator(`[data-tournament="${id}"]`)
    await card.getByRole("link", { name: "APPLY" }).click()
    await expect(page).toHaveURL(`/tournaments/${id}/apply`)

    // The form names the player who applies, with their glyph, and starts in
    // the middle of the scale. The header names the player too, so the check
    // stays inside the form.
    await expect(
        page.locator("form").getByText(player.name, { exact: true }),
    ).toBeVisible()
    expect(await page.locator("form svg rect").count()).toBeGreaterThan(0)
    await expect(page.getByRole("radio", { name: /level 3/ })).toBeChecked()
    await expect(page.getByText("REGULAR")).toBeVisible()
    await page.getByRole("radio", { name: /level 4/ }).check()
    await expect(page.getByText("SHARP")).toBeVisible()
    await expect(page.getByText("REGULAR")).toBeHidden()
    await page.getByRole("button", { name: "CONFIRM ENTRY" }).click()

    await expect(page).toHaveURL(/\/tournaments\?done=applied$/)
    await expect(page.getByRole("status")).toContainText("you are in")
    await expect(card.getByText("you are in.", { exact: true })).toBeVisible()

    const detail = await jsonOf(
        await page.request.get(`/tournaments/${id}/detail.json`),
        detailSchema,
    )
    expect(detail.tournament.entrantCount).toBe(1)
    expect(detail.entrants[0]?.skill).toBe(4)

    // An entrant has nothing to do on the apply page: the level changes
    // through retire and apply.
    await page.goto(`/tournaments/${id}/apply`)
    await expect(page).toHaveURL(/\/tournaments\?done=registered$/)
    await expect(page.getByRole("status")).toContainText("retire first")

    await card.getByRole("button", { name: "RETIRE" }).click()
    await expect(page).toHaveURL(/\/tournaments\?done=retired$/)
    await expect(card.getByRole("link", { name: "APPLY" })).toBeVisible()
})

test("the apply page needs a login, and a draft is hidden from a player", async ({
    page,
    context,
}) => {
    const admin = await newAdmin(context)
    const id = await apiDraft(context.request, admin.token)
    await clearSession(context)

    await page.goto(`/tournaments/${id}/apply`)
    await expect(page).toHaveURL(
        `/login?next=${encodeURIComponent(`/tournaments/${id}/apply`)}`,
    )

    // A draft is not there for a player. The page says so with a 404.
    await newPlayer(context, "nob")
    expect((await page.goto(`/tournaments/${id}/apply`))?.status()).toBe(404)
})

test("an open tournament past its deadline shows registration closed", async ({
    page,
    context,
}) => {
    const admin = await newAdmin(context)
    const past = new Date(Date.now() - 3_600_000).toISOString()
    const id = await apiDraft(context.request, admin.token, {
        registrationClosesAt: past,
    })
    await setTournamentStatus(context.request, admin.token, id, "open")

    await page.goto("/tournaments")
    const card = page.locator(`[data-tournament="${id}"]`)
    await expect(card).toContainText("REGISTRATION CLOSED")
    await expect(card.getByRole("link", { name: "APPLY" })).toHaveCount(0)
    await expect(card.getByRole("link", { name: "LOGIN TO APPLY" })).toHaveCount(0)
})

test("the bracket is seeded by level, and the backoffice shows each level", async ({
    page,
    context,
}) => {
    const admin = await newAdmin(context)
    const id = await apiDraft(context.request, admin.token)
    const [rookie, menace, casual, unrated, sharp] = [
        handle("rk"),
        handle("mn"),
        handle("cs"),
        handle("un"),
        handle("sh"),
    ]
    await enrol(
        context.request,
        admin.token,
        id,
        [rookie, menace, casual, unrated, sharp],
        [1, 5, 2, undefined, 4],
    )

    await page.goto(`/admin/tournaments/${id}`)
    const rows = page.getByRole("table", { name: "entrants" }).locator("tbody tr")
    await expect(rows).toHaveCount(5)
    await expect(rows.filter({ hasText: menace }).locator("[data-skill]")).toHaveText(
        "▮▮▮▮▮",
    )
    await expect(rows.filter({ hasText: rookie }).locator("[data-skill]")).toHaveText(
        "▮▯▯▯▯",
    )
    await expect(rows.filter({ hasText: unrated }).locator("[data-skill]")).toHaveText(
        "-----",
    )

    // The add form takes a level too.
    const rated = handle("rt")
    await apiSignup(context.request, rated)
    await page.getByLabel("handle of the player to add").fill(rated)
    await page.getByLabel("level of the player to add").selectOption("3")
    await page.getByRole("button", { name: "add entrant" }).click()
    await expect(page.getByRole("status")).toContainText("entrant added")
    await expect(rows.filter({ hasText: rated }).locator("[data-skill]")).toHaveText(
        "▮▮▮▯▯",
    )

    await page.getByRole("button", { name: "go live" }).click()
    await expect(page.getByText("seeded by level")).toBeVisible()
    await page.getByRole("button", { name: "generate bracket" }).click()
    await expect(page.getByText("regenerate redraws only among")).toBeVisible()
    const matches = page.locator("[data-match]")
    await expect(matches).toHaveCount(7)

    // Six entrants: the two top levels get the byes, then neighbours pair up.
    // The two threes are drawn at random, so the check is on the level and not
    // on the name.
    await expect(matches.nth(0)).toContainText(menace)
    await expect(matches.nth(0)).toContainText("bye")
    await expect(matches.nth(1)).toContainText(sharp)
    await expect(matches.nth(1)).toContainText("bye")
    await expect(matches.nth(2)).toContainText(unrated)
    await expect(matches.nth(2)).toContainText(rated)
    await expect(matches.nth(3)).toContainText(casual)
    await expect(matches.nth(3)).toContainText(rookie)

    // The seeds in the table follow the same order.
    const seedOf = (row: number) => rows.nth(row).locator("td").first()
    await expect(rows.nth(0)).toContainText(menace)
    await expect(seedOf(0)).toHaveText("1")
    await expect(rows.nth(1)).toContainText(sharp)
    await expect(seedOf(1)).toHaveText("2")
    await expect(rows.nth(4)).toContainText(casual)
    await expect(seedOf(4)).toHaveText("5")
    await expect(rows.nth(5)).toContainText(rookie)
    await expect(seedOf(5)).toHaveText("6")
})

test("a bracket runs from generation to a champion, live on the kiosk", async ({
    page,
    context,
}) => {
    const admin = await newAdmin(context)
    const name = `Cup ${handle("t")}`
    const id = await apiDraft(context.request, admin.token, { name })
    // Six entrants: eight slots, two byes, and three rounds to the final. It is
    // the smallest field that proves a winner advances more than once.
    const players = Array.from({ length: 6 }, (_, i) => `${handle("br")}${i}`)
    await enrol(context.request, admin.token, id, players)

    // Before a bracket exists the public pages are a 404, not an empty frame.
    for (const path of [`/tournaments/${id}/bracket`, `/tournaments/${id}/kiosk`]) {
        expect((await page.goto(path))?.status(), path).toBe(404)
    }

    await page.goto(`/admin/tournaments/${id}`)
    await page.getByRole("button", { name: "go live" }).click()
    await expect(page.getByRole("heading", { level: 1 })).toContainText("# live")
    await page.getByRole("button", { name: "generate bracket" }).click()
    await expect(page.locator("[data-match]")).toHaveCount(7)
    await expect(page.getByText("bye")).toHaveCount(2)
    await expect(page.getByText("semi-finals")).toBeVisible()

    const kiosk = await context.newPage()
    await kiosk.goto(`/tournaments/${id}/kiosk`)
    await expect(kiosk.getByText("■ live")).toBeVisible()
    await expect(kiosk.getByText("champion")).toBeVisible()
    await expect(kiosk.locator("[data-match]")).toHaveCount(7)
    await expect(kiosk.getByText("tbd").first()).toBeVisible()

    // Two real matches in round one, then the two semi-finals, then the final.
    // A side with no opponent yet carries no control, so each click lands on a
    // match that is ready.
    for (let played = 1; played <= 5; played += 1) {
        await openSide(page).click()
        await settled(page)
        await expect(page.getByText("win", { exact: true })).toHaveCount(played)
    }
    await expect(page.locator("[data-champion]")).toHaveAttribute("data-champion", /.+/)
    await expect(page.getByRole("button", { name: "regenerate" })).toBeDisabled()

    page.once("dialog", dialog => dialog.accept())
    await page.getByRole("button", { name: "conclude" }).click()
    await expect(page.getByRole("heading", { level: 1 })).toContainText("# concluded")
    const winner = (await page.locator("[data-winner]").getAttribute("data-winner")) ?? ""
    expect(players).toContain(winner)

    // The kiosk polls the site and follows without a reload.
    await expect(kiosk.getByText("■ concluded")).toBeVisible({ timeout: 10_000 })
    await expect(kiosk.locator("[data-champion]").getByText(winner)).toBeVisible()
    await kiosk.close()

    // The conclusion paid the champion: the entry, one win at least, the cup.
    const rules = await cyclesRules(context.request)
    const standing = await standingOf(context.request, winner)
    // The champion won at least twice in a field of six.
    expect(standing.cycles).toBeGreaterThanOrEqual(
        cyclesOf(rules, "tournament_entry", 6) +
            2 * cyclesOf(rules, "match_win", 6) +
            cyclesOf(rules, "champion", 6),
    )
    await page.goto(`/players/${winner}`)
    await expect(page.getByText("champion", { exact: true })).toBeVisible()
    // Every line of the log links the cup, so any one of them proves the join.
    await expect(page.getByRole("link", { name }).first()).toBeVisible()

    await page.goto("/tournaments")
    const card = page.locator(`[data-tournament="${id}"]`)
    await expect(card).toContainText("CONCLUDED")
    await expect(card.getByRole("link", { name: winner })).toBeVisible()
    expect(await card.locator("svg rect").count()).toBeGreaterThan(0)

    // The public bracket shows the rounds, and no side of it takes a gesture.
    await page.goto(`/tournaments/${id}/bracket`)
    await expect(page.getByText("champion")).toBeVisible()
    await expect(page.getByText("final", { exact: true })).toBeVisible()
    await expect(page.locator("[data-match]")).toHaveCount(7)
    await expect(page.getByText(winner).first()).toBeVisible()
    await expect(page.locator("button[data-pick]")).toHaveCount(0)
})

test("an odd field gets byes, a result can be cleared, the bracket regenerates and drops", async ({
    page,
    context,
}) => {
    const admin = await newAdmin(context)
    const id = await apiDraft(context.request, admin.token)
    await enrol(
        context.request,
        admin.token,
        id,
        Array.from({ length: 5 }, (_, i) => `${handle("o")}${i}`),
    )
    await page.goto(`/admin/tournaments/${id}`)
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

test("a seed swaps by drag, and only a round one side takes the gesture", async ({
    page,
    context,
}) => {
    const admin = await newAdmin(context)
    const id = await apiDraft(context.request, admin.token)
    // Five entrants leave three byes, so two players reach round two with no
    // result behind them. Those sides take a click and must take no drop.
    await enrol(
        context.request,
        admin.token,
        id,
        Array.from({ length: 5 }, (_, i) => `${handle("d")}${i}`),
    )
    await page.goto(`/admin/tournaments/${id}`)
    await page.getByRole("button", { name: "go live" }).click()
    await page.getByRole("button", { name: "generate bracket" }).click()
    await expect(page.locator("[data-match]")).toHaveCount(7)

    // A drag cannot scroll, so both boxes must be on screen at once.
    await page.setViewportSize({ width: 1400, height: 1200 })

    // Five entrants fill five sides of round one, and each one can be dragged.
    const roundOne = page.locator("[data-round='1'] [data-entrant][draggable=true]")
    await expect(roundOne).toHaveCount(5)

    // The two sides trade seeds and appear in each other's box.
    const first = roundOne.nth(0)
    const target = roundOne.nth(4)
    const firstId = await first.getAttribute("data-entrant")
    const targetId = await target.getAttribute("data-entrant")
    expect(firstId).not.toBe(targetId)
    await first.dragTo(target)
    await settled(page)
    await expect(roundOne.nth(0)).toHaveAttribute("data-entrant", targetId ?? "")
    await expect(roundOne.nth(4)).toHaveAttribute("data-entrant", firstId ?? "")

    // A later round holds sides that take a result. None of them may drag or
    // accept a drop: a swap must never ride on a click target.
    const later = page.locator("[data-round]:not([data-round='1'])")
    await expect(later.locator("button[data-pick]")).not.toHaveCount(0)
    await expect(later.locator("[draggable=true]")).toHaveCount(0)
})

test("a field of thirty gets the rounds, the byes and the names of its stages", async ({
    page,
    context,
}) => {
    const admin = await newAdmin(context)
    const id = await apiDraft(context.request, admin.token)
    const names = Array.from({ length: 30 }, (_, i) => `${handle("p")}${i.toString(36)}`)
    await enrol(context.request, admin.token, id, names)

    await page.goto(`/admin/tournaments/${id}`)
    await expect(page.getByText("entrants # 30")).toBeVisible()
    await page.getByRole("button", { name: "go live" }).click()
    await page.getByRole("button", { name: "generate bracket" }).click()

    // Thirty-two slots: sixteen first-round matches, two of them a bye.
    await expect(page.locator("[data-match]")).toHaveCount(16 + 8 + 4 + 2 + 1)
    await expect(page.getByText("bye")).toHaveCount(2)
    await expect(page.getByText("quarter-finals")).toBeVisible()

    // The public bracket carries the same tree.
    await page.goto(`/tournaments/${id}/bracket`)
    await expect(page.locator("[data-match]")).toHaveCount(31)
})
