import { expect, test } from "@playwright/test"

import { API, handle, signup, signupAdmin, tokenFor } from "./support"

/** Every public page answers, and no page leaks a stack or an empty shell. */

const PUBLIC_PAGES = [
    { path: "/", heading: /Enter the bunker/i, title: /LAN BUNKER/ },
    { path: "/events", heading: /upcoming/i, title: /EVENTS/ },
    { path: "/tournaments", heading: /tournaments/i, title: /TOURNAMENTS/ },
    { path: "/players", heading: /players/i, title: /PLAYERS/ },
    { path: "/media", heading: /media/i, title: /MEDIA/ },
    { path: "/shop", heading: /shop/i, title: /SHOP/ },
    { path: "/about", heading: /cat /i, title: /ABOUT/ },
]

for (const entry of PUBLIC_PAGES) {
    test(`the page ${entry.path} answers 200 and renders its own title`, async ({
        page,
    }) => {
        const response = await page.goto(entry.path)
        expect(response?.status()).toBe(200)
        await expect(page).toHaveTitle(entry.title)
        await expect(page.locator("main")).toContainText(entry.heading)
        await expect(page.getByRole("alert")).toHaveCount(0)
    })
}

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

test("the number keys move between sections, and the status bar follows", async ({
    page,
}) => {
    await page.goto("/")
    await page.keyboard.press("2")
    await expect(page).toHaveURL(/\/events$/)
    await expect(page.getByRole("link", { name: /2:\s*EVENTS/ })).toHaveAttribute(
        "aria-current",
        "page",
    )
    await page.keyboard.press("4")
    await expect(page).toHaveURL(/\/players$/)
})

test("the nav marks the section of the page the visitor is on", async ({ page }) => {
    await page.goto("/tournaments")
    await expect(page.getByRole("link", { name: /3:\s*TOURNAMENTS/ })).toHaveAttribute(
        "aria-current",
        "page",
    )
})

test("a bare page carries no nav, and a private page stays out of search", async ({
    page,
}) => {
    await page.goto("/signup")
    await expect(page.getByRole("navigation", { name: "Sections" })).toHaveCount(0)

    await signup(page, handle("idx"))
    await page.goto("/profile")
    await expect(page.locator('meta[name="robots"]')).toHaveAttribute(
        "content",
        "noindex, nofollow",
    )
})

test("the public bracket page shows the rounds once a bracket exists", async ({
    page,
}) => {
    const admin = await signupAdmin(page)
    const token = await tokenFor(page, admin)
    const created = await page.request.post(`${API}/api/admin/tournaments`, {
        data: {
            name: `Cup ${handle("b")}`,
            game: "COD MW2",
            mode: "1v1",
            description: "a public bracket",
            date: "2030-02-02",
            registrationClosesAt: "2029-12-31T20:00:00Z",
        },
        headers: { authorization: `Bearer ${token}` },
    })
    const body: Record<string, unknown> = await created.json()
    const id = String(body.id)

    // Before a bracket exists the page is a 404, not an empty frame.
    expect((await page.goto(`/tournaments/${id}/bracket`))?.status()).toBe(404)

    for (const name of [handle("q1"), handle("q2")]) {
        const player = await page.request.post(`${API}/api/auth/signup`, {
            data: { handle: name, password: "correct-horse-battery" },
        })
        expect(player.status()).toBe(201)
        const found: Record<string, unknown> = await page.request
            .get(`${API}/api/players/${name}`)
            .then(r => r.json())
        const added = await page.request.post(
            `${API}/api/admin/tournaments/${id}/entrants`,
            {
                data: { playerId: found.id },
                headers: { authorization: `Bearer ${token}` },
            },
        )
        expect(added.status()).toBe(201)
    }
    await page.request.post(`${API}/api/admin/tournaments/${id}/status`, {
        data: { status: "live", winner: null },
        headers: { authorization: `Bearer ${token}` },
    })
    const bracket = await page.request.post(
        `${API}/api/admin/tournaments/${id}/bracket`,
        { headers: { authorization: `Bearer ${token}` } },
    )
    expect(bracket.ok()).toBe(true)

    await page.goto(`/tournaments/${id}/bracket`)
    await expect(page.getByText("final", { exact: true })).toBeVisible()
    await expect(page.locator("[data-match]")).toHaveCount(1)
    await expect(page.getByText("champion")).toBeVisible()
    // A public bracket takes no gesture: no side is a control.
    await expect(page.locator("button[data-pick]")).toHaveCount(0)

    await page.goto(`/tournaments/${id}/kiosk`)
    await expect(page.getByText("■ live")).toBeVisible()
    await expect(page.locator("[data-match]")).toHaveCount(1)
})
