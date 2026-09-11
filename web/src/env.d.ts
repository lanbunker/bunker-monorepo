/// <reference types="astro/client" />

import type { Player } from "./lib/api"
import type { Submitted } from "./lib/submitted"

declare global {
    namespace App {
        interface Locals {
            /** The logged-in player, set by the middleware from the session cookie. */
            player?: Player
            /** The bearer token behind `player`, for server-side calls on their behalf. */
            token?: string
            /** Set after an admin reset. The middleware sends the player to `/password`. */
            mustChangePassword?: boolean
            /** The fields of a posted form, minus the secret ones. Empty on a GET. */
            submitted: Submitted
        }
    }
}
