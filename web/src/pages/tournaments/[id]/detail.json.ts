import type { APIRoute } from "astro"

import { call, relayFailure } from "../../../lib/api"
import { uuid } from "../../../lib/schemas"

/**
 * The kiosk polls this route. The browser never talks to the API, so the site
 * relays the public detail and forbids caching: a stale bracket is the one
 * thing a live screen must not show.
 */
export const GET: APIRoute = async ({ params }) => {
    const id = uuid.safeParse(params.id).data
    if (!id) return new Response(null, { status: 404 })

    const result = await call(client =>
        client.GET("/api/tournaments/{id}", { params: { path: { id } } }),
    )
    if (!result.ok) return relayFailure(result.failure)

    return new Response(JSON.stringify(result.data), {
        headers: { "content-type": "application/json", "cache-control": "no-store" },
    })
}
