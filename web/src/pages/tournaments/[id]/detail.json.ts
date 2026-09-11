import type { APIRoute } from "astro"

import { call } from "../../../lib/api"

/**
 * The kiosk polls this route. The browser never talks to the API, so the site
 * relays the public detail and forbids caching: a stale bracket is the one
 * thing a live screen must not show.
 */
export const GET: APIRoute = async ({ params }) => {
    const result = await call(client =>
        client.GET("/api/tournaments/{id}", {
            params: { path: { id: params.id ?? "" } },
        }),
    )
    if (!result.ok) {
        const status = result.failure.kind === "refused" ? result.failure.status : 502
        return new Response(null, { status })
    }

    return new Response(JSON.stringify(result.data), {
        headers: { "content-type": "application/json", "cache-control": "no-store" },
    })
}
