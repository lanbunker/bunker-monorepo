import { defineMiddleware } from "astro:middleware"

import { SESSION_COOKIE, call } from "./lib/api"
import { submittedFields } from "./lib/submitted"

/** Pages a player with a temporary password can still open. */
const ALLOWED_PAGES: ReadonlySet<string> = new Set(["/password", "/login"])
/** Actions a player with a temporary password can still run. */
const ALLOWED_ACTIONS: ReadonlySet<string> = new Set(["changePassword", "logout"])

const ACTION_PATH = "/_actions/"

/** Headers every answer carries. A page that sets one of its own keeps it. */
const SECURITY_HEADERS: Readonly<Record<string, string>> = {
    "x-content-type-options": "nosniff",
    "referrer-policy": "strict-origin-when-cross-origin",
    "x-frame-options": "DENY",
    "content-security-policy": "frame-ancestors 'none'",
}

/**
 * Every action name a request carries. A form names it in `?_action=` on the
 * URL of a page, and a call from an island names it in the path under
 * `/_actions/`. A request can carry both, so the gate reads both.
 */
const actionsOf = (url: URL): string[] => [
    ...url.searchParams.getAll("_action"),
    ...(url.pathname.startsWith(ACTION_PATH)
        ? [url.pathname.slice(ACTION_PATH.length)]
        : []),
]

/**
 * A call under `/_actions/` answers JSON and renders no page, so only its
 * action names decide. A form action renders the page it posts to, so that page
 * must be allowed as well.
 */
const allowedWhileForced = (url: URL): boolean => {
    const actions = actionsOf(url)
    const actionsAllowed = actions.every(action => ALLOWED_ACTIONS.has(action))
    return url.pathname.startsWith(ACTION_PATH)
        ? actionsAllowed
        : actionsAllowed && ALLOWED_PAGES.has(url.pathname)
}

/**
 * Sets the headers every answer needs. A page with a player in it must never
 * sit in a shared cache, so HTML for a session is `no-store` unless the page
 * chose its own policy. The response is copied first, because the headers of a
 * redirect or a fetched response are immutable.
 */
const secured = (response: Response, hasSession: boolean): Response => {
    const copy = new Response(response.body, response)
    for (const [name, value] of Object.entries(SECURITY_HEADERS)) {
        if (!copy.headers.has(name)) copy.headers.set(name, value)
    }
    const isHtml = copy.headers.get("content-type")?.startsWith("text/html") ?? false
    if (hasSession && isHtml && !copy.headers.has("cache-control")) {
        copy.headers.set("cache-control", "private, no-store")
    }
    return copy
}

/**
 * Turns the session cookie into `locals.player` for every request. One call to
 * the API per page load. A dead token clears the cookie, so the header never
 * shows a player who cannot act. A player with a temporary password reaches
 * only `/password` and `/login`, and runs only the password change and the
 * logout. Every other request, a POST included, goes to `/password`.
 */
export const onRequest = defineMiddleware(async (context, next) => {
    // A refused form re-renders the page, and Astro gives it the failure but not
    // the input. The fields are kept here so no field is typed twice.
    context.locals.submitted = await submittedFields(context.request, context.url)

    const token = context.cookies.get(SESSION_COOKIE)?.value
    if (!token) return secured(await next(), false)

    const result = await call(client => client.GET("/api/me"), token)

    if (!result.ok) {
        // A refusal means the token is dead. An outage means nothing about it,
        // so the cookie survives and the player is logged in again by the next
        // request that reaches the API.
        if (result.failure.kind === "refused" && result.failure.status === 401) {
            context.cookies.delete(SESSION_COOKIE, { path: "/" })
        }
        return secured(await next(), true)
    }

    context.locals.player = result.data.player
    context.locals.token = token
    context.locals.mustChangePassword = result.data.mustChangePassword

    if (result.data.mustChangePassword && !allowedWhileForced(context.url)) {
        return secured(context.redirect("/password"), true)
    }

    return secured(await next(), true)
})
