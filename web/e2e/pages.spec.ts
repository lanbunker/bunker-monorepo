import { expect, test } from "@playwright/test"

import { SITE, newPlayer } from "./support"

/**
 * Every public page answers, and no page leaks a stack or an empty shell. The
 * marker of each page is copy the page always carries, never a row, because
 * another worker writes rows at the same time.
 */

const PUBLIC_PAGES = [
    { path: "/", heading: /Enter the bunker/i, title: /LAN BUNKER/ },
    { path: "/events", heading: /upcoming/i, title: /EVENTS/ },
    { path: "/tournaments", heading: /tournaments/i, title: /TOURNAMENTS/ },
    { path: "/players", heading: /players/i, title: /PLAYERS/ },
    { path: "/cycles", heading: /man cycles/i, title: /CYCLES/ },
    { path: "/media", heading: /media/i, title: /MEDIA/ },
    { path: "/shop", heading: /shop/i, title: /SHOP/ },
    { path: "/about", heading: /cat /i, title: /ABOUT/ },
]

test("every public page answers 200 and renders its own title", async ({ page }) => {
    for (const entry of PUBLIC_PAGES) {
        const response = await page.goto(entry.path)
        expect(response?.status(), entry.path).toBe(200)
        await expect(page, entry.path).toHaveTitle(entry.title)
        await expect(page.locator("main"), entry.path).toContainText(entry.heading)
        await expect(page.getByRole("alert"), entry.path).toHaveCount(0)
    }
})

test("the home page shows its tagline, with or without scripts", async ({ browser }) => {
    for (const javaScriptEnabled of [true, false]) {
        const context = await browser.newContext({ baseURL: SITE, javaScriptEnabled })
        const page = await context.newPage()
        await page.goto("/")
        await expect(
            page.getByText("Enter the bunker."),
            `js ${javaScriptEnabled}`,
        ).toBeVisible()
        await expect(page.locator("#status-block")).toBeVisible()
        await context.close()
    }
})

test("a done key the page does not know shows no notice", async ({ page }) => {
    // A plain lookup would find these on the prototype of the notice table.
    for (const key of ["valueOf", "constructor", "toString", "__proto__"]) {
        const response = await page.goto(`/tournaments?done=${key}`)
        expect(response?.status(), key).toBe(200)
        await expect(page.getByRole("status"), key).toHaveCount(0)
    }
})

test("an unknown path answers 404 and names the path", async ({ page }) => {
    const response = await page.goto("/no/such/sector")
    expect(response?.status()).toBe(404)
    await expect(page.locator("main")).toContainText("/no/such/sector")
    await expect(
        page.locator("main").getByRole("link", { name: "HOME", exact: true }),
    ).toBeVisible()
})

test("an unknown tournament is a 404 on every one of its pages", async ({ page }) => {
    const missing = "00000000-0000-0000-0000-000000000001"
    for (const path of [
        `/tournaments/${missing}/bracket`,
        `/tournaments/${missing}/kiosk`,
    ]) {
        const response = await page.goto(path)
        expect(response?.status(), path).toBe(404)
    }
    const json = await page.request.get(`/tournaments/${missing}/detail.json`)
    expect(json.status()).toBe(404)
})

test("the media gallery opens an image in the lightbox", async ({ page }) => {
    await page.goto("/media")
    const first = page.locator("#gallery a").first()
    await expect(first).toBeVisible()
    await first.click()
    await expect(page.locator(".pswp--open")).toBeVisible()
    await page.keyboard.press("Escape")
    await expect(page.locator(".pswp--open")).toHaveCount(0)
})

test("the number keys move between sections, and the nav marks the one shown", async ({
    page,
}) => {
    await page.goto("/")
    await page.keyboard.press("2")
    await expect(page).toHaveURL(/\/events$/)
    await expect(page.getByRole("link", { name: /2:\s*EVENTS/ })).toHaveAttribute(
        "aria-current",
        "page",
    )

    // The script that reads the key sits at the end of the body, so the next
    // press needs the new document parsed, and not only its URL.
    await page.waitForLoadState("domcontentloaded")
    await page.keyboard.press("4")
    await expect(page).toHaveURL(/\/players$/)

    await page.goto("/tournaments")
    await expect(page.getByRole("link", { name: /3:\s*TOURNAMENTS/ })).toHaveAttribute(
        "aria-current",
        "page",
    )
})

test("the homepage reports the uplink, and the way in goes once a player is in", async ({
    page,
    context,
}) => {
    await page.goto("/")
    await expect(page.locator("#boot-log")).toContainText("[  OK ] bunkernet uplink")
    await expect(page.getByText(/\d+ players? enlisted/)).toBeVisible()
    await expect(page.getByRole("link", { name: "ENTER BUNKERNET" })).toBeVisible()

    await newPlayer(context, "hom")
    await page.goto("/")
    await expect(page.getByRole("link", { name: "ENTER BUNKERNET" })).toHaveCount(0)
})

test("a bare page carries no nav, and a private page stays out of search", async ({
    page,
    context,
}) => {
    await page.goto("/signup")
    await expect(page.getByRole("navigation", { name: "Sections" })).toHaveCount(0)

    await newPlayer(context, "idx")
    await page.goto("/profile")
    await expect(page.locator('meta[name="robots"]')).toHaveAttribute(
        "content",
        "noindex, nofollow",
    )
})
