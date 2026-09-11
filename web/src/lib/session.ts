import type { Player } from "./api"

/** A logged-in caller and the token that acts for them. */
export type Session = {
    readonly player: Player
    readonly token: string
}

/**
 * The session of the request, or `undefined`. A page that needs a login
 * redirects to `/login` on `undefined`, and every call it makes afterwards has
 * a token that is not optional.
 */
export const sessionOf = (locals: App.Locals): Session | undefined =>
    locals.player && locals.token
        ? { player: locals.player, token: locals.token }
        : undefined

/**
 * The session of an admin, or `undefined`. A backoffice page rewrites to `/404`
 * on `undefined`, so the backoffice does not tell a stranger that it exists.
 */
export const adminSessionOf = (locals: App.Locals): Session | undefined => {
    const session = sessionOf(locals)
    return session?.player.role === "admin" ? session : undefined
}
