/// <reference types="astro/client" />

declare namespace App {
    interface Locals {
        /** The logged-in player, set by the middleware from the session cookie. */
        player?: import("./lib/api").Player
        /** The bearer token behind `player`, for server-side calls on their behalf. */
        token?: string
        /** Set after an admin reset. The middleware sends the player to `/password`. */
        mustChangePassword?: boolean
    }
}
