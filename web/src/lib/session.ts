import type { Player } from "./api"

/** A logged-in caller and the token that acts for them. */
export type Session = {
    readonly player: Player
    readonly token: string
    /** Set after an admin reset. The middleware holds the player on `/password`. */
    readonly mustChangePassword: boolean
}

/**
 * The session of the request, or `undefined`. A page that needs a login
 * redirects to `/login` on `undefined`, and every call it makes afterwards has
 * a token that is not optional.
 */
export const sessionOf = (locals: App.Locals): Session | undefined =>
    locals.player && locals.token
        ? {
              player: locals.player,
              token: locals.token,
              mustChangePassword: locals.mustChangePassword ?? false,
          }
        : undefined

/**
 * The session of an admin, or `undefined`. A backoffice page rewrites to `/404`
 * on `undefined`, so the backoffice does not tell a stranger that it exists.
 */
export const adminSessionOf = (locals: App.Locals): Session | undefined => {
    const session = sessionOf(locals)
    return session?.player.role === "admin" ? session : undefined
}

/**
 * Whether a handle is the caller's own. An inactive player has no public page,
 * so their own link to it leads back to the profile, which says why.
 */
export const isOwnHandle = (locals: App.Locals, handle: string): boolean =>
    sessionOf(locals)?.player.handle.toLowerCase() === handle.toLowerCase()
