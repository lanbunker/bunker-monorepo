import type { FullConfig } from "@playwright/test"

/**
 * Vite discovers dependencies on the first requests of a fresh dev server, and
 * every time it bundles a new one it reloads every open page. A reload in the
 * middle of the first form post cancels that post, and the first test fails
 * for a reason that has nothing to do with the site. So every kind of page is
 * requested here, twice, before any test opens a browser.
 */
const PATHS = [
    "/",
    "/signup",
    "/login",
    "/events",
    "/tournaments",
    "/players",
    "/cycles",
    "/profile",
    "/admin",
    "/checkin/zzzzzzzzzzzz",
]

const warmup = async (config: FullConfig) => {
    const base = config.projects[0]?.use.baseURL
    if (!base) return
    for (let round = 0; round < 2; round += 1) {
        for (const path of PATHS) {
            // A page that fails here fails its own test with a better message.
            await fetch(`${base}${path}`, { redirect: "manual" })
                .then(r => r.arrayBuffer())
                .catch((cause: unknown) =>
                    console.warn(`warm-up of ${path} failed`, cause),
                )
        }
    }
}

export default warmup
