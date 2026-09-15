import type { APIRoute } from "astro"

import { call } from "../../../../lib/api"
import { checkinUrl } from "../../../../lib/events"
import { checkinPoster, posterFileName } from "../../../../lib/qr"
import { adminSessionOf } from "../../../../lib/session"

/**
 * The QR poster of an event, for the door. Admins only: the code inside it is
 * the secret of the door, so a stranger gets the same 404 as for any page of
 * the backoffice. `?download=1` makes the browser save it instead of showing it.
 */
export const GET: APIRoute = async ({ params, locals, url }) => {
    const session = adminSessionOf(locals)
    if (!session) return new Response(null, { status: 404 })

    const result = await call(
        client =>
            client.GET("/api/admin/events/{id}", {
                params: { path: { id: params.id ?? "" } },
            }),
        session.token,
    )
    if (!result.ok) {
        const status = result.failure.kind === "refused" ? result.failure.status : 502
        return new Response(null, { status })
    }

    const detail = result.data
    const poster = checkinPoster(
        checkinUrl(url.origin, detail.checkinCode),
        detail.event.name,
    )
    const disposition = url.searchParams.has("download") ? "attachment" : "inline"

    return new Response(poster, {
        headers: {
            "content-type": "image/svg+xml",
            "cache-control": "no-store",
            "content-disposition": `${disposition}; filename="${posterFileName(detail.event.name, "svg")}"`,
        },
    })
}
