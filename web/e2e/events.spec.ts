import { expect, test } from "@playwright/test"
import type { Page } from "@playwright/test"
import { z } from "zod"

import {
    API,
    PASSWORD,
    handle,
    jsonOf,
    login,
    logout,
    signup,
    signupAdmin,
    signupMany,
    standingSchema,
    tokenFor,
    tomorrow,
} from "./support"

const HOUR = 3_600_000

const eventSchema = z.object({ id: z.string(), status: z.string() })
const detailSchema = z.object({ checkinCode: z.string(), event: eventSchema })
const receiptSchema = z.object({ cycles: z.number() })

/**
 * A published event whose doors opened `opensIn` ms from now and stay open for
 * `lasts` ms, straight through the API. Answers its id and its check-in code.
 */
const publishedEvent = async (
    page: Page,
    admin: string,
    opensIn: number,
    lasts: number,
) => {
    const token = await tokenFor(page, admin)
    const headers = { authorization: `Bearer ${token}` }
    const startsAt = new Date(Date.now() + opensIn)
    const endsAt = new Date(startsAt.getTime() + lasts)
    const created = await jsonOf(
        await page.request.post(`${API}/api/admin/events`, {
            data: {
                name: `Night ${handle("ev")}`,
                location: "@theoffice",
                games: "Halo 3, Mario Kart 8",
                startsAt: startsAt.toISOString(),
                endsAt: endsAt.toISOString(),
            },
            headers,
        }),
        eventSchema,
    )
    const published = await page.request.post(
        `${API}/api/admin/events/${created.id}/status`,
        { data: { status: "published" }, headers },
    )
    expect(published.ok()).toBe(true)
    const detail = await jsonOf(
        await page.request.get(`${API}/api/admin/events/${created.id}`, { headers }),
        detailSchema,
    )
    return { id: created.id, code: detail.checkinCode }
}

test("an admin creates an event, publishes it, and the public page lists it", async ({
    page,
    request,
}) => {
    await signupAdmin(page)
    const name = `Night ${handle("ev")}`
    const day = tomorrow().day

    await page.goto("/admin/events")
    await page.getByLabel("name", { exact: true }).fill(name)
    await page.getByLabel("location").fill("@theoffice")
    await page.getByLabel("doors open (your local time)").fill(`${day}T21:00`)
    await page.getByLabel("night ends (your local time)").fill(`${day}T23:30`)
    await page.getByLabel("games").fill("Halo 3")
    await page.getByLabel("cover").selectOption("feb2026-cover.webp")
    await page.getByRole("button", { name: "CREATE DRAFT" }).click()
    await expect(page).toHaveURL(/\/admin\/events\/[0-9a-f-]{36}$/)
    const id = page.url().split("/").at(-1) ?? ""
    await expect(page.getByRole("heading", { level: 1 })).toContainText("# draft")

    // A draft is not on the site.
    await page.goto("/events")
    await expect(page.locator(`[data-event="${id}"]`)).toHaveCount(0)

    await page.goto(`/admin/events/${id}`)
    await page.getByRole("button", { name: "publish" }).click()
    await expect(page.getByRole("status")).toContainText("status changed")
    await expect(page.getByRole("heading", { level: 1 })).toContainText("# published")
    // The link for the QR code is on the page, on this site, with a code.
    await expect(page.getByLabel("check-in link")).toHaveValue(
        /^http:\/\/127\.0\.0\.1:4399\/checkin\/[a-z0-9]{12}$/,
    )
    const link = await page.getByLabel("check-in link").inputValue()

    // The poster is an SVG that carries the link and the name, for admins only.
    const posterImage = page.getByRole("img", { name: "check-in QR code" })
    await expect(posterImage).toBeVisible()
    // The route compiles on its first hit in the dev server, so the width is polled.
    await expect
        .poll(() =>
            posterImage.evaluate(img =>
                img instanceof HTMLImageElement ? img.naturalWidth : 0,
            ),
        )
        .toBeGreaterThan(0)
    const poster = await page.request.get(`/admin/events/${id}/qr.svg?download=1`)
    expect(poster.status()).toBe(200)
    expect(poster.headers()["content-type"]).toContain("image/svg+xml")
    expect(poster.headers()["content-disposition"]).toContain("attachment")
    const svg = await poster.text()
    expect(svg).toContain("<svg")
    expect(svg).toContain('<path d="M')
    expect(svg).toContain(link)
    expect(svg).toContain(name)
    // A stranger and a plain player both get the same 404 as the backoffice.
    expect((await request.get(`/admin/events/${id}/qr.svg`)).status()).toBe(404)
    const player = handle("qr")
    await signupMany(request, player, 1)
    const token = await tokenFor(page, `${player}0`)
    const asPlayer = await request.get(`/admin/events/${id}/qr.svg`, {
        headers: { cookie: `bunker_session=${token}` },
    })
    expect(asPlayer.status()).toBe(404)

    await page.goto("/events")
    const card = page.locator(`[data-event="${id}"]`)
    await expect(card.getByRole("heading", { level: 3 })).toContainText(name)
    await expect(card).toContainText("ANNOUNCED")
    await expect(card).toContainText("@theoffice")
    await expect(card.locator("img")).toHaveCount(1)

    // The list in the backoffice names it with its status.
    await page.goto("/admin/events")
    const row = page.getByRole("table", { name: "events" }).getByRole("row", {
        name: new RegExp(name),
    })
    await expect(row).toContainText("published")
})

test("a visitor at the door enlists, comes back, and checks in for 100 cycles", async ({
    page,
}) => {
    const admin = await signupAdmin(page)
    const { id, code } = await publishedEvent(page, admin, -HOUR, 5 * HOUR)
    await logout(page)

    // Nobody is logged in on this phone. The door offers the two ways in.
    await page.goto(`/checkin/${code}`)
    await expect(page.locator("[data-state=anonymous]")).toBeVisible()
    await expect(page.getByRole("navigation", { name: "Sections" })).toHaveCount(0)
    await page.getByRole("link", { name: "ENLIST" }).click()
    await expect(page).toHaveURL(`/signup?next=%2Fcheckin%2F${code}`)

    const player = handle("dor")
    await page.getByLabel("handle:").fill(player)
    await page.getByLabel("password:", { exact: true }).fill(PASSWORD)
    await page.getByLabel("repeat:").fill(PASSWORD)
    await page.getByRole("button", { name: "ENLIST" }).click()

    // The boot sequence brings the new player back to the door, logged in.
    await expect(page).toHaveURL(`/checkin/${code}`, { timeout: 15_000 })
    const form = page.locator("form[data-state=ready]")
    await expect(form).toContainText(player)
    expect(await form.locator("svg rect").count()).toBeGreaterThan(0)
    await page.getByRole("button", { name: "CHECK IN" }).click()

    await expect(page).toHaveURL(`/checkin/${code}?done=checked`)
    await expect(page.getByRole("status")).toContainText("CHECKED IN")
    await expect(page.locator("[data-state=confirmed]")).toContainText(player)
    await expect(page.locator("[data-state=confirmed]")).toContainText("+100 cycles")

    // The door paid, and the log names the night.
    await page.goto("/profile")
    await expect(page.locator("[data-cycles]")).toHaveText("100")
    await expect(page.locator("[data-rank=guest]").first()).toBeVisible()
    await expect(page.locator("[data-cycles-log]")).toContainText("event check-in")
    await expect(page.locator("[data-cycles-log]")).toContainText("Night ")

    // A second look at the door says so. A second scan pays nothing.
    await page.goto(`/checkin/${code}`)
    await expect(page.locator("[data-state=in]")).toContainText("already checked in")
    await expect(page.getByRole("button", { name: "CHECK IN" })).toHaveCount(0)
    const token = await tokenFor(page, player)
    const again = await page.request.post(`${API}/api/checkin/${code}`, {
        headers: { authorization: `Bearer ${token}` },
    })
    expect(again.status()).toBe(200)
    expect((await jsonOf(again, receiptSchema)).cycles).toBe(0)
    const standing = await jsonOf(
        await page.request.get(`${API}/api/players/${player}`),
        standingSchema,
    )
    expect(standing.standing.cycles).toBe(100)

    // A login carries the door along too.
    await logout(page)
    await page.goto(`/checkin/${code}`)
    await page.getByRole("link", { name: "LOGIN" }).click()
    await expect(page).toHaveURL(`/login?next=%2Fcheckin%2F${code}`)
    await page.getByLabel("login:").fill(player)
    await page.getByLabel("password:", { exact: true }).fill(PASSWORD)
    await page.getByRole("button", { name: "LOGIN" }).click()
    await expect(page).toHaveURL(`/checkin/${code}`)
    await expect(page.locator("[data-state=in]")).toBeVisible()

    // The backoffice lists who came.
    await logout(page)
    await login(page, admin)
    await page.goto(`/admin/events/${id}`)
    await expect(page.getByRole("table", { name: "check-ins" })).toContainText(player)
    await expect(page.getByRole("table", { name: "check-ins" })).toContainText("1")
})

test("an admin checks a player in by hand for a night that is over", async ({ page }) => {
    const admin = await signupAdmin(page)
    const { id } = await publishedEvent(page, admin, -30 * 24 * HOUR, 5 * HOUR)
    await logout(page)
    const player = handle("old")
    await signup(page, player)
    await logout(page)
    await login(page, admin)

    await page.goto(`/admin/events/${id}`)
    await page.getByLabel("handle of the player to check in").fill(player)
    await page.getByRole("button", { name: "add check-in" }).click()
    await expect(page.getByRole("status")).toContainText("check-in added")
    await expect(page.getByRole("table", { name: "check-ins" })).toContainText(player)

    const standing = await jsonOf(
        await page.request.get(`${API}/api/players/${player}`),
        standingSchema,
    )
    expect(standing.standing.cycles).toBe(100)

    // An unknown handle gets a sentence, not a stack.
    await page.getByLabel("handle of the player to check in").fill("nobodyhere99")
    await page.getByRole("button", { name: "add check-in" }).click()
    await expect(page.getByRole("alert")).toContainText(/not found/i)
})

test("a logged-in player checks in with one tap, or hands the phone over", async ({
    page,
}) => {
    const admin = await signupAdmin(page)
    const { code } = await publishedEvent(page, admin, -HOUR, 5 * HOUR)
    await logout(page)
    const player = handle("tap")
    await signup(page, player)

    await page.goto(`/checkin/${code}`)
    await expect(page.locator("form[data-state=ready]")).toContainText(player)

    // Not this player: the phone belongs to a friend. Log out, come back.
    await page.getByRole("button", { name: /not you/ }).click()
    await expect(page).toHaveURL(`/login?next=%2Fcheckin%2F${code}`)
    await page.goto(`/checkin/${code}`)
    await expect(page.locator("[data-state=anonymous]")).toBeVisible()

    await login(page, player)
    await page.goto(`/checkin/${code}`)
    await page.getByRole("button", { name: "CHECK IN" }).click()
    await expect(page.locator("[data-state=confirmed]")).toBeVisible()
})

test("the door is closed outside the window, and a bad code is a 404", async ({
    page,
}) => {
    const admin = await signupAdmin(page)
    const early = await publishedEvent(page, admin, HOUR, 5 * HOUR)
    const over = await publishedEvent(page, admin, -6 * HOUR, 5 * HOUR)

    await page.goto(`/checkin/${early.code}`)
    await expect(page.locator("[data-state=early]")).toContainText("doors open at")
    await expect(page.getByRole("timer")).toBeVisible()
    await expect(page.getByRole("button", { name: "CHECK IN" })).toHaveCount(0)

    await page.goto(`/checkin/${over.code}`)
    await expect(page.locator("[data-state=over]")).toContainText("check-in is closed")

    // The API refuses too, whatever the page shows.
    const token = await tokenFor(page, admin)
    const refused = await page.request.post(`${API}/api/checkin/${early.code}`, {
        headers: { authorization: `Bearer ${token}` },
    })
    expect(refused.status()).toBe(409)

    for (const path of ["/checkin/not-a-code", "/checkin/zzzzzzzzzzzz"]) {
        const response = await page.goto(path)
        expect(response?.status(), path).toBe(404)
    }

    // Back to draft, and the door of the early event opens nothing.
    await page.goto(`/admin/events/${early.id}`)
    page.once("dialog", dialog => dialog.accept())
    await page.getByRole("button", { name: "back to draft" }).click()
    await expect(page.getByRole("heading", { level: 1 })).toContainText("# draft")
    expect((await page.goto(`/checkin/${early.code}`))?.status()).toBe(404)
})
