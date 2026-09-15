import { defineAction } from "astro:actions"
import { z } from "zod"

import { requireAdmin, requireToken, unwrap } from "../lib/action"
import { call, callEmpty } from "../lib/api"
import {
    checkinCode,
    eventInput,
    handle,
    eventStatusInput,
    eventUpdateInput,
    uuid,
} from "../lib/schemas"

const byId = z.object({ id: uuid })

/** The door for a player, and the backoffice for an admin. Every form posts. */
export const eventActions = {
    checkIn: defineAction({
        accept: "form",
        input: z.object({ code: checkinCode }),
        handler: async (input, context) => {
            const receipt = unwrap(
                await call(
                    client =>
                        client.POST("/api/checkin/{code}", {
                            params: { path: { code: input.code } },
                        }),
                    requireToken(context.locals),
                ),
            )
            // The API pays on the first scan only, so the receipt says whether
            // this scan paid or found the player already in.
            return { code: input.code, paid: receipt.cycles > 0 }
        },
    }),

    createEvent: defineAction({
        accept: "form",
        input: eventInput,
        handler: async (input, context) => {
            const created = unwrap(
                await call(
                    client =>
                        client.POST("/api/admin/events", {
                            body: {
                                name: input.name,
                                location: input.location,
                                games: input.games,
                                description: input.description,
                                image: input.image ?? null,
                                startsAt: input.startsAt,
                                endsAt: input.endsAt,
                            },
                        }),
                    requireAdmin(context.locals),
                ),
            )
            return { id: created.id }
        },
    }),

    updateEvent: defineAction({
        accept: "form",
        input: eventUpdateInput,
        handler: async (input, context) => {
            unwrap(
                await call(
                    client =>
                        client.PUT("/api/admin/events/{id}", {
                            params: { path: { id: input.id } },
                            body: {
                                name: input.name,
                                location: input.location,
                                games: input.games,
                                description: input.description,
                                image: input.image ?? null,
                                startsAt: input.startsAt,
                                endsAt: input.endsAt,
                            },
                        }),
                    requireAdmin(context.locals),
                ),
            )
            return { id: input.id }
        },
    }),

    setEventStatus: defineAction({
        accept: "form",
        input: eventStatusInput,
        handler: async (input, context) => {
            const event = unwrap(
                await call(
                    client =>
                        client.POST("/api/admin/events/{id}/status", {
                            params: { path: { id: input.id } },
                            body: { status: input.status },
                        }),
                    requireAdmin(context.locals),
                ),
            )
            return { id: input.id, status: event.status }
        },
    }),

    addCheckin: defineAction({
        accept: "form",
        input: z.object({ id: uuid, handle }),
        handler: async (input, context) => {
            const token = requireAdmin(context.locals)
            const player = unwrap(
                await call(
                    client =>
                        client.GET("/api/players/{handle}", {
                            params: { path: { handle: input.handle } },
                        }),
                    token,
                ),
            )
            unwrap(
                await call(
                    client =>
                        client.POST("/api/admin/events/{id}/checkins", {
                            params: { path: { id: input.id } },
                            body: { playerId: player.id },
                        }),
                    token,
                ),
            )
            return { handle: player.handle }
        },
    }),

    deleteEvent: defineAction({
        accept: "form",
        input: byId,
        handler: async (input, context) => {
            unwrap(
                await callEmpty(
                    client =>
                        client.DELETE("/api/admin/events/{id}", {
                            params: { path: { id: input.id } },
                        }),
                    requireAdmin(context.locals),
                ),
            )
            return { deleted: true }
        },
    }),
}
