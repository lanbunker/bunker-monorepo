import { defineMiddleware } from "astro:middleware"

import { SESSION_COOKIE, apiClient } from "./lib/api"

/** Pages a player with a temporary password can still open. */
const ALLOWED_PAGES = new Set(["/password", "/login"])
/** Actions the player can still post. Astro names the action in `?_action=`. */
const ALLOWED_ACTIONS = new Set(["changePassword", "logout"])

/**
 * Turns the session cookie into `locals.player` for every request. One call to
 * the API per page load. A dead token clears the cookie, so the header never
 * shows a player who cannot act. A pending password change sends the player to
 * `/password` and lets nothing else through, not even a POST.
 */
export const onRequest = defineMiddleware(async (context, next) => {
    const token = context.cookies.get(SESSION_COOKIE)?.value
    if (!token) return next()

    const { data, response } = await apiClient(token)
        .GET("/api/me")
        .catch(() => ({ data: undefined, response: undefined }))

    if (!data) {
        if (response?.status === 401) {
            context.cookies.delete(SESSION_COOKIE, { path: "/" })
        }
        return next()
    }

    context.locals.player = data.player
    context.locals.token = token
    context.locals.mustChangePassword = data.mustChangePassword

    if (data.mustChangePassword && !allowedWhileForced(context.url)) {
        return context.redirect("/password")
    }

    return next()
})

const allowedWhileForced = (url: URL): boolean => {
    const action =
        url.searchParams.get("_action") ?? url.pathname.replace(/^\/_actions\//, "")
    return ALLOWED_PAGES.has(url.pathname) || ALLOWED_ACTIONS.has(action)
}
