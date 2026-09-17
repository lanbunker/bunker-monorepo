import { expect, test } from "@playwright/test"
import { z } from "zod"

import {
    API,
    PASSWORD,
    cyclesOf,
    handle,
    jsonOf,
    login,
    logout,
    promote,
    readableFailure,
    rulesSchema,
    signup,
    signupAdmin,
    signupMany,
    tokenFor,
} from "./support"

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

test("the roster search finds a player by a piece of the handle, in any case", async ({
    page,
}) => {
    const name = handle("srch")
    const other = handle("othr")
    await signup(page, name)
    await logout(page)
    await signup(page, other)
    await logout(page)

    await page.goto("/players")
    const field = page.getByLabel("search players by name")
    await field.fill(name.slice(0, 6).toUpperCase())
    await field.press("Enter")
    await expect(page).toHaveURL(/\/players\?q=/)
    await expect(page.getByRole("link", { name })).toBeVisible()
    await expect(page.getByRole("link", { name: other })).toHaveCount(0)
    await expect(page.getByRole("search")).toContainText("1 match")

    await page.getByRole("link", { name: "CLEAR" }).click()
    await expect(page).toHaveURL(/\/players$/)
    await expect(page.getByRole("link", { name: other })).toBeVisible()
})

test("a search with no match says so instead of an empty table", async ({ page }) => {
    await page.goto("/players?q=nobody-has-this")
    await expect(page.getByText("no handle contains")).toBeVisible()
    await expect(page.getByRole("search")).toContainText("0 matches")
    await expect(page.getByRole("table")).toHaveCount(0)
})

test("the pager keeps the search, and a page past its end comes back to the last one", async ({
    page,
    request,
}) => {
    const prefix = handle("pq")
    await signupMany(request, prefix, 21)

    await page.goto(`/players?q=${prefix}`)
    const pager = page.getByRole("navigation", { name: "Pages" })
    await expect(pager).toContainText("page 1 of 2")
    await expect(pager).toContainText("21 players")
    await expect(page.locator("tbody tr")).toHaveCount(20)

    await pager.getByRole("link", { name: "NEXT →" }).click()
    await expect(page).toHaveURL(new RegExp(`/players\\?q=${prefix}&page=2$`))
    await expect(page.locator("tbody tr")).toHaveCount(1)
    await expect(page.getByLabel("search players by name")).toHaveValue(prefix)

    await page.goto(`/players?q=${prefix}&page=9`)
    await expect(page).toHaveURL(new RegExp(`/players\\?q=${prefix}&page=2$`))
})

test("a blank search lists everyone, and a long term is cut to the length of a handle", async ({
    page,
}) => {
    await page.goto("/players")
    const field = page.getByLabel("search players by name")
    await field.fill("   ")
    await field.press("Enter")
    await expect(page).toHaveURL(/\/players\?q=/)
    await expect(page.getByRole("link", { name: "CLEAR" })).toHaveCount(0)
    await expect(page.getByRole("search")).toContainText("enlisted")
    expect(await page.locator("tbody tr").count()).toBeGreaterThan(0)

    await page.goto(`/players?q=${"a".repeat(25)}`)
    await expect(page.getByRole("alert")).toHaveCount(0)
    await expect(field).toHaveValue("a".repeat(20))
    await expect(page.getByText("no handle contains")).toBeVisible()
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

    // Places are shared, so the second page is told apart by its players.
    const firstOnPageOne = await page.locator("tbody tr").first().textContent()
    await pager.getByRole("link", { name: "NEXT →" }).click()
    await expect(page).toHaveURL(/\/players\?page=2$/)
    await expect(page.getByRole("navigation", { name: "Pages" })).toContainText(
        "page 2 of",
    )
    await expect(page.locator("tbody tr").first()).not.toHaveText(firstOnPageOne ?? "")

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

test("the homepage reports the bunkernet uplink and invites a visitor to enlist", async ({
    page,
}) => {
    await page.goto("/")
    await expect(page.locator("#boot-log")).toContainText("[  OK ] bunkernet uplink")
    await expect(page.getByText(/\d+ players? enlisted/)).toBeVisible()
    await expect(page.getByRole("link", { name: "ENTER BUNKERNET" })).toBeVisible()

    await signup(page, handle("hom"))
    await page.goto("/")
    await expect(page.getByRole("link", { name: "ENTER BUNKERNET" })).toHaveCount(0)
})

test("a player renames themself and keeps the glyph", async ({ page }) => {
    const name = handle("old")
    const renamed = handle("new")
    await signup(page, name)
    const glyph = await page.locator("[data-glyph-bits]").textContent()

    await page.getByRole("link", { name: "CHANGE HANDLE" }).click()
    await page.getByLabel("new handle:").fill(renamed)
    await page.getByRole("button", { name: "RENAME" }).click()

    await expect(page).toHaveURL(/\/profile\?changed=handle$/)
    await expect(page.getByRole("status")).toContainText("handle changed")
    await expect(
        page.getByRole("heading", { level: 1, name: renamed, exact: true }),
    ).toBeVisible()
    await expect(page.locator("[data-glyph-bits]")).toHaveText(glyph ?? "")

    const gone = await page.goto(`/players/${name}`)
    expect(gone?.status()).toBe(404)
    await page.goto(`/players/${renamed}`)
    await expect(
        page.getByRole("heading", { level: 1, name: renamed, exact: true }),
    ).toBeVisible()
})

test("a taken handle is refused at rename", async ({ page }) => {
    const taken = handle("tkn")
    await signup(page, taken)
    await logout(page)
    await signup(page, handle("me"))

    await page.goto("/handle")
    await page.getByLabel("new handle:").fill(taken.toUpperCase())
    await page.getByRole("button", { name: "RENAME" }).click()

    await expect(page).toHaveURL(/\/handle(\?|$)/)
    await expect(page.getByRole("alert")).toContainText(/taken/i)
})

test("an admin renames a player from the backoffice", async ({ page }) => {
    const user = handle("usr")
    await signup(page, user)
    await logout(page)
    const admin = handle("adm")
    await signup(page, admin)
    promote(admin)

    await page.goto("/admin/players")
    const row = page.getByRole("row", { name: new RegExp(user) })
    const renamed = handle("ren")
    await row.getByLabel(`new handle for ${user}`).fill(renamed)
    await row.getByRole("button", { name: "rename" }).click()

    await expect(page.getByRole("status")).toContainText(`renamed to ${renamed}`)
    await expect(page.getByRole("row", { name: new RegExp(renamed) })).toBeVisible()
    await expect(page.getByRole("row", { name: new RegExp(user) })).toHaveCount(0)
})

test("an admin adjusts cycles, and the rank, the bar and the log follow", async ({
    page,
}) => {
    const adminName = await signupAdmin(page)
    const name = handle("cyc")
    const created = await page.request.post(`${API}/api/auth/signup`, {
        data: { handle: name, password: PASSWORD },
    })
    expect(created.status()).toBe(201)

    await page.goto("/admin/players")
    const row = () => page.getByRole("row", { name: new RegExp(name) })
    await expect(row().locator("[data-cycles]")).toHaveText("0")
    await row().getByLabel(`cycles for ${name}`).fill("150")
    await row().getByLabel(`reason for the cycles of ${name}`).fill("carried the fridge")
    await row().getByRole("button", { name: "adjust" }).click()
    await expect(page.getByRole("status")).toContainText(`cycles added to ${name}`)
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

    // The leaderboard ranks them above a player with nothing.
    await page.goto("/players")
    const board = page.getByRole("table", { name: "leaderboard" })
    const mine = board.getByRole("row", { name: new RegExp(name) })
    await expect(mine).toContainText("GUEST")
    await expect(mine).toContainText("150")
    await expect(mine.getByText("you", { exact: true })).toHaveCount(0)
    const admin = board.getByRole("row", { name: new RegExp(adminName) })
    await expect(admin.getByText("you", { exact: true })).toBeVisible()
    const myPlace = Number(await mine.getAttribute("data-place"))
    const adminPlace = Number(await admin.getAttribute("data-place"))
    expect(myPlace).toBeLessThan(adminPlace)
    // The first place is on the podium, in bold. A place with nothing is not.
    await expect(board.locator("tbody tr").first().locator("td").first()).toHaveClass(
        /font-bold/,
    )
    await expect(admin.locator("td").first()).not.toHaveClass(/font-bold/)

    // The public page shows the standing and the note.
    await page.goto(`/players/${name}`)
    await expect(page.locator("[data-cycles]")).toHaveText("150")
    // 150 sits 70 cycles into the 520 between the guest floor and 600.
    await expect(page.locator("[data-progress]")).toHaveAttribute("data-progress", "13")
    await expect(page.getByText("+150")).toBeVisible()
    await expect(page.getByText("admin bonus")).toBeVisible()
    await expect(page.getByText("carried the fridge")).toBeVisible()

    // A stranger reads the note too: an admin writes it for everyone.
    await logout(page)
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
}) => {
    // A fresh player has no bar, only the way in.
    const name = handle("end")
    await signup(page, name)
    await expect(page.getByText("wakes up at")).toBeVisible()
    await expect(page.locator("[data-progress]")).toHaveCount(0)
    await expect(
        page.getByRole("link", { name: "HOW TO EARN CYCLES" }).first(),
    ).toBeVisible()
    await logout(page)

    // Below zero the bar comes back, and the sign stays on every total.
    const adminName = await signupAdmin(page)
    const token = await tokenFor(page, adminName)
    const player = await jsonOf(
        await page.request.get(`${API}/api/players/${name}`),
        z.object({ id: z.string() }),
    )
    const adjust = async (amount: number, note: string) => {
        const response = await page.request.post(
            `${API}/api/admin/players/${player.id}/cycles`,
            {
                data: { amount, note },
                headers: { authorization: `Bearer ${token}` },
            },
        )
        expect(response.status()).toBe(201)
    }
    await adjust(-80, "unplugged a cabinet")
    await page.goto(`/players/${name}`)
    await expect(page.locator("[data-cycles]")).toHaveText("-80")
    await expect(page.locator("[data-progress]")).toHaveAttribute("data-progress", "0")
    await expect(page.getByText("160 to go")).toBeVisible()
    await expect(page.locator("[data-source=adjustments]")).toHaveText("-80")

    // At the top of the ladder there is nothing left to fill.
    await adjust(10_000, "founder")
    await page.goto(`/players/${name}`)
    await expect(page.getByText("KERNEL").first()).toBeVisible()
    await expect(page.getByText("top of the ladder")).toBeVisible()
    await expect(page.getByText("to go")).toHaveCount(0)
    await expect(page.locator("[data-progress]")).toHaveAttribute("data-progress", "100")
})

test("the cycles legend shows what the api pays", async ({ page }) => {
    const rules = await jsonOf(
        await page.request.get(`${API}/api/cycles/rules`),
        rulesSchema,
    )

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

test("a long log shows its last five lines and pages the rest", async ({ page }) => {
    const admin = await signupAdmin(page)
    const token = await tokenFor(page, admin)
    const name = handle("log")
    const created = await page.request.post(`${API}/api/auth/signup`, {
        data: { handle: name, password: PASSWORD },
    })
    expect(created.status()).toBe(201)
    const player = await jsonOf(
        await page.request.get(`${API}/api/players/${name}`),
        z.object({ id: z.string() }),
    )
    for (let index = 1; index <= 22; index += 1) {
        const added = await page.request.post(
            `${API}/api/admin/players/${player.id}/cycles`,
            {
                data: { amount: index, note: `line ${index}` },
                headers: { authorization: `Bearer ${token}` },
            },
        )
        expect(added.status()).toBe(201)
    }

    await page.goto(`/players/${name}`)
    await expect(page.locator("[data-cycles-log] li")).toHaveCount(5)
    await expect(page.locator("[data-cycles-log] li").first()).toContainText("+22")
    // The totals cover the whole log, not the five lines shown: 1 + 2 + ... + 22.
    await expect(page.locator("[data-source=adjustments]")).toHaveText("253")
    await page.getByRole("link", { name: /FULL LOG \(22\)/ }).click()

    await expect(page).toHaveURL(`/players/${name}/cycles`)
    await expect(page.locator("[data-cycles-log] li")).toHaveCount(20)
    const pager = page.getByRole("navigation", { name: "Pages" })
    await expect(pager).toContainText("page 1 of 2")
    await expect(pager).toContainText("22 lines")
    await pager.getByRole("link", { name: "NEXT →" }).click()
    await expect(page).toHaveURL(/\/cycles\?page=2$/)
    await expect(page.locator("[data-cycles-log] li")).toHaveCount(2)
    await expect(page.locator("[data-cycles-log] li").last()).toContainText("+1")
    await expect(page.locator("[data-cycles-log] li").last()).toContainText("line 1")

    await page.goto(`/players/${name}/cycles?page=9`)
    await expect(page).toHaveURL(/\/cycles\?page=2$/)
})
