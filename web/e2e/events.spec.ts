import { expect, test } from "@playwright/test"
import type { APIRequestContext } from "@playwright/test"
import { z } from "zod"

import {
    API,
    PASSWORD,
    apiLogin,
    bearer,
    clearSession,
    handle,
    jsonOf,
    login,
    newAdmin,
    newPlayer,
    setSession,
    standingOf,
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
    request: APIRequestContext,
    token: string,
    opensIn: number,
    lasts: number,
) => {
    const headers = bearer(token)
    const name = `Night ${handle("ev")}`
    const startsAt = new Date(Date.now() + opensIn)
    const endsAt = new Date(startsAt.getTime() + lasts)
    const created = await jsonOf(
        await request.post(`${API}/api/admin/events`, {
            data: {
                name,
                location: "@theoffice",
                games: "Halo 3, Mario Kart 8",
                startsAt: startsAt.toISOString(),
                endsAt: endsAt.toISOString(),
            },
            headers,
        }),
        eventSchema,
    )
    const published = await request.post(`${API}/api/admin/events/${created.id}/status`, {
        data: { status: "published" },
        headers,
    })
    expect(published.ok()).toBe(true)
    const detail = await jsonOf(
        await request.get(`${API}/api/admin/events/${created.id}`, { headers }),
        detailSchema,
    )
    return { id: created.id, code: detail.checkinCode, name }
}

test("an admin creates an event, publishes it, and the public page lists it", async ({
    page,
    context,
}) => {
    await newAdmin(context)
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

    await page.goto("/events")
    const card = page.locator(`[data-event="${id}"]`)
    await expect(card.getByRole("heading", { level: 3 })).toContainText(name)
    await expect(card).toContainText("ANNOUNCED")
    await expect(card).toContainText("@theoffice")
    await expect(card.locator("img")).toHaveCount(1)

    // The list in the backoffice names it with its status. The list is ordered
    // by the night that starts last, and this one starts tomorrow, so it is the
    // first row whatever else the suite has published.
    await page.goto("/admin/events")
    const row = page.getByRole("table", { name: "events" }).getByRole("row", {
        name: new RegExp(name),
    })
    await expect(row).toContainText("published")
})

test("the check-in poster is an svg for admins only", async ({ page, context }) => {
    const admin = await newAdmin(context)
    const event = await publishedEvent(context.request, admin.token, -HOUR, 5 * HOUR)

    await page.goto(`/admin/events/${event.id}`)
    const link = await page.getByLabel("check-in link").inputValue()
    const posterImage = page.getByRole("img", { name: "check-in QR code" })
    await expect(posterImage).toBeVisible()
    await expect
        .poll(() =>
            posterImage.evaluate(img =>
                img instanceof HTMLImageElement ? img.naturalWidth : 0,
            ),
        )
        .toBeGreaterThan(0)

    const poster = await page.request.get(`/admin/events/${event.id}/qr.svg?download=1`)
    expect(poster.status()).toBe(200)
    expect(poster.headers()["content-type"]).toContain("image/svg+xml")
    expect(poster.headers()["content-disposition"]).toContain("attachment")
    const svg = await poster.text()
    expect(svg).toContain("<svg")
    expect(svg).toContain('<path d="M')
    expect(svg).toContain(link)
    expect(svg).toContain(event.name)

    // A plain player and a stranger both get the same 404 as the backoffice.
    await newPlayer(context, "qr")
    expect((await page.request.get(`/admin/events/${event.id}/qr.svg`)).status()).toBe(
        404,
    )
    await clearSession(context)
    expect((await page.request.get(`/admin/events/${event.id}/qr.svg`)).status()).toBe(
        404,
    )
})

test("a visitor at the door enlists, comes back, and checks in for 100 cycles", async ({
    page,
    context,
}) => {
    const admin = await newAdmin(context)
    const event = await publishedEvent(context.request, admin.token, -HOUR, 5 * HOUR)
    const code = event.code
    await clearSession(context)

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
    // The guest floor sits below one check-in, so the bar is never empty here.
    await expect(page.locator("[data-progress]")).toHaveAttribute("data-progress", "4")
    await expect(page.locator("[data-cycles-log]")).toContainText("event check-in")
    await expect(page.locator("[data-cycles-log]")).toContainText("Night ")

    // A second look at the door says so. A second scan pays nothing.
    await page.goto(`/checkin/${code}`)
    await expect(page.locator("[data-state=in]")).toContainText("already checked in")
    await expect(page.getByRole("button", { name: "CHECK IN" })).toHaveCount(0)
    const again = await page.request.post(`${API}/api/checkin/${code}`, {
        headers: bearer(await apiLogin(context.request, player)),
    })
    expect(again.status()).toBe(200)
    expect((await jsonOf(again, receiptSchema)).cycles).toBe(0)
    expect((await standingOf(context.request, player)).cycles).toBe(100)

    // A login carries the door along too.
    await clearSession(context)
    await page.goto(`/checkin/${code}`)
    await page.getByRole("link", { name: "LOGIN" }).click()
    await expect(page).toHaveURL(`/login?next=%2Fcheckin%2F${code}`)
    await page.getByLabel("login:").fill(player)
    await page.getByLabel("password:", { exact: true }).fill(PASSWORD)
    await page.getByRole("button", { name: "LOGIN" }).click()
    await expect(page).toHaveURL(`/checkin/${code}`)
    await expect(page.locator("[data-state=in]")).toBeVisible()

    // The backoffice lists who came, and nobody else.
    await setSession(context, admin.token)
    await page.goto(`/admin/events/${event.id}`)
    const checkins = page.getByRole("table", { name: "check-ins" })
    await expect(checkins).toContainText(player)
    await expect(checkins.locator("tbody tr")).toHaveCount(1)
})

test("an admin checks a player in by hand for a night that is over", async ({
    page,
    context,
}) => {
    const admin = await newAdmin(context)
    const event = await publishedEvent(
        context.request,
        admin.token,
        -30 * 24 * HOUR,
        5 * HOUR,
    )
    const player = await newPlayer(context, "old")
    await setSession(context, admin.token)

    await page.goto(`/admin/events/${event.id}`)
    await page.getByLabel("handle of the player to check in").fill(player.name)
    await page.getByRole("button", { name: "add check-in" }).click()
    await expect(page.getByRole("status")).toContainText("check-in added")
    await expect(page.getByRole("table", { name: "check-ins" })).toContainText(
        player.name,
    )
    expect((await standingOf(context.request, player.name)).cycles).toBe(100)

    // This night is over, so the public page files it under the archive.
    await page.goto("/events")
    await expect(page.locator("main")).toContainText("ls -la ./archive")
    await expect(page.locator(`[data-event="${event.id}"]`)).toBeVisible()

    await page.goto(`/admin/events/${event.id}`)
    // An unknown handle gets a sentence, not a stack.
    await page.getByLabel("handle of the player to check in").fill("nobodyhere99")
    await page.getByRole("button", { name: "add check-in" }).click()
    await expect(page.getByRole("alert")).toContainText(/not found/i)
})

test("a logged-in player checks in with one tap, or hands the phone over", async ({
    page,
    context,
}) => {
    const admin = await newAdmin(context)
    const event = await publishedEvent(context.request, admin.token, -HOUR, 5 * HOUR)
    const code = event.code
    const player = await newPlayer(context, "tap")

    await page.goto(`/checkin/${code}`)
    await expect(page.locator("form[data-state=ready]")).toContainText(player.name)

    // Not this player: the phone belongs to a friend. Log out, come back.
    await page.getByRole("button", { name: /not you/ }).click()
    await expect(page).toHaveURL(`/login?next=%2Fcheckin%2F${code}`)
    await page.goto(`/checkin/${code}`)
    await expect(page.locator("[data-state=anonymous]")).toBeVisible()

    await login(page, player.name)
    await page.goto(`/checkin/${code}`)
    await page.getByRole("button", { name: "CHECK IN" }).click()
    await expect(page.locator("[data-state=confirmed]")).toBeVisible()
})

test("the door is closed outside the window, and a bad code is a 404", async ({
    page,
    context,
}) => {
    const admin = await newAdmin(context)
    const [early, over] = await Promise.all([
        publishedEvent(context.request, admin.token, HOUR, 5 * HOUR),
        publishedEvent(context.request, admin.token, -6 * HOUR, 5 * HOUR),
    ])

    await page.goto(`/checkin/${early.code}`)
    await expect(page.locator("[data-state=early]")).toContainText("doors open at")
    await expect(page.getByRole("timer")).toBeVisible()
    await expect(page.getByRole("button", { name: "CHECK IN" })).toHaveCount(0)

    await page.goto(`/checkin/${over.code}`)
    await expect(page.locator("[data-state=over]")).toContainText("check-in is closed")

    // The API refuses too, whatever the page shows.
    const refused = await page.request.post(`${API}/api/checkin/${early.code}`, {
        headers: bearer(admin.token),
    })
    expect(refused.status()).toBe(409)

    for (const path of ["/checkin/not-a-code", "/checkin/zzzzzzzzzzzz"]) {
        expect((await page.goto(path))?.status(), path).toBe(404)
    }

    // Back to draft, and the door of the early event opens nothing.
    await page.goto(`/admin/events/${early.id}`)
    page.once("dialog", dialog => dialog.accept())
    await page.getByRole("button", { name: "back to draft" }).click()
    await expect(page.getByRole("heading", { level: 1 })).toContainText("# draft")
    expect((await page.goto(`/checkin/${early.code}`))?.status()).toBe(404)
})
