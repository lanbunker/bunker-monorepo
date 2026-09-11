import { defineMiddleware } from "astro:middleware"

import { SESSION_COOKIE, call } from "./lib/api"
import { submittedFields } from "./lib/submitted"

/** Pages a player with a temporary password can still open. */
const ALLOWED_PAGES: ReadonlySet<string> = new Set(["/password", "/login"])
/** Actions the player can still post. Astro names the action in `?_action=`. */
const ALLOWED_ACTIONS: ReadonlySet<string> = new Set(["changePassword", "logout"])

const allowedWhileForced = (url: URL): boolean => {
    const action =
        url.searchParams.get("_action") ?? url.pathname.replace(/^\/_actions\//, "")
    return ALLOWED_PAGES.has(url.pathname) || ALLOWED_ACTIONS.has(action)
}

/**
 * Turns the session cookie into `locals.player` for every request. One call to
 * the API per page load. A dead token clears the cookie, so the header never
 * shows a player who cannot act. A pending password change sends the player to
 * `/password` and lets nothing else through, not even a POST.
 */
export const onRequest = defineMiddleware(async (context, next) => {
    // A refused form re-renders the page, and Astro gives it the failure but not
    // the input. The fields are kept here so no field is typed twice.
    context.locals.submitted = await submittedFields(context.request, context.url)

    const token = context.cookies.get(SESSION_COOKIE)?.value
    if (!token) return next()

    const result = await call(client => client.GET("/api/me"), token)

    if (!result.ok) {
        // A refusal means the token is dead. An outage means nothing about it,
        // so the cookie survives and the player is logged in again by the next
        // request that reaches the API.
        if (result.failure.kind === "refused" && result.failure.status === 401) {
            context.cookies.delete(SESSION_COOKIE, { path: "/" })
        }
        return next()
    }

    context.locals.player = result.data.player
    context.locals.token = token
    context.locals.mustChangePassword = result.data.mustChangePassword

    if (result.data.mustChangePassword && !allowedWhileForced(context.url)) {
        return context.redirect("/password")
    }

    return next()
})
