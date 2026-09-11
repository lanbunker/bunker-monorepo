import type { APIRoute } from "astro"

import { apiClient } from "../../../lib/api"

/**
 * The kiosk polls this route. The browser never talks to the API, so the site
 * relays the public detail and forbids caching: a stale bracket is the one
 * thing a live screen must not show.
 */
export const GET: APIRoute = async ({ params }) => {
    const { data, response } = await apiClient()
        .GET("/api/tournaments/{id}", { params: { path: { id: params.id ?? "" } } })
        .catch(() => ({ data: undefined, response: undefined }))
    if (!data) return new Response(null, { status: response?.status ?? 502 })

    return new Response(JSON.stringify(data), {
        headers: { "content-type": "application/json", "cache-control": "no-store" },
    })
}
