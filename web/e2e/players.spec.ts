import { expect, test } from "@playwright/test"

import {
    adjustCycles,
    adminNamed,
    apiDuel,
    apiSignup,
    clearSession,
    cyclesOf,
    cyclesRules,
    handle,
    login,
    newAdmin,
    newPlayer,
    playerId,
    readableFailure,
    setSession,
    signupMany,
} from "./support"

/** The public roster, its search, its pager, and the ledger of a player. */

test("the roster links a player, the old scores path redirects, and a stranger is a 404", async ({
    page,
    context,
}) => {
    const player = await newPlayer(context, "pub")
    await clearSession(context)

    await page.goto("/scores")
    await expect(page).toHaveURL(/\/players$/)

    await page.goto(`/players?q=${player.name}`)
    await page.getByRole("link", { name: player.name }).click()
    await expect(page).toHaveURL(new RegExp(`/players/${player.name}$`))
    await expect(
        page.getByRole("heading", { level: 1, name: player.name, exact: true }),
    ).toBeVisible()
    await expect(page.getByText("you", { exact: true })).toHaveCount(0)

    expect((await page.goto("/players/nobody-here"))?.status()).toBe(404)
})

test("the roster search matches a piece of a handle in any case, and clears", async ({
    page,
    request,
}) => {
    const prefix = handle("srch")
    await signupMany(request, prefix, 2)

    await page.goto("/players")
    const field = page.getByLabel("search players by name")
    // A piece from the middle, in the other case: the match is on a substring
    // and it ignores case. The last character tells the two players apart.
    await field.fill(`${prefix}0`.slice(3).toUpperCase())
    await field.press("Enter")
    await expect(page).toHaveURL(/\/players\?q=/)
    await expect(page.getByRole("link", { name: `${prefix}0` })).toBeVisible()
    await expect(page.getByRole("link", { name: `${prefix}1` })).toHaveCount(0)
    await expect(page.getByRole("search")).toContainText("1 match")

    await page.getByRole("link", { name: "CLEAR" }).click()
    await expect(page).toHaveURL(/\/players$/)
    await expect(page.getByRole("search")).toContainText("enlisted")
})

test("a search with no match says so, a blank one lists everyone, and a long one is cut", async ({
    page,
}) => {
    await page.goto("/players?q=nobody-has-this")
    await expect(page.getByText("no handle contains")).toBeVisible()
    await expect(page.getByRole("search")).toContainText("0 matches")
    await expect(page.getByRole("table")).toHaveCount(0)

    await page.goto("/players")
    const field = page.getByLabel("search players by name")
    await field.fill("   ")
    await field.press("Enter")
    await expect(page).toHaveURL(/\/players\?q=/)
    await expect(page.getByRole("link", { name: "CLEAR" })).toHaveCount(0)
    await expect(page.getByRole("search")).toContainText("enlisted")
    expect(await page.locator("tbody tr").count()).toBeGreaterThan(0)

    // A term longer than a handle cannot match, and it must not be an error.
    await page.goto(`/players?q=${"a".repeat(25)}`)
    await expect(page.getByRole("alert")).toHaveCount(0)
    await expect(field).toHaveValue("a".repeat(20))
    await expect(page.getByText("no handle contains")).toBeVisible()
})

test("the pager keeps the search, and a page outside the range comes back", async ({
    page,
    request,
}) => {
    // Twenty-one players under one prefix: the search scopes the count, so other
    // tests can add players at the same time without changing this one.
    const prefix = handle("pq")
    await signupMany(request, prefix, 21)

    await page.goto(`/players?q=${prefix}`)
    const pager = page.getByRole("navigation", { name: "Pages" })
    await expect(pager).toContainText("page 1 of 2")
    await expect(pager).toContainText("21 players")
    await expect(pager.locator("[aria-disabled='true']")).toContainText("PREV")
    await expect(page.locator("tbody tr")).toHaveCount(20)

    await pager.getByRole("link", { name: "NEXT →" }).click()
    await expect(page).toHaveURL(new RegExp(`/players\\?q=${prefix}&page=2$`))
    await expect(page.locator("tbody tr")).toHaveCount(1)
    await expect(page.getByLabel("search players by name")).toHaveValue(prefix)

    // A page past the end and a page that is not a number both land on a page.
    await page.goto(`/players?q=${prefix}&page=9`)
    await expect(page).toHaveURL(new RegExp(`/players\\?q=${prefix}&page=2$`))
    await page.goto(`/players?q=${prefix}&page=banana`)
    await expect(pager).toContainText("page 1 of 2")
})

test("an underscore in a search term is a character, not a wildcard", async ({
    page,
    request,
}) => {
    const prefix = handle("us")
    await signupMany(request, `${prefix}_o`, 1)
    await signupMany(request, `${prefix}io`, 1)

    await page.goto(`/players?q=${prefix}_o`)
    await expect(page.getByRole("search")).toContainText("1 match")
    await expect(page.getByRole("link", { name: `${prefix}_o0` })).toBeVisible()
    await expect(page.getByRole("link", { name: `${prefix}io0` })).toHaveCount(0)
})

test("an admin adjusts cycles, and the rank, the bar and the log follow", async ({
    page,
    context,
}) => {
    // One prefix for both accounts, so a single search shows the two rows side
    // by side whatever else the suite writes to the leaderboard.
    const base = handle("cyc")
    const admin = await adminNamed(context, `${base}a`)
    const name = `${base}b`
    await apiSignup(context.request, name)
    const roster = `/admin/players?q=${name}`

    await page.goto(roster)
    const row = () => page.getByRole("row", { name: new RegExp(name) })
    await expect(row().locator("[data-cycles]")).toHaveText("0")
    await row().getByLabel(`cycles for ${name}`).fill("150")
    await row().getByLabel(`reason for the cycles of ${name}`).fill("carried the fridge")
    await row().getByRole("button", { name: "adjust" }).click()
    await expect(page.getByRole("status")).toContainText(`cycles added to ${name}`)

    // A change redirects to the whole roster, so the search starts again.
    await page.goto(roster)
    await expect(row().locator("[data-cycles]")).toHaveText("150")
    await expect(row().locator("[data-rank]")).toHaveAttribute("data-rank", "guest")

    // Zero is refused with a sentence, and the reason typed stays in the row.
    await row().getByLabel(`cycles for ${name}`).fill("0")
    await row().getByLabel(`reason for the cycles of ${name}`).fill("nothing at all")
    await row().getByRole("button", { name: "adjust" }).click()
    await readableFailure(page, /not zero/)
    await expect(row().getByLabel(`cycles for ${name}`)).toHaveValue("0")
    await expect(row().getByLabel(`reason for the cycles of ${name}`)).toHaveValue(
        "nothing at all",
    )

    // The leaderboard ranks them above the admin, who has nothing.
    await page.goto(`/players?q=${base}`)
    const board = page.getByRole("table", { name: "leaderboard" })
    const mine = board.getByRole("row", { name: new RegExp(name) })
    await expect(mine).toContainText("GUEST")
    await expect(mine).toContainText("150")
    await expect(mine.getByText("you", { exact: true })).toHaveCount(0)
    const adminRow = board.getByRole("row", { name: new RegExp(admin.name) })
    await expect(adminRow.getByText("you", { exact: true })).toBeVisible()
    expect(Number(await mine.getAttribute("data-place"))).toBeLessThan(
        Number(await adminRow.getAttribute("data-place")),
    )
    // The podium takes a place in the first three with cycles. Nothing earns no
    // podium, whatever the place.
    await expect(adminRow.locator("td").first()).not.toHaveClass(/font-bold/)

    // The other half of the rule, on the whole board. Which player holds the
    // first place depends on the run, but the place is always 1 and the cycles
    // this test just wrote keep the board from being empty.
    await page.goto("/players")
    const first = page
        .getByRole("table", { name: "leaderboard" })
        .locator("tbody tr")
        .first()
    await expect(first).toHaveAttribute("data-place", "1")
    await expect(first.locator("td").first()).toHaveClass(/font-bold/)

    // The public page shows the standing and the note.
    await page.goto(`/players/${name}`)
    await expect(page.locator("[data-cycles]")).toHaveText("150")
    // 150 sits 70 cycles into the 520 between the guest floor and 600.
    await expect(page.locator("[data-progress]")).toHaveAttribute("data-progress", "13")
    await expect(page.getByText("+150")).toBeVisible()
    await expect(page.getByText("admin bonus")).toBeVisible()
    await expect(page.getByText("carried the fridge")).toBeVisible()

    // A stranger reads the note too: an admin writes it for everyone.
    await clearSession(context)
    await page.goto(`/players/${name}`)
    await expect(page.getByText("+150")).toBeVisible()
    await expect(page.getByText("carried the fridge")).toBeVisible()

    // The owner reads the note on the profile.
    await login(page, name)
    await expect(page.getByText("GUEST").first()).toBeVisible()
    await expect(page.getByText("carried the fridge")).toBeVisible()
    await expect(page.getByRole("link", { name: "HOW TO EARN CYCLES" })).toBeVisible()
    await expect(page.locator("[data-coming-soon]")).toBeVisible()
})

test("the two ends of the ladder: a fresh player is called in, a kernel is at the top", async ({
    page,
    context,
}) => {
    const player = await newPlayer(context, "end")

    // A fresh player has no bar, only the way in.
    await page.goto("/profile")
    await expect(page.getByText("wakes up at")).toBeVisible()
    await expect(page.locator("[data-progress]")).toHaveCount(0)
    await expect(
        page.getByRole("link", { name: "HOW TO EARN CYCLES" }).first(),
    ).toBeVisible()

    const admin = await newAdmin(context, "enda")
    const id = await playerId(context.request, player.name)

    // Below zero the bar comes back, and the sign stays on every total.
    await adjustCycles(context.request, admin.token, id, -80, "unplugged a cabinet")
    await page.goto(`/players/${player.name}`)
    await expect(page.locator("[data-cycles]")).toHaveText("-80")
    await expect(page.locator("[data-progress]")).toHaveAttribute("data-progress", "0")
    await expect(page.getByText("160 to go")).toBeVisible()
    await expect(page.locator("[data-source=adjustments]")).toHaveText("-80")

    // At the top of the ladder there is nothing left to fill.
    await adjustCycles(context.request, admin.token, id, 10_000, "founder")
    await page.goto(`/players/${player.name}`)
    await expect(page.getByText("KERNEL").first()).toBeVisible()
    await expect(page.getByText("top of the ladder")).toBeVisible()
    await expect(page.getByText("to go")).toHaveCount(0)
    await expect(page.locator("[data-progress]")).toHaveAttribute("data-progress", "100")
})

test("the cycles legend shows what the api pays", async ({ page, request }) => {
    const rules = await cyclesRules(request)

    await page.goto("/cycles")
    const table = page.getByRole("table", { name: "how to earn cycles" })
    await expect(table.locator("[data-kind=tournament_entry]")).toContainText(
        `+${cyclesOf(rules, "tournament_entry", 4)}`,
    )
    // A placement is a range from the smallest field to the biggest.
    const small = cyclesOf(rules, "champion", 4)
    const large = cyclesOf(rules, "champion", 16)
    expect(large).toBeGreaterThan(small)
    await expect(table.locator("[data-kind=champion]")).toContainText(
        `+${small} to +${large}`,
    )
    await expect(page.getByText("16+ players")).toBeVisible()
    await expect(table.locator("[data-kind=adjustment]")).toContainText("±")
    const ladder = page.getByRole("list", { name: "the ladder" })
    const steps = ladder.getByRole("listitem")
    await expect(steps).toHaveCount(rules.ranks.length)
    await expect(steps.filter({ hasText: "ZOMBIE" })).toContainText("from nothing")
    await expect(steps.first()).toContainText("KERNEL")
})

test("a long log shows its last five lines and pages the rest", async ({
    page,
    context,
}) => {
    const admin = await newAdmin(context, "loga")
    const player = await newPlayer(context, "log")
    const id = await playerId(context.request, player.name)
    // One at a time, because the log shows the newest line first and two writes
    // in the same instant have no order.
    for (let index = 1; index <= 22; index += 1) {
        await adjustCycles(context.request, admin.token, id, index, `line ${index}`)
    }

    await page.goto(`/players/${player.name}`)
    await expect(page.locator("[data-cycles-log] li")).toHaveCount(5)
    await expect(page.locator("[data-cycles-log] li").first()).toContainText("+22")
    // The totals cover the whole log, not the five lines shown: 1 + 2 + ... + 22.
    await expect(page.locator("[data-source=adjustments]")).toHaveText("253")
    await page.getByRole("link", { name: /FULL LOG \(22\)/ }).click()

    await expect(page).toHaveURL(`/players/${player.name}/cycles`)
    await expect(page.locator("[data-cycles-log] li")).toHaveCount(20)
    const pager = page.getByRole("navigation", { name: "Pages" })
    await expect(pager).toContainText("page 1 of 2")
    await expect(pager).toContainText("22 lines")
    await pager.getByRole("link", { name: "NEXT →" }).click()
    await expect(page).toHaveURL(/\/cycles\?page=2$/)
    await expect(page.locator("[data-cycles-log] li")).toHaveCount(2)
    await expect(page.locator("[data-cycles-log] li").last()).toContainText("+1")
    await expect(page.locator("[data-cycles-log] li").last()).toContainText("line 1")

    await page.goto(`/players/${player.name}/cycles?page=9`)
    await expect(page).toHaveURL(/\/cycles\?page=2$/)
})

test("a match log names the opponent, and a second loss names the nemesis", async ({
    page,
    context,
}) => {
    const admin = await newAdmin(context, "nem")
    const winner = handle("win")
    const loser = handle("los")
    await apiSignup(context.request, winner)
    const loserToken = await apiSignup(context.request, loser)

    await apiDuel(context.request, admin.token, winner, loser, "2026-03-01")

    await clearSession(context)
    await page.goto(`/players/${loser}`)
    const rows = page.locator("[data-match-log] li")
    await expect(rows).toHaveCount(1)
    await expect(rows.first()).toContainText("LOSS")
    await expect(rows.first()).toContainText("final")
    await expect(rows.first().getByRole("link", { name: winner })).toBeVisible()
    await expect(page.locator("[data-nemesis]")).toHaveCount(0)

    // The same match, from the other side.
    await page.goto(`/players/${winner}`)
    await expect(page.locator("[data-match-log] li").first()).toContainText("WIN")
    await expect(page.locator("[data-nemesis]")).toHaveCount(0)

    await apiDuel(context.request, admin.token, winner, loser, "2026-03-02")

    await page.goto(`/players/${loser}`)
    const nemesis = page.locator(`[data-nemesis="${winner}"]`)
    await expect(nemesis).toBeVisible()
    await expect(nemesis).toContainText("0-2")
    await expect(nemesis.getByRole("link", { name: winner })).toHaveAttribute(
        "href",
        `/players/${winner}`,
    )

    // The owner sees the same panel on their own profile.
    await setSession(context, loserToken)
    await page.goto("/profile")
    await expect(page.locator(`[data-nemesis="${winner}"]`)).toBeVisible()
    await expect(page.locator("[data-match-log] li")).toHaveCount(2)

    // The whole log, newest first.
    await page.goto(`/players/${loser}/matches`)
    const all = page.locator("[data-match-log] li")
    await expect(all).toHaveCount(2)
    await expect(all.first()).toContainText("02 MAR 2026")
    await expect(all.last()).toContainText("01 MAR 2026")
    await expect(page.locator("[data-record]")).toHaveText("0-2")
})
