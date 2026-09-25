import { defineAction } from "astro:actions"
import type { z } from "zod"

import { playerByHandle, requireAdmin, requireToken, unwrap } from "../lib/action"
import { call, callEmpty } from "../lib/api"
import {
    byId,
    checkinInput,
    eventInput,
    eventStatusInput,
    eventUpdateInput,
    playerHandleInput,
} from "../lib/schemas"

/** The body the create and the update routes share. */
const eventBody = (input: z.output<typeof eventInput>) => ({
    name: input.name,
    location: input.location,
    games: input.games,
    description: input.description,
    image: input.image ?? null,
    startsAt: input.startsAt,
    endsAt: input.endsAt,
})

/** The door for a player, and the backoffice for an admin. Every form posts. */
export const eventActions = {
    checkIn: defineAction({
        accept: "form",
        input: checkinInput,
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
                        client.POST("/api/admin/events", { body: eventBody(input) }),
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
                            body: eventBody(input),
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
        input: playerHandleInput,
        handler: async (input, context) => {
            const token = requireAdmin(context.locals)
            const player = await playerByHandle(input.handle, token)
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
