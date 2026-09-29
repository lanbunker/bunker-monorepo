import { expect, test } from "@playwright/test"

import {
    API,
    FROM_SITE,
    apiDraft,
    bearer,
    detailSchema,
    handle,
    jsonOf,
    newAdmin,
    newPlayer,
    readableFailure,
    setTournamentStatus,
} from "./support"

/** A well-formed UUID that matches no row. A nil-shaped id is not a valid UUID. */
const MISSING_ID = "3f2504e0-4f89-41d3-9a0c-0305e82c3301"

/**
 * Every failure a player can cause must reach the page as one sentence they can
 * act on. Astro reports a refused input as JSON inside the message, and the API
 * reports its own refusals in a body. Both go through the same helper on the
 * site, so both are checked here.
 */

test("an admin adding an unknown handle is told, and the page still works", async ({
    page,
    context,
}) => {
    const admin = await newAdmin(context)
    const id = await apiDraft(context.request, admin.token)

    await page.goto(`/admin/tournaments/${id}`)
    await page.getByLabel("handle of the player to add").fill("nobody-at-all")
    await page.getByRole("button", { name: "add entrant" }).click()
    await readableFailure(page, /not found|no player/i)
    // The failure is a message, not a broken page: the form is still there.
    await expect(page.getByLabel("handle of the player to add")).toBeVisible()
})

test("a tournament form with no date is refused with a sentence", async ({
    page,
    context,
}) => {
    await newAdmin(context)
    const refused = await page.request.post(
        "/admin/tournaments?_action=createTournament",
        {
            form: {
                name: "No date cup",
                game: "COD MW2",
                mode: "1v1",
                description: "",
                date: "",
                registrationClosesAt: "",
            },
            headers: FROM_SITE,
        },
    )
    expect(refused.status()).toBe(400)
    const body = await refused.text()
    expect(body).toMatch(/Pick a date\.|Pick the moment registration closes\./)
    expect(body).not.toContain("Failed to validate")
})

test("a refused draft keeps every field of the tournament form", async ({
    page,
    context,
}) => {
    await newAdmin(context)
    await page.goto("/admin/tournaments")
    const name = `Keep ${handle("t")}`
    await page.getByLabel("name", { exact: true }).fill(name)
    await page.getByLabel("game").fill("COD MW2")
    await page.getByLabel("description").fill("the rules of the cup")
    await page.getByLabel("date").fill("2030-05-05")
    await page
        .getByLabel("registration closes (your local time)")
        .fill("2030-05-04T20:00")
    // A mode of spaces alone passes `required` in the browser and fails the
    // rule on the server, so the post reaches the action.
    await page.getByLabel("mode").fill("   ")
    await page.getByRole("button", { name: "CREATE DRAFT" }).click()

    await readableFailure(page, "A tournament needs a mode.")
    await expect(page.getByLabel("name", { exact: true })).toHaveValue(name)
    await expect(page.getByLabel("game")).toHaveValue("COD MW2")
    await expect(page.getByLabel("description")).toHaveValue("the rules of the cup")
    await expect(page.getByLabel("date")).toHaveValue("2030-05-05")
})

test("a logged out visitor cannot apply, and a user cannot run an admin action", async ({
    page,
    context,
}) => {
    const applyAnonymously = await page.request.post(
        "/tournaments?_action=applyToTournament",
        {
            form: { id: MISSING_ID, skill: "3" },
            headers: FROM_SITE,
        },
    )
    expect(applyAnonymously.status()).toBe(401)

    // The action refuses a user, and the backoffice page it posts to rewrites
    // to 404 for the same user. The answer is 404: the site never tells a
    // stranger that the page exists.
    await newPlayer(context, "usr")
    const asUser = await page.request.post(
        "/admin/tournaments?_action=deleteTournament",
        {
            form: { id: MISSING_ID },
            headers: FROM_SITE,
        },
    )
    expect(asUser.status()).toBe(404)
    expect(await asUser.text()).not.toContain("root@bunker")
})

test("a malformed id is refused before it reaches the api", async ({ page, context }) => {
    await newPlayer(context, "mal")
    const refused = await page.request.post("/tournaments?_action=applyToTournament", {
        form: { id: "not-a-uuid", skill: "3" },
        headers: FROM_SITE,
    })
    expect(refused.status()).toBe(400)
    const body = await refused.text()
    expect(body).toContain("That link is not valid.")
    expect(body).not.toContain("Failed to validate")
})

test("a level outside 1 to 5 is refused with a sentence", async ({ page, context }) => {
    // The apply page renders the failure, and it needs an open tournament to
    // render at all. An admin is a player too, so one account does both.
    const admin = await newAdmin(context)
    const id = await apiDraft(context.request, admin.token)
    await setTournamentStatus(context.request, admin.token, id, "open")

    for (const skill of ["0", "6", "abc", ""]) {
        const refused = await page.request.post(
            `/tournaments/${id}/apply?_action=applyToTournament`,
            { form: { id, skill }, headers: FROM_SITE },
        )
        expect(refused.status(), `level ${JSON.stringify(skill)}`).toBe(400)
        const html = await refused.text()
        expect(html).toContain("Pick a level from 1 to 5.")
        expect(html).not.toContain("Failed to validate")
    }

    // The API is the authority: a level the site let through is refused there
    // too, and nobody is entered.
    const entered = await page.request.post(`${API}/api/tournaments/${id}/registration`, {
        data: { skill: 7 },
        headers: bearer(admin.token),
    })
    expect(entered.status()).toBe(422)
    const detail = await jsonOf(
        await page.request.get(`/tournaments/${id}/detail.json`),
        detailSchema,
    )
    expect(detail.tournament.entrantCount).toBe(0)
})

test("a refused cycles adjustment is a sentence for every field", async ({
    page,
    context,
}) => {
    await newAdmin(context)
    const post = (form: Record<string, string>) =>
        page.request.post("/admin/players?_action=adjustCycles", {
            form,
            headers: FROM_SITE,
        })

    for (const [form, sentence] of [
        [{ id: MISSING_ID, handle: "dave", amount: "0", note: "x" }, "not zero"],
        [{ id: MISSING_ID, handle: "dave", amount: "10", note: "   " }, "Give a reason"],
        [{ id: MISSING_ID, handle: "d", amount: "10", note: "x" }, "A handle is 3 to 20"],
        [{ id: "not-a-uuid", handle: "dave", amount: "10", note: "x" }, "not valid"],
    ] as const) {
        const refused = await post(form)
        expect(refused.status(), JSON.stringify(form)).toBe(400)
        const html = await refused.text()
        expect(html).toContain(sentence)
        expect(html).not.toContain("Failed to validate")
    }
})

test("an event whose night ends before its doors open reads as a sentence", async ({
    page,
    context,
}) => {
    await newAdmin(context)
    await page.goto("/admin/events")
    await page.getByLabel("name", { exact: true }).fill(`Night ${handle("bad")}`)
    await page.getByLabel("doors open (your local time)").fill("2030-02-02T21:00")
    await page.getByLabel("night ends (your local time)").fill("2030-02-02T20:00")
    await page.getByRole("button", { name: "CREATE DRAFT" }).click()

    await expect(page).toHaveURL(/\/admin\/events(\?|$)/)
    await readableFailure(page, "The end must come after the start.")
    // The typed name survives the refusal.
    await expect(page.getByLabel("name", { exact: true })).toHaveValue(/^Night /)
})
